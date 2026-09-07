<a id="k1-duck-detector-xslim-quantization-and-spacemit-ep-measurements"></a>
# K1 鸭子检测器：XSlim 量化与 SpaceMIT EP 实测

[English](k1-duck-quantization.md) · [简体中文](k1-duck-quantization_zh.md)

> 本文保留带日期的实验与失败记录，不代表全部内容仍是当前待办。最新功能状态见[适配进度](spacemit-k1-adaptation-zh.md)，后续相机与浏览器链路见 [IMX219](k1-imx219_zh.md) / [WebRTC](k1-webrtc_zh.md)。翻译不改变原测试条件或把历史数据当成新一轮验收。

日期：2026-09-05。性质：**离线推理性能实验，不是 SDK 摄像头集成或精度验收**。

2026-09-06 核数口径更正：后续 SDK 板测发现 EP 2.0.6 会把调用线程重新绑定到 CPU 4–7，
可以突破初始 `taskset` 掩码。因此本文 EP 的“2/4 核”应读作 **2/4 个 EP 工作线程**，
不是 cgroup 限制下的总物理核数；原先的倍率不能作为严格同总核数加速的验收证据。
延迟原始记录保留。SDK 现已接入 Rust ORT + EP，使用硬 cpuset 的复测见
[SDK 集成与真实 CPU 预算](k1-duck-ort-ep_zh.md#k1-sdk-measurements--2026-09-06)。

<a id="conclusions"></a>
## 结论

- 原模型 320×320、单类别、输入 `images: float32[1,3,320,320]`，输出
  `output0: float32[1,5,2100]`。没有换成 COCO/640 模型。
- XSlim INT8 经最终输出修正后，K1 配置 4 个 EP 工作线程平均 **21.57 ms**，2 个为
  **32.77 ms**。相对 CPU 4/2 核基线的数值比分别约 22.3、21.7，但总 CPU 资源不等价，
  **不作为严格同核数加速比**。
- 速度不等于精度通过：12 帧诊断样本中，原模型输出 11 个框，最终 INT8 模型输出
  8 个对应框。无人工标注，不能把这些数字称作 mAP、真实召回率或准确率。
- 本次量化实验只生成 opt-in 产物，**没有替换 SDK 默认模型**；当时未接 Rust EP，后续
  SDK 接入另见上面的 2026-09-06 报告。
  当时的 SDK 集成候选还包括经 opset 转换后的浮点 EP 路径：4 个 EP 工作线程约
  106.59 ms，本组样本的 11 个框均能匹配，输出仍不与 CPU bit-identical。

<a id="environment-and-measurement-scope"></a>
## 环境与测量口径

- snode5：`source ~/.venvs/quant312/bin/activate`，Python 3.12.13，XSlim 2.0.14，
  PyTorch 2.11.0+cu130，CUDA 不可用。本次在 CPU 上校准，OMP/MKL/OpenBLAS 各设 4 线程。
- K1：riscv64，`spacemit-onnxruntime` / `python3-spacemit-ort` 均为 2.0.6-bpo1+1。
- 测速使用独立 C++ harness 调用 **原生** `/usr/lib/libonnxruntime.so.1.24.2+spacemit.a1`
  和 `libspacemit_ep.so.2`，与 Rust SDK 使用的原生运行时相同；不是 Python 包内的
  1.24.0+spacemit.a3 运行时，也没有把 SDK 改写成 C++。
- 初始亲和性分别为 `taskset -c 0-1` / `taskset -c 0-3`；ORT intra-op 与 EP 线程数
  分别显式设为 2/4，inter-op=1，图优化为 ALL。EP 工作线程 affinity 与初始掩码一致，
  但不是 cgroup 硬限制；后续调用线程会改绑，见开头的核数口径更正。
- 输入是同一份预先处理好的 12 帧 FP32 tensor，循环顺序不变，batch=1。
  主对比为预热 10 次、测量 60 次，串行运行各方案。
- 计时包含 tensor 包装、同步 `Session::Run` 和返回输出，不含视频解码、前处理、
  NMS、磁盘写入、Session 创建、模型加载和 profiling。profiling 在另建 Session 中单独执行。
- 记录时 CPU 频率为 1.6 GHz，温度约 40–43°C；没有修改 governor、频率或散热设置。
  没有并发运行机器人策略、摄像头、音频或舵机闭环，不能把纯推理 FPS 当作整机帧率。
- INT8 成功路径采用 EP 默认 FP16 epilogue；**不是全程 FP32 数学运算**。
  参考：[SpaceMIT ORT provider 选项](https://github.com/spacemit-com/docs-ai/blob/main/en/compute_stack/ai_compute_stack/onnxruntime.md)。

<a id="performance-results"></a>
## 性能结果

主对比均为同一输入、预热 10 次、测量 60 次；数值单位 ms。

| 模型与后端 | CPU 核 / EP 工作线程数 | 平均 | P50 | P95 |
|---|---:|---:|---:|---:|
| 原始 FP32 ONNX / CPU | 2 | 709.60 | 709.08 | 715.33 |
| 原始 FP32 ONNX / CPU | 4 | 480.65 | 479.58 | 520.80 |
| opset 17 + 图简化、未量化 / CPU | 4 | 457.91 | 460.48 | 491.77 |
| opset 17 + 图简化、未量化 / EP | 4 | 106.59 | 105.69 | 113.44 |
| INT8 + 最终浮点输出 / EP | 2 | 32.77 | 32.60 | 34.69 |
| INT8 + 最终浮点输出 / EP | 4 | 21.57 | 21.42 | 23.45 |

补充对照：

- 最终 INT8 / EP / 4 工作线程另测 120 次：平均 **21.70 ms**，P95 **23.68 ms**。
- 最终 INT8 / CPU / 4 核：平均 **1008.85 ms**，仅预热 3 次、测量 12 次，用于输出对照。
  **只量化、不启用 EP，在本板 CPU 后端反而更慢**；该行测量次数不同，不混入主对比。
- 原始模型的随机输入 `onnxruntime_perf_test` 初探为 4 核 456.50 ms/30 次。
  它与上表的真实帧输入不同，不用于计算加速比。
- 图简化 CPU 的约 5% 差异没有重复实验证明，不作为有效优化宣称。
- 成功的浮点 EP、INT8 EP 标准 ORT trace 均显示 **1 个 SpaceMITExecutionProvider
  融合节点被执行，没有 CPUExecutionProvider 计算节点**。这能证明实际进入 EP，
  不代表已经逐条确认每个内部算子使用了哪一种硬件指令。

<a id="calibration-data-and-preprocessing"></a>
## 校准数据与前处理

原训练仓库不可公开访问，没有使用其原始训练/验证集。采用
[Pollen 官方展示页](https://pollen-robotics.com/microduck/) 的实拍视频作代理数据：

- 校准：`balance-recovery.mp4` 24 帧 + `roller-skating.mp4` 24 帧，共 48 帧。
- 输出诊断：另一段 `grab-and-carry.mp4` 的 12 帧，不参与校准。
- 地址均在 `https://pollen-robotics.com/assets/microduck/gallery/` 下。
- 每段视频在 5%–95% 时间区间均匀抽帧，没有人工标注；样本很小、视角有限。
- 按 SDK `letterbox_rgb` 的规则做 RGB、整数最近邻缩放、114 补边，再转 NCHW FP32 /255。
  本实验不涉及 UYVY 摄像头入口、摄像头旋转或 ISP。
- 每帧源视频、帧号、原尺寸、letterbox 参数及源视频 SHA256 都在 `dataset-manifest.json`。

<a id="quantization-and-final-output-correction"></a>
## 量化与输出修正

XSlim 参数：precision_level=0（INT8）、finetune_level=1、CPU calibration、48 steps、
analysis_enable=true、opset=17。校准和量化耗时 **343.24 秒**。

直接导出的模型在最终 `/model.23/Concat_3` 之后还有一对 Q/DQ。该 Concat 将像素坐标
和 0–1 的类别概率放在同一 tensor，最终量化步长实测为 **1.3061963319778442**。
这会把置信度粗化到 0 或约 1.306，不能作为正常概率使用。

`keep-float-output.py` 仅删除最终两个节点 `PPQ_Operation_1359` 和 `PPQ_Operation_1360`，
让最终 Concat 直接输出浮点 `output0`。所有内部 INT8 权重、内部量化器、输入输出 shape
均保留；**不是把整个检测头恢复成 FP32，也不是换模型或修改阈值**。
原始量化文件与修正版分别保存，均通过 `onnx.checker.check_model`。

| 文件 | 字节数/性质 | SHA256 |
|---|---|---|
| `duck_detect.onnx` | 10,477,940；原始 FP32 | `2e34657a655a15221f93f6149730875274da4fb1b1c67dc883cde769fec35c30` |
| `duck_detect.slim.onnx` | 10,498,817；opset 17，未量化 | `e1e8444bc8fe9a53675f1c885b781705f8579ea17abe23bcd2b853d59c8f746f` |
| `duck_detect.int8.onnx` | 2,902,736；原始 XSlim 输出，不宜使用其置信度 | `22da5da57360337c830b2aeaf4c3315c0fa6c7d9fff73f96e6ecee6236133df5` |
| `duck_detect.int8-float-output.onnx` | 约 2.90 MB；最终实验候选 | `7d4c98643471432c8880a0e9543110b864649f49d608e26f42afb43795f0e3d3` |

<a id="output-differences-are-not-labelled-accuracy"></a>
## 输出差异，不等于标注集精度

统一置信度阈值 0.35、NMS IoU 0.5；与原模型框匹配时 IoU 阈值也是 0.5。

| 路径（4 核） | 12 帧总框数 | 与原模型匹配数 | 匹配框平均 IoU | 匹配框置信度平均绝对差 |
|---|---:|---:|---:|---:|
| 原模型 CPU | 11 | 11 | 1 | 0 |
| 未量化简化模型 CPU | 11 | 11 | 1 | 0 |
| 未量化简化模型 EP | 11 | 11 | 0.99977 | 0.00042 |
| 未修正 INT8 CPU | 7 | 7 | 0.90392 | 0.44016 |
| 未修正 INT8 EP，首轮 | 6 | 6 | 0.89997 | 0.42202 |
| 最终 INT8 CPU | 8 | 8 | 0.91704 | 0.09094 |
| 最终 INT8 EP | 8 | 8 | 0.91336 | 0.09535 |

- 未量化简化模型与原模型在本组 CPU 输出上 **bit-identical**。
- 最终 INT8、EP 输出都**不是 bit-identical**；所有保存输出均为有限值。
- 最终 INT8 在 CPU 上也少 3 个框，说明不能把全部差异归因于 EP。
- 同一最终 INT8 模型，EP vs CPU 的 score MAE 为 0.00019058、最大绝对差 0.13419；
  raw box-channel MAE 为 1.93382（模型空间）。小样本框数相同不等于两个运行时完全等价。
- 本次不调整阈值来追平框数。进一步使用前需要代表性校准集和带标注验证集。

<a id="retained-failures-and-limits"></a>
## 保留的失败与限制

1. 原始 opset 12 浮点模型直接走 EP，在 `/model.10/m/m.0/attn/Reshape_2` 编译失败。
   opset 17 转换与图简化后可运行，且上述 CPU 输出一致。
2. XSlim 默认把 opset 12 升至 24，但其 PPQ Resize socket 只支持 10–19，首次量化失败。
   通过本次配置指定 opset 17 解决，未修改虚拟环境或 xslim 安装文件。
3. INT8 使用 `SPACEMIT_EP_DISABLE_FLOAT16_EPILOGUE=1` 时触发 `std::bad_function_call`。
   GDB 抓到 EP 的 `SpineConvNDDispatchMMT4D` / `ComputeMMT4DBlockThreaded` 调用栈。
   成功数据来自 EP 默认 epilogue，不能宣称关闭 FP16 的严格路径已经可用。
4. 默认 2 核 EP 也曾一次异常退出；后续独立进程重试以及最终修正版 2 核测试完成。
   首次 4 核 INT8 成功运行曾有 core 0/2 TCM 分配失败警告，其均值为 25.01 ms；
   后续无该警告的重复及修正版数据约 21.6 ms。未对这些偶发问题做长稳定位。
5. 未修正 INT8 CPU 的 60 次计时、输出和 profile 均完成，但进程收尾触及 90 秒超时。
   最终修正版 CPU 的独立短对照干净退出，主性能数字采用 EP 干净退出的实验。
6. 一次误用 `spacemit-tcm-smi -c` 被当前 v1 后端拒绝；该版本的 `-c` 是强制释放，
   不是状态查询，工具返回 `current backend (v1) does not support force release`。
   没有成功执行 TCM 释放，没有继续操作此功能，也没有重启 K1 或修改驱动。

<a id="artifacts-and-reproduction"></a>
## 产物与复现

完整实验目录：

- snode5：`/data/home2/rongmingjun/WorkSpace/microduck-quant-20260905.EAJ2Qu/`
- K1：`/root/workspace/microduck/target/duck-quant-20260905.9Kv6aZ/`
- Mac：`target/duck-quant-20260905/`（Git ignored）

源脚本为 `prepare.py`、`xslim-int8.json`、`keep-float-output.py`、`bench.cpp`、`report.py`。
数值证据为 `*.summary.json`、`*.samples.csv`、`*.outputs.f32`、`*.profile_*.json`、
`comparison.json`、量化日志及 `duck_detect.int8_report.md`。
逐文件清单见本地实验目录的 `artifact-files.txt`。

在含原模型和上述三段视频的**新实验目录**准备校准及量化（脚本拒绝覆盖最终修正版）：

```sh
source ~/.venvs/quant312/bin/activate
python prepare.py
OMP_NUM_THREADS=4 MKL_NUM_THREADS=4 OPENBLAS_NUM_THREADS=4 \
  python -m xslim -c xslim-int8.json -i duck_detect.onnx -o duck_detect.int8.onnx
python keep-float-output.py
```

K1 原生编译与 4 工作线程测试（非总核数硬限制；CPU 对照换模型路径、去掉 env，并将 `ep` 换为 `cpu`）：

```sh
g++ -std=c++17 -O2 -Wall -Wextra bench.cpp -o bench -lonnxruntime -lspacemit_ep
ulimit -c 0
DUCK_BENCH_ALLOW_FP16_EPILOGUE=1 taskset -c 0-3 \
  ./bench duck_detect.int8-float-output.onnx eval-inputs.f32 ep 4 run 10 60
```

原模型出处：[官方 ONNX](https://github.com/pollen-robotics/microduck/blob/main/duck-detect/models/duck_detect.onnx)。
源 ONNX 内嵌元数据标注 AGPL-3.0；不能把 SDK 根目录的 Apache-2.0 自动当成模型授权。

本实验没有修改或覆盖原始模型、apt/Rust/Python 安装、服务配置及 SDK 默认行为。
