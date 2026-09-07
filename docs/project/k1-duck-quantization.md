<a id="k1-鸭子检测器xslim-量化与-spacemit-ep-实测"></a>
# K1 duck detector: XSlim quantization and SpaceMIT EP measurements

[English](k1-duck-quantization.md) · [简体中文](k1-duck-quantization_zh.md)

> Dated test results and known issues are recorded below. See [adaptation status](spacemit-k1-adaptation.md) for current functionality, and the [IMX219](k1-imx219.md) / [WebRTC](k1-webrtc.md) guides for the integrated camera workflows.

Date: 2026-09-05. **Offline inference experiment, not SDK camera integration or accuracy acceptance.**

CPU-budget correction, 2026-09-06: subsequent SDK board tests found EP 2.0.6 rebinding its caller
to CPUs 4–7, escaping the initial `taskset` mask. “2/4 cores” for EP in this report therefore means
**2/4 EP worker threads**, not a cgroup-enforced total physical CPU budget.
Previous ratios are not strict equal-core speedup evidence; original latencies are retained.
Rust ORT + EP is now integrated. Hard-cpuset reruns are in
[SDK integration and real CPU budgets](k1-duck-ort-ep.md#k1-sdk-measurements--2026-09-06).

<a id="结论"></a>
## Conclusions

- Original 320×320, single-class model: `images: float32[1,3,320,320]` input,
  `output0: float32[1,5,2100]` output. It was not replaced with COCO/640.
- After correcting final outputs, XSlim INT8 averaged **21.57 ms** with four EP workers,
  **32.77 ms** with two. Numerical ratios against four-/two-CPU baselines are about 22.3 / 21.7,
  but total CPU resources differ: **not strict equal-core speedups**.
- Speed is not accuracy: twelve diagnostic frames produced eleven original-model boxes versus
  eight corresponding final-INT8 boxes. Without labels, these are not mAP, true recall or accuracy.
- Quantization generated opt-in artifacts and **did not replace the default SDK model**.
  Rust EP was not integrated during this experiment; the subsequent report is dated 2026-09-06.
  The floating-point opset-converted EP candidate took about 106.59 ms with four EP workers,
  matching all eleven reference boxes in this sample, but not byte-identical to CPU outputs.

<a id="环境与测量口径"></a>
## Environment and measurement scope

- snode5: `source ~/.venvs/quant312/bin/activate`, Python 3.12.13, XSlim 2.0.14,
  PyTorch 2.11.0+cu130; CUDA unavailable. Calibration used CPU with four OMP/MKL/OpenBLAS threads each.
- K1 riscv64: `spacemit-onnxruntime` / `python3-spacemit-ort` both 2.0.6-bpo1+1.
- A standalone C++ harness called **native** `/usr/lib/libonnxruntime.so.1.24.2+spacemit.a1`
  and `libspacemit_ep.so.2`, the same native runtime used by Rust SDK.
  It did not use Python's 1.24.0+spacemit.a3 runtime or rewrite the SDK in C++.
- Initial affinity was `taskset -c 0-1` / `taskset -c 0-3`; ORT intra-op and EP worker counts
  explicitly 2/4, inter-op=1, graph optimization ALL.
  EP workers matched the initial mask, but no hard cgroup constrained the later caller rebinding.
- Identical preprocessed FP32 tensors from twelve frames, unchanged cyclic order, batch 1.
  Main comparison: ten warm-ups, sixty measurements, variants run serially.
- Timing covers tensor wrapping, synchronous `Session::Run` and returning outputs.
  It excludes decoding, preprocessing, NMS, disk writes, session creation, model loading and profiling.
  Profiling used a separate session.
- Recorded CPU frequency 1.6 GHz, temperature about 40–43°C. Governor, frequency and cooling were unchanged.
  No policies, camera, audio or servo feedback loop ran concurrently; inference FPS is not robot FPS.
- Successful INT8 uses EP's default FP16 epilogue; **not exclusively FP32 arithmetic**.
  Reference: [SpaceMIT ORT provider options](https://github.com/spacemit-com/docs-ai/blob/main/en/compute_stack/ai_compute_stack/onnxruntime.md).

<a id="性能结果"></a>
## Performance results

Main comparison: identical input, ten warm-ups and sixty measurements. Units: ms.

| Model/backend | CPU cores / EP workers | Mean | P50 | P95 |
| --- | ---: | ---: | ---: | ---: |
| Original FP32 ONNX / CPU | 2 | 709.60 | 709.08 | 715.33 |
| Original FP32 ONNX / CPU | 4 | 480.65 | 479.58 | 520.80 |
| Opset 17 + simplification, unquantized / CPU | 4 | 457.91 | 460.48 | 491.77 |
| Opset 17 + simplification, unquantized / EP | 4 | 106.59 | 105.69 | 113.44 |
| INT8 + final floating-point output / EP | 2 | 32.77 | 32.60 | 34.69 |
| INT8 + final floating-point output / EP | 4 | 21.57 | 21.42 | 23.45 |

Additional comparisons:

- Final INT8 / EP / four workers, 120 measurements: mean **21.70 ms**, P95 **23.68 ms**.
- Final INT8 / CPU / four CPUs: mean **1008.85 ms**, only three warm-ups and twelve measurements for output comparison.
  **Quantization without EP was slower on this CPU backend.** Its different sample count excludes it from the main table.
- Initial random-input `onnxruntime_perf_test` with the original model: four CPUs, 456.50 ms / 30 runs.
  Different inputs make it unsuitable for ratios against the real-frame table.
- The simplified CPU graph's roughly 5% difference lacks repeated evidence and is not claimed as meaningful optimization.
- Successful float and INT8 EP traces both show **one executed SpaceMITExecutionProvider fused node
  and no CPUExecutionProvider compute nodes**. This proves EP execution, not the hardware instruction used inside each operator.

<a id="校准数据与前处理"></a>
## Calibration data and preprocessing

The original training repository was not publicly accessible; its training/validation set was not used.
Proxy data came from real footage on [Pollen's official showcase](https://pollen-robotics.com/microduck/):

- Calibration: 24 frames each from `balance-recovery.mp4` and `roller-skating.mp4`, 48 total.
- Output diagnostics: twelve frames from separate `grab-and-carry.mp4`, excluded from calibration.
- URLs are under `https://pollen-robotics.com/assets/microduck/gallery/`.
- Frames are evenly sampled across 5%–95% of each video, without labels; the dataset is small and viewpoints limited.
- SDK `letterbox_rgb` rules: RGB, integer nearest-neighbor resize, padding 114, NCHW FP32 /255.
  No UYVY camera input, rotation or ISP is involved.
- `dataset-manifest.json` records source video/frame, original dimensions, letterbox parameters and video SHA256.

<a id="量化与输出修正"></a>
## Quantization and final-output correction

XSlim: precision_level=0 (INT8), finetune_level=1, CPU calibration, 48 steps,
analysis_enable=true, opset=17. Calibration/quantization took **343.24 seconds**.

The direct export adds a Q/DQ pair after final `/model.23/Concat_3`.
That Concat combines pixel coordinates and 0–1 class probabilities in one tensor.
Its measured final quantization step is **1.3061963319778442**, collapsing confidence to zero
or about 1.306, unsuitable as ordinary probabilities.

`keep-float-output.py` removes only final nodes `PPQ_Operation_1359` and `PPQ_Operation_1360`,
letting the final Concat directly produce floating-point `output0`.
All internal INT8 weights/quantizers and input/output shapes remain.
**It does not restore the whole detection head to FP32, change models or adjust thresholds.**
Original and corrected exports are retained separately; both pass `onnx.checker.check_model`.

| File | Bytes / purpose | SHA256 |
| --- | --- | --- |
| `duck_detect.onnx` | 10,477,940; original FP32 | `2e34657a655a15221f93f6149730875274da4fb1b1c67dc883cde769fec35c30` |
| `duck_detect.slim.onnx` | 10,498,817; opset 17, unquantized | `e1e8444bc8fe9a53675f1c885b781705f8579ea17abe23bcd2b853d59c8f746f` |
| `duck_detect.int8.onnx` | 2,902,736; raw XSlim output, unsuitable confidence | `22da5da57360337c830b2aeaf4c3315c0fa6c7d9fff73f96e6ecee6236133df5` |
| `duck_detect.int8-float-output.onnx` | About 2.90 MB; final experiment candidate | `7d4c98643471432c8880a0e9543110b864649f49d608e26f42afb43795f0e3d3` |

<a id="输出差异不等于标注集精度"></a>
## Output differences are not labelled accuracy

Confidence threshold 0.35, NMS IoU 0.5, and IoU 0.5 for matching reference-model boxes.

| Path (four CPUs/workers) | Total boxes, twelve frames | Reference matches | Mean matched IoU | Mean absolute confidence difference |
| --- | ---: | ---: | ---: | ---: |
| Original CPU | 11 | 11 | 1 | 0 |
| Unquantized simplified CPU | 11 | 11 | 1 | 0 |
| Unquantized simplified EP | 11 | 11 | 0.99977 | 0.00042 |
| Uncorrected INT8 CPU | 7 | 7 | 0.90392 | 0.44016 |
| Uncorrected INT8 EP, first run | 6 | 6 | 0.89997 | 0.42202 |
| Final INT8 CPU | 8 | 8 | 0.91704 | 0.09094 |
| Final INT8 EP | 8 | 8 | 0.91336 | 0.09535 |

- Unquantized simplified and original CPU outputs were **bit-identical** for this sample.
- Final INT8 and EP outputs are **not bit-identical**; all saved outputs were finite.
- Final INT8 also misses three boxes on CPU, so differences cannot all be attributed to EP.
- Final INT8 EP versus CPU: score MAE 0.00019058, maximum absolute difference 0.13419;
  raw box-channel MAE 1.93382 in model coordinates.
  Equal small-sample box counts do not establish runtime equivalence.
- Thresholds were not tuned to recover box counts. Representative calibration and labelled validation sets are needed before further use.

<a id="保留的失败与限制"></a>
## Retained failures and limits

1. Original opset 12 FP32 failed EP compilation at `/model.10/m/m.0/attn/Reshape_2`.
   Opset 17 conversion/simplification runs successfully, with matching CPU outputs above.
2. XSlim defaults to upgrading opset 12 to 24, but its PPQ Resize socket supports 10–19,
   causing initial quantization failure. Explicit opset 17 fixed it without editing the environment or installed XSlim.
3. INT8 with `SPACEMIT_EP_DISABLE_FLOAT16_EPILOGUE=1` throws `std::bad_function_call`.
   GDB captured `SpineConvNDDispatchMMT4D` / `ComputeMMT4DBlockThreaded`.
   Successful data uses default epilogues; the strict FP16-disabled path is not qualified.
4. Default two-worker EP also exited abnormally once; later independent retries and final corrected two-worker runs completed.
   The first successful four-worker INT8 run warned of TCM allocation failures on cores 0/2 and averaged 25.01 ms.
   Later warning-free repeats/corrected runs were about 21.6 ms. These intermittent issues were not endurance-debugged.
5. Uncorrected INT8 CPU completed sixty timed outputs and profiling, but cleanup reached a 90-second timeout.
   The final corrected CPU short comparison exited cleanly; main EP numbers use cleanly exiting runs.
6. One mistaken `spacemit-tcm-smi -c` invocation was rejected by the current v1 backend.
   Here `-c` means forced release, not status: `current backend (v1) does not support force release`.
   No TCM release succeeded, no further operation was attempted, and K1/drivers were not rebooted or changed.

<a id="产物与复现"></a>
## Artifacts and reproduction

Complete experiment directories:

- snode5: `/data/home2/rongmingjun/WorkSpace/microduck-quant-20260905.EAJ2Qu/`
- K1: `/root/workspace/microduck/target/duck-quant-20260905.9Kv6aZ/`
- Mac: `target/duck-quant-20260905/` (Git ignored)

Sources: `prepare.py`, `xslim-int8.json`, `keep-float-output.py`, `bench.cpp` and `report.py`.
Evidence: `*.summary.json`, `*.samples.csv`, `*.outputs.f32`, `*.profile_*.json`,
`comparison.json`, quantization logs and `duck_detect.int8_report.md`.
The local experiment directory's `artifact-files.txt` lists each artifact.

In a **new experiment directory** with the original model and three videos
(the scripts refuse to overwrite the final corrected model):

```sh
source ~/.venvs/quant312/bin/activate
python prepare.py
OMP_NUM_THREADS=4 MKL_NUM_THREADS=4 OPENBLAS_NUM_THREADS=4 \
  python -m xslim -c xslim-int8.json -i duck_detect.onnx -o duck_detect.int8.onnx
python keep-float-output.py
```

Native K1 build and four-worker run (not a hard total-CPU limit).
For CPU comparison, change the model path, remove the environment override and replace `ep` with `cpu`:

```sh
g++ -std=c++17 -O2 -Wall -Wextra bench.cpp -o bench -lonnxruntime -lspacemit_ep
ulimit -c 0
DUCK_BENCH_ALLOW_FP16_EPILOGUE=1 taskset -c 0-3 \
  ./bench duck_detect.int8-float-output.onnx eval-inputs.f32 ep 4 run 10 60
```

Original source: [official ONNX](https://github.com/pollen-robotics/microduck/blob/main/duck-detect/models/duck_detect.onnx).
ONNX metadata states AGPL-3.0; the SDK root Apache-2.0 does not automatically license the weights.

The experiment did not modify/overwrite the original model, apt/Rust/Python installation,
service configuration or SDK defaults.
