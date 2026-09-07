<a id="contributing"></a>
# 参与开发

[English](CONTRIBUTING.md) · [简体中文](CONTRIBUTING_zh.md)

本页面向守护程序开发。使用机器人而非修改它，请先看 [README](README_zh.md)。

<a id="building-and-testing"></a>
## 构建与测试

需要稳定 Rust **1.89+**。上游机器人为 aarch64 Linux；
本 fork 也支持 SpaceMIT K1 / RISC-V 原生开发。
Linux/macOS 两种开发环境并不完全一样，差异见下文。

```bash
cargo test --workspace
```

不需要硬件、网络或 Docker。这些测试通过说明工作副本的软件检查通过。

K1 工具链、依赖和安装见 [K1 开发指南](docs/robot/install-k1_zh.md)。
`cargo k1 --locked --bins -j 2` **在 K1 上**构建 riscv64gc-unknown-linux-gnu，
`sh scripts/k1-test.sh` 执行原生软件回归。
`cargo board` 仍用于随机器人交付的 Radxa 交叉构建，
与下方 aarch64 发布/provisioning 流程相互独立。

**Linux** 上需要先安装 CI 使用的 C 库：
padd 经 gilrs 绑定 libudev，mediad 的 pipeline 受 cfg(target_os = "linux") 控制，
因此 Linux 主机会构建 GStreamer 相关代码。

```bash
sudo apt-get install -y libudev-dev libgstreamer1.0-dev
```

```bash
sudo apt-get install -y libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev
```

合成 USB 测试还需要 gstreamer1.0-plugins-base 和 gstreamer1.0-plugins-good。
Bianbu 安装/升级前，按 K1 指南审阅 apt 模拟，确保头文件匹配现有 vendor 运行时。

**macOS** 上，上述测试命令即可；原上游说明的基线为 **942 项测试通过**，未排除测试。
ToF 驱动自身的两个测试不在 Mac 执行，因为没有该驱动：
vendor/platform.c 依赖 linux/i2c.h，build.rs 仅为 Linux 编译，
sensor.rs 在其他平台提供不能打开的 Sensor。
tofd 仍可构建并通过 tofd --fake 提供帧，这是它离板运行的方式。

失败处理也在测试里：签名错误、更新后不健康、安装后 hook 失败、
原子切换到健康门控之间断电，均通过真实引擎注入故障而非 mock。
因此 updater/tests/apply.rs 比手工运行更准确地回答“到底保证什么”。

单个 crate 与格式化：

```bash
cargo test -p <crate>
```

```bash
cargo fmt --all
```

configd 的 NetworkManager 客户端与 btd 的 BlueZ 客户端是 **Linux 专属**；
主机构建/测试通过不证明它们可用。Lint 应针对板端 target，否则可能漏掉问题：

```bash
RUSTFLAGS="-D warnings" cargo clippy -p configd --all-targets --target aarch64-unknown-linux-gnu
```

scripts/board-test.sh 在 CI 对实际交付用户空间交叉构建并执行 60 项断言：
回滚、拒绝篡改产物、启动计数恢复、socket 权限、peer credential 授权、
以及 setup-board.sh/install.sh 对板卡的行为。环境为 Debian 13 Trixie；
BOARD_IMAGES= 可改测其他镜像。

不发布而直接在真机测试时，
`scripts/dev-push.sh <user@board>` 在本机构建，再作为普通门控更新安装。
默认用 cargo zigbuild，也可 --docker 在板端用户空间构建，无需先配置工具链。
参数、安装和失败模式见 [Radxa 开发推送（英文）](docs/robot/dev-push.md)。
此流程不替代 K1 安装指南。

<a id="the-layout"></a>
## 仓库布局

```
the daemons — one crate each, one unit each, all in the same release artifact
  robotd/         control daemon: the 50 Hz loop, the voice, the theremin, the chorale
  updater/        engine + updaterd
  configd/        wifi · robot name · pairing PIN · reboot · gamepad pairing
  btd/            the BLE front door
  padd/           gamepad → intents — an ordinary socket client, no privileged access
  mediad/         camera, mic, WebRTC, the remote gateway, and the console it serves
  tof/            tofd: the head's 8×8 depth sensor. Publishes frames, reads nothing

the libraries they drive — no sockets, no systemd, nothing starts them
  duck-ipc-proto/ the wire contract
  duck-control/   the control core: model · bus · IMU · observations · policy · safety
  kinematics/     the MJCF model and forward kinematics; head and hand chains
  odometry/       where the robot has been, from foot contacts and the IMU
  sounds/         synthesis, per-robot voice personality, the chorale's score
  pet-detect/     a small CNN that hears head scratches on the onboard mic
  robotd-params/  robotd's startup parameters: schema, defaults, validation

the tools
  robotctl/       the local CLI, including `monitor`
  duckctl/        the laptop-side client — never shipped, never cross-built
  xtask/          package · sign · promote — build tooling, never shipped
  test-support/   signed-release fixtures for tests; never shipped

deploy/         what a robot is configured with: updater.toml, robotd.toml, trust anchor, journald
hooks/          preinstall · postinstall — what runs inside an update, from the artifact,
                and the only thing that runs on every board on every update: anything
                install.sh does to a board belongs here too (updater-design.md §9.1)
scripts/        provision-board.sh · dev-push.sh + dev-build.Dockerfile (from your machine) ·
                provision.sh → setup-board.sh → setup-gstreamer.sh · setup-rkaiq.sh ·
                migrate-network.sh · install.sh (on the board) ·
                robot-boot-check · robot-rescue (recovery, installed to /usr/local/sbin) ·
                pad-link-test.sh · pad-stack-report.sh (gamepad radio, on the board) ·
                board-test.sh · systemd-test.sh (CI) · cross-sysroot.sh (cross-builds) ·
                bake-duck-mesh.py (the monitor's 3D model, run by hand)
docs/           robot/ (using one) · design/ (how it works) · project/ (roadmap, records) ·
                ideas/ (not designed yet)
```

服务之间使用 Unix socket、每行一个 JSON-RPC 2.0 对象。
契约在 duck-ipc-proto，仅依赖 serde/semver，
因此 btd/robotd 不会引入 updater 的 HTTP/tar/crypto 依赖树。

[架构设计（英文）](docs/design/architecture.md) `1 解释服务职责及如何组合；
[官方路线图（英文）](docs/project/roadmap.md) 记录官方已实现能力。
K1 当前状态看[适配进度](docs/project/spacemit-k1-adaptation-zh.md)。

<a id="conventions"></a>
## 开发约定

- **注释说明为什么，而不只是做了什么。** 原因比某个实现本身更持久。
- **非显然的决策都要有测试**，注释应说明该测试防止哪种失败。
  回滚路径尤其需要，因为它们触发最少、最容易无声退化。
- **优先复用现有 crate**。依赖数量本身不是目标，可维护性才是。
- AI 辅助提交使用 `Assisted-by:` trailer，不用 `Co-Authored-By:`。
- K1 双语文档配对关系由 `docs/i18n.json` 维护；正文链接留在当前语言，
  显式语言切换除外。官方未翻译资料须标注“英文”。
- 修改配对文档时同步另一语言，保留命令、配置、校验值和实测数据；
  提交前运行 `python3 scripts/check-docs-i18n.py`。

<a id="media-in-the-readme"></a>
## README 中的媒体

视频与头图是 **GitHub 附件，不是仓库文件**。
把素材拖进 issue/PR 评论框，GitHub 会生成
`https://github.com/user-attachments/assets/<id>` URL。
无需提交/同步素材文件，clone 里也不会有；没有网络时媒体为空白。

README 的 HTML table 中，URL 必须放入元素，因为 block HTML 内不解析 Markdown，
裸 URL 不会变成视频：

```html
<video src="https://github.com/user-attachments/assets/<id>" controls width="100%"></video>
```

**视频不能自动播放或循环。** 通过 GitHub POST /api/markdown 验证：
src/autoplay/muted/loop/controls/playsinline/preload/poster/width 中，
只有 src、muted、controls、width 保留。
视频需点击；若需自动运动，用动画 GIF 或体积更小的 WebP：

```bash
ffmpeg -i clip.mp4 -vf "fps=15,scale=560:-1:flags=lanczos" -loop 0 -q:v 55 walk.webp
```

循环两三秒可作缩略图；有声音或较长内容用视频。

<a id="releasing"></a>
## 发布

发布在 **CI 中签名**，不在本地签名。入口为 GitHub Releases，tag 决定行为：

| 创建的内容 | CI 行为 |
| --- | --- |
| tag 为 daemon-staging-v0.4.0 的 pre-release | 构建、签名、通过真实更新引擎验证，再发到 staging |
| tag 为 daemon-v0.4.0 的 release | 若已有 staging 0.4.0，提升同一份字节并重新签名；否则直接构建 0.4.0 |

终端推送对应 tag 会触发同样流程：

```bash
git tag daemon-staging-v0.4.0 && git push --tags
```

灰度路径有意分两步：创建 prerelease，在机器人安装，然后创建正式 release。
允许不经过 staging 直接 release，但发布说明会明确：
只在 CI 验证，未在机器人执行。

先改 workspace 版本；xtask package 会拒绝与 Cargo.toml 不符的 tag，
避免报告一个从未运行的版本。

`gh workflow run promote --field version=0.4.0` 可在不创建 release 的情况下做同样的提升，
也是 min_supported 的设置入口。

[CI 发布配置（英文）](docs/project/ci-setup.md)说明密钥保管、secrets 与轮换。
这些继承的签名/发布流程仍面向 Radxa，不表示已有 K1 固件/OTA 发布。
