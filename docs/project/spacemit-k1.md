<a id="spacemit-k1原生-sdk-回归记录"></a>
# SpaceMIT K1: native SDK regression

[English](spacemit-k1.md) · [简体中文](spacemit-k1_zh.md)

> This is dated experiment/failure evidence, not a list of entirely current blockers. See [current status](spacemit-k1-adaptation.md) and subsequent [IMX219](k1-imx219.md) / [WebRTC](k1-webrtc.md) integration. Translation does not change the original test conditions or imply new acceptance runs.

Historical development branch: [muggle-stack/microduck:spacemit-k1](https://github.com/muggle-stack/microduck/tree/spacemit-k1).
Based on upstream `bc41fb5` (2026-09-05 checkout). This is a native-build and software-regression
path, not a claim that the Radxa HAT, camera, radio, or installation image is interchangeable.

For a fresh checkout, start with [native build and development installation](../robot/install-k1.md).
That page owns the install procedure; this page keeps the dated measurements and bring-up record.
Upstream `main` was checked again on 2026-09-06 and remained `bc41fb5`; no additional rebase
was needed for the installation guide.

<a id="在-k1-构建"></a>
## Build on the K1

The Rust target is **`riscv64gc-unknown-linux-gnu`**. Bianbu reports `riscv64` from `uname -m`;
that shorter string is not the Rust target triple. The board's own C compiler and installed
GStreamer development files provide the native sysroot.

```sh
cd /root/workspace/microduck
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
cargo k1 --locked --bins -j 2
```

The isolated Rust installation does not replace `/usr/bin/rustc` or the apt packages.
`cargo board` still cross-compiles for aarch64. `cargo k1` is a **native K1** command; running it
on a Mac does not supply a RISC-V Linux linker or the board's GStreamer libraries.

A cold full-workspace release/test build on the K1 takes over an hour. For quick feedback
during an edit, check or test only the affected crate without release optimisation:

```sh
cargo check --locked --target riscv64gc-unknown-linux-gnu -p duck-control
cargo test --locked --target riscv64gc-unknown-linux-gnu -p duck-control
```

Linux build dependencies are `libudev-dev`, `libgstreamer1.0-dev`,
`libgstreamer-plugins-base1.0-dev`, and `libgstreamer-plugins-bad1.0-dev`.
Check an apt dry run before installing: the default candidates on the tested Bianbu 2.1.1
image would also upgrade its multimedia and Wayland runtime. Matching development packages
were used instead, leaving every existing runtime package at its installed version:

| Development package | Version used |
| --- | --- |
| libudev-dev | 255.4-1ubuntu8bb2 |
| libgstreamer1.0-dev | 1.24.2-1bb3 |
| libgstreamer-plugins-base1.0-dev | 1.24.2-1ubuntu0.1bb2 |
| libgstreamer-plugins-bad1.0-dev | 1.24.2-1ubuntu4bb10 |
| libwayland-dev / libwayland-bin | 1.22.0-2.1build1 |

The matching `libgstreamer-plugins-bad1.0-dev` and `libgstreamer-opencv1.0-0` packages remain
in the official Bianbu archive's `pool/universe/g/gst-plugins-bad1.0/`, even though apt's current
index only advertises the newer `bb18` vendor revision. Installing the matching dependencies
added 17 packages, upgraded none, and removed none.

<a id="开发安装指南复验2026-09-06"></a>
### Development installation guide check — 2026-09-06

The [development installation](../robot/install-k1.md) was exercised with the native SDK at
`cc7c471`, using a new prefix under `target/k1-install-doc-20260906.CpdgHg/installed/`
instead of a user's persistent installation directory. The cached full `--bins` release
build passed in 2.63 s; all **12** installed daemon/diagnostic executables passed `--help`
with an isolated identity directory, and every installed executable compared byte-identical
to its build output (`cmp` exit 0).

The pinned private MPP build also passed again. After copying its bundle to the new prefix,
`ldd` resolved MPP inside that prefix and OpenCV at `/opt/opencv-spacemit`, with no missing
libraries. The relocated image test passed **20 byte-exact synthetic cases and 13 invalid-input
checks**. It uses V2D but opens no camera. The apt simulation still proposed five multimedia
upgrades and two new packages; that transaction was not applied. This check did not reinstall Rust,
upgrade apt packages, open the microphone/camera, provision a robot, or restart services.
The official Rust archive checksum and installed 1.89.0/apt 1.75.0 coexistence were checked separately; a fresh-image
installation and the full workspace test suite were not rerun for this documentation-only change.
Logs are retained in the directory above (`verification.log`, `apt-simulation.log`, and
`relocated-library-paths.log`).

<a id="重复软件检查"></a>
## Repeat the software checks

```sh
sh scripts/k1-test.sh
```

This runs the workspace's release-mode tests on the RISC-V target, builds the board binaries,
and runs the seven daemons, `robotctl`, and `camera-check` with `--help` to check executable loading.
The existing robotd integration tests use `--fake`, exercising IPC and the update health gate
without a motor bus. The script does not install systemd services, run provisioning scripts,
change networking, or enable motors.

The build and test concurrency default to two to bound memory use and scheduling contention
on the 4 GB board. Override with `K1_BUILD_JOBS` and `K1_TEST_THREADS`. If dependencies have
already been fetched, `CARGO_NET_OFFLINE=true` also works; the crate sources/cache can be copied
from a development machine, but compilation and execution still happen on the K1.

<a id="安装真实运行时后暴露的测试修正"></a>
### Runtime-present test fixes

The first K1 run recorded **1,245 passed, two failed, six ignored**. Both failures were test
fixtures exposed by the installed ONNX Runtime, not RISC-V compilation failures:

- `setting_a_skill_keeps_what_the_call_left_out` used a text file as its ONNX model. That
  reaches the intended merge logic when the runtime is absent, but real model validation
  correctly rejects it when the runtime is installed. It now uses a tiny valid Gather graph.
- `an_unloadable_policy_holds_the_pose_and_reports_why` used a nonexistent override. Startup
  validation discards that override and loads the board's default instead, so the result
  depended on installed policy files. It now uses a shape-valid graph that fails at warm-up
  inference, exercising the failure/hold/health contract regardless of installed defaults.

Both fixtures are embedded protobuf bytes; they need no download or Python dependency. The
production validation, override fallback and failure-handling logic are unchanged. The original
negative result remains in `target/k1-regression.log`; the rerun uses
`target/k1-regression-fixed.log` rather than overwriting it.

After the fixes, all 52 suites passed: **1,247 tests passed, zero failed, six ignored** on
K1 / Rust 1.89 in release mode. The corresponding macOS / Rust 1.93 host run passed
**1,215 tests, zero failed, six ignored**. The different totals include Linux-only code and
tests; the host result alone does not validate the Linux daemons. The six ignored tests are
the existing two kinematics timing probes and four robotctl timing/visual probes, not newly
excluded failures.

The host's Rust 1.93 Clippy also found a duplicated `#[cfg(test)]` on the chorale tests in the
upstream checkout. Removing the redundant attribute makes `robotd --tests` pass Clippy with
`-D warnings`; it changes neither the production binary nor which tests are enabled.

The separate production `cargo k1 --locked --bins -j 4` build also passed, as did all eight
executable-loader checks. Its first build of the non-test dependency feature set took
30m26s; the earlier full test build reported 88m00s. The final cached regression's build phases
took 2.99s (tests) and 2.59s (production binaries); test execution is additional to those
build times. The complete revised script, including policy inference below, exited zero.

<a id="验证真实策略推理"></a>
## Validate real policy inference

The latest upstream checkout downloads its policies from
[`pollen-robotics/microduck-policies`](https://huggingface.co/pollen-robotics/microduck-policies),
rather than carrying them in Git. `scripts/seed-policies.sh` can populate a separate test root;
pass that root explicitly to avoid touching an installed robot's policy set.

On this network, the K1's Hub TLS connection timed out, and the seeding script's eight-second
per-file deadline also expired for one download on the Mac. The nine v1 files were downloaded
on the Mac with a longer deadline and copied to `target/k1-policy-files/` on the K1. All nine
were byte-identical (`cmp` exit 0) to the policies in the previous `2c61dcc` checkout. The
seeding script's production update-hook timeout was not changed.

```sh
sh scripts/k1-test.sh target/k1-policy-files 200
# Or only the model check, after building:
target/riscv64gc-unknown-linux-gnu/release/examples/policy-bench target/k1-policy-files 200
```

The regression script reuses the example built by `cargo test --workspace`. Running
`cargo run -p duck-control --example policy-bench` separately selects a narrower dependency
feature set and triggers another native build; no extra build is needed after the regression.
For a fresh checkout where only this example is wanted, build it with
`cargo k1 --locked -p duck-control --example policy-bench -j 2` first.

`policy-bench` uses the SDK's `Policy::load` and `Policy::infer`, including its shape validation
and single-threaded CPU session configuration. Each model receives a fixed upright home-pose
observation. It checks that actions are finite, warms up 20 times after loading, then reports
mean/P50/P95/P99/max over the requested iteration count. It never opens a serial port or writes
actions to hardware. Timings exclude model loading and do not measure gait quality, UART
latency, or the complete 50 Hz control loop.

This is a runtime measurement, not a claimed speedup. No model quantisation or weight changes
were made. Finite actions are checked; output bit-equivalence with Radxa or a previous runtime
has not been tested.

<a id="k1-结果2026-09-05"></a>
### K1 results, 2026-09-05

Bianbu 2.1.1, 4 GB board, eight CPUs available, performance governor at 1.6 GHz, no CPU
affinity pinning. The SDK uses CPU execution with one intra-op thread; no SpaceMIT EP was
added. Its loaded library was `/usr/lib/libonnxruntime.so.1.24.2+spacemit.a1`, not the
separate Python runtime. Compiler processes had exited before measuring. Each row is one
model loaded independently, with 20 warm-ups and 200 measured inferences; units are ms.

| Model | Mean | P95 | P99 | Max |
| --- | ---: | ---: | ---: | ---: |
| alpha_ground_pick | 0.9323 | 1.0288 | 1.0890 | 1.1042 |
| alpha_sitstand | 0.9355 | 1.0345 | 1.1022 | 3.2334 |
| alpha_stand | 0.8817 | 0.9637 | 0.9888 | 1.1972 |
| alpha_walking | 0.9055 | 0.9781 | 0.9912 | 0.9916 |
| ball_kick_left | 0.8965 | 0.9765 | 0.9937 | 0.9962 |
| ball_kick_right | 0.9033 | 0.9758 | 0.9876 | 0.9942 |
| roller | 0.8889 | 0.9660 | 0.9792 | 0.9838 |
| roller_crouch | 0.8991 | 0.9758 | 0.9883 | 1.0583 |
| roulade | 0.8764 | 0.9564 | 0.9668 | 0.9690 |

All actions were finite. The 3.2334 ms sit/stand outlier is retained, not discarded. Even
that sample is below a 20 ms tick, but this benchmark excludes real bus and peripheral work.
The complete CSV, including P50, is at the end of `target/k1-regression-fixed.log`.

<a id="原生启动检查"></a>
## Native startup check

The generated `robotd` was identified as an ELF64 RISC-V executable with the LP64D ABI and
ran `--version` on the K1, reporting `0.10.0`. A separate `robotd --fake` process then loaded
all seven walking-mode policy slots through the real ONNX Runtime and answered `robot.health`
with `healthy: true`; `robot.policies` reported no slot errors.

That functional check used an isolated socket/runtime directory, disabled audio and theremin,
and set the serial path to a deliberately nonexistent device in addition to `--fake`.
It ran while compilation was still active and was stopped cleanly afterward. It proves native
startup, policy loading and IPC, **not** sustained loop timing or operation with real motors.
The throwaway fixture and log are `target/k1-fake.toml` and `target/k1-fake.log` on the K1.

After the full regression, a second isolated `--fake` process was enabled through IPC and
given `vx = 0.2 m/s` commands approximately every 20 ms. After a three-second enable warm-up,
the 30.008-second measurement completed **1,500 ticks (49.986 Hz), zero missed deadlines**.
Five health samples reported 49.978–50.041 Hz; every sample and the final verdict were healthy.
The final `robot.state` frame reported `policy: "walk"`, the requested forward command was
applied, and all joint/target values were finite. `/proc/<pid>/maps` confirmed the runtime
library named above. The process was then disabled and stopped cleanly.

This remains **fake IO on a real K1**, not a servo-bus, IMU, or gait-quality test. No media
or other compute workload ran concurrently. The process log is `target/k1-loop.log` and its
captured IPC output is `target/k1-loop-result.log`; the fixture and logs are ignored artifacts,
not installed robot configuration or committed models.

<a id="es8326-音频"></a>
## ES8326 audio

The K1's existing ES8326 driver and mixer are used as-is. The user confirmed real
48 kHz / S16_LE / stereo recording and playback with `hw:1,0`. The SDK profile uses
the stable ALSA card ID **`sndes8326`**, not a card number that can change after boot.
This is an opt-in board profile; the original Radxa/AIC3104 default is unchanged.

```sh
# K1, as root. No apt install, kernel/DT change, mixer write or service restart.
sh scripts/setup-k1-audio.sh
```

This installs `deploy/audio/es8326.conf` as
`/etc/alsa/conf.d/99-microduck-es8326.conf`, without changing `pcm.default`, PipeWire,
`/etc/asound.conf` or the user's `.asoundrc`. On a board without a robot config it also
creates `/etc/robot/robotd.toml` from `deploy/k1/robotd-audio.toml`. An existing robot
config is **never overwritten**: merge this into its existing `[audio]` section:

```toml
[audio]
enabled = true
device = "microduck_es8326"
```

This is only an audio profile, not K1 UART/HAT provisioning. Do **not** run the
Radxa `setup-board.sh` audio section: its AIC3X DKMS driver, Rockchip kernel and
device-tree overlays are unrelated to the K1's already-working codec. The setup
helper preserves a locally modified ES8326 profile too, and asks for a manual merge.
It does not generate a voice bank or turn on microphone monitoring. A provisioned
SDK uses `sounds ensure-bank`; a development bank can live anywhere selected by
`audio.bank`. `audio.pet_detect = true` and a valid `audio.pet_model` explicitly
enable the existing microphone worker; its default remains off.

<a id="为什么需要-pcm-profile"></a>
### Why a PCM profile is needed

The SDK plays 48 kHz mono S16_LE and captures 16 kHz mono S16_LE. Direct `plughw`
calls work separately but **fail in either full-duplex startup order** on this
board: the second stream cannot install its different hardware rate. Fixing both
hardware streams at 48 kHz stereo avoids that clock conflict. ALSA's
[`plug` conversion and `mmap_emul` plugins](https://www.alsa-project.org/alsa-doc/alsa-lib/pcm_plugins.html)
provide the SDK formats without changing its DSP, models or subprocess commands.

The explicit `mmap_emul` layer matters: this Bianbu PCM advertises only
`RW_INTERLEAVED`, and a bare `plug` with fixed rate/channels also failed hw_params.
The tested hardware uses a 1,024-frame period and 4,096-frame buffer at 48 kHz
(21.33 / 85.33 ms), including when the live synth requests 10 / 40 ms. Those are
ALSA buffer settings, **not measured end-to-end audio latency**.

Mono playback is duplicated to the two outputs. Stereo capture is averaged and
resampled to the model's 16 kHz mono input; it is not bit-identical to a raw stereo
capture. No model weights, quantisation or detection thresholds were changed.
Petting accuracy on a different microphone/enclosure still needs acoustic testing.

`AudioParams::capture_device()` now preserves named PCMs verbatim instead of
turning `microduck_es8326` into the invalid name `microduck_es8326,0`. Existing
`plughw:aic3104` and explicit `hw`/`plughw` device-0 specifications retain their
previous behaviour. The standalone `sounds` CLI already supports
`--device microduck_es8326`; its Radxa default is intentionally not changed.

<a id="重复硬件格式检查"></a>
### Repeat the hardware format check

```sh
# Opens the real mic for two six-second captures; audio is discarded on exit.
# Playback is silence. Refuses a busy card; all children have bounded timeouts.
sh scripts/k1-audio-test.sh
```

Both capture-first and playback-first passed on the K1 with the SDK's requested
formats, 96,000 mono samples per capture, both hardware streams at 48 kHz stereo,
and no ALSA errors or xruns. A separate six-second capture took 6.37 s wall time
including process/device startup. These checks also ran while the two-job native
Rust build was active; this is a functional check, not a CPU or latency benchmark.
`scripts/k1-test.sh` stays software-only and does not implicitly open the microphone.

The initial failed `plug` attempt and the passing explicit-emulation run are kept
in `target/k1-es8326-duplex.log` and `target/k1-es8326-duplex-mmap-emul.log` on the
development host. No recorded microphone audio is retained by the test script.

<a id="sdk-集成与回归结果"></a>
### SDK integration and regression results

The rebuilt native `robotd` ran with `--fake --no-policy`, an isolated IPC/runtime
directory, the ES8326 PCM, the real `pet-detect/models/pet_detect.onnx`, and a
generated test voice bank. `robotctl quack` played through the **SDK's own** sound
path while its microphone worker continued recording. `/proc/asound` and both
children's command lines confirmed that capture and playback used the named PCM;
the microphone PID was unchanged and its hardware pointer advanced throughout.
The final health sample was healthy, 50.012 Hz, zero missed ticks. This is a short
fake-IO integration check with policies disabled, not a walking/load benchmark.
Stopping only `robotd` released both PCM streams within the bounded cleanup wait,
without a microphone restart. A first harness attempt sent SIGINT to the entire
timeout process group (including `arecord`) and checked closure instantaneously;
that produced a transient restart/SETUP state on exit, so it is not used as the
clean-shutdown result. Both attempts' logs are retained.

A separate five-second live capture fed the SDK's standalone `pet-detect` and
produced inference results; it is not a petting-accuracy test. A three-second mono
level check produced 48,000 samples, 43,819 nonzero samples, peak 156 and RMS 5.381
in signed-16-bit units, with no clipped samples. These are that room's observed
levels, not prescribed microphone gain settings.

After this change, native K1 / Rust 1.89 release tests passed **1,249 tests, zero
failed, six existing ignored**; macOS / Rust 1.93 passed **1,217, zero failed, six
ignored**. The K1 incremental release compilation took 20m08s. Formatting,
ShellCheck, and host Clippy for the changed parameter crate and `robotd` passed.
A wider Clippy probe still found an unchanged upstream `nonminimal_bool` warning
in `robotctl/src/monitor.rs:2188` under Rust 1.93; this audio change leaves that
unrelated code alone.

Logs: `target/k1-es8326-tests.log`, `target/k1-es8326-pet-detect.log`, and
`target/k1-es8326-sdk-recheck.log` on the development host; the SDK process log is
`target/k1-es8326-robotd-recheck.log` on the K1. The test bank/config live under
`target/k1-es8326-*`, not in the repository's tracked files.

After regression, the board's previously absent default voice bank was populated
with `sounds ensure-bank` (82 sounds under `/var/lib/robot/sounds`, seeded from
this board's hardware identity). The installed audio-only config still leaves
microphone monitoring off, and no systemd service was started or enabled.

<a id="rust-ort--spacemit-视觉后端"></a>
## Rust ORT + SpaceMIT vision backend

The opt-in EP backend is now wired through `duck-detect`, `[detect]` and `robotctl configure`
into mediad. CPU/two-thread and RKNN defaults are preserved. Use the compatible opset 17 float
model, not the original opset 12 ONNX or a generic COCO model. No Python worker is required.

The [SDK integration report](k1-duck-ort-ep.md) records native Rust measurements and exactness:
with hard cgroup CPU budgets, RGB preprocessing + inference + NMS averages **243.0 ms on two
CPUs** and **121.3 ms on four CPUs** for the floating-point EP model. The experimental INT8
path averages **33.9 ms on four CPUs**, but its output is not accuracy-qualified. All successful
EP profiles executed the SpaceMIT fused node with no CPU-provider compute nodes.

This runtime needs both EP worker CPUs (0-3) and a caller CPU from 4-7. A four-CPU budget was
tested as three workers on 0-2 plus caller CPU 4; `taskset` alone does not enforce the total
budget because the EP rebinds its caller. The integration report includes reproducible cpuset
commands, the failed probes, and correction of the earlier standalone experiment's core labels.

<a id="usb-相机单目与拼接双目"></a>
## USB camera: mono and packed stereo

Optional hardware processing is now available through `camera.acceleration = "spacemit"`:
private MPP codec2 JPEG decode, V2D crop/letterbox/rotation and SpaceMIT OpenCV UYVY packing.
It does not replace system MPP or become the default. See [build instructions, pixel
comparison, SDK measurements and remaining limits](k1-mpp-camera.md). The software-only
measurements below predate this opt-in backend.

The opt-in USB backend now feeds the SDK's existing selected-eye media/detection path.
`camera-check` validates the same source and detector without the still-missing WebRTC plugin.
The existing Radxa/IMX219 source remains the default; K1 CSI/ISP integration is separate.

The current acceptance scope is **one selected eye**, including when the physical camera is
packed stereo. Dual-eye extraction/inference below is an extra diagnostic, not a required
deliverable. With the final MPP build, selected-eye 720p capture measured **30.05 fps** and
live detection **192.8 ms mean** at 2 Hz under a hard four-CPU budget; see the MPP report for
matching software baselines, repeated-run variation and pixel differences.

On the connected DECXIN UVC camera (MJPEG 4000×1200 @30 advertised), native SDK tests passed
for left/right selection, mono cropping, paired extraction, invalid-mode/device handling and
camera release. The selected 1280×720 software path measured 5.3–5.4 fps, and two native
1920×1200 ROIs measured 8.81 pairs/s: **neither is a 30 fps decoded-video claim**.

With a hard four-CPU budget and the existing floating-point SpaceMIT EP model, 60 selected-eye
inferences ran at approximately 2 Hz: preprocessing + inference + NMS mean **253.7 ms**, P95
**264.5 ms**, while capture continued. A separate live ORT profile showed the SpaceMIT fused
node with no CPU-provider compute nodes. Dual-eye serial inference was only 1.96 pairs/s in a
short test, so stable dual-eye 2 Hz is not accepted. No weights/precision defaults were changed.

Follow-up profiling attributes about 96% of the profiled live detector time to the EP
node. The existing fused RGB-byte preprocessing costs only ~3 ms; isolated SpaceMIT
OpenCV takes ~4.5 ms for a direct three-operation replacement. A fixed-frame off/on/off
camera-load test moves the model invocation from 123 → 235 → 125 ms under the same hard
four-CPU budget, pointing to concurrent capture-resource contention rather than that
preprocessing loop. No OpenCV package was installed or preprocessing default changed.

See [USB camera configuration, exactness, measurements and file inventory](k1-usb-camera.md).
Only one physical packed-stereo device was tested; the mono software path used its left ROI.
This adds neither stereo depth/calibration nor synchronized independent USB devices.

<a id="本轮软件检查之外"></a>
## Outside this software check

- Real Dynamixel half-duplex UART, 15 servos and IMU feedback.
- CSI camera/sensor/ISP configuration and exposure control.
- K1 H.264 encoder selection and the missing `webrtcsink` runtime plugin.
- Labelled detector validation and simultaneous vision/control/media load. Live USB frames
  now reach the integrated Rust EP detector as described above; INT8 is not the SDK default.
- Real ToF, Bluetooth controller and gamepad bring-up; ES8326 audio is covered above,
  but microphone/enclosure-specific petting accuracy is not.
- RISC-V provisioning, signed release packaging, OTA assets and CI. The inherited release
  workflows and setup scripts still describe the Radxa/aarch64 platform; do not install their
  artifacts on a K1 simply because the branch is named `spacemit-k1`.
