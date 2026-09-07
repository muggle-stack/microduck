<a id="k1--muse-pi-pro-live-imx219-capture-and-ort--ep"></a>
# K1 / MUSE-Pi-Pro：IMX219 实时采集与 ORT＋EP

[English](k1-imx219.md) · [简体中文](k1-imx219_zh.md)

新增的 `camera.backend = "spacemit_csi"` 是显式启用的 K1 CSI 输入。
它复用 SDK 的取帧、图像前后处理和 ORT / SpaceMIT EP，不修改原 Rockchip / RKNN 默认路径，
也不替换已有 USB 单眼方案。本页的无头开发测试不启动舵机、音频、编码器或 WebRTC；
后续浏览器图传和同源推理的使用与验收见 [K1 WebRTC](k1-webrtc_zh.md)。

<a id="pipeline-and-supported-hardware"></a>
## 链路与适用硬件

```text
IMX219（CSI3，sensor_id=2）
  → K1 sensor / ISP / CPP（spacemitsrc，1280×720 NV12，30 fps）
  → videoconvert（UYVY / BT.601）→ SDK 最新帧
  → 原有 RGB letterbox / NCHW → Rust ORT＋SpaceMIT EP
  → 原有单类鸭子检测解码 / NMS / media.detections
```

本次板卡是 **MUSE-Pi-Pro / K1，8 GB，Bianbu 2.3.5，内核 6.6.63 #2.2.9.2**。
原生 GStreamer 1.24.2，`k1x-cam` 0.2.34、`k1x-cam-lib` 0.1.8、MPP 0.1.6~bpo2+1；
sensor 名称 `imx219_spm`，工作模式 0：1920×1080 RAW10、2 lane、30 fps。
720p 是 ISP 输出，不是宣称 sensor 原生模式为 720p。其他接口/镜像/模组需重新验证。

ISP 继续由系统相机栈管理，不调用 RKISP / RKAIQ，也不改内核、设备树或系统相机配置。
`close-dmabuf=true` 让现有 CPU 取帧路径能够映射数据，**不是端到端零拷贝**。
颜色转换使用 GStreamer，CSI 不加载 USB 专用的 MPP/OpenCV 桥接库；
`camera.acceleration` 保持默认 `software`，这个字段仅控制 USB 桥接，不表示 ISP 没有硬件处理。

<a id="build-and-configuration"></a>
## 编译和配置

Rust / 系统依赖的首次安装见 [K1 安装指南](../robot/install-k1_zh.md)。已有构建缓存时：

```sh
cd ~/workspace/microduck
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
export CARGO_TARGET_DIR="$PWD/target"
export ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so
cargo k1 --locked --bin camera-check --bin mediad -j 2
```

这只更新两个程序，不需要再执行整个 `k1-test.sh`。没有加 `-p mediad` 是为了沿用首次
`cargo k1 --bins` 的 workspace 依赖 feature 集合，减少切换 feature 引起的重复编译。

先确认 `gst-inspect-1.0 spacemitsrc` 能找到系统插件，再检查以下两个文件：

- [camera-imx219.toml](../../deploy/k1/camera-imx219.toml)：SDK 选择 CSI、NV12、720p30、旋转 0；
  默认 `detect.enabled=false`，不自动开启推理。
- [imx219-csi3-720p.json](../../deploy/k1/imx219-csi3-720p.json)：vendor sensor / ISP profile，
  CSI3 / sensor 2，禁用自动探测、tuning server 和隐式帧保存。

TOML 的 `camera.isp_config` 和 `detect.model` 是**绝对路径**；如果不是 root 用户或仓库不在
`/root/workspace/microduck`，需修改。不要把别的 CSI 接口直接当作 CSI3。
SDK 在开相机前检查 JSON 的输出尺寸、帧率、单路 ISP/CPP 和在线输入；不匹配时报错，
不自动换 sensor 或回退 USB / Rockchip。新后端当前只支持 NV12 单目，不接受 SBS / ROI 配置。

<a id="user-verification-commands"></a>
## 用户验证命令

确认没有其他程序占用相机后，先只取帧：

```sh
timeout --signal=TERM --kill-after=5s 30s \
  target/riscv64gc-unknown-linux-gnu/release/camera-check \
  --config deploy/k1/camera-imx219.toml --warmup 5 --frames 120
```

然后运行检测。模型须已放到示例路径；没有模型时先停在采集验证即可。

```sh
export ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so
systemd-run --scope --quiet --collect --property=AllowedCPUs=0-2,4 \
  taskset -c 0-2,4 timeout --signal=TERM --kill-after=5s 60s \
  target/riscv64gc-unknown-linux-gnu/release/camera-check \
  --config deploy/k1/camera-imx219.toml \
  --detect --model /root/.local/share/microduck/models/duck_detect.slim.onnx \
  --warmup 5 --frames 60 --hz 2
```

这里的 scope 需要相应 systemd 权限；示例实测在 root shell 执行。
`--detect --model` 显式启用工具中的检测，不要求修改默认关闭推理的配置文件。
硬限制为 **4 个 CPU：0、1、2、4**，3 个 EP worker 使用 `0;1;2`，调用线程在 CPU 4。
只有 `taskset` 不能限制此 EP 的全部线程，TOML 也不会自动创建 cgroup。

每帧输出检测通知，最后输出 `event=summary`。耗时 `inference_mean_ms` 包含
前处理、推理和 NMS，不含取帧和 ISP 处理，也不是浏览器延迟；`--hz 2` 是消费频率，
不是将 sensor 帧率改成 2 fps。工具使用容量为 1 的最新帧队列，慢消费者会丢弃旧帧。
vendor 相机库还会直接向标准输出写诊断信息，可能打断 JSON 行。
需要机器读取时增加 `--report /absolute/path/to/new-report.jsonl`：SDK 事件单独写到新文件，
vendor 日志仍留在标准输出/错误输出，不做全进程文件描述符重定向，也不覆盖已有报告。

需要验证 EP 实际执行时，在一次**独立的短测**中增加
`--profile /absolute/path/to/new-profile`；检查 ORT profile 中的
`SpaceMITExecutionProvider` 节点，而不是只看注册成功日志。不要用开启 profiling 的数据当性能基准。
需要保存一帧时增加 `--dump-dir /absolute/path/to/new-directory`；目录必须不存在，输出
`mono.uyvy` 和 `frame.json`，不会覆盖旧图像。

<a id="model-and-accuracy-boundaries"></a>
## 模型和精度边界

使用浮点 opset 17 的 **`duck_detect.slim.onnx`**，10,498,817 字节，SHA256：

```text
e1e8444bc8fe9a53675f1c885b781705f8579ea17abe23bcd2b853d59c8f746f
```

输入 `images: float32[1,3,320,320]`，输出 `output0: float32[1,5,2100]`。
这是原鸭子模型的兼容导出，不是通用 COCO YOLO11；没有改成 INT8，也没有开启 FP16 epilogue。
沿用阈值 0.35、NMS 0.5 及 SDK 的 RGB letterbox。Native ORT 为
`/usr/lib/libonnxruntime.so`（1.24.2+spacemit.a1），SpaceMIT EP 2.0.6。
安装的是原生 `spacemit-onnxruntime`，不是借用 Python wheel 的动态库。

**模型已于 2026-09-07 发布为独立的 [models-duck-detect-v1 预发布资产](https://github.com/muggle-stack/microduck/releases/tag/models-duck-detect-v1)。**
[固定版本模型下载](https://github.com/muggle-stack/microduck/releases/download/models-duck-detect-v1/duck_detect.slim.onnx)，
下载后核对上述 SHA256，再将相机配置中的 `detect.model` 指向本机绝对路径。
普通 git clone 仍不下载模型；不要使用 `releases/latest`，该模型不是 SDK 固件或 OTA 发布。

Release 同时提供 `SHA256SUMS`、`model-manifest.json`、`MODEL_CARD.md`、许可正文和
`duck-detect-source-v1.tar.gz`（原始 ONNX 与转换脚本）。发布前已重新执行转换，输出 SHA256
与本节模型完全一致。原始及导出 ONNX 的 metadata 都保留 Ultralytics AGPL-3.0 许可字段；
SDK 的 Apache-2.0 不是模型权重再许可，下游需按模型许可评估自身使用。
开发板上的稳定位置仍为 `/root/.local/share/microduck/models/duck_detect.slim.onnx`。
历史实验及精度边界见 [模型实验](k1-duck-quantization_zh.md)。

原始 opset 12 ONNX 不兼容本次 EP 的 attention reshape；不能通过改文件名代替转换。
EP 注册/加载失败会报错，不另开一个 CPU-only session；但原生运行时仍可能做逐节点 CPU fallback。
新 runtime/model 组合必须重新看 profile。参考 [ORT / EP 适配说明](k1-duck-ort-ep_zh.md)。

<a id="verification-record-2026-09-07"></a>
## 2026-09-07 验证记录

- Mac：`cargo test --locked -p mediad -p robotd-params --lib`，32＋98 项通过；
  Linux GStreamer / CSI 代码不由 Mac 验证代替。
- MUSE-Pi-Pro：Rust 1.89，`cargo test --locked --release --workspace --lib
  --target riscv64gc-unknown-linux-gnu -j 2 -- --test-threads=2`，**719 项通过，2 项外设测试默认忽略**。
  这是 library 单元回归，不是再跑全套二进制、集成测试和 `k1-test.sh`。
- 再显式执行 `camera::csi::tests::repeated_open_read_drop`：同一进程中
  **连续 3 次打开、每次读取 10 帧、关闭**，通过；每轮 PTS 递增。
  通过环境变量 `MICRODUCK_K1_CSI_TEST_CONFIG` 指定 profile，普通 CI 不打开相机。
- 新 `camera-check` 的负向检查：已有报告文件、缺失模型、单目配置配 `--both-eyes` 均以
  exit 1 拒绝，未启动 sensor；已有报告文件的 SHA256 保持不变。
- 先行的实时 EP 短测 profile 显示 4 次 `SpineSubgraph` 执行均归于
  `SpaceMITExecutionProvider`，未记录 CPU 节点执行。该轮含编译并发与 profiling，**不作为性能结果**。

最终 release 构建的两轮板测如下。均无编译器、运动、音频、编码或 WebRTC 并发；
硬 cgroup CPU 集合 `0-2,4`，同一 720p30 profile、旋转 0、浮点模型和阈值，未开 profiling。

| 检查 | 预热 / 测量 | 第一轮 | 第二轮 |
| --- | --- | --- | --- |
| 单独采集，消费帧率 | 5 / 120 帧 | 26.72 fps | 26.74 fps |
| 实时检测，平均前处理＋推理＋NMS | 5 / 60 次，按 2 Hz 消费 | 248.94 ms | 237.55 ms |
| 实时检测 P95 | 同上 | 259.21 ms | 247.90 ms |
| 检测所取帧 PTS 跨度频率 | 同上 | 1.9979 Hz | 1.9980 Hz |

两轮都满足当前 2 Hz 消费目标，PTS 无倒退；**采集实测没有达到配置的 30 fps**，
也不将此跨板/跨相机的数字解释为相对旧 K1 USB 路径的加速。18 个采样到的用户态线程
均受该 4-CPU cgroup 限制，3 个 EP worker 分别在 0、1、2，调用线程在 4。
最终构建另做独立短 profile，同样记录到 4 次 SpaceMIT 子图执行、未记录 CPU 节点执行。
两轮各 60 个报告均为 1280×720、1,843,200 字节 UYVY，通知格式保持 `media.detections`。
该轮场景未检出鸭子；这里只证明实时数据链路和执行后端，不是带标注的识别精度验收。
该轮保存的实拍帧可辨认天花板与灯具，但明显偏青、亮部过曝；保留此早期结果作为
曝光/白平衡及色彩链路的后续专项回归依据。该测试未使用额外滤镜，模型精度保持不变。

原始日志保存在本次板端目录
`/root/workspace/microduck/target/imx219-sdk-check.HjPwiy/`。
保留的负面结果：vendor 库打印 `open /dev/ion failed!`；重开测试的关闭阶段偶发
CPP `invalid state (1) for evt (6)` / `cam_cpp_post_buffer ... failed`。
本次仍能结束并重新出帧，但这不是“底层零错误”或长期稳定性已通过。
系统 vendor 库保持不变。

<a id="subsequent-user-acceptance-2026-09-07"></a>
<a id="后续用户实测确认2026-09-07"></a>
<a id="browser-video-and-detection-2026-09-07"></a>
### 浏览器图传与检测（2026-09-07）

指定 MUSE-Pi-Pro / CSI3 IMX219 已打通采集、浏览器视频和同源检测链路。
[WebRTC 测试记录](k1-webrtc_zh.md)包含 Mac 浏览器解码、检测通知、重连与关闭结果。

这些功能检查未覆盖长时间运行和完整光照测试矩阵；早期偏青/过曝现象与 vendor 告警仍保留跟踪。
带标注的检测精度、量化长稳和整机并发需单独测试。

<a id="completion-boundary"></a>
## 完成边界

实时采集和检测已经接入 SDK；`mediad` 的 K1 H.264 / WebRTC 路径也已完成单观众视频、
DataChannel 和同源 ORT / EP 检测的集成与实测。浏览器图传的依据是独立的
[WebRTC 测试记录](k1-webrtc_zh.md)，与独立 `camera-check` 测试分别记录。
另有显式启用的 SDK [`camera-rtsp` 工具](k1-rtsp_zh.md)，将 ISP 的 NV12 DMA-BUF 接入
K1 硬编码和 RTSP/TCP，可在电脑播放器实时预览；它不加载检测器，也不等同于 WebRTC。
vendor ISP 管理 AE/AWB；SDK 不复用 Rockchip 手动曝光控制。
不同光照下的曝光/PQ、量化长稳与异常恢复、带标注的检测精度，以及和运动/音频并发运行，
仍需后续独立验收。编码与推理并发已有 WebRTC 单观众短测，不等于整机负载验收。
