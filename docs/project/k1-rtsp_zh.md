<a id="k1--imx219-live-rtsp-preview"></a>
# K1 / IMX219：RTSP 实时预览

[English](k1-rtsp.md) · [简体中文](k1-rtsp_zh.md)

> 本文按测试日期记录实验结果与已知问题。当前功能状态见[适配进度](spacemit-k1-adaptation-zh.md)，已集成的相机使用方法见 [IMX219](k1-imx219_zh.md) / [WebRTC](k1-webrtc_zh.md) 指南。

`camera-rtsp` 是 SDK 中**可选编译、单独运行**的预览工具。
它复用 `spacemit_csi` 的配置与 ISP profile 校验，不启动 `robotd`、舵机、音频、检测器或 WebRTC。
普通 `cargo k1 --bins` 和原 Radxa 构建不新增 RTSP server 的系统依赖。

```text
IMX219 → K1 ISP / CPP → NV12 DMA-BUF
       → spacemith264enc（MPP / VPU）→ H.264 → RTP → RTSP/TCP
       → SSH 隧道 → 本机 ffplay / VLC
```

当前范围：MUSE-Pi-Pro 的 CSI3 IMX219、1280×720、配置 30 fps、单目、旋转 0。
工具要求 `media.quality` 与相机尺寸/帧率相同，不偷偷缩放、旋转或回退软件编码。
USB 采集和原 Rockchip WebRTC 代码不变，但本工具暂不接这两种输入。

<a id="1-build-on-k1"></a>
## 1. 在 K1 编译

先完成 [K1 安装指南](../robot/install-k1_zh.md) 和 [IMX219 配置](k1-imx219_zh.md)。
额外需要 `libgstrtspserver-1.0-dev`，**先模拟安装**，避免替换 vendor GStreamer：

```sh
sudo apt-get -s install libgstrtspserver-1.0-dev
```

确认不会升级/移除现有 vendor 相机栈，再安装：

```sh
sudo apt-get install libgstrtspserver-1.0-dev
pkg-config --modversion gstreamer-rtsp-server-1.0
gst-inspect-1.0 spacemitsrc
gst-inspect-1.0 spacemith264enc
```

本次板卡只新增 `libgstrtspserver-1.0-0`、`libgstrtspserver-1.0-dev`、
`gir1.2-gst-rtsp-server-1.0` 三个 1.24.2-1 包，没有升级或移除现有包。
Rust 工具不依赖 Python GI；GI 是本次开发探测使用的 apt 配套包。

```sh
cd ~/workspace/microduck
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
export CARGO_TARGET_DIR="$PWD/target"
cargo k1 --locked --features mediad/rtsp --bin camera-rtsp -j 2
```

首次启用 `rtsp` 会下载/编译相应 Rust 绑定，不是重新编译系统 GStreamer。
无需再运行整个 `k1-test.sh`；此可选工具也不由现有开发安装脚本自动安装或启动。

<a id="2-start-camera-preview"></a>
## 2. 启动相机预览

先停止其他占用同一相机的程序，包括 `camera-check` 和 `mediad`。
检查 TOML 中绝对 `camera.isp_config` 路径适合当前用户/仓库位置。

在 K1 上运行：

```sh
cd ~/workspace/microduck
target/riscv64gc-unknown-linux-gnu/release/camera-rtsp \
  --config deploy/k1/camera-imx219.toml --duration 600
```

默认只监听 `127.0.0.1:8554`，路径 `/camera`，十分钟后退出；Ctrl-C 可提前停止。
`--duration 0` 为持续运行。输出 `RTSP listening` 只表示监听成功；**有客户端连接才打开相机**，
相机是否真正出帧仍以播放器/解码器为准。多个客户端共享同一路 sensor 和编码器。

<a id="3-view-from-a-computer"></a>
## 3. 在电脑看实时画面

电脑的一个终端建立隧道（`musepi` 换成自己的 SSH 别名或 `用户@板卡IP`）：

```sh
ssh -N -L 127.0.0.1:8554:127.0.0.1:8554 musepi
```

另一个终端打开播放器：

```sh
ffplay -rtsp_transport tcp -fflags nobuffer -flags low_delay -framedrop \
  rtsp://127.0.0.1:8554/camera
```

VLC 可打开同一地址，需启用 RTP over RTSP/TCP；本工具不启用 UDP 传输。
浏览器地址栏不能直接播放 RTSP；手机也需支持 RTSP/TCP 的播放器，不是网页即开即看。
按播放器的退出键或关闭窗口，再在 K1 上 Ctrl-C 停止服务，最后 Ctrl-C 关闭 SSH 隧道。

如果明确需要可信局域网内的手机直连，可在 K1 加 `--listen 板卡的局域网IP`，
播放器打开 `rtsp://板卡的局域网IP:8554/camera`，不需要 SSH 隧道。
**此模式没有 RTSP 鉴权，也没有视频加密**；不要开放公网、不要添加路由器端口映射。

<a id="boundaries-and-known-issues"></a>
## 边界与已知问题

- 这是实时原图预览，不画检测框，不加载 ORT 模型，也不启动音频或运动控制。
  后续检测框需在**同一个采集源**分出检测支路后叠加；不能另开一个进程抢同一 sensor。
- 这是 H.264/RTSP 子路径，不代表 `mediad` 的完整 WebRTC、浏览器控制、音视频同步已完成。
- 硬编码需要 `close-dmabuf=false` 及 `video/x-raw(memory:DMABuf)`。
  实测 `close-dmabuf=true` 直接接编码器时报 `Failed to queue buffer ... Cannot allocate memory`，
  产物为 0 字节且 EOS 卡住，超时终止；DMA-BUF 版本生成 90 帧有效 H.264 并正常退出。
  原 CPU 检测采集路径仍用 `close-dmabuf=true`，没有被这一修复改动。
- 本次编码器插件未暴露 bitrate / GOP 属性，工具不套用 Rockchip 属性或宣称码率可调。
  硬编 H.264 是有损视频，不宣称与原 NV12/UYVY 逐字节等价；检测路径本次不变。
- 相机仍有偏色/高光过曝问题，需相机团队验收 AE/AWB/PQ；本工具不通过滤镜掩盖。
  冷启动、客户端缓冲、关键帧间隔会影响首帧时间，端到端低延迟和长稳仍需专项测量。
- 客户端正常退出可释放媒体；网络突然断开时，RTSP session 超时清理可能有等待。
  最后一位客户端退出时先发送 EOS 排空编码器；新连接等待旧流关闭，再创建新流，
  不复用已结束的 vendor 编码器实例。等待关闭超过 5 秒会记录错误，新连接可能失败。
  到期或 Ctrl-C 时会关闭监听并尝试释放相机；底层清理超过 5 秒则报错、以失败状态退出进程，
  **不把超时说成正常释放**。异常 sensor 可能仍需硬件复位。vendor 关闭阶段的 CPP 告警需上游排查。

<a id="development-validation-2026-09-07"></a>
## 2026-09-07 开发验证记录

- Rust 1.89 原生构建 `camera-rtsp` 成功；不是以 Python demo 代替 SDK 集成。
- 最终 Linux 软件回归：`cargo test --locked --release --workspace --lib --bin camera-rtsp
  --features mediad/rtsp --target riscv64gc-unknown-linux-gnu -j 2 -- --test-threads=2`，
  **724 项通过、0 失败、2 项外设测试默认忽略**。包含默认仅本地监听、参数拒绝、URL 共享 sensor、
  异步关闭后更换共享 key，以及模拟 vendor 清理失败/超时的测试。
  CSI 的忽略项随后显式运行：同一进程三次打开、各取 10 帧再关闭，1 项通过（2.05 秒）。
  USB 实物项本轮未重跑。Mac 的 mediad / params 32＋98 项以及 CLI 1 项也通过。
  默认 ARM 依赖树不包含新增 RTSP-server Rust 绑定；不据此宣称 ARM 实物回归完成。
- SDK 的 720p H.264 通过 SSH 隧道在 Mac 解码：一轮约 10 秒 / 300 帧；
  ffplay 可显示实时画面。
- 保持 ffplay 连接，再用带 `?viewer=2` 的 URL 加入第二个解码客户端，约 8 秒 / 238 帧；
  同一个 factory key 避免 URL 参数触发第二个 sensor。
- 上述 SDK 运行在硬 cgroup `AllowedCPUs=0-2,4` 中，采样的全部用户线程均受限。
  当时有测试编译并发，**这里只作功能验证，不是性能基准或端到端延迟报告**。
- 端口占用时新进程以 exit 1 拒绝，未另开 sensor。
- 在约 17:02 的一次运行中，ISP 出现中断超时，退出清理等待；随后约 17:03 板卡发生重启，
  最后一轮编译测试和 SSH 中断。该次重启原因未确定，接线状态需要排查；
  journal 不足以独立确定具体原因，不能直接归因于 SDK，也不能忽略为正常退出。
  该轮不计作通过的关闭/稳定性验收，退出超时保护仍保留。
- 重启后，未改变编码/采集设置的短运行重新出帧，15 秒到期时主动断开正在拉流的客户端，
  SDK exit 0，日志包含 `sensor stream off` / `finish power off`；板卡 boot ID 未变。
  短测首帧等待占用部分运行时长，因此只解码约 2.3 秒 / 70 帧，不把它记录为完整 4 秒验收。

<a id="reconnect-and-shutdown-fixes"></a>
### 重连和退出修复

接线线索不替代软件排查：板卡 boot ID 保持不变时，早期版本仍复现冷重连失败。
保留的负面结果及修复如下：

- 不发送 EOS 直接进入 NULL，一轮关闭阻塞约 37 秒，下一客户端的 OPTIONS 请求超时。
- 仅开启 EOS，立即重连会拿到还在退出的缓存媒体，出现 RTSP 503。
- 尝试复用已经结束的 vendor GstBin，第二次虽可协商 RTSP，但没有有效视频帧；未采用此路径。
- 最终采用 **EOS 排空 + 等待关闭信号 + 每轮新的共享缓存 key / GstBin**。
  同时在线的客户端仍共享一路 sensor；不会因 URL query 不同而另开相机。
  服务退出时还会显式移除 session，避免残留引用阻碍释放。

修复后的原生 Rust 工具验收：

- 同一进程连续三轮“播放 3 秒 → 断开 → 立即重连”，每轮解码 91 帧，全部 exit 0。
- 无编译/其他业务并发、硬 cgroup `AllowedCPUs=0-2,4`，采样的全部用户线程均受限。
  720p 单客户端拉流 20 秒得到 **599 帧**；解码 PTS 跨度 19.975056 秒，平均 **29.937 fps**。
  客户端使用 passthrough 帧率和 demux 时间基；首两帧 PTS 都为 0，之后递增。
  该启动时间戳现象尚未定位，不宣称零时间戳问题、零丢帧或已测得端到端延迟。
- 两个 TCP 客户端同时拉流（其中一个 URL 带 `?viewer=2`），分别解码 891 / 829 帧。
  在两者仍在线时向 SDK 发 SIGINT，客户端收到 EOF，SDK **exit 0**，日志确认 sensor stream off / power off。
  此轮有软件测试编译并发，只算多客户端和退出的功能验证，不作为性能基准。
- 随后再次打开相机，`--duration 12` 到期主动断开客户端，解码 360 帧，SDK exit 0，
  日志确认正常关流和断电；与 SIGINT 退出路径分别验证。
- 上述轮次 boot ID 均保持 `385fa665-52e8-423f-96b4-7cfd0af98659`，没有再借助重启恢复相机。

板端日志：`/root/workspace/microduck/target/imx219-rtsp.59nbn6/`。
重连/退出验收见 `generation-acceptance.log`；Mac 解码记录为仓库 `target/rtsp-rust-generation-pass-*.log`、
`target/rtsp-generation-final-frames.md5` 和 `target/rtsp-generation-overlap-*.log`。
这些有限轮次不替代长时间稳定性、热插拔、网络异常和整机并发测试。
