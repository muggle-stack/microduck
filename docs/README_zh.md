<a id="docs"></a>
# 文档

[English](README.md) · [简体中文](README_zh.md)

[README](../README_zh.md)是项目入口，介绍 Microduck 和阅读路径。
K1 用户先看下方原生安装与适配进度。
已有官方机器人并要操作它，可参考[官方命令速查（英文）](robot/cheatsheet.md)。

策略发布者还需阅读 [policy-manifest.md（英文）](policy-manifest.md)：
它定义 Microduck ONNX 旁 manifest.json 的全部字段，设计文档只解释原因并引用该契约。

<a id="k1--risc-v-development"></a>
## K1 / RISC-V 开发

先安装，再选择相机路径。本节所有指南均有中英文对应版本，正文链接保持当前语言。

| 指南 | 内容 |
| --- | --- |
| [原生编译与安装](robot/install-k1_zh.md) | 独立 Rust、依赖、构建、测试与隔离开发安装。 |
| [平台与适配进度](project/spacemit-k1-adaptation-zh.md) | 平台、功能状态、剩余工作与验收边界。 |
| [板级回归记录](project/spacemit-k1_zh.md) | 原生测试、策略与 ES8326 的历史证据。 |
| [Rust ORT / EP](project/k1-duck-ort-ep_zh.md) | 检测集成、模型要求与硬 CPU 预算。 |
| [XSlim 量化实验](project/k1-duck-quantization_zh.md) | 历史实验、核数更正与精度限制。 |
| [USB 选眼采集](project/k1-usb-camera_zh.md) | USB 配置及软件路径证据。 |
| [MPP / V2D / OpenCV](project/k1-mpp-camera_zh.md) | 可选加速、像素差异与实测。 |
| [IMX219 采集与推理](project/k1-imx219_zh.md) | MUSE-Pi-Pro CSI3、模型下载与实时检测。 |
| [RTSP 预览](project/k1-rtsp_zh.md) | 可选硬编图传与电脑播放器。 |
| [WebRTC 图传与检测](project/k1-webrtc_zh.md) | 私有插件、浏览器与同源推理。 |
| [私有插件说明](../native/k1-webrtc/README_zh.md) | 插件来源、补丁与许可。 |

<a id="robot--you-have-a-robot"></a>
## robot/：使用机器人

以下官方原有页面暂未翻译，链接明确标注英文；不代表其中全部能力已在 K1 验收。

| 官方资料 | 内容 |
| --- | --- |
| [cheatsheet.md（英文）](robot/cheatsheet.md) | 全部 robotctl 命令。 |
| [pair-a-gamepad.md（英文）](robot/pair-a-gamepad.md) | 手柄配对模式、pad pair 与绑定失败处理。 |
| [cheatsheet-dev.md（英文）](robot/cheatsheet-dev.md) | 开发板分支构建、候选版与推送命令。 |
| [dev-push.md（英文）](robot/dev-push.md) | 无需 CI，通过 SSH 从主机安装 Radxa 开发构建。 |
| [duckctl.md（英文）](robot/duckctl.md) | 从笔记本通过蓝牙操作机器人。 |
| [install-dev.md（英文）](robot/install-dev.md) | 从空白 Radxa 开发板开始配置。 |
| [install-by-hand.md（英文）](robot/install-by-hand.md) | 拆分安装命令，便于逐步测试。 |

<a id="design--you-are-changing-the-daemon"></a>
## design/：修改守护程序

说明实现和设计理由，变动较少；行为与设计文档不符时应修正文档。
**一个机制只由一页负责，其他页引用。**
同一事实复制多份容易分别漂移，因此发生冲突时，以负责机制的页面为准。
以下官方设计资料目前提供英文版本。

| 官方设计 | 负责机制 |
| --- | --- |
| [architecture.md（英文）](design/architecture.md) | 服务拆分、IPC、状态所有权、安全与权限。 |
| [robotd-design.md（英文）](design/robotd-design.md) | DXL 总线/串口所有权、模型、感知、策略和控制 tick。 |
| [updater-design.md（英文）](design/updater-design.md) | 验证、原子切换、健康门控、回滚与发布格式。 |
| [policy-channel-design.md（英文）](design/policy-channel-design.md) | ONNX 策略来源、policies 组件、试用与 reset。 |
| [restart-order.md（英文）](design/restart-order.md) | current 切换与启动时各服务的重启顺序。 |
| [app-path-design.md（英文）](design/app-path-design.md) | btd/configd 与手机 BLE 配置。 |
| [remote-webrtc.md（英文）](design/remote-webrtc.md) | WebRTC session、信令、控制与观测。 |
| [webrtc-console.md（英文）](design/webrtc-console.md) | 网页客户端的提供、发现和设计。 |
| [remote-access-design.md（英文）](design/remote-access-design.md) | 局域网外访问、Hugging Face、device flow 和会合服务。 |
| [boot-recovery-net.md（英文）](design/boot-recovery-net.md) | 启动失败时回退 golden。 |

<a id="project--you-are-running-the-project"></a>
## project/：项目记录

带日期的记录不同于当前参考，描述当时状态，因此允许随时间过时。
K1 专属记录见上方双语入口，下方为暂未翻译的官方资料。

| 官方记录 | 内容 |
| --- | --- |
| [roadmap.md（英文）](project/roadmap.md) | 里程碑、已实现与已设计能力。 |
| [ci-setup.md（英文）](project/ci-setup.md) | 发布密钥、secrets、轮换的一次性设置。 |
| [install-path-gap.md（英文）](project/install-path-gap.md) | 四个安装问题的成因与修复；规范归属 [updater-design.md（英文）](design/updater-design.md) §9.1。 |
| [slice-2-bringup.md（英文）](project/slice-2-bringup.md) | Radxa Zero 3W 的 slice 2 板测。 |
| [update-over-ble.md（英文）](project/update-over-ble.md) | 手机更新路径与无线回滚决策。 |
| [media-bringup.md（英文）](project/media-bringup.md) | Radxa VPU、MPP 和所需插件。 |
| [pad-minimal-pairing.md（英文）](project/pad-minimal-pairing.md) | 用删减配置找到最小手柄绑定条件。 |
| [idle-cpu.md（英文）](project/idle-cpu.md) | 空闲 CPU 行为、已优化与待板测事项。 |

<a id="ideas--not-designed-yet"></a>
## ideas/：尚未设计

这里只保存待设计想法，避免丢失思路，也避免被误当成已确定决策。

| 官方想法 | 内容 |
| --- | --- |
| [autonomous_behavior.md（英文）](ideas/autonomous_behavior.md) | 行为栈、runtime 职责与 chorale/theremin 后续想法。 |

<a id="elsewhere"></a>
## 其他资料

| 文档 | 内容 |
| --- | --- |
| [参与开发](../CONTRIBUTING_zh.md) | 构建、测试、布局、约定与发布。 |
| [npu-bringup.md（英文）](project/npu-bringup.md) | RK3566 NPU 检测、基准与当时缺失的帧路径。 |
| [deploy/README.md（英文）](../deploy/README.md) | 官方镜像配置与 provisioning 行为。 |
