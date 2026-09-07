#!/usr/bin/env python3
"""Check the declared bilingual docs without network or third-party packages."""

from __future__ import annotations

import argparse
from collections import Counter
from html import unescape
from html.parser import HTMLParser
import json
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit


FENCES = re.compile(r"^(?P<fence>`{3,}|~{3,})(?P<lang>[^\n]*)\n(?P<body>.*?)^(?P=fence)[ \t]*$", re.M | re.S)
MD_LINK = re.compile(r"\[([^\]\n]+)\]\(([^)\s]+)\)")
COMMAND_LANGS = {"sh", "bash", "shell", "toml", "json", "rust", "python", "c", "cpp", "html"}
HAN = re.compile(r"[\u3400-\u9fff]")


def without_fences(text: str) -> str:
    return FENCES.sub(lambda match: "\n" * match[0].count("\n"), text)


def slug(title: str) -> str:
    title = re.sub(r"<[^>]*>", "", title)
    title = MD_LINK.sub(lambda match: match[1], title).lower()
    return re.sub(r"\s", "-", re.sub(r"[^\w\s-]", "", title))


def headings(text: str) -> list[tuple[int, str]]:
    seen: set[str] = set()
    result = []
    for match in re.finditer(r"^(#{1,6})[ \t]+(.+?)(?:[ \t]+#+)?[ \t]*$", without_fences(text), re.M):
        base = identifier = slug(match[2])
        suffix = 0
        while identifier in seen:
            suffix += 1
            identifier = f"{base}-{suffix}"
        seen.add(identifier)
        result.append((len(match[1]), identifier))
    return result


class Markup(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.ids: list[str] = []
        self.links: list[tuple[str, str]] = []
        self.text: list[str] = []
        self.href: str | None = None
        self.label: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = dict(attrs)
        if values.get("id"):
            self.ids.append(values["id"] or "")
        if tag == "a" and values.get("href"):
            self.href = values["href"]
            self.label = []

    def handle_data(self, data: str) -> None:
        self.text.append(data)
        if self.href is not None:
            self.label.append(data)

    def handle_endtag(self, tag: str) -> None:
        if tag == "a" and self.href is not None:
            self.links.append(("".join(self.label), self.href))
            self.href = None


def anchors(text: str) -> list[str]:
    markup = Markup()
    markup.feed(without_fences(text))
    return [identifier for _, identifier in headings(text)] + markup.ids


def links(text: str) -> list[tuple[str, str]]:
    plain = without_fences(text)
    markup = Markup()
    markup.feed(plain)
    return [(match[1], match[2]) for match in MD_LINK.finditer(plain)] + markup.links


def commands(text: str) -> list[tuple[str, str]]:
    return [(match["lang"].strip(), match["body"]) for match in FENCES.finditer(text)
            if match["lang"].strip() in COMMAND_LANGS]


def table_numbers(text: str) -> list[list[str]]:
    """Compare data cells, not translated captions, headers or measurement prose."""
    result = []
    lines = without_fences(text).splitlines()
    in_data = False
    for line in lines:
        if re.fullmatch(r"\|[\s|:\-]+\|", line):
            in_data = True
        elif in_data and line.startswith("|"):
            result.append(re.findall(r"\d+(?:\.\d+)?", line))
        else:
            in_data = False
    return result


def check(root: Path) -> list[str]:
    root = root.resolve()
    manifest = json.loads((root / "docs/i18n.json").read_text(encoding="utf-8"))
    errors: list[str] = []
    registry: dict[Path, tuple[dict, str]] = {}
    pages: dict[Path, str] = {}
    for pair in manifest["pairs"]:
        for lang in ("en", "zh"):
            path = (root / pair[lang]).resolve()
            if not path.is_relative_to(root) or path in registry:
                errors.append(f"Duplicate or out-of-tree document: {pair[lang]}")
                continue
            registry[path] = (pair, lang)
            if not path.is_file():
                errors.append(f"Missing paired document: {pair[lang]}")
            else:
                pages[path] = path.read_text(encoding="utf-8")

    for pattern in ("docs/project/k1-*.md", "docs/project/spacemit-k1*.md",
                    "docs/robot/install-k1*.md", "native/k1*/README*.md"):
        for path in root.glob(pattern):
            if path.resolve() not in registry:
                errors.append(f"K1 document has no language pair: {path.relative_to(root)}")

    cache: dict[Path, set[str]] = {}
    link_count = 0
    for path, text in pages.items():
        pair, lang = registry[path]
        label_path = path.relative_to(root)
        all_anchors = anchors(text)
        duplicate = [item for item, count in Counter(all_anchors).items() if count > 1]
        if duplicate:
            errors.append(f"Duplicate anchors in {label_path}: {duplicate}")
        cache[path] = set(all_anchors)
        if "{{CODE" in text or "{{TAIL}}" in text or "§" in text and "{{" in text:
            errors.append(f"Unexpanded translation placeholder: {label_path}")

        # Navigation and hidden compatibility IDs may contain the other language.
        prose = MD_LINK.sub(lambda match: match[1], without_fences(text))
        prose = re.sub(r"`+[^`\n]*`+", "", prose)
        markup = Markup()
        markup.feed(prose)
        visible = "".join(markup.text).replace("简体中文", "")
        if lang == "en" and HAN.search(visible):
            errors.append(f"Chinese prose in English page: {label_path}")
        if lang == "zh" and not HAN.search(visible):
            errors.append(f"No Chinese prose in Chinese page: {label_path}")

        switches = set()
        for label, target in links(text):
            url = urlsplit(unescape(target))
            if url.scheme or url.netloc:
                continue
            linked = (path.parent / unquote(url.path)).resolve() if url.path else path
            if not linked.is_relative_to(root) or not linked.exists():
                errors.append(f"Missing local target: {label_path} -> {target}")
                continue
            link_count += 1
            clean_label = re.sub(r"<[^>]*>|`", "", label).strip()
            if linked in registry:
                _, linked_lang = registry[linked]
                switch_lang = {"English": "en", "简体中文": "zh"}.get(clean_label)
                if switch_lang is not None:
                    if linked != (root / pair[switch_lang]).resolve():
                        errors.append(f"Language switch points to another page: {label_path} -> {target}")
                    else:
                        switches.add(switch_lang)
                elif linked_lang != lang:
                    errors.append(f"Cross-language body link: {label_path} -> {target}")
            elif lang == "zh" and linked.suffix == ".md" and "英文" not in clean_label:
                errors.append(f"Unlabelled upstream English reference: {label_path} -> {target}")
            if url.fragment and linked.suffix == ".md":
                if linked not in cache:
                    cache[linked] = set(anchors(linked.read_text(encoding="utf-8")))
                if unquote(url.fragment) not in cache[linked]:
                    errors.append(f"Missing section anchor: {label_path} -> {target}")
        if switches != {"en", "zh"}:
            errors.append(f"Missing English/Chinese language switches: {label_path}")

    for pair in manifest["pairs"]:
        a, b = ((root / pair[lang]).resolve() for lang in ("en", "zh"))
        if a not in pages or b not in pages:
            continue
        en, zh = pages[a], pages[b]
        if commands(en) != commands(zh):
            errors.append(f"Command/config blocks differ: {pair['en']} / {pair['zh']}")
        if Counter(re.findall(r"\b[a-f0-9]{64}\b", en)) != Counter(re.findall(r"\b[a-f0-9]{64}\b", zh)):
            errors.append(f"SHA256 values differ: {pair['en']} / {pair['zh']}")
        if pair.get("shared_anchors", True):
            en_heads, zh_heads = headings(en), headings(zh)
            if [level for level, _ in en_heads] != [level for level, _ in zh_heads]:
                errors.append(f"Heading structure differs: {pair['en']} / {pair['zh']}")
            elif not {h for _, h in en_heads}.issubset(set(anchors(zh))) or not {h for _, h in zh_heads}.issubset(set(anchors(en))):
                errors.append(f"Missing cross-language anchor aliases: {pair['en']} / {pair['zh']}")
        if pair["en"].startswith("docs/project/") and table_numbers(en) != table_numbers(zh):
            errors.append(f"Measurement-table numbers differ: {pair['en']} / {pair['zh']}")

    if not errors:
        print(f"OK: {len(manifest['pairs'])} bilingual pairs, {len(pages)} pages, {link_count} local links; commands, SHA256 and measurement tables agree.")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        errors = check(args.root)
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f"Documentation check failed: {error}\n")
    for error in errors:
        print(f"ERROR: {error}")
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
