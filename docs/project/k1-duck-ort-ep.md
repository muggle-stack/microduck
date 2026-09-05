# K1 duck detector: Rust ORT + SpaceMIT EP

The SDK can select SpaceMIT EP through its existing Rust `ort` binding. No Python process,
new model protocol, or duplicated preprocessing/postprocessing is needed. The original RKNN
path and CPU/two-thread ONNX default remain available. This is **opt-in**, not a provisioned
camera service or a claim of complete K1 hardware integration.

## Model and native runtime

Use the **floating-point opset 17** `duck_detect.slim.onnx` from the
[quantization experiment](k1-duck-quantization.md) first. Its SHA-256 is
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
the session drops. Native `OrtStatus` errors propagate, and CPU EP fallback is disabled when
SpaceMIT is explicitly selected. Actual provider execution must still be checked in a profile.

The EP also needs ORT's **C++ symbols in the global loader scope**. The initial board probe
failed with `undefined symbol: _ZTIN11onnxruntime18IExecutionProviderE`: EP 2.0.6 does not declare
`libonnxruntime` in its `DT_NEEDED` list, and Rust `ort` loads libraries with `RTLD_LOCAL`.
The adapter verifies the ORT API pointer identity, then promotes that same native ORT mapping
with `RTLD_NOW | RTLD_GLOBAL` before opening the EP. This matches the symbol visibility of the
working C++ executable without introducing a link-time dependency or mixing Python's runtime.

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
device. Example with four cores (a benchmark choice, not the SDK's two-thread default):

```sh
taskset -c 0-3 target/riscv64gc-unknown-linux-gnu/release/duck-bench \
  --model target/duck-quant-20260905.9Kv6aZ/duck_detect.slim.onnx \
  --frames /path/to/eval-jpegs --onnx-provider spacemit \
  --threads 4 --spacemit-affinity '0;1;2;3' \
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

For a fair CPU comparison use the same slim model, frames, core affinity, thread count, warmup
and passes with `--onnx-provider cpu`. Use distinct output/profile paths for every run.

## Configuration consumed by the daemon and editor

After offline verification, merge these entries into the existing `[detect]` section of a
per-robot config; do not replace the rest of the file or create a duplicate table:

```toml
[detect]
enabled = true
model = "/absolute/path/to/duck_detect.slim.onnx"
onnx_provider = "spacemit"
onnx_threads = 4
spacemit_affinity = "0;1;2;3"
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

## Regression commands

```sh
cargo test --locked --release --target riscv64gc-unknown-linux-gnu \
  -p duck-detect -p robotd-params -p mediad -j 2 -- --test-threads=2
cargo k1 --locked -p mediad --bin mediad -j 2
```

No services need to be installed or restarted for these checks. The shared schema tests cover
the editor's choices/completeness and preserve shipped defaults; Linux mediad tests cover the
configuration-to-detector mapping and the unchanged sighting notification.
