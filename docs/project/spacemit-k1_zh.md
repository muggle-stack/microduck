<a id="spacemit-k1-native-sdk-regression"></a>
# SpaceMIT K1：原生 SDK 回归记录

[English](spacemit-k1.md) · [简体中文](spacemit-k1_zh.md)

> 本文按测试日期记录实验结果与已知问题。当前功能状态见[适配进度](spacemit-k1-adaptation-zh.md)，已集成的相机使用方法见 [IMX219](k1-imx219_zh.md) / [WebRTC](k1-webrtc_zh.md) 指南。

历史开发分支：[muggle-stack/microduck:spacemit-k1](https://github.com/muggle-stack/microduck/tree/spacemit-k1)，
基于 2026-09-05 拉取的上游 bc41fb5。
本页记录原生构建与软件回归，不表示 Radxa HAT、相机、无线或安装镜像可直接互换。

新用户从[原生编译与开发安装](../robot/install-k1_zh.md)开始；安装流程由该页维护，
本页保留带日期的实测和 bring-up 记录。
2026-09-06 再检查官方 main 仍为 bc41fb5，当时安装指南无需额外 rebase。

<a id="build-on-the-k1"></a>
## 在 K1 构建

Rust 目标为 **`riscv64gc-unknown-linux-gnu`**。
Bianbu 的 uname -m 输出 riscv64，不是 Rust target triple。
板上 C 编译器和已安装 GStreamer 开发文件提供原生 sysroot。

```sh
cd /root/workspace/microduck
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
cargo k1 --locked --bins -j 2
```

隔离 Rust 不替换 /usr/bin/rustc 或 apt 包。
cargo board 仍交叉构建 aarch64；cargo k1 是 **K1 原生命令**，
在 Mac 运行不会提供 RISC-V Linux 链接器或板上 GStreamer 库。

K1 全 workspace release/test 冷构建超过一小时。
编辑时要快速反馈，可只检查受影响 crate，不启用 release 优化：

```sh
cargo check --locked --target riscv64gc-unknown-linux-gnu -p duck-control
cargo test --locked --target riscv64gc-unknown-linux-gnu -p duck-control
```

Linux 依赖为 libudev-dev、libgstreamer1.0-dev、
libgstreamer-plugins-base1.0-dev、libgstreamer-plugins-bad1.0-dev。
先检查 apt dry run：已测 Bianbu 2.1.1 默认候选会同时升级多媒体/Wayland 运行时。
当时安装匹配开发包，保留全部已装运行时版本：

| 开发包 | 使用版本 |
| --- | --- |
| libudev-dev | 255.4-1ubuntu8bb2 |
| libgstreamer1.0-dev | 1.24.2-1bb3 |
| libgstreamer-plugins-base1.0-dev | 1.24.2-1ubuntu0.1bb2 |
| libgstreamer-plugins-bad1.0-dev | 1.24.2-1ubuntu4bb10 |
| libwayland-dev / libwayland-bin | 1.22.0-2.1build1 |

匹配的 libgstreamer-plugins-bad1.0-dev 与 libgstreamer-opencv1.0-0
仍位于官方 Bianbu pool/universe/g/gst-plugins-bad1.0/，
尽管当时 apt 索引只列更高 vendor bb18。
安装匹配依赖新增 17 包，无升级、无删除。

<a id="development-installation-guide-check--2026-09-06"></a>
### 开发安装指南复验——2026-09-06

[开发安装](../robot/install-k1_zh.md)用 cc7c471 原生 SDK 复验，
新前缀为 `target/k1-install-doc-20260906.CpdgHg/installed/`，不使用用户持久目录。
缓存完整 --bins release 构建 2.63 秒通过；
隔离身份目录下 **12** 个已安装守护程序/诊断工具 --help 通过，
安装程序与构建产物逐字节一致（cmp exit 0）。

固定版本私有 MPP 再构建通过。复制组件后，ldd 将 MPP 解析到新前缀内、
OpenCV 解析到 /opt/opencv-spacemit，无缺库。
迁移后的图像测试过 **20 项逐字节合成检查和 13 项非法输入检查**；
使用 V2D，不打开相机。
apt 模拟仍拟升级五个多媒体包并新增两个包，未执行。
本次未重装 Rust、升级 apt、打开麦克风/相机、部署机器人或重启服务。
官方 Rust checksum、独立 1.89.0 与 apt 1.75.0 共存另行核对；
仅文档改动没有重做新镜像安装或完整 workspace 测试。
verification.log、apt-simulation.log、relocated-library-paths.log 保留在上述目录。

<a id="repeat-the-software-checks"></a>
## 重复软件检查

```sh
sh scripts/k1-test.sh
```

脚本在 RISC-V target 执行 workspace release 测试、构建板端程序，
并对七个守护程序、robotctl、camera-check 执行 --help 检查加载。
现有 robotd 集成测试用 --fake，在无舵机总线时验证 IPC 和更新健康门控。
脚本不安装 systemd、不运行 provisioning、不改网络或启用舵机。

构建/测试并发默认均为 2，限制 4 GB 板的内存和调度争用。
可用 K1_BUILD_JOBS、K1_TEST_THREADS 覆盖。
依赖已下载时 CARGO_NET_OFFLINE=true 可用；也可从主机复制 crate 源码/缓存，
但编译与执行仍发生在 K1。

<a id="runtime-present-test-fixes"></a>
### 安装真实运行时后暴露的测试修正

首轮 K1 为 **1,245 通过、2 失败、6 忽略**。
两处都是已安装 ONNX Runtime 暴露的 fixture 问题，不是 RISC-V 编译失败：

- setting_a_skill_keeps_what_the_call_left_out 用文本文件冒充 ONNX。
  缺运行时时会走到预期合并逻辑，真实运行时正确拒绝它。
  改用小型有效 Gather 图。
- an_unloadable_policy_holds_the_pose_and_reports_why 用不存在的 override。
  启动校验丢弃 override 并加载板上默认，结果依赖安装的策略文件。
  改用 shape 合法但 warm-up 推理失败的图，独立于板上默认验证失败/保持姿态/health 契约。

两份 fixture 都是内嵌 protobuf 字节，无下载或 Python 依赖。
生产校验、override fallback 和失败处理逻辑未改。
原失败 target/k1-regression.log 保留，复测另存 target/k1-regression-fixed.log。

修复后全部 52 suites 通过：K1 / Rust 1.89 release
**1,247 通过、零失败、6 忽略**。
对应 macOS / Rust 1.93 为 **1,215 通过、零失败、6 忽略**。
差异来自 Linux 专属代码/测试；主机结果不验证 Linux 守护程序。
忽略项为既有两个 kinematics timing probe、四个 robotctl timing/visual probe，不是新排除的失败。

主机 Rust 1.93 Clippy 还发现上游 chorale 测试重复 #[cfg(test)]。
去掉冗余属性使 robotd --tests 在 -D warnings 下通过，不改生产二进制或测试选择。

独立生产 cargo k1 --locked --bins -j 4 也通过，八个加载检查通过。
首次非测试 feature 集合构建 30m26s，早期完整测试构建 88m00s；
最终缓存回归构建阶段测试 2.99s、生产 2.59s，测试执行时间另计。
修订后的完整脚本连同下方策略推理 exit 0。

<a id="validate-real-policy-inference"></a>
## 验证真实策略推理

该上游版本从
[`pollen-robotics/microduck-policies`](https://huggingface.co/pollen-robotics/microduck-policies)
下载策略，不放在 Git。scripts/seed-policies.sh 可填充独立测试目录，
必须显式传目录，避免触碰已安装策略。

此网络中 K1 连接 Hub TLS 超时，Mac seeding 单文件八秒期限也曾过期。
后来在 Mac 用更长期限下载九个 v1 文件，复制到 K1 target/k1-policy-files/；
九个都与旧 2c61dcc 工作副本逐字节相同（cmp exit 0）。
未改生产 update hook 的下载超时。

```sh
sh scripts/k1-test.sh target/k1-policy-files 200
# Or only the model check, after building:
target/riscv64gc-unknown-linux-gnu/release/examples/policy-bench target/k1-policy-files 200
```

回归脚本复用 cargo test --workspace 构建的 example。
另用 cargo run -p duck-control --example policy-bench 会选择更窄 feature 集合并重编，
回归后无需再构建。
只想构建此例的新仓库可先执行
cargo k1 --locked -p duck-control --example policy-bench -j 2。

policy-bench 使用 SDK Policy::load / Policy::infer，包含 shape 校验与单线程 CPU session。
每模型输入固定直立 home-pose observation，检查动作有限；
加载后预热二十次，再报告指定次数的 mean/P50/P95/P99/max。
不打开串口或写硬件动作。计时不含加载，也不测步态质量、UART 或完整 50 Hz 循环。

这是运行时实测，不是加速声明。未量化/改权重；
动作有限，但未比较与 Radxa 或之前运行时的逐字节等价。

<a id="k1-results-2026-09-05"></a>
### K1 结果：2026-09-05

Bianbu 2.1.1、4 GB、八核可用、performance governor 1.6 GHz、无 affinity。
CPU 执行、一个 intra-op 线程，未加 SpaceMIT EP。
加载 /usr/lib/libonnxruntime.so.1.24.2+spacemit.a1，不是 Python 单独运行时。
编译先退出，各模型独立加载，20 预热＋200 推理，单位 ms。

| 模型 | 平均 | P95 | P99 | 最大 |
| --- | ---: | ---: | ---: | ---: |
| alpha_ground_pick | 0.9323 | 1.0288 | 1.0890 | 1.1042 |
| alpha_sitstand | 0.9355 | 1.0345 | 1.1022 | 3.2334 |
| alpha_stand | 0.8817 | 0.9637 | 0.9888 | 1.1972 |
| alpha_walking | 0.9055 | 0.9781 | 0.9912 | 0.9916 |
| ball_kick_left | 0.8965 | 0.9765 | 0.9937 | 0.9962 |
| ball_kick_right | 0.9033 | 0.9758 | 0.9876 | 0.9942 |
| roller | 0.8889 | 0.9660 | 0.9792 | 0.9838 |
| roller_crouch | 0.8991 | 0.9758 | 0.9883 | 1.0583 |
| roulade | 0.8764 | 0.9564 | 0.9668 | 0.9690 |

动作全部有限，3.2334 ms sit/stand 离群值保留未剔除。
该样本虽低于 20 ms tick，但基准未包含真实总线/外设。
含 P50 的完整 CSV 在 target/k1-regression-fixed.log 末尾。

<a id="native-startup-check"></a>
## 原生启动检查

生成的 robotd 是 ELF64 RISC-V、LP64D，K1 --version 报 0.10.0。
独立 robotd --fake 通过真实 ORT 加载 walking 模式全部七个策略槽，
robot.health 返回 healthy: true，robot.policies 无槽位错误。

功能检查使用隔离 socket/runtime，关闭音频/theremin，
除 --fake 外还把串口设为故意不存在的路径。
当时仍有编译并发，检查后干净退出。
只证明启动、策略加载、IPC，**不是**持续循环时序或真实舵机。
临时 fixture/log 为板上 target/k1-fake.toml、target/k1-fake.log。

完整回归后第二个隔离 --fake 进程通过 IPC 启用，
每约 20 ms 发送 vx = 0.2 m/s。
启用预热三秒，30.008 秒窗口完成 **1,500 ticks（49.986 Hz）、零 missed deadline**。
五个健康样本 49.978–50.041 Hz，全部及最终结果 healthy。
最终 robot.state 为 policy: "walk"，前进指令已应用，关节/目标值有限。
`/proc/<pid>/maps` 确认上述运行库，随后 disable 并干净停止。

仍然是**真实 K1 上的 fake IO**，不是舵机总线、IMU 或步态验证。
没有其他媒体/计算并发。
日志 target/k1-loop.log、IPC target/k1-loop-result.log，
fixture/log 属 ignored 产物，不是安装配置或提交的模型。

<a id="es8326-audio"></a>
## ES8326 音频

使用 K1 现有 ES8326 驱动/Mixer。板级录放音检查使用 `hw:1,0`，
硬件格式为 48 kHz / S16_LE / 双声道；SDK 全双工测试见下文。
SDK 使用稳定 ALSA card ID **sndes8326**，而非重启可变的卡号。
这是可选板级 profile，原 Radxa/AIC3104 默认不变。

```sh
# K1, as root. No apt install, kernel/DT change, mixer write or service restart.
sh scripts/setup-k1-audio.sh
```

把 deploy/audio/es8326.conf 安装为 /etc/alsa/conf.d/99-microduck-es8326.conf，
不修改 pcm.default、PipeWire、/etc/asound.conf 或用户 .asoundrc。
无机器人配置时还用 deploy/k1/robotd-audio.toml 创建 /etc/robot/robotd.toml。
已有配置**永不覆盖**，手动合并进原 [audio]：

```toml
[audio]
enabled = true
device = "microduck_es8326"
```

这只是音频配置，不是 K1 UART/HAT 部署。
**不要**执行 Radxa setup-board.sh 的音频段；
其 AIC3X DKMS、Rockchip kernel 和设备树 overlay 与已工作的 K1 codec 无关。
本地改过的 ES8326 profile 也保留，提示手动合并。
不生成音色库或打开麦克风监控。
已部署 SDK 用 sounds ensure-bank；开发音色库可放 audio.bank 指定的位置。
显式 audio.pet_detect = true 和有效 audio.pet_model 启用现有麦克风 worker，默认关闭。

<a id="why-a-pcm-profile-is-needed"></a>
### 为什么需要 PCM profile

SDK 播放 48 kHz mono S16_LE、录制 16 kHz mono S16_LE。
直接 plughw 单独可用，但**无论哪个先启动，全双工都失败**：
第二路无法设置不同硬件采样率。
把两路硬件固定为 48 kHz 双声道可避免时钟冲突。
ALSA [plug 转换与 mmap_emul 插件](https://www.alsa-project.org/alsa-doc/alsa-lib/pcm_plugins.html)
适配 SDK 格式，不修改 DSP、模型或子进程命令。

显式 mmap_emul 很重要：此 Bianbu PCM 只宣告 RW_INTERLEAVED，
仅 plug 固定 rate/channels 也 hw_params 失败。
已测硬件为 48 kHz、period 1,024 帧、buffer 4,096 帧（21.33 / 85.33 ms），
即使 live synth 请求 10 / 40 ms 也是如此。
这些是 ALSA 缓冲参数，**不是实测端到端音频延迟**。

单声道播放复制到两个输出；双声道录音平均后重采样到模型 16 kHz 单声道，
不与原始立体声逐字节一致。模型/量化/阈值不变，
不同麦克风/外壳的抚摸精度需声学测试。

AudioParams::capture_device() 现在原样保留命名 PCM，
不再把 microduck_es8326 改成非法 microduck_es8326,0。
原 plughw:aic3104 与显式 hw/plughw 的 device-0 规格保持原行为。
sounds CLI 原本已支持 --device microduck_es8326，Radxa 默认有意保留。

<a id="repeat-the-hardware-format-check"></a>
### 重复硬件格式检查

```sh
# Opens the real mic for two six-second captures; audio is discarded on exit.
# Playback is silence. Refuses a busy card; all children have bounded timeouts.
sh scripts/k1-audio-test.sh
```

录制先/播放先两种顺序都通过：SDK 请求格式，每次录到 96,000 单声道样本，
硬件两路均 48 kHz 双声道，无 ALSA 错误或 xrun。
独立六秒录音墙钟 6.37 秒，含进程/设备启动。
部分检查与两任务 Rust 构建并发，因此是功能验证，不是 CPU/延迟基准。
k1-test.sh 保持仅软件，不隐式开麦克风。

初始失败 plug 与通过的 emulation 日志在主机
target/k1-es8326-duplex.log、target/k1-es8326-duplex-mmap-emul.log。
测试脚本不保留麦克风音频。

<a id="sdk-integration-and-regression-results"></a>
### SDK 集成与回归结果

重建原生 robotd 使用 --fake --no-policy、隔离 IPC/runtime、ES8326 PCM、
真实 pet-detect/models/pet_detect.onnx 和生成的测试音色库。
robotctl quack 经 **SDK 自身**音频路径播放，麦克风 worker 继续录音。
/proc/asound 和子进程命令确认两路均命名 PCM；
麦克风 PID 不变、硬件指针持续前进。
最终 healthy、50.012 Hz、零 missed tick。
这是策略关闭的短时 fake-IO 集成，不是行走/负载基准。

仅停 robotd 后，在有界清理窗口内释放两路 PCM，无麦克风重启。
首个 harness 把 SIGINT 发给整个 timeout 进程组（包括 arecord）并立即检查，
导致退出瞬间 restart/SETUP，不采用为干净关闭结果；两轮日志都保留。

另五秒 live capture 输入 SDK 独立 pet-detect 产生推理结果，不是抚摸精度测试。
三秒单声道电平检查为 48,000 样本、43,819 非零、peak 156、RMS 5.381（signed-16 单位），无削波。
这是该房间观测，不是规定的麦克风增益。

修改后 K1 / Rust 1.89 release **1,249 通过、零失败、6 个已有忽略**；
macOS / Rust 1.93 **1,217 通过、零失败、6 忽略**。
K1 增量 release 编译 20m08s。格式、ShellCheck、受影响 params/robotd 主机 Clippy 通过。
更广 Clippy 遇到上游未改 monitor.rs:2188 的 nonminimal_bool，
此音频变更未修改无关代码。

主机日志 target/k1-es8326-tests.log、target/k1-es8326-pet-detect.log、
target/k1-es8326-sdk-recheck.log；
板端 target/k1-es8326-robotd-recheck.log。
测试音色库/配置在 target/k1-es8326-*，不属于已跟踪文件。

回归后用 sounds ensure-bank 填充之前缺失的默认音色库：
/var/lib/robot/sounds 中 82 个声音，以本板硬件身份为种子。
已装仅音频配置仍关闭麦克风监控，未启动/启用 systemd 服务。

<a id="rust-ort--spacemit-vision-backend"></a>
## Rust ORT + SpaceMIT 视觉后端

可选 EP 已通过 duck-detect、[detect]、robotctl configure 接入 mediad。
CPU/双线程与 RKNN 默认保留。使用兼容 opset 17 浮点模型，
不是原 opset 12 或通用 COCO，无需 Python worker。

[SDK 集成记录](k1-duck-ort-ep_zh.md)记录原生 Rust 与精确性：
硬 cgroup 下 RGB 前处理＋推理＋NMS 均值两核 **243.0 ms**、四核 **121.3 ms**。
实验 INT8 四核 **33.9 ms**，但输出未经精度验收。
成功 EP profile 都执行 SpaceMIT 融合节点、无 CPU 计算节点。

该运行时需 worker CPU 0-3 与调用线程 CPU 4-7。
四核实测为三个 worker 0-2 加调用线程 CPU 4；
EP 重绑调用线程，taskset 单独不控制总预算。
报告包括可复现 cpuset 命令、失败探测及早期独立实验核数修正。

<a id="usb-camera-mono-and-packed-stereo"></a>
## USB 相机：单目与拼接双目

显式 camera.acceleration = "spacemit" 可用私有 MPP codec2 JPEG 硬解、
V2D 裁剪/补边/旋转及 SpaceMIT OpenCV UYVY 打包。
不替换系统 MPP、不成为默认。
见[构建、像素对照、SDK 实测与边界](k1-mpp-camera_zh.md)；
下方纯软件结果早于该可选后端。

USB 后端向 SDK 已有选眼媒体/检测供帧，
camera-check 在当时仍缺的 WebRTC 插件之外验证相同 source/detector。
原 Radxa/IMX219 源仍默认，K1 CSI/ISP 是单独工作。

当前验收是**选一眼**，物理拼接双目也一样。
双眼提取/推理只是额外诊断，不是要求。
最终 MPP 构建下四核硬预算：选眼 720p **30.05 fps**，
实时检测均值 **192.8 ms**、2 Hz。
匹配软件基线、重复波动和像素差异见 MPP 报告。

DECXIN 原生宣告 MJPEG 4000×1200 @30；
左右选择、单目裁剪、成对提取、非法模式/设备处理及释放通过。
选眼 1280×720 软件为 5.3–5.4 fps，
两路原生 1920×1200 ROI 为 8.81 对/秒，**都不是已解码视频 30 fps 的声明**。

硬四核与同一浮点 EP 模型，六十次单眼推理约 2 Hz；
前处理＋推理＋NMS 均值 **253.7 ms**、P95 **264.5 ms**，采集并行。
独立实时 profile 为 SpaceMIT 节点、无 CPU 节点。
短测双眼只有 1.96 对/秒，稳定双眼 2 Hz 未验收，未改权重/精度默认值。

后续 profile 中约 96% 的检测时间在 EP 内；
原融合 RGB 字节前处理约 3 ms，
独立 SpaceMIT OpenCV 直接三操作替换约 4.5 ms。
同硬四核固定帧关/开/关相机负载，模型调用从 123 → 235 → 125 ms，
指向采集资源争用而非该前处理循环。
该探测未安装 OpenCV 或修改前处理默认项。

见 [USB 配置、精确性、实测与文件清单](k1-usb-camera_zh.md)。
仅测试一台拼接双目实物，单目用其左 ROI，
未添加双目深度/标定或独立 USB 相机同步。

<a id="outside-this-software-check"></a>
## 本轮软件检查之外

- 真实 Dynamixel 半双工 UART、十五舵机与 IMU 反馈。
- 当时尚未接入的 CSI/sensor/ISP 与曝光控制。
- 当时尚未接入的 K1 H.264 编码器及缺失的 webrtcsink。
- 标注检测精度与视觉/控制/媒体并发；实时 USB 已进入 Rust EP，INT8 非默认。
- 真实 ToF、蓝牙控制器/手柄；ES8326 已覆盖，但具体麦克风/外壳抚摸精度未覆盖。
- RISC-V 部署、签名打包、OTA 资产与 CI。
  继承的发布/安装流程仍面向 Radxa/aarch64，不要因为分支名 spacemit-k1 就把那些产物装到 K1。
