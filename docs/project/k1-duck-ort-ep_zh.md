<a id="k1-duck-detector-rust-ort--spacemit-ep"></a>
# K1 鸭子检测器：Rust ORT + SpaceMIT EP

[English](k1-duck-ort-ep.md) · [简体中文](k1-duck-ort-ep_zh.md)

> 本文按测试日期记录实验结果与已知问题。当前功能状态见[适配进度](spacemit-k1-adaptation-zh.md)，已集成的相机使用方法见 [IMX219](k1-imx219_zh.md) / [WebRTC](k1-webrtc_zh.md) 指南。

SDK 可通过已有 Rust `ort` 绑定选择 SpaceMIT EP，无需 Python 进程、新模型协议或重复前后处理。
原 RKNN 路径和 CPU/双线程 ONNX 默认项保留。
这是**显式启用**的能力，不表示已部署相机服务或已完成全部 K1 硬件适配。

<a id="model-and-native-runtime"></a>
## 模型与原生运行时

使用 [models-duck-detect-v1 Release](https://github.com/muggle-stack/microduck/releases/tag/models-duck-detect-v1)
中的**浮点 opset 17** `duck_detect.slim.onnx`。这个独立模型预发布包含 manifest、
原始计算图/转换方法与模型许可声明，不是 SDK/OTA 发布，git clone 不会下载它。
[量化实验](k1-duck-quantization_zh.md)记录来源与精度边界。SHA-256：
`e1e8444bc8fe9a53675f1c885b781705f8579ea17abe23bcd2b853d59c8f746f`。

原 `duck-detect/models/duck_detect.onnx` 不变：EP 2.0.6 无法编译其 opset 12 attention reshape。
通用 YOLO11 COCO 模型不能替代；此解码器要求名为 `images` 的单个 f32 输入、
固定 `[1,3,H,W]`，以及单个 f32 `[1,5,N]` 输出。

已测 K1 原生库为 `/usr/lib/libonnxruntime.so`（`1.24.2+spacemit.a1`）
与 `/usr/lib/libspacemit_ep.so`（`2.0.6`）。
设置 `ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so`，也可用 `SPACEMIT_EP_DYLIB_PATH`
指定可信 EP 库的绝对路径。使用匹配的原生库，不混用 Python site-packages 中不同的 ORT。
apt Rust 和 Rust `ort` crate 版本均未改变。

`duck-detect/src/spacemit.rs` 按板上 `spacemit_ort_env_c_api.h` 的 ABI 动态加载
`OrtSessionOptionsSpaceMITEnvInit`。只在此检测器的 session options 注册，robotd 策略 session 不受影响。
库的存活期长于 session。原生 `OrtStatus` 错误向上传播，注册/模型加载失败不重试 CPU-only。
必须通过 profile 检查实际 provider 执行。

EP 2.0.6 初始化器会显式同时加入 CPU EP。设置 ORT `session.disable_cpu_ep_fallback=1`
与此冲突并在板上被拒绝，因此适配层**不设置它**。
仍可能发生逐节点 CPU fallback，甚至整图分配给 CPU；仅注册日志不能证明加速。
每个新模型/运行时组合都用 `duck-bench --profile-prefix` 检查。守护程序会记录此 fallback 策略。

EP 还需要 ORT 的 **C++ 符号进入全局动态加载作用域**。首次板测报
`undefined symbol: _ZTIN11onnxruntime18IExecutionProviderE`：
EP 2.0.6 的 `DT_NEEDED` 未声明 `libonnxruntime`，Rust `ort` 又以 `RTLD_LOCAL` 加载。
适配层先核对 ORT API 指针身份，再以 `RTLD_NOW | RTLD_GLOBAL` 提升同一原生 ORT 映射，
然后打开 EP。这与可工作的 C++ 程序符号可见性一致，不新增链接期依赖或混用 Python 运行时。

<a id="offline-verification-before-enabling-mediad"></a>
## 启用 mediad 前先离线验证

使用独立 Rust 1.89 原生构建：

```sh
cd /root/workspace/microduck
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
export ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so
cargo k1 --locked -p duck-detect --bin duck-bench -j 2
```

当时 K1 的实验模型位于 `target/duck-quant-20260905.9Kv6aZ/`。
提供包含 JPEG 帧的目录；基准不会打开相机、串口或音频设备。
下面使用严格**四核预算**：三个 EP worker 加一个调用线程 CPU。

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

基准保留 RGB letterbox、HWC-to-NCHW `/255`、单类解码和 NMS。
分别报告推理＋解码（含 NCHW 转换）及 letterbox＋推理＋解码，不含 JPEG 解码、相机采集或节流等待。
计时后按排序后的 JPEG 顺序保存小端 f32 原始输出，拒绝覆盖已有文件。
Profiling 在**释放计时 session 后的独立 session**执行一帧，不污染计时，也不并发两个 EP 线程池。
Profile 应显示 `SpaceMITExecutionProvider` 执行融合节点，而非只有注册日志。

公平 CPU 对照使用相同 slim 模型、帧、硬 CPU 集合、预热与轮数，
改为 `--onnx-provider cpu --threads 4` 并去掉 EP affinity。
总 CPU 预算相等，但 EP worker 数刻意不同，因为调用线程也占一个 CPU。
每轮用独立输出/profile 路径。`systemd-run --scope --collect` 只创建退出自动回收的临时基准 scope，
不会安装服务。

<a id="k1-ep-caller-affinity-is-part-of-the-cpu-budget"></a>
### EP 调用线程亲和性也计入 CPU 预算

EP 2.0.6 的 `/proc/PID/task/TID/status` 显示：推理开始后调用线程从初始 `taskset 0-1`
移到 **CPU 4-7**，两个 EP worker 仍在 0、1。
仅 taskset 不是全进程硬预算。失败探测与成功数据一并保留：

- 硬 cpuset 0-3：推理报 `Spine Executor Set thread affinity error`。
- 硬 cpuset 4-7，worker ID `4;5;6;7`：session 创建拒绝 affinity ID 4。
- 硬 cpuset **0-2,4**，worker **0;1;2**：全部线程受限，调用线程在 4，成功。
- 硬 cpuset **0,4**，单 worker **0**：全部受限，调用线程在 4，成功。

因此该运行时使用 0-3 中的 worker ID，调用线程掩码位于 4-7。
不要把 `onnx_threads` 当成总 CPU 占用，或设置排除调用线程的服务 cpuset。
严格预算需要 cgroup `AllowedCPUs`。守护程序的 GStreamer 线程也共享其服务预算；
此处未测试实时视频/控制争用。
[厂商线程选项文档](https://github.com/spacemit-com/docs-ai/blob/main/en/compute_stack/ai_compute_stack/onnxruntime.md#provider-option-reference)
也区分 EP 池和 ORT intra-op 池。

<a id="configuration-consumed-by-the-daemon-and-editor"></a>
## 守护程序与配置编辑器使用的参数

离线验证后，把以下项合并进机器人现有 `[detect]` section，
不要替换文件其他内容或重复建表：

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

所有新字段均进入共享参数 schema 和 `robotctl configure` 注册表。
旧配置仍表示 `onnx_provider = "cpu"`、`onnx_threads = 2`、affinity `"auto"`、FP16 epilogue 关闭。
SpaceMIT 必须显式提供 `.onnx` 模型，不自动选择不兼容的随附模型。
线程数和 affinity 格式会校验，不隐式量化或替换模型。
`mediad` 把参数传给与基准相同的 `duck-detect` 模型，保留原 `media.detections` 通知。
示例 worker 配置可与 0-2,4 服务 CPU 预算配合，但 TOML 本身不设置 cgroup 或预留调用线程。

<a id="k1-sdk-measurements--2026-09-06"></a>
## K1 SDK 实测——2026-09-06

Rust 1.89 原生 release，检测实现 `d5b42cd`。
每组同样十二张 JPEG、十次预热、六十次计时（五轮），置信度 0.35、NMS 0.5。
计时无编译/机器人服务并发；每组独立自动回收 cgroup，记录有效 cpuset 和所有线程掩码。
Profiling 在计时 session 外执行。

下表包含 **SDK RGB letterbox＋NCHW 转换＋推理＋输出校验＋解码/NMS**，
不含 JPEG 解码、camera/ISP、UYVY 转换、WebRTC 或节流等待。

| 后端/模型 | 硬 CPU 集合（预算） | ORT / EP worker | 平均 ms | P95 ms |
| --- | --- | --- | ---: | ---: |
| 原 ONNX / CPU | 0,4（2 核） | 2 / — | 732.114 | 734.768 |
| 浮点 opset 17 / EP | 0,4（2 核） | 1 / 1 + caller | 242.995 | 254.051 |
| 原 ONNX / CPU | 0-2,4（4 核） | 4 / — | 457.038 | 486.367 |
| 浮点 opset 17 / CPU | 0-2,4（4 核） | 4 / — | 454.090 | 485.994 |
| 浮点 opset 17 / EP | 0-2,4（4 核） | 3 / 3 + caller | 121.293 | 129.989 |
| 实验 INT8 / EP | 0-2,4（4 核） | 3 / 3 + caller | 33.851 | 35.779 |

浮点 EP 即使两核也满足 **500 ms / 2 Hz 离线预算**。
四核同模型 EP/CPU 约 **3.74×**；CPU 图简化仅约 3 ms 差异，不是有效优化。
INT8 更快，但模型不同且未经精度验收。
成功 EP profile 均为**一个 SpaceMITExecutionProvider 融合节点执行、零 CPUExecutionProvider 计算节点**。
这证明 provider 执行，不证明每个内部操作用了哪种硬件指令。

<a id="output-agreement-not-labelled-accuracy"></a>
### 输出一致性，不是带标注精度

- 四核组原模型与 slim CPU 输出**逐字节一致**，均十个框。
- 浮点 EP 十一个框：十个基准框全部匹配（平均 IoU 0.999716，匹配置信度差 0.000604），
  另一个跨过阈值。第 2 帧候选 1954 从 **0.34894353 到 0.35001078**，
  跨过未修改的 0.35 阈值，因此不是 bit-identical。
- INT8 九个框：八个匹配，缺两个基准框，多一个框；平均匹配 IoU 0.903745，
  置信度差 0.081226。继续保留实验/显式启用状态。
- JPEG 解码输入与早期 C++ 实验直接取视频 tensor 的像素不一致，
  不可把其十一个框基准混入本次十个框基准。

更早仅 taskset 的 SDK 探索结果：浮点 EP 四 worker 112.872 ms、两 worker 145.964 ms，
INT8 四 worker 29.865 ms。原始记录保留，**不当成严格四/两物理核结果**，
因为调用线程逃出初始掩码。C++ 报告的 worker 数也不能证明总 CPU 预算相同。

计时日志早于 `duck-bench` 的报告修正：原无节流 CPU 百分比公式总输出 100%。
CPU-ms/frame、延迟、原始输出与 profile 不受影响；当前代码改用实测墙钟吞吐计算，并有测试。

产物（不把模型二进制加入 Git）：

- K1：`/root/workspace/microduck/target/duck-ort-ep-20260905.p05HnR/`。
- 主机：`target/duck-ort-ep-20260905.6nnWCR/`，板端结果在 `k1-results/`。
- `run-benchmarks.sh`、`run-bounded.sh`、`probe-affinity.sh`、`compare.py` 可复现探测。
- `bounded-comparison.json`、`*.outputs.f32`、`*.profile_*.json`、`*.affinity.txt`、`*.log`
  保存数值证据。SDK 集成未重做校准或模型转换。

<a id="precision-and-remaining-limits"></a>
## 精度与剩余限制

- 初始 EP 候选使用浮点模型。早期 C++ 数据见量化报告，不能当 Rust SDK 或完整相机延迟。
- `duck_detect.int8-float-output.onnx` 仍为实验：此前十二帧保留基准十一个框中的八个，
  只是输出一致性计数，不是标注精度/mAP。INT8 不是默认，也未通过精度验收。
- EP 2.0.6 INT8 需显式 `spacemit_allow_fp16_epilogue = true`
  （基准 `--spacemit-allow-fp16-epilogue`）。禁用时曾抛 `std::bad_function_call` 并 abort。
  原生 `SIGABRT` 不能被 Rust `Result` 捕获；适配不提供进程隔离或保证崩溃恢复。
- false 默认发送 `SPACEMIT_EP_DISABLE_FLOAT16_EPILOGUE=1`，true 则允许 provider 默认选择。
  两者均不保证相对 CPU 逐字节一致，也不控制 provider 所有内部精度决策。
- 本次离线检查之外仍需 camera/ISP、WebRTC 编码、实时帧与 50 Hz 真实硬件控制联调。
  离线检测结果不是整机验收。

<a id="regression-commands"></a>
## 回归命令

```sh
cargo test --locked --release --target riscv64gc-unknown-linux-gnu \
  -p duck-detect -p robotd-params -p mediad -j 2 -- --test-threads=2
cargo k1 --locked -p mediad --bin mediad -j 2
```

这些检查无需安装/重启服务。共享 schema 测试覆盖编辑器选项/完整性并保留默认值；
Linux mediad 测试覆盖配置到检测器的映射和不变的 sighting 通知。

`7d8b00a` 版本上，K1 原生 release 三个指定 crate 回归 **155 通过、零失败、零忽略**。
macOS 全 workspace **1,223 通过、零失败、六个已有 timing/visual probe 忽略**。
主机 `duck-detect`、`robotd-params`、`mediad` 全 target Clippy `-D warnings` 和格式检查通过。
这些不是 K1 全 workspace 或 aarch64 Linux 实物回归声明。

最终原生生产版 `duck-bench`、`mediad` 构建通过，隔离 `DUCK_RUNTIME_DIR` 的 `mediad --help` 通过。
报告修正版最后四核 EP 短测通过：十二帧，RGB 路径均值 120.513 ms，
独立 profile 再次只有一个 SpaceMIT 节点、无 CPU 计算节点。
此短测不替代表中六十次计时。基准进程/临时 scope 均已退出；
未启用机器人/相机/音频服务，未修改安装配置或运行时包，也未替换原模型。

<a id="files-changed-for-this-integration"></a>
## 此次集成的文件清单

- 新建：`duck-detect/src/spacemit.rs`
- 新建：`docs/project/k1-duck-ort-ep.md`
- 修改：`duck-detect/src/onnx.rs`
- 修改：`duck-detect/src/lib.rs`
- 修改：`duck-detect/src/bin/duck-bench.rs`
- 修改：`duck-detect/Cargo.toml`（只改注释，依赖不变）
- 修改：`mediad/src/detect.rs`
- 修改：`mediad/src/main.rs`
- 修改：`robotd-params/src/lib.rs`
- 修改：`robotd-params/src/registry.rs`
- 修改：`deploy/robotd.toml`（参数说明，默认不变）
- 修改：`docs/project/spacemit-k1.md`
- 修改：`docs/project/k1-duck-quantization.md`（CPU 预算更正与 SDK 后续）
- 删除：无。生成实验文件保留在上述 ignored 目录。
