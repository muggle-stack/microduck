<a id="文档"></a>
# Docs

[English](README.md) · [简体中文](README_zh.md)

The [README](../README.md) is the front door — what a microduck is, and where to go. If you have
one in front of you and want to drive it, start at the [cheat sheet](robot/cheatsheet.md).

It is also where a **publisher** starts: [`policy-manifest.md`](policy-manifest.md) is the
contract for a `manifest.json` beside a microduck `.onnx`, and it owns every field. The design
docs give the reasoning and point at it.

<a id="k1--risc-v-开发"></a>
## K1 / RISC-V development

Start with the installation guide, then follow a camera path. These paired guides stay in the selected language.

| Guide | Contents |
| --- | --- |
| [install-k1.md](robot/install-k1.md) | Native build, isolated Rust and development installation. |
| [spacemit-k1-adaptation.md](project/spacemit-k1-adaptation.md) | Supported platforms, feature progress and remaining acceptance work. |
| [spacemit-k1.md](project/spacemit-k1.md) | Dated native regression, policy and ES8326 bring-up evidence. |
| [k1-duck-ort-ep.md](project/k1-duck-ort-ep.md) | Rust ORT / SpaceMIT EP integration, model and hard CPU budgets. |
| [k1-duck-quantization.md](project/k1-duck-quantization.md) | Historical XSlim experiment, CPU-budget corrections and accuracy limits. |
| [k1-usb-camera.md](project/k1-usb-camera.md) | Selected-eye USB configuration and software-path evidence. |
| [k1-mpp-camera.md](project/k1-mpp-camera.md) | Optional MPP / V2D / OpenCV acceleration and pixel comparisons. |
| [k1-imx219.md](project/k1-imx219.md) | MUSE-Pi-Pro CSI3 capture, model download and live inference. |
| [k1-rtsp.md](project/k1-rtsp.md) | Optional hardware RTSP preview with desktop players. |
| [k1-webrtc.md](project/k1-webrtc.md) | Private plugins, browser streaming and same-source detection. |
| [README.md](../native/k1-webrtc/README.md) | Private plugin provenance, patch and license. |

<a id="robot使用机器人"></a>
## `robot/` — you have a robot

| | |
|---|---|
| [`cheatsheet.md`](robot/cheatsheet.md) | Every `robotctl` command. |
| [`pair-a-gamepad.md`](robot/pair-a-gamepad.md) | Once per pad: pairing mode, `pad pair`, and what to do when it will not bond. |
| [`cheatsheet-dev.md`](robot/cheatsheet-dev.md) | The commands that need a dev board: branch builds, candidates, dev pushes. |
| [`dev-push.md`](robot/dev-push.md) | Build on your machine and install on the board over ssh, with no CI run. |
| [`duckctl.md`](robot/duckctl.md) | Every `duckctl` command — the robot from a laptop, over Bluetooth. |
| [`install-dev.md`](robot/install-dev.md) | Setting up a board for development, from nothing. |
| [`install-by-hand.md`](robot/install-by-hand.md) | The same install as separate commands, for testing one step at a time. |

<a id="design修改守护程序"></a>
## `design/` — you are changing the daemon

How it works and why. These change rarely; when behaviour and a design doc disagree, the doc is
the bug.

**One page owns a mechanism, and the others link to it.** The table below is that assignment: if a
fact belongs to a page listed here, every other page says one sentence and points, rather than
explaining it again. A fact written down in six places drifts in six directions, each of them locally
reasonable — which is how six documents came to promise that `updaterd` and `btd` kept their old
binaries until the next reboot, two releases after they stopped doing so, including the two pages
someone reads while diagnosing exactly that. So when two documents disagree, the one that does not
own the mechanism is the bug.

| | |
|---|---|
| [`architecture.md`](design/architecture.md) | The service split, the IPC contract, state ownership, safety and authority. |
| [`robotd-design.md`](design/robotd-design.md) | The control loop: the Dynamixel bus and who owns the port, the model, sensing, observations, policy, safety — and what else hangs off the tick. |
| [`updater-design.md`](design/updater-design.md) | The update engine: verification, atomic swap, health gate, rollback, release format. |
| [`policy-channel-design.md`](design/policy-channel-design.md) | Where the ONNX policies come from: the `policies` component, trying someone else's, and what `reset` puts back. |
| [`restart-order.md`](design/restart-order.md) | Which unit restarts, at which step, on every path that moves `current` — and at boot. |
| [`app-path-design.md`](design/app-path-design.md) | `btd` and `configd` — how a phone configures a robot over BLE. |
| [`remote-webrtc.md`](design/remote-webrtc.md) | WebRTC sessions, signalling, and the control channel — how a peer drives and observes the robot. |
| [`webrtc-console.md`](design/webrtc-console.md) | The WebRTC client: serving it from the robot, finding the robot, and what the page should be. |
| [`remote-access-design.md`](design/remote-access-design.md) | Reaching a duck from outside the LAN: the Hugging Face account, the device flow, and the bridge to a rendezvous service. |
| [`boot-recovery-net.md`](design/boot-recovery-net.md) | Falling back to golden when the release that booted cannot start its daemons. |

<a id="project项目记录"></a>
## `project/` — you are running the project

Dated records rather than reference. They describe a moment, and go stale on purpose.

| | |
|---|---|
| [`roadmap.md`](project/roadmap.md) | Milestones, and what works today versus what is designed. |
| [`ci-setup.md`](project/ci-setup.md) | One-time setup for the release pipeline: keys, secrets, rotation. |
| [`install-path-gap.md`](project/install-path-gap.md) | Why four install-path bugs reached a board, and what closed it. Closed — the rule it taught is [`updater-design.md`](design/updater-design.md) §9.1. |
| [`slice-2-bringup.md`](project/slice-2-bringup.md) | What a real Radxa Zero 3W did with slice 2. |
| [`update-over-ble.md`](project/update-over-ble.md) | Driving the update path from a phone: what it turned up, and what rollback over a radio was decided on. |
| [`media-bringup.md`](project/media-bringup.md) | What a Radxa Zero 3W does about video: the VPU, what MPP needs, and the two plugins that have to be built. |
| [`pad-minimal-pairing.md`](project/pad-minimal-pairing.md) | The smallest board configuration a gamepad will bond under, found by taking one away at a time. |
| [`idle-cpu.md`](project/idle-cpu.md) | What the daemons do when nobody is asking them to: four things that stopped, two that were measured and left alone, and what still wants a board. |

<a id="ideas尚未设计"></a>
## `ideas/` — not designed yet

Holding pens. Something that is going to need a design doc, written down before it has one, so the
thinking is not lost and does not get mistaken for a decision.

| | |
|---|---|
| [`autonomous_behavior.md`](ideas/autonomous_behavior.md) | The behavior stack: what the runtime's brain has to give up, and the ideas the chorale and theremin work left behind. |

<a id="其他资料"></a>
## Elsewhere

| | |
|---|---|
| [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | Building, testing, repo layout, conventions, releasing. |
| [`project/npu-bringup.md`](project/npu-bringup.md) | The duck detector on the RK3566's NPU: what runs, how to benchmark it, and the frame path that is still missing. |
| [`../deploy/README.md`](../deploy/README.md) | What a robot image is configured with, and what provisioning actually does. |
