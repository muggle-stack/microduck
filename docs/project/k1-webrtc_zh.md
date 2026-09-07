<a id="k1--imx219-webrtc-browser-streaming"></a>
# K1 / IMX219：WebRTC 浏览器图传

[English](k1-webrtc.md) · [简体中文](k1-webrtc_zh.md)

这是 `mediad` 的显式 K1 CSI 适配：沿用官方信令、网页控制台和 `control` DataChannel，
不是另起一套 Python 推流程序。RTSP 工具保持独立，不需同时运行，也不能同时抢占 sensor。

```text
IMX219 → K1 ISP / CPP → NV12 DMA-BUF → tee
    ├─ webrtcsink → spacemith264enc → H.264 Main → RTP / WebRTC → 网页视频
    └─ 按需映射 DMA-BUF → 原生 GstVideoConverter → UYVY BT.601 → Rust ORT / SpaceMIT EP
                               → media.detections → DataChannel → 网页 SVG 框
```

目前限定 MUSE-Pi-Pro、CSI3 IMX219、1280×720、30 fps 配置、可信局域网。
检测默认关闭；原 Rockchip、USB 采集及 `camera-rtsp` 路径不被替换。
网页的控制 API 被保留不等于 K1 已完成真实舵机/IMU 闭环。

<a id="1-system-dependencies"></a>
## 1. 系统依赖

先完成 [SDK 原生构建准备](../robot/install-k1_zh.md) 和 [IMX219 采集验证](k1-imx219_zh.md)。
不要在 K1 执行 Radxa 的 `setup-gstreamer.sh`，也不要复制 aarch64 插件。

WebRTC 还需要 ICE 的 GStreamer 插件。先模拟，确认不升级/移除 vendor 多媒体包后再安装：

```sh
sudo apt-get -s install gstreamer1.0-nice
sudo apt-get install gstreamer1.0-nice
gst-inspect-1.0 nice
gst-inspect-1.0 webrtcbin
gst-inspect-1.0 spacemitsrc
gst-inspect-1.0 spacemith264enc
```

本次 MUSE-Pi 使用 GStreamer 1.24.2，只新增 `gstreamer1.0-nice` 0.1.21-2build3，
没有替换系统 GStreamer、相机驱动、设备树或 MPP。
`webrtcbin` 存在不代表 `webrtcsink` 已安装，后者单独构建。

<a id="2-a-separate-rust-192-for-the-plugins"></a>
## 2. 给插件准备独立 Rust 1.92

SDK 本身继续使用 Rust **1.89**；固定的 `gst-plugins-rs 0.15.3` 需要 Rust **1.92+**。
如果 `/opt/microduck-gst-rust-1.92.0/bin/rustc -V` 已存在且正确，可跳过这一段。
以下下载官方预编译工具链，不编译 Rust，不替换 apt 或 SDK 的 Rust：

```sh
(
  set -eu
  test ! -e /opt/microduck-gst-rust-1.92.0
  test ! -L /opt/microduck-gst-rust-1.92.0
  K1_WEBRTC_RUST_STAGE=$(mktemp -d)
  cd "$K1_WEBRTC_RUST_STAGE"
  K1_WEBRTC_RUST_DIST=rust-1.92.0-riscv64gc-unknown-linux-gnu
  curl -fL "https://static.rust-lang.org/dist/$K1_WEBRTC_RUST_DIST.tar.xz" \
    -o "$K1_WEBRTC_RUST_DIST.tar.xz"
  printf '%s  %s\n' \
    5492105083990bc0fd91008b1dc66b748b9687bbaf16719b5b37b1f09bf59458 \
    "$K1_WEBRTC_RUST_DIST.tar.xz" | sha256sum -c -
  tar -xJf "$K1_WEBRTC_RUST_DIST.tar.xz"
  sudo sh "$K1_WEBRTC_RUST_DIST/install.sh" \
    --prefix=/opt/microduck-gst-rust-1.92.0 \
    --components=rustc,cargo,rust-std-riscv64gc-unknown-linux-gnu --disable-ldconfig
)
```

校验值来源：[官方 Rust 1.92 RISC-V distribution checksum](https://static.rust-lang.org/dist/rust-1.92.0-riscv64gc-unknown-linux-gnu.tar.xz.sha256)。
这两个独立前缀无需改 shell 启动文件；只在对应构建命令里选择工具链。

<a id="3-build-the-private-webrtc-plugins-and-sdk"></a>
## 3. 编译私有 WebRTC 插件和 SDK

在 K1 的 SDK 仓库中：

```sh
cd ~/workspace/microduck
PATH=/opt/microduck-gst-rust-1.92.0/bin:$PATH sh scripts/build-k1-webrtc.sh

export CARGO_TARGET_DIR="$PWD/target"
PATH=/opt/microduck-rust-1.89.0/bin:$PATH cargo k1 --locked --bin mediad -j 2
```

脚本固定上游 commit 和本仓库补丁，只构建 `rswebrtc` / `rsrtp`，输出到
`target/k1-webrtc/plugins`，不安装到 `/usr/lib` 或 `/usr/local/lib`。
它有独立 Cargo 缓存；第一次仍需下载/编译 Rust crate，不是重新构建系统 GStreamer。
本次在 8 GB 板构建，默认 4 个构建任务。上游 release 启用了 LTO，首次原生构建较慢；
4 GB 板建议先用 `K1_BUILD_JOBS=1` 限制编译峰值内存，未将 8 GB 的构建结果视为 4 GB 验收。
源码来源、许可和补丁说明见 [native/k1-webrtc](../../native/k1-webrtc/README_zh.md)。

<a id="4-start-and-view-from-a-desktop-browser"></a>
## 4. 启动并在电脑浏览器查看

检查 [webrtc-imx219.toml](../../deploy/k1/webrtc-imx219.toml) 中的 `camera.isp_config` 绝对路径。
退出所有其他相机程序，在 K1 运行：

```sh
cd ~/workspace/microduck
export GST_PLUGIN_PATH="$PWD/target/k1-webrtc/plugins"
export GST_REGISTRY="$PWD/target/k1-webrtc/registry.bin"
export ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so
target/riscv64gc-unknown-linux-gnu/release/mediad \
  --config deploy/k1/webrtc-imx219.toml --host 127.0.0.1
```

电脑另一个终端：

```sh
ssh -N -L 127.0.0.1:8080:127.0.0.1:8080 \
  -L 127.0.0.1:8443:127.0.0.1:8443 musepi
```

浏览器打开 **http://127.0.0.1:8080/**，点击 **connect**。
网页和信令经过 SSH；**视频的 ICE/UDP 直连仍要求电脑能访问板卡的局域网地址**，
仅能 SSH 到跳板机并不足够。当前没有配置公网 STUN / TURN。

要从同一可信局域网的手机查看，可将 K1 的 `--host` 改为板卡 LAN IP，
打开 `http://板卡IP:8080/`；无需 SSH 隧道。
**官方控制台没有用户鉴权，能连接者可调用控制 API**。不要监听公网、转发路由器端口，
也不要在未知网络运行。当前网页/信令是 HTTP/WS，不以 WebRTC 媒体加密代替访问控制。

关闭网页连接不会关闭整个 `mediad`；相机仍可供检测支路使用。
在 K1 按 Ctrl-C 结束服务，再关闭 SSH 隧道。程序尝试 EOS 排空后释放相机；
清理超时会报错并失败退出，不伪装成正常关闭。

<a id="5-enable-detection-and-boxes"></a>
## 5. 开启检测和框

先确认预览正常，再将示例的 `[detect] enabled` 改为 `true`，重启 `mediad`。
从 [models-duck-detect-v1 模型 Release](https://github.com/muggle-stack/microduck/releases/tag/models-duck-detect-v1)
下载 `duck_detect.slim.onnx`，按 [IMX219 指南的 SHA256](k1-imx219_zh.md#模型和精度边界) 校验，
并调整配置中的模型绝对路径。普通 git clone 不下载 Release 资产，SDK 也不会自动获取它。
这是带独立模型许可声明的 FP32 模型预发布，不是 SDK 固件或 INT8 实验模型。

推理复用原 Rust ORT＋EP 和前后处理，结果以 `media.detections` 从同一 DataChannel 发往网页。
框由官方网页 SVG 绘制，不烧进 H.264：下载原始视频不会自动带框。
只有模型检出目标才有框；没有框不直接等于图传或推理停止，先看检测计数和服务日志。
检测模型识别 Microduck 机器人，不是任意鸭子图片或通用 COCO 检测器。

<a id="6-current-boundaries"></a>
## 6. 当前边界

- 原始 DMA-BUF 交给 `webrtcsink` 内部硬编码，不回退软件 H.264，不隐式旋转/缩放。
  `media.quality` 必须匹配相机尺寸和帧率；显示方向可用浏览器旋转。
- 板上编码器实际输出 **H.264 Main**，补丁按真实 profile 协商；不伪装 Baseline。
  仅支持 Baseline 的客户端不在本次范围内。
- 该版本 vendor 编码器未暴露码率、GOP、profile 调节属性。
  必须配置 `congestion_control="disabled"`；`media.bitrate` 不控制它的实际输出。
  不宣称已完成自适应码率、丢包快速恢复或端到端延迟验收。
- 仅验证视频和 DataChannel；没有启动麦克风、扬声器、运动服务或舵机。
  网页中的 `robot.*` / `system.*` 可能因相应守护进程未运行而报错，需与图传故障区分。
- `webrtcsink` 按观众建立编码会话，不等同于 RTSP 工具的共享编码器。
  本阶段先验收单观众及重连，多观众资源上限和整机并发另测。
  网页的 RTT 是网络往返时间，不是 sensor 到屏幕的端到端视频时延。
- 用户已确认当前 IMX219 图传质量和运行稳定性可接受。早期天花板场景的偏色/高光过曝记录保留，
  不同光照下的 PQ 和曝光仍需专项回归，SDK 不加滤镜掩盖。
  H.264 有损编码、YUV 转换和不同 EP 输出不宣称逐字节一致；本次不改模型精度。

<a id="7-development-record-2026-09-07"></a>
## 7. 2026-09-07 开发过程记录

- 固定 0.15.3 的私有插件使用 Rust 1.92 原生构建成功，首次构建约 44 分钟；
  `gst-inspect-1.0` 确认从私有路径加载，运行时仍是 vendor GStreamer 1.24.2。
  SDK 仍由 Rust 1.89 编译。Mac 相关 130 项测试、K1 library 726 项测试通过，
  K1 的 2 项实物测试默认忽略；这些软件测试不替代 WebRTC 相机实测。
- 保留首轮失败：`videoconvert` 无法协商 DMA-BUF NV12 到普通 UYVY，
  SDK 在启动 sensor 前以 `Noformat` 拒绝。改成 appsink 接收真实 DMA-BUF，
  只映射被请求的帧，通过原生 `GstVideoConverter` 生成普通 BT.601 UYVY；
  没有使用改 caps 标签的方式伪装内存，没有额外打开一个 sensor。
- 首轮浏览器出帧和 DataChannel 请求成功，`media.video` 返回 1280×720、rotate 0。
  ICE 实际选择板卡的 LAN host / UDP candidate；网页和信令通过本地 SSH 隧道。
  offer 的 H.264 `profile-level-id=4d401f`，Chromium answer 为 `4d001f`，都为 Main。
  8 秒采样新增解码 237 帧；当时有测试构建并发，仅作为功能证据。
- 同一采集源开启 ORT / EP 后，15 秒内浏览器新增解码 447 帧、收到 30 条检测通知。
  场景为天花板，通知中的 `boxes=[]`；不将空场景当作目标检出或精度验收。
  同时有插件构建，未将这轮结果与独立 `camera-check` 的耗时进行性能比较。
- 保留重连失败：快速重连前两轮分别新增解码 149 / 152 帧，第三轮只有首帧，
  接下来 5 秒新增帧数为 0，DataChannel 却仍正常；板端采集随后恢复。
  另一次带检测运行退出触发 5 秒清理期限、exit 1，即使日志含 sensor power off，
  也不计为正常释放。针对退出的修复是在 K1 消费会话 NULL 前排空编码器，
  保留 EOS 失败日志与 SDK 的总退出期限。

原始日志在板端 `target/k1-webrtc/`，浏览器记录在开发机 `target/webrtc-*.log`。

<a id="acceptance-after-the-fixes"></a>
### 修复后的验收

- 无编译或其他 SDK 业务并发，硬 cgroup `AllowedCPUs=0-2,4`，采样的全部用户线程均在
  这四个 CPU 内；EP 三个 worker 使用 `0;1;2`，模型及精度设置不变。
- 开启检测，连续 8 轮“播放 3 秒 → 断开 → 立即重连”，每轮新增解码
  **89 / 89 / 89 / 90 / 89 / 89 / 91 / 89 帧**，每轮收到 **6 条**检测通知；
  8 次日志均确认编码器已排空，未出现 EOS 排空超时。
- 同一进程再通过实际可见的 Chromium 控制台采样 20 秒，新增解码 **597 帧**，
  按浏览器统计时间差约 **29.85 fps**；收到 **40 条**检测通知，该窗口统计丢包增量为 0。
  这是静态天花板场景、单观众、模型已热身的短测，不代表复杂画面码率或长稳保证。
- 保持浏览器连接并运行检测时向 SDK 发送 SIGINT：浏览器观察到控制通道关闭，
  SDK **exit 0**，日志包含 `K1 media stopped`、sensor stream off / power off。
- 随后重新打开 sensor，关闭检测，连续 6 轮立即重连，每轮新增解码
  **90 / 90 / 89 / 89 / 90 / 91 帧**；以 SIGTERM 停止空闲服务，SDK **exit 0**，相机释放。
  这轮与最初失败的“关闭检测”配置对应，不能只用开着检测的新配置代替回归。
- 上述轮次 boot ID 始终为 `385fa665-52e8-423f-96b4-7cfd0af98659`，未依赖重启恢复。
  部分客户端断开时仍有底层 SCTP association error 日志，后续视频与控制续连均通过；
  不宣称无告警日志或网络异常恢复已经全部完成。

最终日志为板端 `run-d.log` / `run-e.log`、`library-tests-final.log`，
开发机 `webrtc-final-reconnect.log` / `webrtc-final-combined.log` /
`webrtc-active-stop.log` / `webrtc-final-video-only.log`。本轮测试服务、浏览器和 SSH 隧道均已停止。

本次插件构建产物校验值（重新构建不保证字节可复现）：

```text
555c37dc09a370210c27f905e3fc915bd4779b90f93a6083d39cc36858042d6c  libgstrswebrtc.so
59eb4c1c09dd016b8beb525584cec06c0a38e64780c07b09a9f59ba263b9178f  libgstrsrtp.so
```

<a id="subsequent-user-acceptance-2026-09-07"></a>
### 后续用户实测确认（2026-09-07）

用户在 Mac 浏览器自行启动并使用后确认图传可用、推理正常，随后确认
**IMX219 稳定性和图传质量可接受**。当前单观众 IMX219 视频与同源检测可供开发使用，
无需继续把用户可见图像质量列为未验收的阻塞项。

此项是用户实际使用反馈，未附时长、帧统计或标注集，不追加新的帧率、延迟、精度或长稳数字；
上面的 4 核开发短测仍保留其原始条件。音视频、多观众、网络/相机异常恢复与整机并发仍待专项验收。
