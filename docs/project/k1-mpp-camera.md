# K1 MPP + SpaceMIT OpenCV camera backend

This is an **explicit, opt-in USB processing backend** for the existing SDK camera
and detector consumers. It does not port IMX219/ISP or complete H.264/WebRTC.
The Radxa source and portable USB software path remain the defaults. No model,
quantisation, threshold, EP options or detector preprocessing is changed.

## What runs where

```text
one MJPEG UVC frame (mono or packed SBS)
  -> private MPP UVC / codec2 VDEC: hardware decode into NV12 DMA
  -> V2D: selected ROI + aspect-preserving resize/black bars + optional rotation
  -> SpaceMIT OpenCV: full-to-limited YUV range LUTs + NV12-to-UYVY packing
  -> bounded GStreamer AppSrc
  -> existing SDK frame/detector consumers
```

For native `--both-eyes` capture the full frame is retained and the existing SDK
extracts both configured ROIs with the same PTS. This is not disparity, calibrated
depth, synchronized independent cameras, or a dual-stream WebRTC implementation.
The mono configuration was tested using one ROI of the attached DECXIN camera;
a separate physical monocular UVC device has not been tested.

The bridge is a small C ABI loaded by Rust only when `camera.acceleration` is
`"spacemit"`. MPP and OpenCV are **not** new link/build requirements for normal
Rust builds, aarch64/Radxa, or software USB capture. An unsupported architecture,
missing bridge, ABI mismatch, device/mode error or processing error fails explicitly;
there is no silent backend, model, precision or resolution fallback.

The C++ bridge uses one camera context per process, a bounded compressed-input
feeder, latest decoded frames, bounded read waits, joined teardown and guarded
frame releases. The private UVC patch tracks V4L2 base-reference ownership so
shutdown does not release a buffer already returned to the free list. The AppSrc
has one queued buffer; the existing headless sink also keeps only the latest frame.
Capture PTS intervals are preserved and rebased into the GStreamer running clock;
this is not a measured audio/video synchronization guarantee.
MPP camera-module diagnostics are patched to stderr so native stdio cannot split
the SDK's stdout frame JSON records. Errors are retained, not suppressed.

V2D consumes a contiguous NV12 DMA allocation. The bridge checks plane FDs,
offsets, allocation sizes and strides; separated planes are copied into a persistent
contiguous buffer rather than misinterpreted as zero-copy. DMA CPU accesses are
explicitly synchronized. OpenCV uses one CPU thread, persistent intermediate arrays,
range lookup tables, nearest vertical chroma expansion and channel packing. The final
UYVY buffer and SDK frame copy are still CPU-visible copies: **not end-to-end zero-copy**.
Here "contiguous" means the two-plane layout within one DMA-BUF, not a claim of
physically contiguous pages from Linux's `system` DMA heap.

## Build and select it

Tested on K1/Bianbu 2.1.1 with `opencv-spacemit` **4.14.0-1bb3**, whose libraries
report `4.14.0-pre` and RVV support. Its CMake files are under
`/opt/opencv-spacemit/lib/cmake/opencv4`. System OpenCV 4.6 remains installed.
Native prerequisites are a C++17 compiler, CMake, Git, pthreads, Linux DMA headers
and the usual SDK GStreamer development packages. The board must expose working
UVC capture, hardware decoder, `/dev/v2d_dev` and `/dev/dma_heap/system` devices.
Do not grant blanket device permissions; provision the service user's access separately.

The build takes an existing clone of [SpaceMIT MPP](https://github.com/spacemit-com/mpp)
containing commit **`2b97ffe84c06071774301fad5542fbe76fa62774`**. It archives that
exact revision into an SDK-local build directory, applies the checked-in patches,
builds only the required targets and copies the resulting libraries beside the bridge.
It does not modify the upstream clone or run upstream's post-build plugin installers.

```sh
cd /root/workspace/microduck
sh scripts/build-k1-camera.sh /root/workspace/spacemit-sdk/components/multimedia/mpp
export MICRODUCK_K1_CAMERA_LIB=/root/workspace/microduck/target/k1-camera/libmicroduck_k1_camera.so
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
cargo k1 --locked --bins -j 2

timeout 30 target/riscv64gc-unknown-linux-gnu/release/camera-check \
  --config deploy/k1/camera-usb-decxin-mpp.toml --frames 30
```

The template configures the camera only. For inference retain the existing verified
`[detect]` ONNX/SpaceMIT EP settings and model path, then add `--detect --hz 2` to
the check. See [detector configuration and hard CPU-budget command](k1-usb-camera.md#configuration-and-headless-check).
The bridge does not turn an upstream `.rknn` model into an EP-compatible ONNX model.
Build all SDK binaries before using the new shared config with other daemons:
`robotctl`, `robotd` and `padd` also consume `robotd-params`. A quick `-p mediad`
build is enough for isolated camera checks, but leaves those old executables unaware
of the new strict `camera.acceleration` field. No running service is replaced here.

The optional second build argument must be a **dedicated subdirectory of this
workspace's `target/`**. `K1_BUILD_JOBS` defaults to 2. Cached source revision and
patch hashes are checked; use a new output subdirectory when changing a pinned
revision/patch. `MICRODUCK_K1_CAMERA_DEPS_ONLY=1` builds just the private MPP libraries.
Never run `cmake --build ... --target all` or `cmake --install` on this MPP snapshot:
unrelated upstream ISP targets still contain their own installers.

The bundle layout is:

```text
target/k1-camera/
  libmicroduck_k1_camera.so
  lib/libmpp.so -> libmpp.so.1 -> libmpp.so.1.0.0
  lib/libv4l2_linlonv5v7_codec2.so
  camera-native-check
  camera-image-test
```

The bridge uses `$ORIGIN/lib` and `/opt/opencv-spacemit/lib`. Its patched MPP loader
loads codec2 **only beside that MPP library**, not from `/usr/lib` or a user plugin
directory. Private MPP functions are bound internally with `-Bsymbolic-functions`.
Do not add the bundle to the global loader path or overwrite the board's legacy MPP.
Stop consumers before rebuilding/replacing a loaded bundle. No service file, apt
package, system library or global `LD_LIBRARY_PATH` is changed by the build script.

The example profile is not installed or activated automatically. To use an existing
valid USB profile, set `camera.acceleration = "spacemit"` and provide the absolute
bridge environment variable to the process. The key is exposed by `robotctl configure`
and uses the existing camera-change/restart handling. To return to the portable path,
select `"software"` and restart the consumer; the bridge is then not loaded.

Supported accelerated inputs are MJPEG, even dimensions up to 4096×2160, and
even `[x,y,width,height]` ROIs. The requested native mode must match what the UVC
driver negotiates. V2D supports 1/8–8× scaling per axis; the fitted rectangle and
padding must remain even (NV12 alignment). Unaligned layouts are rejected rather
than silently rounded. Raw YUYV/UYVY/NV12 cameras continue to use the software path.

`camera.rotate` remains mount metadata by default. Only an explicit
`mediad --flip-in-pipeline` requests physical rotation; for this backend it is done
by V2D after crop/letterbox. `camera-check --flip-in-pipeline` tests that same source
behavior. 90°/270° swap output dimensions. Native `--both-eyes` extraction cannot
be combined with this flag; detector rotation must not be applied a second time.

## Reproduce correctness checks

```sh
# 20 exact synthetic image checks + 13 rejected invalid configurations/frames.
timeout 30 target/k1-camera/camera-image-test

# Optional comparison input: the saved native 4000x1200 JPEG from USB validation.
# Output must be a NEW file. No camera is opened in this mode.
timeout 15 target/k1-camera/camera-image-test /path/to/frame.jpg /path/to/new.uyvy

cargo test --release --locked --target riscv64gc-unknown-linux-gnu \
  -p robotd-params -p mediad -j 2

# Explicit live test: three open/read/drop cycles in the SAME Rust process.
MICRODUCK_K1_CAMERA_TEST_CONFIG=/root/workspace/microduck/deploy/k1/camera-usb-decxin-mpp.toml \
cargo test --release --locked --target riscv64gc-unknown-linux-gnu -p mediad --lib \
  k1_repeated_open_read_drop -- --ignored --nocapture --test-threads=1
```

Synthetic tests cover all four rotations, crop-only pixel mapping, constant-color
resize/black bars, limited-range packing, padded strides and separate DMA planes.
All **20** compare every output byte against the reference (`differences=0`);
**13** bad ABI/configuration/short-buffer/format cases fail closed.

SDK verification: host full workspace **1,232 passed / 6 ignored**; K1 release
`mediad` + `robotd-params` **154 passed / 1 hardware test ignored**. That hardware
test was then explicitly run and passed: three open/read/drop cycles in the same
Rust process, 15 frames total, in 6.21 s. Left/right, mono-via-ROI and physical
90°/180°/270° SDK capture all passed with increasing PTS. Missing bridge and
unsupported native 1280×720 mode failed explicitly; no device remained open.
The native full-workspace `--bins` build then passed (14m46s), including all
shared-config consumers. All nine CLI loader checks passed: `robotd`, `robotctl`,
`updaterd`, `configd`, `btd`, `padd`, `mediad`, `tofd`, `camera-check`. Their
`--help` checks used an isolated runtime directory, not `/run`; no hardware control
service was started or installed. The new camera key is present in the rebuilt
`robotctl`, `robotd` and `padd` executables.

For a fair software comparison, pin **pixel-aspect-ratio=1/1 at both ends** of
`videoscale`, as the production SDK does. Omitting it allows GStreamer to negotiate
non-square pixels instead of letterboxing and is not the same image geometry.

Exactness for the saved native JPEG (SHA-256
`2a69d3c2bfbe53dede8b2514fa9778f07b7ac91c1ce0e51f7465729ad08df3af`),
same left ROI, same 1280×720 square-pixel output:

- Hardware decode + V2D resize vs SDK-equivalent software chain is **not bit-identical**:
  1,125,646 of 1,843,200 UYVY bytes differ; mean absolute difference **1.762664**,
  P99 **17**, maximum **41**.
- Both paths have exactly the same 64-pixel black sidebars (`Y=16`). Interior luma
  mean is 116.833501 software / 116.759251 hardware; this is not an unconverted
  full-range image simply relabelled as limited range.
- Decoder/scaler/chroma-sampling implementations differ. These byte statistics
  do **not** prove equivalent labelled detection accuracy. No lower model precision
  or alternate detector preprocessing was enabled. The backend stays **opt-in**.

## Measurements and limitations

SDK performance was recorded after all board compiler jobs finished (2026-09-06).
The acceptance configuration is the same camera/native mode, selected left ROI,
720p30 output, unchanged floating-point slim ONNX, SpaceMIT EP with 3 workers
`0;1;2`, caller CPU 4, FP16 epilogue disabled and threshold 0.35. The whole command
is in a hard `AllowedCPUs=0-2,4` cgroup, not just an initial `taskset` mask.

Model SHA-256:
`e1e8444bc8fe9a53675f1c885b781705f8579ea17abe23bcd2b853d59c8f746f`.
Live image content is not identical across captures; the same-JPEG correctness
comparison above is separate from live throughput/latency measurements.

Unprofiled single-eye live sequence, **5 warm-up + 60 measured frames**, paced at
2 Hz. Detection time includes existing preprocessing + ORT/EP + NMS, but excludes
camera acquisition/crop/scale. The CPU column is GNU `time` user+system CPU divided
by whole-command wall time, **including startup/warm-up**, where 100% means one core.

| Backend, execution order | Detection mean / P95 | Whole-process CPU | User + system / wall |
| --- | ---: | ---: | ---: |
| Software | 243.280 / 252.151 ms | 227% | 80.04 / 35.15 s |
| MPP + OpenCV | 187.411 / 207.695 ms | 153% | 53.72 / 34.92 s |
| Software repeat | 244.943 / 251.096 ms | 229% | 80.49 / 35.05 s |
| MPP final regression, diagnostics routed to stderr | 205.659 / 215.716 ms | 165% | 57.90 / 34.91 s |
| MPP, final full-workspace build | 192.812 / 209.435 ms | 155% | 54.29 / 34.92 s |

The meaningful camera improvement is throughput and reduced CPU pressure. The
live detector improves by about **38–58 ms (16–24%)**, not an order of magnitude;
it still does not reach the previous offline ~125 ms model-invocation baseline.
The hardware runs vary between **187–206 ms**; the variation's cause was not isolated,
so 187 ms is not a guaranteed latency. All five maintained ~2 Hz consumption with increasing PTS. No servo, audio,
encoder or WebRTC workload was active. Run-start temperatures across the whole
test sequence were 40–50°C. The headless tool opens the EP before the camera;
its main thread is bound to CPU 4, while the hard cgroup bounds every child thread.

Final capture-only selected 1280×720 output, no detector, same four-CPU cgroup,
**5 warm-up + 120 measured frames for BOTH backends**: software **5.393 fps**,
hardware **30.055 fps** (**5.57×**); PTS-based rates were **5.393 / 30.049 fps**.
The earlier 60-frame software / 120-frame hardware run measured 5.419 / 30.069 fps
and remains in the evidence directory; the matched-count final run is the acceptance row.
These are latest-frame consumer rates, not transport-loss certification. Native
packed-SBS capture + extraction of two 1920×1200 ROIs measured **20.773 pairs/s**
(5 / 60); the full-frame CPU packing/copies are still more costly than selected-eye
720p. Do not equate this with two independent camera streams at that rate.

Both-eye inference (5 warm-up + 20 measured pairs, two serial inferences per pair,
requested 2 Hz) achieved only **1.981 pairs/s**. Mean per-eye inference was
**227.048 ms**, P95 **257.872 ms**; summed pair inference P95 was about **517 ms**,
with **5/20 pairs over 500 ms** even before acquisition costs. **Stable dual-eye
2 Hz is not accepted**. It needs further scheduling/processing work and a longer
combined-load test.

The separate profiled hardware run (5 + 20, 2 Hz) measured **201.634 ms** total
detection mean. All **25** recorded compute-node executions belonged to
`SpaceMITExecutionProvider` (one fused subgraph per inference; no recorded CPU
compute nodes). After five warm-ups the EP node averaged **189.535 ms**; the
remaining **12.099 ms** was outside that node in this same run. Do not subtract
that profile average from the separate unprofiled 187.411 ms result.

Final native bridge-only diagnostic, same 4-CPU cgroup, selected 720p, 3 warm-up
and 120 measured frames, **no detector**: 30.023 consumed / 30.049 PTS fps;
V2D crop/resize stage **6.362 ms**, OpenCV range/packing **6.371 ms**, average
frame wait **20.443 ms**. Waiting for the next frame is not the JPEG decoder's
execution time; these stage figures also exclude Rust/GStreamer copying.

The logging-only patch was followed by all 20 exact image/13 invalid-input tests,
same-process reopening, and the final SDK runs above. The same-JPEG UYVY before
and after the patch compared **byte-identical (`cmp` exit 0)**. All 60 final live
frame records and both sets of 120 capture records parsed intact as JSON; only
the scope's explicit CPU-budget diagnostic was outside the JSON records.

Retained negative results:

- Installed legacy `spacemitdec` decoded a standalone JPEG but returned an EOS
  error; in live capture it emitted only **3 frames** before a **20 s timeout**
  (0.24 s user / 18.65 s system CPU). It is not the SDK backend used here.
- The legacy `G2D_Init` probe crashed through a null function pointer, confirmed
  with GDB. This was not evidence that K1 lacks V2D: the new direct V2D API passed
  the board tests above.
- The initial new-MPP capture needed `SYS_Init` before `VB_Init`. Its initial
  teardown also logged a duplicate UVC base-reference release; both were fixed
  and the failing logs retained. The second fix is isolated in the source patch.
- An initial private-source patch was silently skipped because `git apply` saw
  the enclosing SDK worktree. The builder now creates a standalone snapshot Git
  root and verifies reverse applicability. A later formatted build exposed
  vendor MIN/MAX macro include-order conflicts; header ordering was fixed while
  retaining `-Werror`. Neither failing build installed system plugins.
- Full-workspace host Clippy with `-D warnings` stops at the existing unrelated
  `robotctl/src/monitor.rs:2188` `nonminimal_bool` warning under Rust 1.93. The
  affected `mediad`/`robotd-params` Clippy check passes; this task does not rewrite
  the unrelated monitor expression.

Do not infer a complete K1 media daemon from these tests. `webrtcsink` is still
absent on the tested board, encoder selection needs separate integration, IMX219
is not connected, and simultaneous servo/vision/audio/network load plus labelled
detection validation remain outstanding. A second physical monocular camera and
other USB modes need their own hardware acceptance.

Evidence: K1 `target/k1-mpp-camera-20260906.ZGXnqy/`; Mac
`target/k1-mpp-camera-20260906.WZDoGI/`. Failed probes and build logs are retained;
generated frames, private dependency builds and model files are not committed.

## File inventory

New:

- `deploy/k1/camera-usb-decxin-mpp.toml`
- `mediad/src/camera/k1.rs`
- `native/k1-camera/CMakeLists.txt`
- `native/k1-camera/.gitattributes`
- `native/k1-camera/camera_bridge.h`
- `native/k1-camera/camera_bridge.cpp`
- `native/k1-camera/native_check.cpp`
- `native/k1-camera/image_tests.inc`
- `native/k1-camera/mpp-isolation.patch`
- `native/k1-camera/mpp-uvc-ownership.patch`
- `native/k1-camera/mpp-logging.patch`
- `scripts/build-k1-camera.sh`
- `docs/project/k1-mpp-camera.md`

Modified:

- `Cargo.lock`
- `mediad/Cargo.toml`
- `mediad/src/camera.rs`
- `mediad/src/camera/usb.rs`
- `mediad/src/pipeline.rs`
- `mediad/src/bin/camera-check.rs`
- `robotd-params/src/camera.rs`
- `robotd-params/src/lib.rs`
- `robotd-params/src/registry.rs`
- `docs/project/k1-usb-camera.md`
- `docs/project/spacemit-k1.md`

Deleted: none.
