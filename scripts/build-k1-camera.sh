#!/bin/sh
# Build a private MPP/OpenCV bridge on K1. Never install system libraries/plugins.
set -eu

if [ "$(uname -m)" != riscv64 ]; then
    echo 'Run this native build on the K1 (riscv64).' >&2
    exit 1
fi
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo 'Usage: sh scripts/build-k1-camera.sh /path/to/spacemit-com/mpp [output-directory]' >&2
    exit 1
fi
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
mpp_source=$(CDPATH= cd -- "$1" && pwd)
revision=2b97ffe84c06071774301fad5542fbe76fa62774
jobs=${K1_BUILD_JOBS:-2}
case "$jobs" in ''|*[!0-9]*|0) echo 'K1_BUILD_JOBS must be a positive integer' >&2; exit 1;; esac
git -C "$mpp_source" cat-file -e "$revision^{commit}"
out=${2:-"$repo/target/k1-camera"}
# Resolve without creating it first. Refuse broad or system output directories.
out=$(realpath -m -- "$out")
case "$out" in
    "$repo/target/"*) ;;
    *) echo 'Choose a dedicated subdirectory of this SDK workspace target/.' >&2; exit 1;;
esac
mkdir -p -- "$out"
out=$(CDPATH= cd -- "$out" && pwd)
for part in mpp-source mpp-build bridge-build lib libmicroduck_k1_camera.so camera-native-check camera-image-test lib/libmpp.so.1.0.0 lib/libv4l2_linlonv5v7_codec2.so; do
    if [ -L "$out/$part" ]; then
        echo "Refusing redirected build output: $out/$part" >&2
        exit 1
    fi
done
if [ ! -d "$out/mpp-source" ]; then
    staging=$(mktemp -d "$out/mpp-source.XXXXXX")
    git -C "$mpp_source" archive "$revision" | tar -x -C "$staging"
    # Prevent git apply from treating this as a subdirectory of the SDK worktree
    # and silently ignoring paths outside that subdirectory's prefix.
    git -C "$staging" init --quiet
    # --recount also tolerates harmless line-number changes in the checked-in patch.
    (cd "$staging" && git apply --recount --check "$repo/native/k1-camera/mpp-isolation.patch")
    (cd "$staging" && git apply --recount "$repo/native/k1-camera/mpp-isolation.patch")
    (cd "$staging" && git apply --recount --reverse --check "$repo/native/k1-camera/mpp-isolation.patch")
    mv -- "$staging" "$out/mpp-source"
    printf '%s\n' "$revision" > "$out/mpp-revision"
    sha256sum "$repo/native/k1-camera/mpp-isolation.patch" > "$out/mpp-patch.sha256"
else
    test "$(cat "$out/mpp-revision")" = "$revision"
    sha256sum -c "$out/mpp-patch.sha256"
    (cd "$out/mpp-source" && git apply --recount --reverse --check "$repo/native/k1-camera/mpp-isolation.patch")
fi

# A separately checked patch can upgrade an existing private snapshot. It never
# touches the caller's upstream tree and fails if the expected context changed.
ownership_patch="$repo/native/k1-camera/mpp-uvc-ownership.patch"
if [ -f "$out/mpp-uvc-patch.sha256" ]; then
    sha256sum -c "$out/mpp-uvc-patch.sha256"
else
    git -C "$out/mpp-source" apply --recount --check "$ownership_patch"
    git -C "$out/mpp-source" apply --recount "$ownership_patch"
    sha256sum "$ownership_patch" > "$out/mpp-uvc-patch.sha256"
fi
git -C "$out/mpp-source" apply --recount --reverse --check "$ownership_patch"

# Native diagnostics must not split the SDK's stdout JSON records. Keep them,
# including errors, on stderr instead of mutating process-wide stdout FDs.
logging_patch="$repo/native/k1-camera/mpp-logging.patch"
if [ -f "$out/mpp-logging-patch.sha256" ]; then
    sha256sum -c "$out/mpp-logging-patch.sha256"
else
    # Narrow macro-only hunks; sys.c in the pinned upstream uses CRLF.
    git -C "$out/mpp-source" apply --recount --unidiff-zero --ignore-whitespace --check "$logging_patch"
    git -C "$out/mpp-source" apply --recount --unidiff-zero --ignore-whitespace "$logging_patch"
    sha256sum "$logging_patch" > "$out/mpp-logging-patch.sha256"
fi
git -C "$out/mpp-source" apply --recount --unidiff-zero --ignore-whitespace --reverse --check "$logging_patch"

cmake -S "$out/mpp-source" -B "$out/mpp-build" \
    -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTS=ON -DBUILD_ROS2_EXAMPLES=OFF \
    -DMICRODUCK_K1_CAMERA_ONLY=ON -DCMAKE_BUILD_RPATH_USE_ORIGIN=ON \
    -DCMAKE_SHARED_LINKER_FLAGS=-Wl,-Bsymbolic-functions
# Do not build ALL: upstream's unrelated ISP plugins have post-build installers.
cmake --build "$out/mpp-build" --target mpp v4l2_linlonv5v7_codec2 -j "$jobs"
mkdir -p "$out/lib"
cp -P "$out/mpp-build/lib/libmpp.so" "$out/mpp-build/lib/libmpp.so.1" \
    "$out/mpp-build/lib/libmpp.so.1.0.0" "$out/lib/"
cp "$out/mpp-build/al/vcodec/libv4l2_linlonv5v7_codec2.so" "$out/lib/"

if [ "${MICRODUCK_K1_CAMERA_DEPS_ONLY:-0}" = 1 ]; then
    echo "Private MPP libraries built in $out/lib"
    exit 0
fi
cmake -S "$repo/native/k1-camera" -B "$out/bridge-build" \
    -DCMAKE_BUILD_TYPE=Release -DMPP_SOURCE_DIR="$out/mpp-source" \
    -DMPP_LIBRARY_DIR="$out/lib" -DCAMERA_OUTPUT_DIR="$out" \
    -DOpenCV_DIR=/opt/opencv-spacemit/lib/cmake/opencv4
cmake --build "$out/bridge-build" -j "$jobs"
echo "Camera bridge: $out/libmicroduck_k1_camera.so"
echo "Select camera.acceleration = 'spacemit' and set MICRODUCK_K1_CAMERA_LIB to that absolute path."
