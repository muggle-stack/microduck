<a id="k1-usb-相机单目与拼接双目"></a>
# K1 USB camera: mono and packed stereo

[English](k1-usb-camera.md) · [简体中文](k1-usb-camera_zh.md)

> Dated test results and known issues are recorded below. See [adaptation status](spacemit-k1-adaptation.md) for current functionality, and the [IMX219](k1-imx219.md) / [WebRTC](k1-webrtc.md) guides for the integrated camera workflows.

This is an **opt-in USB backend**, not an IMX219 ISP port and not a WebRTC port.
The existing Radxa source remains the default (`camera.backend = "rockchip"`, mount 90°).
USB defaults to mount 0°. `--rotate` overrides `camera.rotate` in `mediad`.

An additional explicit `camera.acceleration = "spacemit"` path now uses a private
MPP codec2/V2D bridge and the installed RVV OpenCV. The software results below remain
historical baselines; see [K1 MPP build, exactness and acceptance](k1-mpp-camera.md).
Neither the default backend nor detector preprocessing/model precision was changed.

<a id="已实现的配置契约"></a>
## Implemented contract

- `mediad` selects `camera.backend = "usb"` from the shared robotd TOML file.
  `robotctl configure` exposes the camera fields and associates their changes with `mediad`.
- An explicit V4L2 capture device and its advertised native format/size/rate are required.
  No `/dev/video0` USB default, device scanning, Rockchip sensor-mode pinning, RKAIQ startup,
  or Rockchip exposure writes. Existing UVC exposure/white-balance settings are left alone.
- Input formats: MJPEG (software `jpegdec`), YUYV (`YUY2` in GStreamer), UYVY, NV12.
  Conversion negotiates BT.601 limited-range UYVY, matching the existing detector contract.
- `mono`: full frame or `left_roi`. `stereo_sbs`: two disjoint ROIs in **one USB frame**.
  Empty ROIs mean full mono / conventional equal-width halves; vendor-specific strips require
  explicit `[x,y,width,height]` ROIs. Horizontal crop coordinates and widths must be even.
- `camera.view` selects one eye for the existing media/detection consumers. That eye is scaled
  and letterboxed, **not stretched**, to `media.quality`. Capture dimensions are independent
  from the output quality; output fps must not exceed the configured native fps.
- `mediad::camera::usb::Capture` also exposes native frames; `CameraFrame::views` extracts
  left/right images from that same frame with one PTS. Consumers get tightly packed pixels;
  `GstVideoMeta` offsets/strides are honored. Dropping `Capture` releases the device.
- `ImageDetector` is the same preprocessing → RKNN/ORT → box-decoding implementation used by
  the daemon and the headless check. No new model, quantisation, or provider fallback was added.

**Stereo here does not mean** rectification, camera intrinsics/extrinsics, disparity/depth,
proven simultaneous sensor exposures, or two independently clocked USB cameras. Those are
separate tasks. `mediad` still publishes only the selected eye, not a new stereo network API.

Invalid/missing explicit config now fails before opening hardware. Unlike the old media
loader, it does not replace a broken config with Rockchip defaults. A missing *implicit*
`/etc/robot/robotd.toml` still resolves to the original defaults. This is an intentional
fail-closed change, including on Radxa; normal valid Radxa configurations are unchanged.
Unknown keys inside the new `[camera]` section are also rejected instead of pruned. Other
sections retain the existing warn-and-ignore policy for unknown keys.

<a id="配置与无头检查"></a>
## Configuration and headless check

Templates (neither is installed automatically):

- `deploy/k1/camera-usb-mono.toml`: replace device and native mode with the camera's values.
- `deploy/k1/camera-usb-decxin-sbs.toml`: tested device identity/native mode; its ROIs are
  **visual candidates, not vendor-confirmed calibration**.

Runtime plugins come from `gstreamer1.0-plugins-base` and `gstreamer1.0-plugins-good`, already
present on the tested K1. CI installs these for the synthetic camera tests, in addition to
the existing development libraries. No K1 package upgrades or firmware changes were made.

```sh
v4l2-ctl --list-devices
v4l2-ctl -d /dev/v4l/by-id/YOUR_CAMERA-video-index0 --list-formats-ext
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
cargo k1 --locked -p mediad --bins -j 2

# Same selected-eye source builder as mediad; no signalling server or encoder required.
timeout 60 target/riscv64gc-unknown-linux-gnu/release/camera-check \
  --config deploy/k1/camera-usb-decxin-sbs.toml --warmup 3 --frames 30

# Native frame -> both ROIs, with a shared source timestamp.
timeout 60 target/riscv64gc-unknown-linux-gnu/release/camera-check \
  --config deploy/k1/camera-usb-decxin-sbs.toml --both-eyes --frames 5
```

For the selected right eye set `camera.view = "right"`. Mono uses `layout = "mono"`,
`view = "left"`, no `right_roi`, and optionally `left_roi` to crop. Restarting a daemon is
deliberate; changing the file does not hot-switch an open camera.

`--dump-dir` must name a **new directory with an existing parent**. The first measured frame
is saved as `<eye>.uyvy` plus `frame.json`; `--both-eyes` also saves `native.uyvy`. Geometry
is in the JSON. For an inspection image, use FFmpeg with the exact saved geometry, e.g.:

```sh
ffmpeg -nostdin -f rawvideo -pixel_format uyvy422 -video_size 1920x1200 \
  -i CHECK_DIR/left.uyvy -frames:v 1 CHECK_DIR/left.png
```

Add `--detect` for inference, optionally `--model /absolute/path/to/model.onnx`. Without
`--model`, exactly one enabled `detect.model` must resolve. SpaceMIT still requires its
compatible ONNX model and native runtime; see [ORT/EP validation](k1-duck-ort-ep.md).
The original `.rknn` cannot be given to SpaceMIT EP. Example additional config section:

```toml
[detect]
enabled = true
model = "/absolute/path/to/duck_detect.slim.onnx"
onnx_provider = "spacemit"
onnx_threads = 3
spacemit_affinity = "0;1;2"
spacemit_allow_fp16_epilogue = false
hz = 2.0
threshold = 0.35
```

Use `--hz 2` to pace the check (the CLI's default is unpaced, independently of `detect.hz`).
For the same hard four-CPU budget as the prior EP tests:

```sh
ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so \
systemd-run --scope --quiet --collect -p AllowedCPUs=0-2,4 \
  taskset -c 0-2,4 timeout 60 \
  target/riscv64gc-unknown-linux-gnu/release/camera-check \
  --config /absolute/path/to/usb-with-ep.toml --detect --hz 2 --frames 20
```

The cgroup bounds **all** capture/conversion/inference threads; `taskset` alone does not
constrain the EP's caller rebinding. `--profile PREFIX` requests a native ORT profile to
check provider assignment. `--both-eyes --detect` performs two serial inferences per source
frame; do not compare its pair rate directly with a single-eye detector's inference rate.

The check prints JSON frame records and a summary. `consumed_fps` includes frame acquisition,
copying, optional inference and pacing, after warm-up. `inference_mean_ms`/P95 include the
existing UYVY preprocessing + runtime + NMS but exclude acquisition/cropping/scaling. The
latest-frame sink can intentionally discard frames; PTS/buffer offsets are diagnostics,
**not a USB transport sequence-loss test**. Native libraries can print their own log lines.

No command above starts `robotd`, serial/servos, ToF, audio or a network listener. A complete
K1 `mediad` still needs the separate encoder/WebRTC work (`webrtcsink` is absent on the tested
board). The headless check validates this source and detector without that dependency.

<a id="硬件证据2026-09-06"></a>
## Hardware evidence, 2026-09-06

K1 / Bianbu 2.1.1, `1bcf:2d50` DECXIN Camera, UVC on the USB2 hub:

- `/dev/video20` is video capture; `/dev/video21` is **metadata**, not the right eye.
- Stable capture path: `/dev/v4l/by-id/usb-DECXIN_DECXIN_Camera_01.00.00-video-index0`.
- Advertised modes: **MJPG 4000×1200 @30 fps** and **YUYV 4000×1200 @1 fps** only.
  Neither 1280×720 nor 1920×1080 is an advertised native mode on this camera.
- A decoded image visibly contains two views and a leading strip. Candidate ROIs are
  `[160,0,1920,1200]` / `[2080,0,1920,1200]`; confirm their optical meaning separately.

Baseline, before SDK implementation:

| Check | Actual result |
| --- | --- |
| MJPEG V4L2 mmap capture, 360 buffers | Exit 0; sequence 0–359, no sequence gaps |
| Last 300 capture timestamps | 30.043 fps; intervals 31.885–36.138 ms |
| Startup included | 26.935 timestamp fps; first interval 1416.119 ms; wall 13.52 s |
| MJPEG frame extracted, strict FFmpeg decode | Success, 4000×1200; nonfatal APP-field warnings |
| GStreamer `jpegdec` → fakesink, 60 frames | Exit 0 / EOS; 7.55 s wall, 5.72 s user + 0.31 s system CPU |

Thus compressed capture at 30 fps is **not** evidence of software decoding at 30 fps.
The camera's `v4l2-ctl --all` also reports an ext-control Privacy query error (32); it did
not prevent these captures. Neither negative result is hidden or counted as fixed.

Evidence directory on K1: `target/k1-usb-camera-20260906.Lbddrk/`. Mac evidence directory:
`target/k1-usb-camera-20260906.F8Gx23/`. The copied native JPEG is byte-identical on both,
SHA-256 `2a69d3c2bfbe53dede8b2514fa9778f07b7ac91c1ce0e51f7465729ad08df3af`.

<a id="sdk-实时结果"></a>
### SDK live results

Native release binaries, Rust 1.89. All compiler processes had exited; the board was at
43–44°C before capture. The camera remained in MJPEG 4000×1200 @30 mode throughout the
successful checks. No other camera consumer, motors, audio or WebRTC workload was active.

| SDK path | Output | CPU budget | Warm-up / measured source frames | Consumed rate | Image detection mean / P95 |
| --- | --- | --- | --- | ---: | ---: |
| Selected left | 1280×720 | 8 available, unpinned | 3 / 30 | 5.316 fps | off |
| Selected right | 1280×720 | 8 available, unpinned | 3 / 30 | 5.389 fps | off |
| Mono layout with left ROI | 1280×720 | 8 available, unpinned | 3 / 30 | 5.430 fps | off |
| Both native ROIs | 2 × 1920×1200 | 8 available, unpinned | 3 / 30 | 8.810 pairs/s | off |
| Selected left + floating-point EP, `--hz 2` | 1280×720 | hard 4: 0–2,4 | 5 / 60 | 2.016 fps | 253.716 / 264.537 ms |
| Both ROIs + floating-point EP, `--hz 2` | 2 × 1920×1200 | hard 4: 0–2,4 | 3 / 10 | 1.965 pairs/s | 241.963 / 255.268 ms per eye |

The selected-eye 60-frame run completed in 35.02 s including startup/warm-up, consuming
74.98 user + 5.22 system CPU seconds. All returned PTS values increased; their span gives
1.990 fps for those 60 consumed frames. The small `consumed_fps` excess over 2 is the finite
measurement window, **not an acceleration**. All 60 requested inferences completed; no
timeout occurred. This verifies the selected-eye 2 Hz workflow for a short live run, not a
long-duration or walking/audio/media stress test.

During that run, `cpuset.cpus.effective` was `0-2,4`; the model's three worker threads were
bound to 0/1/2, the caller to 4, and other capture/helper threads stayed within the same
four-CPU set. The separate 20-frame profiled run (five warm-ups) recorded **25 SpaceMIT
fused-node events, zero CPU-provider compute-node events**. Its mean/P95 was 248.481/254.029 ms.
The profile is `live-ep-profile_2026-09-06_16-17-12.json` in the evidence directory.

The loaded floating-point model was the existing `duck_detect.slim.onnx`, SHA-256
`e1e8444bc8fe9a53675f1c885b781705f8579ea17abe23bcd2b853d59c8f746f`, unchanged before/after.
FP16 epilogue stayed disabled; no INT8 model was substituted. The scene was a ceiling, with
zero returned boxes: successful capture/inference is **not** labelled detector accuracy.

Negative/performance boundaries are material:

- The selected 720p path is only ~5.4 fps, slower than native paired extraction. Software
  JPEG decode/conversion/crop/scale is not optimized here. Neither path achieves 30 fps.
- Two serial eye inferences cost ~484 ms before pair acquisition/copy overhead; the observed
  1.965 pairs/s leaves essentially no margin. **Stable 2 Hz dual-eye inference is not accepted**.
  The original SDK detector consumes one selected eye, which is the accepted path above.
- The earlier offline RGB result (~121 ms at four CPUs) used different input preprocessing
  and no live camera load; it is not an apples-to-apples baseline for the ~254 ms live result.
- A real request for unsupported native 1280×720 MJPEG was refused with GStreamer
  `not-negotiated (-4)`, exit 1. After cleanup, native 4000×1200 capture reopened successfully.
- Invalid ROI, nonexistent device, right-eye request on mono, and a near-zero nonzero pacing
  rate were all refused (exit 1), without falling back to another camera/backend.

Repeated capture/open/close completed; `fuser /dev/video20` was empty after every successful
run and after the unsupported-mode failure. The transient CPU-budget scope is inactive.
The production `mediad` and `camera-check` are RISC-V ELF executables; both `--help` checks
passed. No installed TOML, system service, exposure control, or sensor/ISP settings were changed.

Independent FFmpeg crops of the saved **same native UYVY frame** matched both SDK ROIs with
`cmp` exit 0. Native/left/right SHA-256 respectively:

```text
93c011e9f732e5770438b6fef5ddb3f968d8b4b90715c034e753fd48b32a4b3f
4eba24e461b7b4aeafae7fd4d75094681f486fe9639b492e75ab7ceced19c5bd
7ec971e666f3e371a0a388cf833b7907dfdf8ef402e629ec3181fa985bb36955
```

Both views and the aspect-preserving 720p output were also visually inspected. The first
FFmpeg verification shell attempt omitted `-nostdin`, consumed part of the SSH script input
and exited 127 before the right-eye check; the corrected `-nostdin` rerun passed both byte
comparisons. This was a verification-runner error, not an SDK capture failure.

<a id="opencv-rvv-与实时相机资源争用"></a>
### OpenCV RVV and live-camera contention

Follow-up measurements on the same K1, 2026-09-06: **253.7 ms is the detector's
preprocessing + model invocation + NMS time, not preprocessing alone**. Acquisition,
MJPEG decode, crop and output scaling run upstream and are outside that timer, but
their concurrent resource use still affects inference latency.

`apt show opencv-spacemit` describes the available package, not its installation status.
The board had `libopencv-dev` / Python OpenCV `4.6.0+dfsg-13.1ubuntu1bb1` installed and
`opencv-spacemit` **not installed**, with candidate `4.14.0-1bb3`. For this comparison,
the candidate deb was downloaded and extracted under `target/`; a standalone C++ probe
used its headers and an explicit library RUNPATH. No apt installation, system-library
replacement, SDK dependency, or service change was made.

The isolated 4.14 build reported `Baseline: RVV`, `Custom HAL: YES (RVV HAL (ver 0.0.1))`
and runtime features `RVV`. The system 4.6 build reported no runtime CPU features or
custom HAL. This is consistent with the vendor's
[OpenCV RVV documentation](https://bianbu.spacemit.com/en/brdk/Basic_applications/3.5_High_Performance_Computing_Library/3.5.1_opencv_rvv/),
but the following timings are **this board's measurements**, not the documentation's
illustrative benchmark.

All probes used the same saved selected-left **1280×720 UYVY** frame, with rotation 0,
nearest-neighbour resize to 320×180 and padding value 114 to make 320×320 RGB bytes.
The Rust probe called the SDK's compiled `duck_detect::letterbox_from_uyvy` directly.
The C++ probe used full-frame `cvtColor(COLOR_YUV2RGB_UYVY)`, `resize(INTER_NEAREST)` and
`copyMakeBorder`, reusing its Mats. It ran on CPU 4 with `cv::setNumThreads(1)`.
The Rust probe used one calling thread inside the same hard four-CPU scope described
below. The camera was off; each microbenchmark discarded 10 warm-ups and measured
100 calls. These are exploratory function comparisons, not an accepted SDK speedup.

| RGB-byte preprocessing path | Mean / P95 (ms) |
| --- | ---: |
| Existing fused Rust implementation | 3.020 / 3.046 |
| SpaceMIT OpenCV 4.14: three operations | 4.537 / 4.612 |
| System OpenCV 4.6: same three operations | 14.339 / 14.580 |

SpaceMIT OpenCV is **3.16× faster than system OpenCV for these operations**, but that
does not make this direct replacement faster than the existing SDK implementation.
The latter only converts the 57,600 sampled pixels that survive the resize; full-frame
OpenCV color conversion processes 921,600 pixels before resizing. The comparison does
not exhaust possible OpenCV algorithms or upstream capture optimizations. These rows
exclude JPEG decode and HWC RGB → NCHW float tensor packing.

Exactness: both OpenCV variants differed from the SDK reference in **2,014 / 307,200
RGB bytes**, with maximum absolute byte difference 1. This is not bit-identical;
neither detection-accuracy equivalence nor a precision tradeoff was approved. No
replacement was integrated, and the original preprocessing remains the default.

To separate camera load from image-content changes, a second experiment repeatedly
processed the **same saved frame and unchanged floating-point model**, first without
capture, then while a separate SDK `camera-check` continuously acquired/decoded/scaled
the real 4000×1200 camera into selected-left 720p, then again after capture stopped.
Both processes in the loaded run shared **one** `AllowedCPUs=0-2,4` cgroup; the EP had
three workers with affinity `0;1;2`, caller CPU 4, FP16 epilogue disabled. Each phase
discarded five model warm-ups and measured 30 inferences, unpaced.

| Fixed-frame stage, mean ms | Camera off | Camera running | Camera stopped again |
| --- | ---: | ---: | ---: |
| Fused UYVY → letterboxed RGB bytes | 3.064 | 3.027 | 3.075 |
| `Model::infer`: tensor packing + ORT + output checks/copy | 123.157 | 234.919 | 125.415 |
| NMS | 0.021 | 0.021 | 0.021 |
| Total | 126.242 | 237.967 | 128.510 |
| Total P95 | 131.286 | 249.051 | 133.366 |

The background capture completed all 90 requested frames in 24.22 s including startup,
with increasing timestamps, and released the device afterward. Its aggregate rate is
not a loaded-only throughput benchmark: the fixed-frame inference process ran during
only part of that capture. All three fixed-frame RGB references matched (`cmp` exit 0),
SHA-256 `96412bf6ccef29ab3745ca4a3748053ee1fec48727f252380af6751c7ea0ea59`.
This is a controlled contention diagnostic, not a replacement for live SDK latency
acceptance or a claim of bit-identical model outputs across runs.

Independently, the **same** 20-frame live ORT-profiled run reported above averaged
248.481 ms for the detector, of which the SpaceMIT fused node averaged **238.397 ms**
after excluding its five warm-ups: about 96% was inside the EP node, with only 10.084 ms
outside it. Do not subtract that node average from the separate unprofiled 60-frame
result. The controlled off/on/off test supports concurrent capture-resource contention
as the main reason for the offline/live gap; it does not distinguish CPU scheduling,
memory/cache contention, or individual decoder/converter costs.

The next useful optimization target is therefore the **upstream full-resolution
MJPEG decode/conversion/scaling workload and its scheduling**, not replacing the
already ~3 ms detector resize/color loop. Reducing acquisition work or using a suitable
hardware decoder still requires separate measurement and pixel/accuracy checks; no
such acceleration, stable dual-eye 2 Hz result, or new throughput claim is made here.

Probe sources, binaries, complete OpenCV build information and logs are retained at
`target/k1-opencv-check-20260906.gBuOpC/` on K1. Relevant files are
`preprocess-check.rs`, `opencv-check.cpp`, `rust-offline.log`, `rust-with-capture.log`,
`rust-offline-repeat.log`, `opencv-spacemit.log`, `opencv-system.log` and
`capture-load.jsonl`. A copy excluding the deb/extracted libraries is at
`target/k1-opencv-check-20260906.dbfOVP/` on Mac. The downloaded deb SHA-256 is
`c74de8b27fdac8193a1b8777826fac7a20727fa04e5e2c69c9d41f8607d12f4e`.
The first standalone Rust link attempt lacked the host proc-macro dependency directory
and failed with E0463 (`rust-build.log`); adding both target and host dependency search
paths passed (`rust-build-fixed.log`). That diagnostic build error is retained.

<a id="精确性与验收边界"></a>
### Exactness and acceptance boundaries

Software regression on this change:

- macOS / Rust 1.93: full workspace, **1,231 passed, 0 failed, 6 existing ignored tests**.
- K1 / isolated Rust 1.89, release, two build jobs/two test threads: `mediad` and
  `robotd-params`, **153 passed, 0 failed, 0 ignored** (including the camera-check CLI test).
  Linux tests cover synthetic NV12 normalization, exact UYVY row-padding/offset removal,
  left/right pixel identity, aspect-preserving borders, shared-frame crops and missing devices.
- Host Clippy `-D warnings` passes for `mediad` and `robotd-params`. Adding `robotctl` to that
  Rust 1.93 lint command exposes the existing `nonminimal_bool` at `robotctl/src/monitor.rs:2188`;
  that unrelated code was not changed. The K1's isolated toolchain has no Clippy component;
  its compile/runtime tests, not host lint, validate the Linux-specific source.
- `cargo fmt --all --check`, `git diff --check` and `sh -n scripts/k1-test.sh` pass.

The full workspace host total and affected-crate K1 total cover different suites and must
not be added or compared as a platform speedup. No GitHub CI run is claimed here.

ROI/stride removal is byte copying **after** normalization. Tests compare the resulting
bytes exactly, including padded/offset buffers and distinct left/right source values.
MJPEG decode, color conversion and output scaling are image transformations, not bit-identical
copies of compressed input. Model weights/precision and detector preprocessing are unchanged;
cross-camera/cross-ISP pixel identity and labelled detection accuracy are not claimed.

Only one physical DECXIN packed-stereo camera is available for this run. Exercising its crop
as `mono` validates the mono software path, not interoperability with every monocular UVC
camera. IMX219 on K1, real stereo calibration/depth, other UVC models, USB disconnect/reconnect
recovery, and simultaneous walking/audio/WebRTC load remain separate hardware acceptance work.

<a id="本轮文件清单"></a>
<a id="files-changed-in-this-integration"></a>
<a id="相关文件"></a>
## Related files

- New: `robotd-params/src/camera.rs`
- New: `mediad/src/camera.rs`
- New: `mediad/src/camera/usb.rs`
- New: `mediad/src/bin/camera-check.rs`
- New: `deploy/k1/camera-usb-mono.toml`
- New: `deploy/k1/camera-usb-decxin-sbs.toml`
- New: `docs/project/k1-usb-camera.md`
- Modified: `robotd-params/src/lib.rs`
- Modified: `robotd-params/src/registry.rs`
- Modified: `robotd-params/src/edit.rs`
- Modified: `robotctl/src/configure.rs`
- Modified: `mediad/Cargo.toml`
- Modified: `mediad/src/lib.rs`
- Modified: `mediad/src/main.rs`
- Modified: `mediad/src/pipeline.rs`
- Modified: `mediad/src/config.rs`
- Modified: `mediad/src/detect.rs`
- Modified: `scripts/k1-test.sh`
- Modified: `.github/workflows/ci.yml`
- Modified: `docs/project/spacemit-k1.md`
- Deleted: none.

Generated logs, test configurations, sampled images and build outputs remain in the evidence
and Rust `target/` directories above. They are not committed to Git and do not overwrite
`/etc/robot/robotd.toml`.
