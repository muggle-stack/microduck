<a id="k1-mpp--spacemit-opencv-camera-backend"></a>
# K1 MPP + SpaceMIT OpenCV 相机后端

[English](k1-mpp-camera.md) · [简体中文](k1-mpp-camera_zh.md)

> 本文保留带日期的实验与失败记录，不代表全部内容仍是当前待办。最新功能状态见[适配进度](spacemit-k1-adaptation-zh.md)，后续相机与浏览器链路见 [IMX219](k1-imx219_zh.md) / [WebRTC](k1-webrtc_zh.md)。翻译不改变原测试条件或把历史数据当成新一轮验收。

这是服务于 SDK 现有相机/检测消费者的**显式启用 USB 处理后端**。
它本身不移植 IMX219/ISP，也不代表完整 H.264/WebRTC。
Radxa 源与通用 USB 软件路径仍为默认；模型、量化、阈值、EP 参数和检测器前处理均不变。

本阶段要求**从相机选一眼使用**，即使 USB 原始帧包含两眼。
下方双眼结果保留为额外诊断，不是剩余交付要求。
工具链与可迁移的开发安装从 [K1 安装指南](../robot/install-k1_zh.md)开始。

<a id="what-runs-where"></a>
## 各处理步骤的位置

```text
one MJPEG UVC frame (mono or packed SBS)
  -> private MPP UVC / codec2 VDEC: hardware decode into NV12 DMA
  -> V2D: selected ROI + aspect-preserving resize/black bars + optional rotation
  -> SpaceMIT OpenCV: full-to-limited YUV range LUTs + NV12-to-UYVY packing
  -> bounded GStreamer AppSrc
  -> existing SDK frame/detector consumers
```

原生 `--both-eyes` 保留完整帧，由 SDK 从同一 PTS 提取两个配置的 ROI。
这不包含视差、标定深度、独立相机同步或双路 WebRTC。
mono 使用连接的 DECXIN 一眼 ROI 验证，尚未测试独立物理单目 UVC 相机。

桥接提供小型 C ABI，仅在 `camera.acceleration = "spacemit"` 时由 Rust 动态加载。
普通 Rust 构建、aarch64/Radxa 和软件 USB 不新增 MPP/OpenCV 链接/构建依赖。
不支持的架构、缺失桥接、ABI 不匹配、设备/模式/处理错误均显式失败，
不静默切换后端、模型、精度或分辨率。

C++ 桥接每进程一个相机上下文，使用有界压缩输入队列、最新解码帧、有限读等待、
join 式退出和受保护的帧释放。
私有 UVC 补丁跟踪 V4L2 基础引用所有权，避免关闭时再次释放已归还空闲列表的缓冲。
AppSrc 仅排队一个 buffer，现有无头 sink 也仅保留最新帧。
采集 PTS 间隔保留并重新定位到 GStreamer 运行时钟；这不是已测音视频同步保证。
MPP 相机诊断改向 stderr，防止 native stdio 打断 SDK stdout JSON；错误仍保留。

V2D 使用连续布局的 NV12 DMA 分配。桥接检查 plane FD、offset、分配大小及 stride；
分离 plane 会复制到持久连续缓冲，不冒充零拷贝。CPU 访问 DMA 显式同步。
OpenCV 单线程，复用中间数组、范围 LUT、垂直色度最近邻扩展与通道打包。
最终 UYVY 和 SDK 帧仍有 CPU 可见复制，**不是端到端零拷贝**。
这里“连续”指同一个 DMA-BUF 的双 plane 布局，不宣称 Linux `system` DMA heap 的物理页连续。

<a id="build-and-select-it"></a>
## 构建与启用

已测 K1/Bianbu 2.1.1，`opencv-spacemit` **4.14.0-1bb3**，库报告 `4.14.0-pre` 和 RVV。
CMake 配置位于 `/opt/opencv-spacemit/lib/cmake/opencv4`，系统 OpenCV 4.6 仍保留。
原生依赖为 C++17、CMake、Git、pthreads、Linux DMA 头文件及 SDK 常规 GStreamer 开发包。
板上必须有可用 UVC、硬解码器、`/dev/v2d_dev`、`/dev/dma_heap/system`。
不要笼统开放设备权限，应单独配置服务用户访问。

构建输入为已有 [SpaceMIT MPP](https://github.com/spacemit-com/mpp) 克隆，
其中含固定提交 **`2b97ffe84c06071774301fad5542fbe76fa62774`**。
脚本把该修订归档到 SDK 本地构建目录，应用已提交补丁，只构建必需目标，并把库放在桥接旁。
不修改原 MPP 工作副本，也不运行上游 post-build 插件安装器。

```sh
cd /root/workspace/microduck
sh scripts/build-k1-camera.sh /root/workspace/spacemit-sdk/components/multimedia/mpp
export MICRODUCK_K1_CAMERA_LIB=/root/workspace/microduck/target/k1-camera/libmicroduck_k1_camera.so
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
cargo k1 --locked --bins -j 2

timeout 30 target/riscv64gc-unknown-linux-gnu/release/camera-check \
  --config deploy/k1/camera-usb-decxin-mpp.toml --frames 30
```

模板仅配置相机。推理保留已验证 `[detect]` ONNX/SpaceMIT EP 配置及模型路径，
为检查命令增加 `--detect --hz 2`；见
[检测参数和硬 CPU 预算](k1-usb-camera_zh.md#configuration-and-headless-check)。
桥接不会把上游 `.rknn` 变成 EP 兼容 ONNX。

向其他守护程序提供新共享配置前，应构建全部 SDK 二进制：
`robotctl`、`robotd`、`padd` 也读取 `robotd-params`。
单独 `-p mediad` 足以隔离检查相机，但旧程序不认识严格的 `camera.acceleration` 字段。
这里不替换运行中的服务。

可选第二个构建参数必须是**当前 workspace 的 `target/` 下专用子目录**。
`K1_BUILD_JOBS` 默认 2。缓存源码修订与补丁 hash 会检查；
修改固定修订/补丁时使用新输出目录。`MICRODUCK_K1_CAMERA_DEPS_ONLY=1` 只构建私有 MPP 库。
此 MPP 快照不要执行 `cmake --build ... --target all` 或 `cmake --install`，
无关上游 ISP 目标仍带安装器。

组件布局：

```text
target/k1-camera/
  libmicroduck_k1_camera.so
  lib/libmpp.so -> libmpp.so.1 -> libmpp.so.1.0.0
  lib/libv4l2_linlonv5v7_codec2.so
  camera-native-check
  camera-image-test
```

桥接使用 `$ORIGIN/lib` 和 `/opt/opencv-spacemit/lib`。
修补后的 MPP 加载器只从**该 MPP 库旁边**加载 codec2，不搜索 `/usr/lib` 或用户插件目录。
私有 MPP 函数通过 `-Bsymbolic-functions` 内部绑定。
不要加入全局加载路径或覆盖旧系统 MPP；重建/替换已加载组件前停止消费者。
脚本不改 service、apt 包、系统库或全局 `LD_LIBRARY_PATH`。

示例配置不自动安装/启用。已有有效 USB 配置可设 `camera.acceleration = "spacemit"`，
并向进程提供桥接绝对路径环境变量。
字段已加入 `robotctl configure`，沿用相机变更/重启处理。
返回通用路径时选择 `"software"` 并重启消费者，此时不加载桥接。

加速输入支持 MJPEG、最大 4096×2160 的偶数尺寸、偶数 `[x,y,width,height]` ROI。
请求模式必须与 UVC 实际协商一致。
V2D 每轴缩放 1/8–8×；拟合矩形和补边须保持 NV12 偶数对齐，非法布局报错而非静默取整。
原始 YUYV/UYVY/NV12 相机继续用软件路径。

`camera.rotate` 默认仍是安装方向元数据。
只有显式 `mediad --flip-in-pipeline` 执行物理旋转，此后端在裁剪/letterbox 后由 V2D 完成；
`camera-check --flip-in-pipeline` 检查相同行为。90°/270° 会交换输出宽高。
原生 `--both-eyes` 不可与该选项组合，检测器也不可二次旋转。

<a id="reproduce-correctness-checks"></a>
## 复现正确性检查

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

合成测试覆盖四种旋转、仅裁剪像素映射、恒色缩放/黑边、limited-range 打包、
带 padding stride 和分离 DMA planes。
**20** 项逐字节对照参考（`differences=0`），**13** 项非法 ABI/配置/短 buffer/格式均拒绝。

SDK 验证：主机全 workspace **1,232 通过 / 6 忽略**；
K1 release `mediad` + `robotd-params` **154 通过 / 1 个硬件测试忽略**。
之后显式执行该硬件测试并通过：同一 Rust 进程三次开/读/关、共十五帧，6.21 秒。
左右选择、ROI mono、物理 90°/180°/270° 采集均通过，PTS 递增。
缺失桥接、原生不支持的 1280×720 模式均显式失败，设备未残留占用。

随后全 workspace `--bins` 原生构建通过（14m46s），覆盖所有共享配置消费者。
九个 CLI 加载检查全部通过：`robotd`、`robotctl`、`updaterd`、`configd`、`btd`、
`padd`、`mediad`、`tofd`、`camera-check`。
`--help` 使用隔离运行目录而非 `/run`，未启动/安装硬件控制服务。
重建的 `robotctl`、`robotd`、`padd` 均包含新字段。

公平的软件对照必须像生产 SDK 一样，在 `videoscale` **两端固定 pixel-aspect-ratio=1/1**。
否则 GStreamer 可能协商非方形像素而不补边，图像几何不再相同。

已存原 JPEG SHA-256：
`2a69d3c2bfbe53dede8b2514fa9778f07b7ac91c1ce0e51f7465729ad08df3af`。
同左 ROI、同 1280×720 方形像素输出的精确性：

- 硬解＋V2D 与 SDK 等效软件链**不是逐字节一致**：
  1,843,200 个 UYVY 字节中 1,125,646 不同；平均绝对差 **1.762664**，P99 **17**，最大 **41**。
- 两路都有完全相同的 64 像素侧黑边（`Y=16`）。
  内部亮度均值软件 116.833501 / 硬件 116.759251；
  不是把未经转换的 full-range 直接改标签冒充 limited-range。
- 解码器、缩放器和色度采样不同；字节差异**不能**证明标注检测精度等价。
  未降低模型精度或切换检测前处理，后端仍为**显式启用**。

<a id="measurements-and-limitations"></a>
## 实测与限制

2026-09-06 所有板端编译结束后测 SDK 性能。
验收条件：相同相机/原生模式、左 ROI、720p30 输出、未变的浮点 slim ONNX、
SpaceMIT EP 三个 worker `0;1;2`、调用线程 CPU 4、FP16 epilogue 关闭、阈值 0.35。
整条命令处于硬 `AllowedCPUs=0-2,4` cgroup，不只是初始 taskset。

模型 SHA-256：`e1e8444bc8fe9a53675f1c885b781705f8579ea17abe23bcd2b853d59c8f746f`。
各实时采集画面并非相同；上方同 JPEG 正确性比较与实时吞吐/延迟分开。

无 profile 单眼实时测试：**5 次预热＋60 帧计时**，按 2 Hz 消费。
检测耗时含原前处理＋ORT/EP＋NMS，不含采集/裁剪/缩放。
CPU 列为 GNU time 的 user+system 除以完整墙钟，**包含启动/预热**，100% 表示一核。

| 后端（执行顺序） | 检测平均 / P95 | 整进程 CPU | User + system / wall |
| --- | ---: | ---: | ---: |
| 软件 | 243.280 / 252.151 ms | 227% | 80.04 / 35.15 s |
| MPP + OpenCV | 187.411 / 207.695 ms | 153% | 53.72 / 34.92 s |
| 软件复测 | 244.943 / 251.096 ms | 229% | 80.49 / 35.05 s |
| MPP 最终回归，诊断改向 stderr | 205.659 / 215.716 ms | 165% | 57.90 / 34.91 s |
| MPP 最终全 workspace 构建 | 192.812 / 209.435 ms | 155% | 54.29 / 34.92 s |

实质改进是采集吞吐和 CPU 压力下降。
实时检测仅改善约 **38–58 ms（16–24%）**，不是数量级提升，仍未达到离线约 125 ms 模型调用基线。
硬件组波动 **187–206 ms**，原因未分离，187 ms 不是保证值。
五组均维持约 2 Hz、PTS 递增；无舵机、音频、编码器或 WebRTC 并发。
各组开始温度 40–50°C。工具先开 EP 再开相机，主线程绑定 CPU 4，硬 cgroup 约束全部子线程。

最终纯采集、选眼 1280×720、无检测、同四核 cgroup，两后端均 **5 预热＋120 测量**：
软件 **5.393 fps**，硬件 **30.055 fps（5.57×）**；按 PTS 为 **5.393 / 30.049 fps**。
此前 60 帧软件/120 帧硬件的 5.419 / 30.069 fps 保留在证据目录，
但采用相同样本数的最终结果验收。
这些是最新帧消费者速率，不是传输丢帧认证。
原生 SBS 全帧＋两路 1920×1200 ROI 提取测得 **20.773 对/秒**（5 / 60），
完整帧 CPU 打包/复制仍比选眼 720p 更贵，不能等同于两台独立相机的同速双流。

双眼推理（5 预热＋20 对测量，每对串行两次，要求 2 Hz）仅 **1.981 对/秒**。
每眼平均 **227.048 ms**、P95 **257.872 ms**；
每对推理总 P95 约 **517 ms**，**5/20 对超过 500 ms**，还未计采集。
**稳定双眼 2 Hz 未验收**。若需要，可选模式还需调度/处理与更长并发测试，不阻塞单眼验收。

独立硬件 profile 组（5＋20，2 Hz）检测均值 **201.634 ms**。
全部 **25** 次计算节点执行属于 `SpaceMITExecutionProvider`，
每次一个融合子图、无记录到的 CPU 计算节点。
排除五次预热后，EP 节点平均 **189.535 ms**，同次测试节点外 **12.099 ms**。
不可从另一组无 profile 的 187.411 ms 中减去此节点均值。

最终纯桥接诊断，同四核、选眼 720p、3 预热＋120 测量、**无检测**：
消费 30.023 fps / PTS 30.049 fps；V2D 裁剪/缩放 **6.362 ms**，
OpenCV 范围/打包 **6.371 ms**，平均等帧 **20.443 ms**。
等待下一帧不等于 JPEG 解码耗时，这些阶段也不含 Rust/GStreamer 复制。

仅日志路由补丁后，再过 20 项逐字节/13 项拒绝测试、同进程重开及上述最终 SDK 实测。
同 JPEG UYVY 在补丁前后**逐字节一致（cmp exit 0）**。
六十条最终实时记录和两组 120 条采集记录均可完整解析 JSON，
只有 scope 显式 CPU 预算诊断位于 JSON 外。

保留的失败：

- 旧 `spacemitdec` 可解独立 JPEG，但 EOS 报错；实时采集只出 **3 帧**就触及 **20 秒超时**
  （0.24 秒 user / 18.65 秒 system）。本 SDK 不使用该后端。
- 旧 `G2D_Init` 经空函数指针崩溃，GDB 确认。这不证明 K1 没有 V2D；
  新直接 V2D API 通过上述板测。
- 初始新 MPP 需在 `VB_Init` 前 `SYS_Init`；退出还出现重复 UVC 基础引用释放。
  两处均修复并保留失败日志，第二处独立放在源码补丁中。
- 首次私有源码补丁因 `git apply` 识别外层 SDK 工作区而静默跳过。
  构建器现创建独立 snapshot Git 根并验证反向可应用性。
  后续格式化构建遇到 vendor MIN/MAX 宏头文件顺序冲突，调整 include 顺序并保留 `-Werror`。
  两种失败构建都未安装系统插件。
- Rust 1.93 主机全 workspace Clippy `-D warnings` 遇到既有无关
  `robotctl/src/monitor.rs:2188` 的 `nonminimal_bool`；受影响的 mediad/params 检查通过，
  本次未重写无关监控表达式。

这些历史测试不代表完整 K1 媒体守护服务。当时板上缺 `webrtcsink`、编码器待单独接入、
IMX219 未连接；舵机/视觉/音频/网络并发及标注检测精度仍未验收。
第二个物理单目相机和其他 USB 模式也需各自硬件验证。

证据：K1 `target/k1-mpp-camera-20260906.ZGXnqy/`，
Mac `target/k1-mpp-camera-20260906.WZDoGI/`。
失败探测/构建日志保留；生成图像、私有依赖构建和模型不提交。

<a id="file-inventory"></a>
## 文件清单

新建：

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

修改：

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

删除：无。
