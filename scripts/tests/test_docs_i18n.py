"""Regression fixtures for language routing and executable-document consistency."""

from contextlib import redirect_stdout
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest


SPEC = importlib.util.spec_from_file_location(
    "docs_i18n", Path(__file__).resolve().parents[1] / "check-docs-i18n.py"
)
assert SPEC is not None and SPEC.loader is not None
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class BilingualDocsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="microduck-docs-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.en = self.root / "docs/project/k1-demo.md"
        self.zh = self.root / "docs/project/k1-demo_zh.md"
        self.en.parent.mkdir(parents=True)
        self.en_text = (
            '<a id="示例"></a>\n# Demo\n\n'
            '[English](k1-demo.md) · [简体中文](k1-demo_zh.md)\n\n'
            '<a id="使用"></a>\n## Usage\n\nRun the command.\n\n'
            '```sh\nprintf "hello\\n"\n```\n\n'
            '| Metric | Value |\n| --- | --- |\n| Time | 3.25 ms |\n\n'
            + f'Checksum: `{ "a" * 64 }`\n'
        )
        self.zh_text = (
            '<a id="demo"></a>\n# 示例\n\n'
            '[English](k1-demo.md) · [简体中文](k1-demo_zh.md)\n\n'
            '<a id="usage"></a>\n## 使用\n\n运行以下命令。\n\n'
            '```sh\nprintf "hello\\n"\n```\n\n'
            '| 指标 | 数值 |\n| --- | --- |\n| 耗时 | 3.25 ms |\n\n'
            + f'校验值：`{ "a" * 64 }`\n'
        )
        self.en.write_text(self.en_text, encoding="utf-8")
        self.zh.write_text(self.zh_text, encoding="utf-8")
        (self.root / "docs/i18n.json").write_text(json.dumps({
            "version": 1,
            "pairs": [{"en": "docs/project/k1-demo.md", "zh": "docs/project/k1-demo_zh.md",
                       "shared_anchors": True}],
        }), encoding="utf-8")

    def errors(self):
        with redirect_stdout(io.StringIO()):
            return CHECKER.check(self.root)

    def assert_problem(self, expected):
        self.assertTrue(any(expected in error for error in self.errors()), expected)

    def test_valid_pair_and_explicit_language_switches(self):
        self.assertEqual([], self.errors())

    def test_cross_language_body_link_is_rejected(self):
        self.zh.write_text(self.zh_text + '\n[指南](k1-demo.md)\n', encoding="utf-8")
        self.assert_problem("Cross-language body link")

    def test_html_body_link_is_checked(self):
        self.zh.write_text(self.zh_text + '\n<a href="k1-demo.md">指南</a>\n', encoding="utf-8")
        self.assert_problem("Cross-language body link")

    def test_missing_file_is_rejected(self):
        self.en.write_text(self.en_text + '\n[Guide](missing.md)\n', encoding="utf-8")
        self.assert_problem("Missing local target")

    def test_missing_anchor_is_rejected(self):
        self.en.write_text(self.en_text + '\n[Usage](k1-demo.md#absent)\n', encoding="utf-8")
        self.assert_problem("Missing section anchor")

    def test_percent_encoded_and_compatibility_anchors_work(self):
        self.en.write_text(self.en_text + '\n[Usage](#%E4%BD%BF%E7%94%A8)\n', encoding="utf-8")
        self.zh.write_text(self.zh_text + '\n[使用](#usage)\n', encoding="utf-8")
        self.assertEqual([], self.errors())

    def test_upstream_reference_requires_an_explicit_english_label(self):
        (self.root / "docs/upstream.md").write_text('# Upstream\n', encoding="utf-8")
        self.zh.write_text(self.zh_text + '\n[设计](../upstream.md)\n', encoding="utf-8")
        self.assert_problem("Unlabelled upstream English reference")
        self.zh.write_text(self.zh_text + '\n[设计（英文）](../upstream.md)\n', encoding="utf-8")
        self.assertEqual([], self.errors())

    def test_missing_translation_is_rejected(self):
        self.zh.unlink()
        self.assert_problem("Missing paired document")

    def test_unregistered_k1_document_is_rejected(self):
        (self.en.parent / "k1-extra.md").write_text('# Extra\n', encoding="utf-8")
        self.assert_problem("K1 document has no language pair")

    def test_command_drift_is_rejected(self):
        self.zh.write_text(self.zh_text.replace('hello', 'different'), encoding="utf-8")
        self.assert_problem("Command/config blocks differ")

    def test_sha256_drift_is_rejected(self):
        self.zh.write_text(self.zh_text.replace('a' * 64, 'b' * 64), encoding="utf-8")
        self.assert_problem("SHA256 values differ")

    def test_measurement_table_drift_is_rejected(self):
        self.zh.write_text(self.zh_text.replace('3.25', '4.25'), encoding="utf-8")
        self.assert_problem("Measurement-table numbers differ")

    def test_duplicate_anchor_is_rejected(self):
        self.en.write_text(self.en_text + '\n<a id="usage"></a>\n', encoding="utf-8")
        self.assert_problem("Duplicate anchors")

    def test_fenced_example_links_and_headings_are_not_navigation(self):
        example = '\n```text\n# Not a heading\n[example](missing.md)\n```\n'
        self.en.write_text(self.en_text + example, encoding="utf-8")
        self.zh.write_text(self.zh_text + example, encoding="utf-8")
        self.assertEqual([], self.errors())

    def test_untranslated_english_page_prose_is_rejected(self):
        self.en.write_text(self.en_text + '\n尚未翻译的正文。\n', encoding="utf-8")
        self.assert_problem("Chinese prose in English page")

    def test_heading_slug_ignores_inline_markup_and_retains_double_hyphens(self):
        self.assertEqual('k1-sdk-measurements--2026-09-06', CHECKER.slug('K1 SDK measurements — 2026-09-06'))
        self.assertEqual('use-cargo', CHECKER.slug('Use `cargo`'))


if __name__ == "__main__":
    unittest.main()
