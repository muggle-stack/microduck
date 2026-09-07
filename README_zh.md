<p align="center">
  <img src="https://github.com/user-attachments/assets/c2f7c245-8217-46a1-8d1e-e0ba967cd969" alt="microduck" width="820">
</p>

<h1 align="center">Microduck</h1>

<p align="center">
  <a href="README.md">English</a> · <a href="README_zh.md">简体中文</a>
</p>

<p align="center">
  <em>通过强化学习策略运动的小型双足机器人。</em>
</p>

<p align="center">
  <a href="https://pollen-robotics.com/microduck">官方项目</a> ·
  <a href="docs/project/spacemit-k1-adaptation-zh.md">功能适配进度</a> ·
  <a href="docs/robot/install-k1.md">K1 编译安装</a> ·
  <a href="https://github.com/pollen-robotics/microduck_rl">策略训练</a> ·
  <a href="CONTRIBUTING.md">参与开发</a>
</p>

---

**这个仓库是 Microduck 的“大脑”。** 原项目的机器人约高 25 cm、重 800 g，使用
Radxa Zero 3W / Rockchip RK3566，通过 50 Hz 控制循环和神经网络策略驱动 15 个舵机，
并提供相机、音频、无线控制和软件更新等能力。

本仓库是 [muggle-stack/microduck](https://github.com/muggle-stack/microduck) 维护的 fork，
在保留官方主要后端的基础上，已将 **SpaceMIT K1 / RISC-V** 开发支持合入本 fork 的 `main` 分支。
策略训练由独立的 [microduck_rl](https://github.com/pollen-robotics/microduck_rl) 项目负责，
其中包含 MuJoCo、PPO、仿真到实物迁移和 ONNX 导出；本仓库负责运行 SDK。

## SpaceMIT K1 适配

**支持的平台、已验证功能和接下来的工作，请统一查看 [中文适配进度文档](docs/project/spacemit-k1-adaptation-zh.md)。**
进度文档区分源码接入、K1 板级验证和真实机器人验收，功能数量和剩余工作以该文档为准。

当前已提供原生 Rust 构建与开发安装、策略推理验证、Rust ORT / SpaceMIT EP 视觉后端、
USB 单眼采集及可选 MPP / V2D / OpenCV 加速、指定 MUSE-Pi-Pro / CSI3 IMX219 采集、
RTSP 预览、WebRTC 图传与同源检测，以及 ES8326 音频支持。
物理上是双目相机时，可以选择其中一眼使用，不要求双眼同时推理。
EP 兼容模型已在独立的 [模型 Release](https://github.com/muggle-stack/microduck/releases/tag/models-duck-detect-v1)
提供下载与校验材料，普通 git clone 不会自动下载模型。

**当前是开发适配版本，不是已完成整机验收的 K1 镜像。** IMX219 当前画质和稳定性已获用户确认；
真实运动硬件、音视频与多观众图传、ToF、蓝牙/手柄及产品化部署仍需继续推进。
原项目的演示和功能介绍不代表这些能力已经在 K1 实物上通过验收。

## 原项目演示

以下视频来自官方项目，用于展示 Microduck 的原有能力，不作为 K1 适配验收证据。

<table>
<tr>
<td width="50%">
  <video src="https://github.com/user-attachments/assets/356a6011-8e0d-4b28-bda9-da78646583a3" controls width="100%"></video>
</td>
<td width="50%">
  <video src="https://github.com/user-attachments/assets/abfbf250-1b1c-42cb-8430-00267e2b148a" controls width="100%"></video>
</td>
</tr>
<tr>
<td><b>行走。</b> 通过手柄控制机器人运动。</td>
<td><b>轮式运动。</b> 更换轮式配置，使用对应策略。</td>
</tr>
<tr>
<td width="50%">
  <video src="https://github.com/user-attachments/assets/7e70c1da-e120-428f-ae0b-f4de62f25984" controls width="100%"></video>
</td>
<td width="50%">
  <video src="https://github.com/user-attachments/assets/3eef63a5-6f84-47cf-90de-e717e6d7f8f0" controls width="100%"></video>
</td>
</tr>
<tr>
<td><b>拾取物体。</b> 使用喙部执行拾取动作。</td>
<td><b>跌倒恢复。</b> 从跌倒状态重新站起。</td>
</tr>
</table>

## 从哪里开始

| 文档 | 内容 |
| --- | --- |
| [中文适配进度](docs/project/spacemit-k1-adaptation-zh.md) | 当前平台、功能状态、后续计划和验收边界。 |
| [K1 编译与开发安装](docs/robot/install-k1.md) | 独立 Rust 1.89、Bianbu 依赖、原生编译、隔离安装及外设入口；详细步骤为英文。 |
| [K1 板级适配记录](docs/project/spacemit-k1.md) | 策略、控制软件、ES8326、视觉和安装的实测记录。 |
| [Rust ORT / SpaceMIT EP](docs/project/k1-duck-ort-ep.md) | 检测模型要求、配置、CPU 预算和性能验证。 |
| [USB 相机](docs/project/k1-usb-camera.md) / [MPP 加速](docs/project/k1-mpp-camera.md) | 原生模式、选眼配置、硬件处理和输出差异。 |
| [IMX219 采集与检测](docs/project/k1-imx219.md) | 指定 MUSE-Pi-Pro / CSI3 的 ISP 输入、模型下载和 ORT / EP 检测。 |
| [WebRTC 图传与检测](docs/project/k1-webrtc.md) / [RTSP 预览](docs/project/k1-rtsp.md) | 电脑浏览器同源图传/检测、私有插件构建与独立播放器预览。 |
| [机器人命令速查](docs/robot/cheatsheet.md) | 官方 SDK 的控制、配置、音频、网络、更新和日志命令；K1 可用范围见适配进度。 |
| [系统架构](docs/design/architecture.md) | 服务职责、总线、IPC 和更新流程。 |
| [开发说明](CONTRIBUTING.md) / [文档索引](docs/README.md) | 构建测试约定与全部文档入口。 |

### 在 K1 上编译

先按 [安装指南](docs/robot/install-k1.md) 准备好工具链和匹配的 Bianbu 开发依赖，
再在 **K1 的仓库根目录**执行：

```sh
export PATH="/opt/microduck-rust-1.89.0/bin:$PATH"
cargo k1 --locked --bins -j 2
```

RISC-V 目标为 `riscv64gc-unknown-linux-gnu`。`cargo k1` 使用 K1 本机编译器和库，
不能在 Mac 上直接代替完整的交叉编译环境；原来的 `cargo board` 仍面向 Radxa/aarch64。

软件回归入口：

```sh
sh scripts/k1-test.sh
```

该脚本不执行整机部署，也不会启用真实舵机。冷构建可能超过一小时；具体测试范围、
原生运行库和模型要求见安装指南。源码构建不会自动生成 EP 兼容视觉模型或启用加速后端。

**不要在 K1 上直接运行 Radxa 专用的 `setup-board.sh`、`provision-board.sh`、`install.sh`，
也不要安装 aarch64 发布包。** 原生开发安装与系统服务部署、签名发布和 OTA 是不同阶段。

## SDK 如何工作

SDK 使用一个 Rust workspace，核心由多个独立进程组成：

- `robotd`：控制循环、运动策略、舵机总线和行为执行。
- `updaterd`：签名更新、健康检查与回滚。
- `configd`：网络配置、设备身份和配对相关配置。
- `btd` / `padd`：蓝牙入口及手柄输入。
- `mediad`：相机、视觉检测、WebRTC 与控制台相关功能。
- `tofd`：ToF 深度传感器服务。

进程通过 Unix socket 上的 JSON-RPC 协作，客户端共享 `duck-ipc-proto` 协议。
以上是 SDK 的职责划分，不是所有服务都已在 K1 完成硬件验收。

设计说明放在 [`docs/design/`](docs/design/)，适配和实测记录放在
[`docs/project/`](docs/project/)。贡献代码请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)，
并保留官方后端、明确新路径的启用条件，以可复现的测试结果更新适配进度。
