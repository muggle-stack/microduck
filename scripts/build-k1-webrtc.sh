#!/bin/sh
# Private rswebrtc plugins only; never replace the vendor GStreamer installation.
set -eu

if [ "$(uname -m)" != riscv64 ]; then
    echo 'Run this native build on the K1 (riscv64).' >&2
    exit 1
fi
repo=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
source_dir="$repo/target/k1-gst-plugins-rs"
work="$repo/target/k1-webrtc"
patch="$repo/native/k1-webrtc/rswebrtc-k1.patch"
revision=6302bea23b53e6461c104f9543df887a63b2d6ec
jobs=${K1_BUILD_JOBS:-4}
case "$jobs" in ''|*[!0-9]*|0) echo 'K1_BUILD_JOBS must be a positive integer' >&2; exit 1;; esac

for command in git cargo rustc pkg-config gst-inspect-1.0 sha256sum; do
    command -v "$command" >/dev/null || { echo "Missing $command" >&2; exit 1; }
done
rustc -vV
rustc -vV | grep -qx 'host: riscv64gc-unknown-linux-gnu' || {
    echo 'Use the native RISC-V toolchain; gst-plugins-rs 0.15.3 needs Rust 1.92+.' >&2
    exit 1
}
rustc --version | awk '{ split($2, v, "."); exit !(v[1] > 1 || (v[1] == 1 && v[2] >= 92)) }' || {
    echo 'Select Rust 1.92+ for this plugin build; keep SDK builds on their existing toolchain.' >&2
    exit 1
}
pkg-config --atleast-version=1.22 gstreamer-1.0
pkg-config --exists gstreamer-app-1.0 gstreamer-video-1.0 gstreamer-webrtc-1.0 gstreamer-sdp-1.0
# Missing ICE transport otherwise appears much later as a failed peer connection.
for element in nice webrtcbin dtlssrtpenc sctpenc rtph264pay spacemitsrc spacemith264enc; do
    gst-inspect-1.0 "$element" >/dev/null
done

for path in "$repo/target" "$source_dir" "$work" "$work/build" "$work/plugins" "$work/registry.bin" \
    "$work/plugins/libgstrswebrtc.so" "$work/plugins/libgstrsrtp.so"; do
    if [ -L "$path" ]; then
        echo "Refusing redirected build output: $path" >&2
        exit 1
    fi
done
mkdir -p "$repo/target" "$work"
if [ ! -e "$source_dir" ]; then
    git clone --depth 1 --branch 0.15.3 \
        https://github.com/GStreamer/gst-plugins-rs.git "$source_dir"
fi
test "$(git -C "$source_dir" rev-parse HEAD)" = "$revision" || {
    echo "Source must be gst-plugins-rs 0.15.3 at $revision; refusing to reset it." >&2
    exit 1
}
if git -C "$source_dir" diff --quiet HEAD; then
    git -C "$source_dir" apply --check "$patch"
    git -C "$source_dir" apply "$patch"
fi
git -C "$source_dir" apply --reverse --check "$patch"
# Do not silently reuse a source tree with additional, unreproducible edits.
actual=$(git -C "$source_dir" diff --binary HEAD | sha256sum | cut -d ' ' -f 1)
expected=$(sha256sum "$patch" | cut -d ' ' -f 1)
test "$actual" = "$expected" || {
    echo "Source has edits beyond $patch; preserve them and use a fresh private source directory." >&2
    exit 1
}

export CARGO_TARGET_DIR="$work/build"
(cd "$source_dir" && cargo build --locked --release --no-default-features --lib \
    -p gst-plugin-webrtc -p gst-plugin-rtp -j "$jobs")
mkdir -p "$work/plugins"
# Replace inodes, never truncate a library an already-running process has mapped.
staging=$(mktemp -d "$work/plugin-stage.XXXXXX")
cp "$work/build/release/libgstrswebrtc.so" "$work/build/release/libgstrsrtp.so" "$staging/"
mv "$staging/libgstrswebrtc.so" "$work/plugins/libgstrswebrtc.so"
mv "$staging/libgstrsrtp.so" "$work/plugins/libgstrsrtp.so"
rmdir "$staging"
sha256sum "$work/plugins/libgstrswebrtc.so" "$work/plugins/libgstrsrtp.so"
GST_PLUGIN_PATH="$work/plugins${GST_PLUGIN_PATH:+:$GST_PLUGIN_PATH}" \
    GST_REGISTRY="$work/registry.bin" \
    gst-inspect-1.0 webrtcsink
echo "Private plugins: $work/plugins"
echo 'Set GST_PLUGIN_PATH to this directory only when running the K1 mediad.'
