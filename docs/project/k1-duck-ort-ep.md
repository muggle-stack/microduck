<a id="k1-鸭子检测器rust-ort--spacemit-ep"></a>
# K1 duck detector: Rust ORT + SpaceMIT EP

[English](k1-duck-ort-ep.md) · [简体中文](k1-duck-ort-ep_zh.md)

> This is dated experiment/failure evidence, not a list of entirely current blockers. See [current status](spacemit-k1-adaptation.md) and subsequent [IMX219](k1-imx219.md) / [WebRTC](k1-webrtc.md) integration. Translation does not change the original test conditions or imply new acceptance runs.

The SDK can select SpaceMIT EP through its existing Rust `ort` binding. No Python process,
new model protocol, or duplicated preprocessing/postprocessing is needed. The original RKNN
path and CPU/two-thread ONNX default remain available. This is **opt-in**, not a provisioned
camera service or a claim of complete K1 hardware integration.

<a id="模型与原生运行时"></a>
## Model and native runtime

Use the **floating-point opset 17** `duck_detect.slim.onnx` from the
[models-duck-detect-v1 release](https://github.com/muggle-stack/microduck/releases/tag/models-duck-detect-v1).
This model-only prerelease includes its manifest, source graph/conversion recipe and model
license notices; it is not an SDK/OTA release and is not downloaded by git clone.
The [quantization experiment](k1-duck-quantization.md) records its origin and accuracy limits.
Its SHA-256 is
`e1e8444bc8fe9a53675f1c885b781705f8579ea17abe23bcd2b853d59c8f746f`.
The original `duck-detect/models/duck_detect.onnx` is left untouched: EP 2.0.6 cannot compile
its opset 12 attention reshape. A generic YOLO11 COCO model is not a substitute: this decoder
requires one f32 input named `images`, static `[1,3,H,W]`, and one f32 `[1,5,N]` output.

On the tested K1 the native runtime is `/usr/lib/libonnxruntime.so`
(`1.24.2+spacemit.a1`) and `/usr/lib/libspacemit_ep.so` (`2.0.6`). Set
`ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so`; optionally set `SPACEMIT_EP_DYLIB_PATH` to a trusted
EP library's absolute path. Use the matching native libraries, not the different ORT bundled
under Python's site-packages. Neither apt Rust nor the Rust `ort` crate version is changed.

`duck-detect/src/spacemit.rs` dynamically loads `OrtSessionOptionsSpaceMITEnvInit`, using the
ABI in the board's `spacemit_ort_env_c_api.h`. Registration happens on this detector's session
options only; robotd's policy sessions are unaffected. The library remains loaded until after
the session drops. Native `OrtStatus` errors propagate; registration/session-load failures are
not retried with CPU-only settings. Actual provider execution must be checked in a profile.

EP 2.0.6's initializer explicitly adds CPU EP alongside SpaceMIT. Setting ORT's
`session.disable_cpu_ep_fallback=1` conflicts with that and was rejected on the board, so the
adapter **does not set it**. Native per-node CPU fallback remains possible (including an entirely
CPU-assigned graph); a registration log alone is not an acceleration claim. Validate every new
model/runtime pair with `duck-bench --profile-prefix`. The daemon logs this fallback policy.

The EP also needs ORT's **C++ symbols in the global loader scope**. The initial board probe
failed with `undefined symbol: _ZTIN11onnxruntime18IExecutionProviderE`: EP 2.0.6 does not declare
`libonnxruntime` in its `DT_NEEDED` list, and Rust `ort` loads libraries with `RTLD_LOCAL`.
The adapter verifies the ORT API pointer identity, then promotes that same native ORT mapping
with `RTLD_NOW | RTLD_GLOBAL` before opening the EP. This matches the symbol visibility of the
working C++ executable without introducing a link-time dependency or mixing Python's runtime.

<a id="启用-mediad-前先离线验证"></a>
## Offline verification before enabling mediad

Build natively, using the independent Rust 1.89 installation:

```sh
cd /root/workspace/microduck
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
export ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so
cargo k1 --locked -p duck-detect --bin duck-bench -j 2
```

The experiment model is currently in `target/duck-quant-20260905.9Kv6aZ/` on K1. Supply a
directory containing JPEG frames; the benchmark never opens a camera, serial port or audio
device. Example with a hard **four-CPU budget** (three EP workers and one caller CPU):

```sh
systemd-run --scope --quiet --collect --property=AllowedCPUs=0-2,4 \
  taskset -c 0-2,4 target/riscv64gc-unknown-linux-gnu/release/duck-bench \
  --model target/duck-quant-20260905.9Kv6aZ/duck_detect.slim.onnx \
  --frames /path/to/eval-jpegs --onnx-provider spacemit \
  --threads 3 --spacemit-affinity '0;1;2' \
  --warmup 10 --passes 5 --hz 0 --verbose \
  --dump-outputs /path/to/new-run.outputs.f32 \
  --profile-prefix /path/to/new-run.profile
```

The benchmark retains RGB letterboxing, HWC-to-NCHW `/255`, one-class decoding and NMS. It
reports inference+decode (including NCHW conversion) and letterbox+inference+decode separately.
Neither includes JPEG decoding, camera acquisition or pacing. Raw outputs are saved in sorted
JPEG order as little-endian f32 after timing; existing output files are refused. Profiling runs
one frame in a **separate session after releasing the timed session**, so it neither distorts
the timed runs nor overlaps two EP worker pools. The profile should show
`SpaceMITExecutionProvider` executing the fused node, not just a provider-registration log.

For a fair CPU comparison use the same slim model, frames, hard CPU set, warmup and passes,
with `--onnx-provider cpu --threads 4` and without the EP affinity argument. The total CPU
budget is equal; EP worker count is deliberately different because its caller needs a CPU too.
Use distinct output/profile paths for every run. `systemd-run --scope --collect` only creates
a transient benchmark scope, automatically reclaimed at exit; it does not install a service.

<a id="ep-调用线程亲和性也计入-cpu-预算"></a>
### K1 EP caller affinity is part of the CPU budget

With EP 2.0.6, `/proc/PID/task/TID/status` showed the caller moving from the initial
`taskset 0-1` mask to **CPU 4-7** after inference began, while the two EP workers stayed on
0 and 1. `taskset` alone was not a hard process-wide CPU budget. These negative probes are
preserved with the successful measurements:

- Hard cpuset 0-3: inference failed with `Spine Executor Set thread affinity error`.
- Hard cpuset 4-7 and EP worker IDs `4;5;6;7`: session creation rejected affinity ID 4.
- Hard cpuset **0-2,4**, EP workers **0;1;2**: all threads confined, caller on 4; successful.
- Hard cpuset **0,4**, one EP worker **0**: all threads confined, caller on 4; successful.

Thus this tested runtime uses worker IDs from 0-3 and a caller mask in 4-7. Do not equate
`onnx_threads` with total occupied CPUs, or apply a service cpuset that excludes the caller.
Use a cgroup `AllowedCPUs` constraint if a deployment must enforce a hard budget. The daemon's
GStreamer threads share its service budget too; live video/control contention is not tested here.
The [vendor thread-option documentation](https://github.com/spacemit-com/docs-ai/blob/main/en/compute_stack/ai_compute_stack/onnxruntime.md#provider-option-reference)
also distinguishes the EP pool from ORT's intra-op pool.

<a id="守护程序与配置编辑器使用的参数"></a>
## Configuration consumed by the daemon and editor

After offline verification, merge these entries into the existing `[detect]` section of a
per-robot config; do not replace the rest of the file or create a duplicate table:

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

All new keys are in the shared params schema and `robotctl configure` registry. Old configs
still mean `onnx_provider = "cpu"`, `onnx_threads = 2`, affinity `"auto"`, FP16 epilogues off.
SpaceMIT requires an explicit `.onnx` model; it does not auto-select the incompatible shipped
model. Thread count and affinity format are validated. Models are never quantized or replaced
implicitly. `mediad` passes the configured options into the same `duck-detect` model used by
the benchmark and publishes the existing `media.detections` notification unchanged.
The example worker selection pairs with a 0-2,4 service CPU budget if one is imposed; these
TOML entries alone do not impose the cgroup limit or automatically reserve the caller CPU.

<a id="k1-sdk-实测2026-09-06"></a>
## K1 SDK measurements — 2026-09-06

Rust 1.89 native release build, detector implementation at `d5b42cd`. Same 12 JPEGs for every
case, 10 warmups, 60 timed runs (five passes), confidence 0.35 and NMS 0.5. No compiler jobs or
robot daemons ran during timing. Each case used an automatically collected cgroup scope; the
effective cpuset and all thread masks were captured. Profiling was outside the timed session.

Times below include **SDK RGB letterbox + NCHW conversion + inference + output validation +
decode/NMS**; they exclude JPEG decoding, camera/ISP, UYVY conversion, WebRTC and pacing.

| Backend/model | Hard CPU set (budget) | ORT / EP workers | Mean ms | P95 ms |
| --- | --- | --- | ---: | ---: |
| Original ONNX / CPU | 0,4 (2 CPUs) | 2 / — | 732.114 | 734.768 |
| Float opset 17 / EP | 0,4 (2 CPUs) | 1 / 1 + caller | 242.995 | 254.051 |
| Original ONNX / CPU | 0-2,4 (4 CPUs) | 4 / — | 457.038 | 486.367 |
| Float opset 17 / CPU | 0-2,4 (4 CPUs) | 4 / — | 454.090 | 485.994 |
| Float opset 17 / EP | 0-2,4 (4 CPUs) | 3 / 3 + caller | 121.293 | 129.989 |
| Experimental INT8 / EP | 0-2,4 (4 CPUs) | 3 / 3 + caller | 33.851 | 35.779 |

The floating-point EP path fits the **500 ms / 2 Hz offline budget even with two CPUs**.
The four-CPU same-model EP/CPU comparison is about **3.74×**; the roughly 3 ms CPU-only graph
simplification difference is not a meaningful optimization. INT8 is faster but is a different,
not accuracy-qualified model. All successful EP profiles show **one executed
SpaceMITExecutionProvider fused node and zero CPUExecutionProvider compute nodes**. That
proves provider execution, not which hardware instruction each internal operation used.

<a id="输出一致性不是带标注精度"></a>
### Output agreement, not labelled accuracy

- Original and slim CPU models are **byte-identical** on this four-CPU run; both give 10 boxes.
- Float EP gives 11 boxes: all 10 baseline boxes match (mean IoU 0.999716, mean matched score
  delta 0.000604), plus one threshold crossing. On frame 2, candidate 1954 moves from
  **0.34894353 to 0.35001078**, crossing the unchanged 0.35 threshold. It is not bit-identical.
- INT8 gives 9 boxes: 8 match the baseline, 2 baseline boxes are absent and 1 is additional.
  Mean matched IoU is 0.903745 and score delta 0.081226. Keep it experimental/opt-in.
- These JPEG-decoded inputs are not pixel-identical to the earlier C++ experiment's tensors
  made directly from video frames, so its 11-box baseline must not be mixed with this 10-box one.

Earlier `taskset`-only exploratory SDK runs reported float EP 112.872 ms (four EP workers) and
145.964 ms (two workers), and INT8 29.865 ms (four workers). They are preserved, **not presented
as strict four-/two-CPU results**, because the caller escaped the initial affinity mask.
Likewise the prior C++ report's EP worker counts are not proof of the same total CPU budget.

The timing logs predate a reporting-only correction to `duck-bench`: its inherited unpaced
CPU-percentage formula always printed 100%. CPU-ms/frame, latency, raw outputs and profiles
are unaffected; current code uses measured wall throughput for that percentage and has a test.

Artifacts (no model binaries added to Git):

- K1: `/root/workspace/microduck/target/duck-ort-ep-20260905.p05HnR/`
- Host: `target/duck-ort-ep-20260905.6nnWCR/`, with board results in `k1-results/`
- `run-benchmarks.sh`, `run-bounded.sh`, `probe-affinity.sh`, `compare.py` reproduce the probes.
- `bounded-comparison.json`, `*.outputs.f32`, `*.profile_*.json`, `*.affinity.txt` and `*.log`
  contain numerical evidence. No calibration or model conversion was repeated for SDK integration.

<a id="精度与剩余限制"></a>
## Precision and remaining limits

- Keep the floating-point model as the initial EP candidate. Prior C++ measurements are in the
  quantization report; do not confuse those with Rust SDK or full-camera pipeline latency.
- `duck_detect.int8-float-output.onnx` remains experimental: in the prior 12-frame comparison,
  it retained 8 of the baseline's 11 detections. Those are output-agreement counts, not labelled
  accuracy/mAP. INT8 is not the default and is not accuracy-qualified.
- EP 2.0.6's INT8 path requires explicit `spacemit_allow_fp16_epilogue = true` (benchmark:
  `--spacemit-allow-fp16-epilogue`). With FP16 epilogues disabled it previously threw
  `std::bad_function_call` and aborted. A native `SIGABRT` cannot be caught as a Rust `Result`;
  this implementation does not provide process isolation or promise crash recovery.
- The default false setting sends `SPACEMIT_EP_DISABLE_FLOAT16_EPILOGUE=1`; true leaves the
  provider's default epilogue selection enabled. Neither setting promises bit-identical output
  versus CPU or controls every internal provider precision decision.
- K1 camera/ISP, WebRTC encoding, live frames and simultaneous 50 Hz hardware control still
  need integration testing. An offline detector result is not a complete-robot acceptance test.

<a id="回归命令"></a>
## Regression commands

```sh
cargo test --locked --release --target riscv64gc-unknown-linux-gnu \
  -p duck-detect -p robotd-params -p mediad -j 2 -- --test-threads=2
cargo k1 --locked -p mediad --bin mediad -j 2
```

No services need to be installed or restarted for these checks. The shared schema tests cover
the editor's choices/completeness and preserve shipped defaults; Linux mediad tests cover the
configuration-to-detector mapping and the unchanged sighting notification.

At code revision `7d8b00a`, native K1 release regression for the three selected crates passed
**155 tests, zero failures, zero ignored**. The macOS full-workspace regression passed
**1,223 tests, zero failures, six existing ignored timing/visual probes**. Host Clippy for
`duck-detect`, `robotd-params` and `mediad` (all targets, `-D warnings`) and formatting passed.
These are not full-workspace K1 or aarch64 Linux hardware regression claims.

The final native production `duck-bench` and `mediad` build passed, as did `mediad --help`
using an isolated `DUCK_RUNTIME_DIR`. A final four-CPU EP smoke check of the reporting-corrected
binary passed (12 frames, 120.513 ms RGB-path mean; separate profile again showed one SpaceMIT
node and no CPU compute nodes). That short check is not substituted for the 60-run table above.
No benchmark processes or transient scopes remained. No robot/camera/audio service was enabled,
no installed service config or runtime package was changed, and the original model was not replaced.

<a id="此次集成的文件清单"></a>
## Files changed for this integration

- New: `duck-detect/src/spacemit.rs`
- New: `docs/project/k1-duck-ort-ep.md`
- Modified: `duck-detect/src/onnx.rs`
- Modified: `duck-detect/src/lib.rs`
- Modified: `duck-detect/src/bin/duck-bench.rs`
- Modified: `duck-detect/Cargo.toml` (comment only; dependencies unchanged)
- Modified: `mediad/src/detect.rs`
- Modified: `mediad/src/main.rs`
- Modified: `robotd-params/src/lib.rs`
- Modified: `robotd-params/src/registry.rs`
- Modified: `deploy/robotd.toml` (documented options; defaults unchanged)
- Modified: `docs/project/spacemit-k1.md`
- Modified: `docs/project/k1-duck-quantization.md` (CPU-budget correction and SDK follow-up)
- Deleted: none. Generated experiment files are under the ignored artifact directories above.
