#!/bin/sh
# Native SDK regression on K1. Does not provision the board or enable any motors.
set -eu

cd "$(dirname "$0")/.."

if [ "$(uname -s)" != Linux ] || [ "$(uname -m)" != riscv64 ]; then
    echo "Run this on the K1, with Rust 1.89+ on PATH." >&2
    exit 1
fi

TARGET=riscv64gc-unknown-linux-gnu
JOBS="${K1_BUILD_JOBS:-2}"
TEST_THREADS="${K1_TEST_THREADS:-2}"

# Use headers matching the installed Bianbu runtime. Installing the newest -dev
# packages can otherwise upgrade the vendor's camera/encoder stack along with them.
pkg-config --exists libudev 'gstreamer-1.0 >= 1.22' gstreamer-app-1.0 \
    gstreamer-video-1.0 gstreamer-webrtc-1.0 || {
    echo "Missing development libraries; see docs/project/spacemit-k1.md." >&2
    exit 1
}

rustc -Vv
cargo test --locked --release --workspace --no-fail-fast --target "$TARGET" -j "$JOBS" \
    -- --test-threads="$TEST_THREADS"
cargo k1 --locked --bins -j "$JOBS"

# --help exercises the executable loader, not a serial port, radio, or camera.
BIN="${CARGO_TARGET_DIR:-target}/$TARGET/release"
for name in robotd robotctl updaterd configd btd padd mediad tofd camera-check; do
    # Some daemons publish identity before parsing --help; never replace a live
    # service's /run identity with the binary this script is checking.
    DUCK_RUNTIME_DIR="$BIN/k1-cli-runtime" timeout 15 "$BIN/$name" --help >/dev/null
    echo "$name: executable OK"
done

# Optional: validate real policies through the SDK's Rust API, after all compiler
# jobs have exited so their CPU load cannot become part of the inference result.
if [ "$#" -gt 0 ]; then
    # cargo test --workspace already builds this example. A separate cargo run -p
    # selects a narrower dependency feature set and recompiles it on the slow board.
    "$BIN/examples/policy-bench" "$@"
fi
