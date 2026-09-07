# SpaceMIT K1 / RISC-V: build and install for development

This guide is for **muggle-stack/microduck, branch `spacemit-k1`**, running natively on
K1/Bianbu. It installs developer binaries into a new private directory, not a complete robot
image. It does not provision UART/CSI, install systemd units, enable motors, change networking,
or configure signed releases/OTA. Those inherited workflows still target Radxa/aarch64.

Validated environment: Bianbu **2.1.1**, Rust **1.89.0**, native GStreamer **1.24.2**,
`spacemit-onnxruntime` **2.0.6-bpo1+1** (ORT 1.24.2+spacemit.a1, EP 2.0.6).
The optional camera bridge uses `opencv-spacemit` **4.14.0-1bb3**. Other images/versions need
their own checks; a successful build does not validate a different board's device tree.

Run the following commands **on the K1**, in the same Bash session unless stated otherwise.
Root may omit `sudo`. Stop on any failed step. Do not run `provision-board.sh`, `setup-board.sh`,
`setup-gstreamer.sh`, `install.sh`, release hooks or `dev-push.sh` on the K1.

## 1. Get this fork

For a new checkout only:

```sh
mkdir -p "$HOME/workspace"
git clone --branch spacemit-k1 https://github.com/muggle-stack/microduck.git \
  "$HOME/workspace/microduck"
cd "$HOME/workspace/microduck" || exit 1
```

For an existing checkout, enter it and inspect `git status --short --branch` first; do not
clone over it or replace local work. This guide assumes a clean, reviewed revision of
`spacemit-k1`. `uname -m` must report `riscv64`; the Rust target is the longer
**`riscv64gc-unknown-linux-gnu`**. Running `cargo k1` on a Mac does not provide a Linux
sysroot, linker or the board's GStreamer libraries.

## 2. Check Bianbu dependencies before installing

Use the board's configured Bianbu apt repositories. First simulate the transaction:

```sh
apt-get -s install build-essential binutils pkg-config git cmake curl ca-certificates \
  xz-utils file libudev-dev libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev \
  gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  spacemit-onnxruntime
```

Only after reviewing the plan, run the same command without `-s`, with `sudo` if needed.
**Do not blindly accept multimedia upgrades.** On the tested image, newer `-dev` candidates
would also replace vendor runtime packages; matching development versions were installed
instead, with no runtime upgrades. The [dependency version record](../project/spacemit-k1.md#build-on-the-k1)
lists those versions. An absent apt candidate is not a reason to copy Radxa libraries onto K1.

Confirm the native headers and runtime:

```sh
pkg-config --modversion libudev gstreamer-1.0 gstreamer-app-1.0 \
  gstreamer-video-1.0 gstreamer-webrtc-1.0
readlink -f /usr/lib/libonnxruntime.so
readlink -f /usr/lib/libspacemit_ep.so
(
  set -eu
  for element in videotestsrc jpegdec videoconvert videocrop videoscale videorate appsink; do
    gst-inspect-1.0 "$element" >/dev/null
  done
)
```

The `gstreamer-webrtc-1.0` **development library is not the `webrtcsink` runtime plugin**.
The SDK can compile and `camera-check` can run without that plugin; browser streaming still
needs separate integration. The Rust SDK uses native `/usr/lib` ORT/EP, not Python's bundled
runtime. `python3-spacemit-ort` is optional for Python experiments, not a Rust runtime requirement.

## 3. Install Rust 1.89 without replacing apt Rust

If `/opt/microduck-rust-1.89.0/bin/rustc -V` already reports 1.89.0, skip the installation
block and select it on `PATH` below. Otherwise use the official standalone distribution in
a fresh staging directory. This pinned checksum is from the
[Rust distribution checksum](https://static.rust-lang.org/dist/rust-1.89.0-riscv64gc-unknown-linux-gnu.tar.xz.sha256).

```sh
(
  set -eu
  if [ -e /opt/microduck-rust-1.89.0 ] || [ -L /opt/microduck-rust-1.89.0 ]; then
    echo 'Existing Rust prefix: inspect it rather than overwriting it.' >&2
    exit 1
  fi
  K1_RUST_STAGE=$(mktemp -d)
  cd "$K1_RUST_STAGE"
  K1_RUST_DIST=rust-1.89.0-riscv64gc-unknown-linux-gnu
  curl --fail --location --connect-timeout 15 --max-time 600 \
    "https://static.rust-lang.org/dist/$K1_RUST_DIST.tar.xz" -o "$K1_RUST_DIST.tar.xz"
  printf '%s  %s\n' \
    4ded289e6a43e4e2bef660c74c8d833e00d87a9e30ad2c376468f41429a12614 \
    "$K1_RUST_DIST.tar.xz" | sha256sum --check -
  tar -xJf "$K1_RUST_DIST.tar.xz"
  sudo sh "$K1_RUST_DIST/install.sh" --prefix=/opt/microduck-rust-1.89.0 \
    --components=rustc,cargo,rust-std-riscv64gc-unknown-linux-gnu --disable-ldconfig
  printf 'Retained Rust download/extraction at %s\n' "$K1_RUST_STAGE"
)
```

This installs only the compiler, Cargo and native standard library. It does not change
`/usr/bin/rustc`, apt packages, rustup defaults or shell startup files. Rustfmt/Clippy are
not included in this minimal board installation; the host can run those checks separately.

```sh
export PATH="/opt/microduck-rust-1.89.0/bin:$PATH"
rustc -Vv
cargo -V
/usr/bin/rustc -V
```

The tested board reports 1.89.0 from the selected compiler and still 1.75.0 from apt's
`/usr/bin/rustc`. Keeping both is intentional.

## 4. Build and run software regression

From the repository root:

```sh
export CARGO_TARGET_DIR="$PWD/target"
export ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so
cargo k1 --locked --bins -j 2
K1_BIN="$CARGO_TARGET_DIR/riscv64gc-unknown-linux-gnu/release"
file "$K1_BIN/robotd"
```

Expect an ELF64 RISC-V/LP64D executable, not aarch64. Build **all** binaries after changing
the shared config schema: rebuilding only `mediad` leaves older `robotctl`, `robotd` and
`padd` unable to read new camera keys. `cargo board` intentionally remains the Radxa alias.

```sh
sh scripts/k1-test.sh
```

The script runs release-mode workspace tests, builds binaries and checks nine executable
loaders using `--help` and an isolated runtime directory. Tests use fake robot IO; no motors,
microphone, camera or system services are started. Cold native builds can take over an hour;
the default two build jobs bound memory use on the 4 GB board. With Cargo dependencies already
cached, `CARGO_NET_OFFLINE=true sh scripts/k1-test.sh` also works.

Real policy inference is an optional separate check. Use a **new private policy root** when
running `scripts/seed-policies.sh ROOT`, never its production default merely for a test.
Its downloads are best-effort: check that `ROOT/current` and the expected ONNX files actually
exist before passing that directory to `sh scripts/k1-test.sh ROOT/current 200`.
[The policy validation record](../project/spacemit-k1.md#validate-real-policy-inference)
explains the models, network failure cases and what the benchmark does not prove.

## 5. Install developer binaries into a new private prefix

This is a **manual development installation**, not an OTA release. The following creates a
new revision-labelled directory; it does not replace a previous installation or running service.
Keep `K1_SDK_PREFIX` for the later optional steps.

```sh
K1_INSTALL_BASE="$HOME/.local/opt/microduck-k1"
mkdir -p "$K1_INSTALL_BASE"
K1_SDK_PREFIX=$(mktemp -d "$K1_INSTALL_BASE/$(git rev-parse --short=12 HEAD).XXXXXX")
K1_PROGRAMS='robotd robotctl updaterd configd btd padd mediad tofd camera-check duck-bench sounds pet-detect'
(
  set -eu
  : "${K1_SDK_PREFIX:?Creating the private prefix must succeed first}"
  : "${K1_BIN:?Complete the native build step first}"
  test -d "$K1_SDK_PREFIX"
  install -d "$K1_SDK_PREFIX/bin" "$K1_SDK_PREFIX/config"
  for name in $K1_PROGRAMS; do
    install -m 755 "$K1_BIN/$name" "$K1_SDK_PREFIX/bin/$name"
  done
  install -m 644 deploy/k1/camera-usb-mono.toml deploy/k1/camera-usb-decxin-sbs.toml \
    deploy/k1/camera-usb-decxin-mpp.toml deploy/k1/camera-imx219.toml \
    deploy/k1/imx219-csi3-720p.json deploy/k1/robotd-audio.toml "$K1_SDK_PREFIX/config/"
  git rev-parse HEAD > "$K1_SDK_PREFIX/REVISION"
)
printf 'Developer SDK: %s\n' "$K1_SDK_PREFIX"
```

No `/opt/robot`, `/etc/robot`, `/usr/bin`, systemd unit, default config or `current` symlink is
changed. Config files here are **separate examples**, not an automatically merged robot profile.
Models, voice banks, firmware and system libraries are not bundled by this copy step.

Check the installed executables without contacting a running robot:

```sh
K1_HELP_RUNTIME=$(mktemp -d "$K1_SDK_PREFIX/cli-runtime.XXXXXX")
(
  set -eu
  : "${K1_HELP_RUNTIME:?Creating the isolated runtime directory must succeed first}"
  for name in $K1_PROGRAMS; do
    DUCK_RUNTIME_DIR="$K1_HELP_RUNTIME" timeout 15 "$K1_SDK_PREFIX/bin/$name" --help >/dev/null
    printf '%s: executable OK\n' "$name"
  done
)
```

The runtime override matters even for `--help`: some daemons publish identity before parsing
arguments. Use explicit binary paths for checks; a plain `robotctl` may be an older installed
copy. If desired, `export PATH="$K1_SDK_PREFIX/bin:$PATH"` selects this install in the current
shell only. That does **not** change the daemons behind existing sockets.

## 6. Optional USB camera and private MPP bundle

First identify the actual capture device and advertised mode with `v4l2-ctl --list-devices`
and `v4l2-ctl -d DEVICE --list-formats-ext` (`v4l-utils`). Review the copied camera profile;
do not use the DECXIN preset for an unrelated camera. Its second `/dev/video*` node is metadata,
not necessarily a second eye.

The current use is **one selected eye**: a physical stereo camera can supply a packed frame,
and the SDK crops `camera.view = "left"` or `"right"`. Do not pass `--both-eyes` for this use.
No calibration, stereo depth or simultaneous dual-eye inference is required.

The portable USB path needs no MPP/OpenCV bridge. For the opt-in accelerated path, review an
apt simulation for `opencv-spacemit`, then install a matching vendor version if absent. A C++17
compiler, CMake, working VDEC/V2D/DMA-heap devices and appropriate device permissions are needed.
Do not grant blanket `chmod 777` access. Use an existing SpaceMIT SDK MPP checkout, or clone
[SpaceMIT MPP](https://github.com/spacemit-com/mpp) into a new directory. The source must contain
commit `2b97ffe84c06071774301fad5542fbe76fa62774`.

```sh
# Substitute the actual MPP checkout path if different.
sh scripts/build-k1-camera.sh "$HOME/workspace/spacemit-sdk/components/multimedia/mpp"
(
  set -eu
  : "${K1_SDK_PREFIX:?Complete the private installation step first}"
  test -d "$K1_SDK_PREFIX"
  install -d "$K1_SDK_PREFIX/lib/k1-camera/lib"
  install -m 755 target/k1-camera/libmicroduck_k1_camera.so \
    target/k1-camera/camera-native-check target/k1-camera/camera-image-test \
    "$K1_SDK_PREFIX/lib/k1-camera/"
  cp -a target/k1-camera/lib/. "$K1_SDK_PREFIX/lib/k1-camera/lib/"
)
export MICRODUCK_K1_CAMERA_LIB="$K1_SDK_PREFIX/lib/k1-camera/libmicroduck_k1_camera.so"
ldd "$MICRODUCK_K1_CAMERA_LIB"
```

There must be no `not found` entries. The relative `lib/` layout is mandatory: the bridge
finds private MPP there, and that MPP finds its sibling codec2 plugin. Keep the system
OpenCV installation at `/opt/opencv-spacemit`; do not install private MPP into `/usr/lib`,
set a global `LD_LIBRARY_PATH`, run MPP's `all` target or run `cmake --install`.
The [camera backend report](../project/k1-mpp-camera.md) owns the build details and limits.

On a board with working V2D, synthetic image checks do not open the camera:

```sh
timeout 30 "$K1_SDK_PREFIX/lib/k1-camera/camera-image-test"
```

Then, **only after confirming the camera is available and the profile matches it**, open the
selected eye without motors, audio, encoder or WebRTC:

```sh
timeout 60 "$K1_SDK_PREFIX/bin/camera-check" \
  --config "$K1_SDK_PREFIX/config/camera-usb-decxin-mpp.toml" --warmup 5 --frames 30
```

For software capture use the reviewed `camera-usb-decxin-sbs.toml` or `camera-usb-mono.toml`
instead; the bridge is then not loaded. MPP pixel output is not bit-identical to the software
decoder/scaler, which is one reason acceleration stays explicit rather than default.

### Optional IMX219 / K1 CSI input

For a MUSE-Pi-Pro with an IMX219 on vendor CSI3 (`sensor_id=2`), the opt-in
`camera.backend = "spacemit_csi"` uses the installed `spacemitsrc` and a vendor ISP JSON.
It does not use the USB MPP/OpenCV bridge or Rockchip sensor controls. Before opening the camera,
review `camera-imx219.toml` and `imx219-csi3-720p.json`; adjust the absolute `camera.isp_config`
and model paths, especially when using the private installation above.
The [IMX219 guide](../project/k1-imx219.md) contains the tested board/runtime, bounded capture
and ORT/EP commands, model requirements and remaining image-quality/streaming limits.
This is not automatic support for other CSI ports or unverified sensor modules.

## 7. Optional detector and ES8326

- **Vision:** supply the verified floating-point opset 17 `duck_detect.slim.onnx` and the
  explicit `[detect]` SpaceMIT settings in the [ORT/EP guide](../project/k1-duck-ort-ep.md).
  That guide includes the model SHA-256. The EP-ready model is an experiment artifact,
  **not yet a published K1 release asset or generated by this installation**. The original
  opset 12 ONNX, `.rknn` file and a generic YOLO11 COCO model are not substitutes.
  No Python worker or Rust-specific duplicate preprocessing is required.
- Merge the detection settings into a **copy of the chosen camera config**, without
  duplicating TOML sections. For a hard four-CPU budget use the
  [headless detection command](../project/k1-usb-camera.md#configuration-and-headless-check):
  cgroup `AllowedCPUs=0-2,4`, three EP workers `0;1;2`, caller on CPU 4,
  `--detect --hz 2`. Keep FP16 epilogues off for the validated floating-point model.
  `taskset` alone does not enforce this runtime's process-wide CPU budget. Pass both native
  library environment variables into the process when using `sudo` or a service scope.
- **Audio:** `sudo sh scripts/setup-k1-audio.sh` installs the opt-in ES8326 ALSA profile.
  Unlike the private binary copy, this explicitly writes `/etc/alsa/conf.d` and creates an
  audio-only `/etc/robot/robotd.toml` if absent; an existing config is preserved. It does not
  change mixer settings or start services. Read [ES8326 setup and duplex validation](../project/spacemit-k1.md#es8326-audio)
  before running the hardware test, which opens the microphone. Install `alsa-utils` if needed.

## 8. Upstream alignment and completion boundary

On **2026-09-06**, upstream `pollen-robotics/microduck:main` was still `bc41fb5`, already
the base of this branch. This guide did not require a rebase. To check later, fetch `upstream`
and inspect its changes; only rebase a clean worktree, coordinate shared branches before
rewriting history, and repeat K1 regression afterward. Do not force-push just to align names.

The port retains the original motion-control core, RKNN implementation and Radxa defaults,
but also changes shared integration code. In particular, invalid explicit media configuration
now fails instead of silently selecting Rockchip defaults, including on ARM. See the
[USB configuration contract](../project/k1-usb-camera.md#implemented-contract); this is not
a claim of zero behavioural changes or a completed RK3566 hardware regression.

Native build/tests, policy inference, ES8326 and selected-eye USB/EP checks are recorded in
[the K1 bring-up report](../project/spacemit-k1.md). Real HAT/UART/servo/IMU feedback,
IMX219 image-quality/long-run acceptance, ToF, controller/gamepad bring-up, complete H.264/WebRTC, combined-load and
labelled detection acceptance, and RISC-V provisioning/signed releases/OTA remain separate
work. Installing these binaries does not make `robotctl health` a hardware acceptance test.
