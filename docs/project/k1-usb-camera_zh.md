<a id="k1-usb-camera-mono-and-packed-stereo"></a>
# K1 USB 相机：单目与拼接双目

[English](k1-usb-camera.md) · [简体中文](k1-usb-camera_zh.md)

> 本文按测试日期记录实验结果与已知问题。当前功能状态见[适配进度](spacemit-k1-adaptation-zh.md)，已集成的相机使用方法见 [IMX219](k1-imx219_zh.md) / [WebRTC](k1-webrtc_zh.md) 指南。

这是**可选 USB 后端**，不是 IMX219 ISP 或 WebRTC 移植。
现有 Radxa 仍为默认（`camera.backend = "rockchip"`，安装旋转 90°），USB 默认旋转 0°。
`mediad` 的 `--rotate` 可覆盖 `camera.rotate`。

另一个显式 `camera.acceleration = "spacemit"` 路径使用私有 MPP codec2/V2D 桥接与已安装 RVV OpenCV。
下方软件数据是历史基线，见 [K1 MPP 构建、精确性与验收](k1-mpp-camera_zh.md)。
默认后端、检测前处理和模型精度均不变。

<a id="implemented-contract"></a>
## 已实现的配置契约

- `mediad` 从共享 robotd TOML 选择 `camera.backend = "usb"`；
  `robotctl configure` 暴露相机字段，并将变更关联到 mediad。
- 必须显式给出 V4L2 采集设备及其原生格式/尺寸/帧率。
  不默认 `/dev/video0`，不扫描设备、不固定 Rockchip sensor 模式、不启动 RKAIQ 或写 Rockchip 曝光。
  已有 UVC 曝光/白平衡不变。
- 输入支持 MJPEG（软件 `jpegdec`）、YUYV（GStreamer 称 `YUY2`）、UYVY、NV12。
  转换协商 BT.601 limited-range UYVY，与原检测器契约一致。
- `mono`：完整帧或 `left_roi`。`stereo_sbs`：**同一 USB 帧**中两个不重叠 ROI。
  ROI 留空分别表示完整单目/常规等宽左右两半。
  有 vendor 前缀条带时显式给 `[x,y,width,height]`；横向坐标和宽度须为偶数。
- `camera.view` 为现有媒体/检测消费者选一眼，按比例缩放并补边到 `media.quality`，**不拉伸**。
  采集尺寸与输出画质独立，输出帧率不得超过原生配置帧率。
- `mediad::camera::usb::Capture` 还暴露原生帧；
  `CameraFrame::views` 从同一帧、同一 PTS 提取左右图。
  消费者拿到紧密排列像素，遵守 `GstVideoMeta` 的 offset/stride。
  释放 Capture 会释放设备。
- `ImageDetector` 与守护程序/无头工具使用同一套前处理→RKNN/ORT→解码实现，
  没有新增模型、量化或 provider fallback。

这里的**双目不包含**校正、内外参、视差/深度、已证明同时曝光或两台独立时钟 USB 相机。
这些需另行实现。mediad 仍只发布选定眼，不新增双目网络 API。

无效/缺失的显式配置会在打开硬件前失败，不再像旧媒体加载器那样用 Rockchip 默认替代坏配置。
缺失的**隐式** `/etc/robot/robotd.toml` 仍采用原默认。
这是包括 Radxa 在内的有意 fail-closed 改动，有效 Radxa 配置不变。
新 `[camera]` 中未知字段也会拒绝而非删除；其他 section 保留未知字段告警并忽略的策略。

<a id="configuration-and-headless-check"></a>
## 配置与无头检查

模板均不自动安装：

- `deploy/k1/camera-usb-mono.toml`：替换为相机实际设备与原生模式。
- `deploy/k1/camera-usb-decxin-sbs.toml`：已测设备标识/模式；
  ROI 是**目视候选，不是厂商确认的标定**。

运行插件来自已测 K1 已有的 `gstreamer1.0-plugins-base` 和 `gstreamer1.0-plugins-good`。
CI 在开发库之外安装它们执行合成相机测试。未升级 K1 包或固件。

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

选右眼用 `camera.view = "right"`。
单目用 `layout = "mono"`、`view = "left"`，无 right_roi，可选 left_roi 裁剪。
重启守护程序是显式动作，改文件不会热切换已打开的相机。

`--dump-dir` 必须是**父目录已存在的新目录**。
首个计时帧保存为 `<eye>.uyvy` 与 `frame.json`；`--both-eyes` 另存 `native.uyvy`。
几何参数在 JSON 中。按实际保存尺寸用 FFmpeg 生成检查图，例如：

```sh
ffmpeg -nostdin -f rawvideo -pixel_format uyvy422 -video_size 1920x1200 \
  -i CHECK_DIR/left.uyvy -frames:v 1 CHECK_DIR/left.png
```

加 `--detect` 启用推理，可另给 `--model /absolute/path/to/model.onnx`。
不传 model 时必须恰好解析出一个已启用的 detect.model。
SpaceMIT 仍需兼容 ONNX 和原生运行时，见 [ORT/EP 验证](k1-duck-ort-ep_zh.md)。
原 `.rknn` 不能直接交给 SpaceMIT EP。可追加配置：

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

用 `--hz 2` 节流；CLI 默认不节流，与 detect.hz 独立。
与此前 EP 测试相同的严格四核预算：

```sh
ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so \
systemd-run --scope --quiet --collect -p AllowedCPUs=0-2,4 \
  taskset -c 0-2,4 timeout 60 \
  target/riscv64gc-unknown-linux-gnu/release/camera-check \
  --config /absolute/path/to/usb-with-ep.toml --detect --hz 2 --frames 20
```

cgroup 约束**全部**采集/转换/推理线程，仅 taskset 约束不了 EP 调用线程重绑。
`--profile PREFIX` 生成原生 ORT profile，检查 provider 分配。
`--both-eyes --detect` 每个原始帧串行推理两次，不能把帧对速率直接与单眼推理速率比较。

工具打印 JSON 帧记录和汇总。预热后的 consumed_fps 包含取帧、复制、可选推理与节流。
inference_mean_ms/P95 含已有 UYVY 前处理＋运行时＋NMS，不含采集/裁剪/缩放。
最新帧 sink 可主动丢旧帧；PTS/buffer offset 是诊断，**不是 USB 传输序号丢失测试**。
原生库还可能打印自己的日志。

以上命令不启动 robotd、串口/舵机、ToF、音频或网络监听。
该次原板完整 mediad 仍需独立编码器/WebRTC 工作（当时缺 webrtcsink）。
无头工具不依赖它来验证输入和检测器。

<a id="hardware-evidence-2026-09-06"></a>
## 硬件证据：2026-09-06

K1 / Bianbu 2.1.1，`1bcf:2d50` DECXIN Camera，接 USB2 hub：

- `/dev/video20` 是视频采集；`/dev/video21` 是**元数据，不是右眼**。
- 稳定路径：`/dev/v4l/by-id/usb-DECXIN_DECXIN_Camera_01.00.00-video-index0`。
- 只宣告 **MJPG 4000×1200 @30 fps** 和 **YUYV 4000×1200 @1 fps**。
  1280×720、1920×1080 都不是原生模式。
- 解码图目视有两眼及前置条带。候选 ROI 为
  `[160,0,1920,1200]` / `[2080,0,1920,1200]`，光学含义需另行确认。

SDK 实现前基线：

| 检查 | 实际结果 |
| --- | --- |
| MJPEG V4L2 mmap，360 buffers | exit 0；序号 0–359，无缺口 |
| 后 300 个采集时间戳 | 30.043 fps；间隔 31.885–36.138 ms |
| 包含启动 | 时间戳 26.935 fps；首间隔 1416.119 ms；墙钟 13.52 s |
| 提取 MJPEG，FFmpeg 严格解码 | 成功，4000×1200；非致命 APP 字段告警 |
| GStreamer jpegdec → fakesink，60 帧 | exit 0 / EOS；7.55 s 墙钟，5.72 s user + 0.31 s system |

压缩采集 30 fps **不证明**软件解码 30 fps。
`v4l2-ctl --all` 还报 ext-control Privacy 查询错误 32，不影响上述采集；
这些负面结果未隐藏，也未计作已修复。

K1 证据 `target/k1-usb-camera-20260906.Lbddrk/`；
Mac `target/k1-usb-camera-20260906.F8Gx23/`。
两端原生 JPEG 逐字节一致，SHA-256：
`2a69d3c2bfbe53dede8b2514fa9778f07b7ac91c1ce0e51f7465729ad08df3af`。

<a id="sdk-live-results"></a>
### SDK 实时结果

Rust 1.89 原生 release，所有编译退出，采集前温度 43–44°C。
成功检查始终 MJPEG 4000×1200 @30，无其他相机消费者、运动、音频或 WebRTC 并发。

| SDK 路径 | 输出 | CPU 预算 | 预热 / 测量源帧 | 消费速率 | 检测平均 / P95 |
| --- | --- | --- | --- | ---: | ---: |
| 选左眼 | 1280×720 | 8 核可用，未绑核 | 3 / 30 | 5.316 fps | 关闭 |
| 选右眼 | 1280×720 | 8 核可用，未绑核 | 3 / 30 | 5.389 fps | 关闭 |
| 单目布局＋左 ROI | 1280×720 | 8 核可用，未绑核 | 3 / 30 | 5.430 fps | 关闭 |
| 两个原生 ROI | 2 × 1920×1200 | 8 核可用，未绑核 | 3 / 30 | 8.810 对/秒 | 关闭 |
| 左眼＋浮点 EP，--hz 2 | 1280×720 | 硬 4 核：0–2,4 | 5 / 60 | 2.016 fps | 253.716 / 264.537 ms |
| 两 ROI＋浮点 EP，--hz 2 | 2 × 1920×1200 | 硬 4 核：0–2,4 | 3 / 10 | 1.965 对/秒 | 每眼 241.963 / 255.268 ms |

单眼六十帧完整运行 35.02 秒（含启动/预热），CPU 74.98 user＋5.22 system 秒。
PTS 全部递增，跨度速率 1.990 fps。
consumed_fps 略高于 2 只是有限窗口效果，**不是加速**。
六十次推理完成且无超时，只验证短时单眼 2 Hz，不是长期或运动/音频/媒体压力测试。

该次 cpuset.cpus.effective 为 0-2,4；
三个 model worker 在 0/1/2，调用线程在 4，采集/辅助线程也在四核集合内。
独立二十帧 profile（五次预热）有 **25 次 SpaceMIT 融合节点、零 CPU 计算节点**，
均值/P95 为 248.481/254.029 ms。
文件是证据目录的 `live-ep-profile_2026-09-06_16-17-12.json`。

模型仍为 duck_detect.slim.onnx，前后 SHA-256 均为
`e1e8444bc8fe9a53675f1c885b781705f8579ea17abe23bcd2b853d59c8f746f`。
FP16 epilogue 关闭，未换 INT8。天花板场景无框，成功采集/推理**不是**标注检测精度。

重要边界：

- 选眼 720p 仅约 5.4 fps，比原生双眼提取慢；软件 JPEG 解码/转换/裁剪/缩放未优化，
  两种路径都不是 30 fps。
- 两次串行推理已约 484 ms，尚未含帧对采集/复制，1.965 对/秒几乎无余量。
  **稳定双眼 2 Hz 未验收**；SDK 原检测器使用的一眼才是上方验收范围。
- 离线 RGB 四核约 121 ms 的输入前处理不同、无相机负载，不是约 254 ms 实时结果的同条件基线。
- 真实请求不支持的原生 1280×720 MJPEG 时，GStreamer 报 not-negotiated (-4)，exit 1；
  清理后 4000×1200 可重新采集。
- 非法 ROI、缺失设备、单目选右眼、极小非零节流速率均 exit 1，不回退其他相机/后端。

重复开/读/关完成；每轮成功及不支持模式失败后 `fuser /dev/video20` 均为空，临时 CPU scope 已退出。
生产 mediad/camera-check 为 RISC-V ELF，--help 通过。
未改已安装 TOML、系统服务、曝光或 sensor/ISP 设置。

FFmpeg 独立裁剪**同一原生 UYVY 帧**与 SDK 两 ROI 均 cmp exit 0。
原生/左/右 SHA-256 依次为：

```text
93c011e9f732e5770438b6fef5ddb3f968d8b4b90715c034e753fd48b32a4b3f
4eba24e461b7b4aeafae7fd4d75094681f486fe9639b492e75ab7ceced19c5bd
7ec971e666f3e371a0a388cf833b7907dfdf8ef402e629ec3181fa985bb36955
```

两眼及按比例补边的 720p 均做目视检查。
首次 FFmpeg shell 验证遗漏 -nostdin，消费部分 SSH 脚本输入并在右眼检查前 exit 127；
补上 -nostdin 后两次字节比较均通过。
这是验证脚本错误，不是 SDK 采集故障。

<a id="opencv-rvv-and-live-camera-contention"></a>
### OpenCV RVV 与实时相机资源争用

同板 2026-09-06 后续测量：**253.7 ms 是前处理＋模型调用＋NMS，不是纯前处理**。
采集、MJPEG 解码、裁剪、输出缩放在计时上游，不计入该数值，但并发资源消耗仍影响推理延迟。

apt show opencv-spacemit 说明可用包，不证明已安装。
当时已装 libopencv-dev / Python OpenCV 4.6.0+dfsg-13.1ubuntu1bb1，
opencv-spacemit **未安装**，候选 4.14.0-1bb3。
本对照下载 deb 并解到 target，用独立 C++ 探测链接其头文件和显式 RUNPATH，
未 apt 安装、替换系统库、新增 SDK 依赖或修改服务。

隔离 4.14 报告 Baseline: RVV、Custom HAL: YES (RVV HAL (ver 0.0.1))、运行时 RVV；
系统 4.6 无 CPU 特性或 custom HAL。
这符合 [厂商 OpenCV RVV 文档](https://bianbu.spacemit.com/en/brdk/Basic_applications/3.5_High_Performance_Computing_Library/3.5.1_opencv_rvv/)，
但下面是**本板实测**，不是引用文档示例数字。

所有探测用同一选左眼 1280×720 UYVY、旋转 0，
最近邻缩到 320×180、padding 114，得到 320×320 RGB 字节。
Rust 直接调用 SDK 编译的 duck_detect::letterbox_from_uyvy；
C++ 做全帧 cvtColor(COLOR_YUV2RGB_UYVY)、resize(INTER_NEAREST)、copyMakeBorder，复用 Mat，
CPU 4、cv::setNumThreads(1)。Rust 单调用线程，位于下述同一硬四核 scope。
相机关闭，各十次预热、一百次测量。
这些是函数探索比较，不是已验收 SDK 加速。

| RGB 字节前处理 | 平均 / P95（ms） |
| --- | ---: |
| 现有融合 Rust 实现 | 3.020 / 3.046 |
| SpaceMIT OpenCV 4.14：三操作 | 4.537 / 4.612 |
| 系统 OpenCV 4.6：同三操作 | 14.339 / 14.580 |

这些操作下 SpaceMIT OpenCV 比系统快 **3.16×**，
但直接替换并不比现有 SDK 快：SDK 只转换缩放后实际取到的 57,600 像素，
全帧 OpenCV 先处理 921,600 像素。
这未穷尽其他 OpenCV 算法或上游采集优化；不含 JPEG 解码和 RGB→NCHW float 打包。

精确性：两种 OpenCV 都与 SDK 相差 **2,014 / 307,200 个 RGB 字节**，最大绝对差 1。
不是 bit-identical；未经检测精度等价或精度取舍批准，所以未接入替换，原前处理仍默认。

为分离相机负载与画面变化，另一实验固定**同一保存帧和浮点模型**：
先关采集，再让独立 camera-check 持续采集/解码/缩放 4000×1200 到选左眼 720p，
最后再次停采集。负载组两进程共享**同一个** AllowedCPUs=0-2,4 cgroup；
EP 三 worker 0;1;2，调用线程 CPU 4，FP16 epilogue 关闭。
每阶段五次预热、三十次不节流推理。

| 固定帧阶段，均值 ms | 相机关闭 | 相机运行 | 再次停止 |
| --- | ---: | ---: | ---: |
| 融合 UYVY→补边 RGB 字节 | 3.064 | 3.027 | 3.075 |
| Model::infer：打包＋ORT＋输出校验/复制 | 123.157 | 234.919 | 125.415 |
| NMS | 0.021 | 0.021 | 0.021 |
| 总计 | 126.242 | 237.967 | 128.510 |
| 总 P95 | 131.286 | 249.051 | 133.366 |

后台九十帧全部完成，24.22 秒含启动，时间戳递增并释放设备。
其总速率不是纯负载阶段吞吐，因为固定帧推理仅覆盖采集的一部分。
三份 RGB 参考 cmp exit 0，SHA-256：
`96412bf6ccef29ab3745ca4a3748053ee1fec48727f252380af6751c7ea0ea59`。
这是受控资源争用诊断，不替代实时 SDK 验收，也不宣称各轮模型输出逐字节一致。

上面**同一次**二十帧实时 profile 检测均值 248.481 ms，
排除五次预热后 SpaceMIT 节点均值 **238.397 ms**，约 96% 在 EP 内、节点外 10.084 ms。
不可从独立无 profile 六十帧结果中减这个节点均值。
受控关/开/关实验支持相机并发资源争用是离线/实时差距主因，
但没有进一步区分 CPU 调度、内存/cache 或单个转换器成本。

因此下一优化点是**上游全分辨率 MJPEG 解码/转换/缩放及调度**，
不是替换已经约 3 ms 的检测器 resize/color 循环。
减少采集工作或硬解仍需独立性能、像素与精度检查；
本记录不宣称实现了这种加速、稳定双眼 2 Hz 或新的吞吐。

探测源码、二进制、完整 OpenCV 构建信息和日志：
K1 `target/k1-opencv-check-20260906.gBuOpC/`。
文件有 preprocess-check.rs、opencv-check.cpp、rust-offline.log、rust-with-capture.log、
rust-offline-repeat.log、opencv-spacemit.log、opencv-system.log、capture-load.jsonl。
不含 deb/展开库的 Mac 副本在 `target/k1-opencv-check-20260906.dbfOVP/`。
deb SHA-256：`c74de8b27fdac8193a1b8777826fac7a20727fa04e5e2c69c9d41f8607d12f4e`。
首次独立 Rust 链接缺 host proc-macro 依赖目录而 E0463 失败（rust-build.log），
加 target/host 两处搜索路径后通过（rust-build-fixed.log）；保留该诊断构建错误。

<a id="exactness-and-acceptance-boundaries"></a>
### 精确性与验收边界

此次软件回归：

- macOS / Rust 1.93 全 workspace：**1,231 通过、0 失败、6 个已有忽略**。
- K1 / 独立 Rust 1.89 release，两个构建任务/测试线程，mediad 和 robotd-params：
  **153 通过、0 失败、0 忽略**，含 camera-check CLI。
  Linux 覆盖合成 NV12 归一化、UYVY row-padding/offset 精确去除、左右像素一致、
  按比例黑边、同帧裁剪和缺失设备。
- mediad/params 主机 Clippy -D warnings 通过；加入 robotctl 会遇到 Rust 1.93 下既有
  monitor.rs:2188 的 nonminimal_bool，未修改无关代码。
  K1 独立工具链无 Clippy；Linux 路径由板上编译/运行而非主机 lint 验证。
- cargo fmt --all --check、git diff --check、sh -n scripts/k1-test.sh 均通过。

主机全 workspace 与 K1 指定 crate 范围不同，不能相加或比较为平台加速；此处不宣称 GitHub CI 已执行。

ROI/stride 去除是**归一化之后**的字节复制，测试精确比较 padded/offset 和不同左右源值。
MJPEG 解码、色彩与缩放是图像变换，不是压缩输入的逐字节复制。
模型权重/精度及检测前处理未改；不宣称跨相机/ISP 像素一致或带标注精度。

本轮仅一台物理 DECXIN 拼接双目，相同 ROI 作为 mono 只验证软件单目路径，
不代表兼容任意 UVC 单目。此历史验证之外，K1 IMX219、双目标定/深度、
其他 UVC、USB 拔插恢复以及运动/音频/WebRTC 并发仍需独立硬件验收。

<a id="files-changed-in-this-integration"></a>
<a id="本轮文件清单"></a>
<a id="related-files"></a>
## 相关文件

- 新建：`robotd-params/src/camera.rs`
- 新建：`mediad/src/camera.rs`
- 新建：`mediad/src/camera/usb.rs`
- 新建：`mediad/src/bin/camera-check.rs`
- 新建：`deploy/k1/camera-usb-mono.toml`
- 新建：`deploy/k1/camera-usb-decxin-sbs.toml`
- 新建：`docs/project/k1-usb-camera.md`
- 修改：`robotd-params/src/lib.rs`
- 修改：`robotd-params/src/registry.rs`
- 修改：`robotd-params/src/edit.rs`
- 修改：`robotctl/src/configure.rs`
- 修改：`mediad/Cargo.toml`
- 修改：`mediad/src/lib.rs`
- 修改：`mediad/src/main.rs`
- 修改：`mediad/src/pipeline.rs`
- 修改：`mediad/src/config.rs`
- 修改：`mediad/src/detect.rs`
- 修改：`scripts/k1-test.sh`
- 修改：`.github/workflows/ci.yml`
- 修改：`docs/project/spacemit-k1.md`
- 删除：无。

生成的日志、测试配置、采样图像和编译产物保留在上述两端 `target/` 证据目录及 Rust
target 目录中，不加入 Git，不覆盖 `/etc/robot/robotd.toml`。
