<a id="spacemit-k1--risc-v-build-and-install-for-development"></a>
# SpaceMIT K1 / RISC-V：原生编译与开发安装

[English](install-k1.md) · [简体中文](install-k1_zh.md)

本指南适用于 **muggle-stack/microduck 的 `main` 分支**，在 K1/Bianbu 上原生运行。
这里把开发用二进制安装到新的私有目录，不制作完整机器人镜像；不会配置 UART/CSI、
安装 systemd 服务、启用舵机、修改网络或配置签名发布/OTA。继承的这些流程仍面向 Radxa/aarch64。

已验证环境：原 K1 为 Bianbu **2.1.1**，IMX219 使用的 MUSE-Pi-Pro 为 Bianbu **2.3.5**；
SDK Rust **1.89.0**、原生 GStreamer **1.24.2**、`spacemit-onnxruntime` **2.0.6-bpo1+1**
（ORT 1.24.2+spacemit.a1、EP 2.0.6）。可选相机桥接使用 `opencv-spacemit` **4.14.0-1bb3**。
其他镜像和版本需要单独检查；构建成功不能验证另一块板的设备树。

除非另有说明，以下命令均在 **K1 的同一个 Bash 会话**中执行。root 可省略 `sudo`。
任一步失败都应停止。不要在 K1 执行 `provision-board.sh`、`setup-board.sh`、
`setup-gstreamer.sh`、`install.sh`、发布 hooks 或 `dev-push.sh`。

<a id="1-get-this-fork"></a>
## 1. 获取本 fork

仅在创建新工作副本时执行：

```sh
mkdir -p "$HOME/workspace"
git clone --branch main https://github.com/muggle-stack/microduck.git \
  "$HOME/workspace/microduck"
cd "$HOME/workspace/microduck" || exit 1
```

已有仓库应先进入目录并检查 `git status --short --branch`，不要覆盖克隆或替换本地工作。
本指南假设使用已检查、干净的 `main` 版本。旧 `spacemit-k1` 分支不含后续 IMX219 / RTSP / WebRTC 增量。
`uname -m` 必须输出 `riscv64`，Rust 目标则是更长的 **`riscv64gc-unknown-linux-gnu`**。
在 Mac 执行 `cargo k1` 不会自动提供 Linux sysroot、链接器或板上的 GStreamer 库。

<a id="2-check-bianbu-dependencies-before-installing"></a>
## 2. 安装前检查 Bianbu 依赖

使用板上已配置的 Bianbu apt 源，先模拟安装：

```sh
apt-get -s install build-essential binutils pkg-config git cmake curl ca-certificates \
  xz-utils file libudev-dev libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev \
  gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  spacemit-onnxruntime
```

审阅计划后，再去掉 `-s` 执行同一命令，需要时使用 `sudo`。
**不要盲目接受多媒体升级。** 已测镜像上较新的 `-dev` 候选还会替换 vendor 运行库；
当时选择了匹配现有运行时的开发包，没有升级运行库。
[依赖版本记录](../project/spacemit-k1_zh.md#build-on-the-k1)列出了使用的版本。
apt 中没有候选包，不是把 Radxa 库复制到 K1 的理由。

确认原生头文件和运行时：

```sh
pkg-config --modversion libudev gstreamer-1.0 gstreamer-app-1.0 \
  gstreamer-video-1.0 gstreamer-webrtc-1.0
readlink -f /usr/lib/libonnxruntime.so
readlink -f /usr/lib/libspacemit_ep.so
(
  set -eu
  for element in videotestsrc jpegdec videoconvert videocrop videoscale videorate appsink; do
    gst-inspect-1.0 "$element" >/dev/null
  done
)
```

`gstreamer-webrtc-1.0` **开发库不等于 `webrtcsink` 运行时插件**。
没有该插件仍可构建 SDK、运行 `camera-check`；可选的 [K1 WebRTC 指南](../project/k1-webrtc_zh.md)
会在私有目录构建它。Rust SDK 使用 `/usr/lib` 下原生 ORT/EP，不使用 Python 捆绑的运行时。
`python3-spacemit-ort` 仅供可选 Python 实验，不是 Rust 运行依赖。

<a id="3-install-rust-189-without-replacing-apt-rust"></a>
## 3. 独立安装 Rust 1.89，保留 apt Rust

若 `/opt/microduck-rust-1.89.0/bin/rustc -V` 已输出 1.89.0，跳过安装块，只执行下方 PATH 选择。
否则在新暂存目录使用官方独立发行包。固定校验值来自
[Rust 官方发行包校验文件](https://static.rust-lang.org/dist/rust-1.89.0-riscv64gc-unknown-linux-gnu.tar.xz.sha256)。

```sh
(
  set -eu
  if [ -e /opt/microduck-rust-1.89.0 ] || [ -L /opt/microduck-rust-1.89.0 ]; then
    echo 'Existing Rust prefix: inspect it rather than overwriting it.' >&2
    exit 1
  fi
  K1_RUST_STAGE=$(mktemp -d)
  cd "$K1_RUST_STAGE"
  K1_RUST_DIST=rust-1.89.0-riscv64gc-unknown-linux-gnu
  curl --fail --location --connect-timeout 15 --max-time 600 \
    "https://static.rust-lang.org/dist/$K1_RUST_DIST.tar.xz" -o "$K1_RUST_DIST.tar.xz"
  printf '%s  %s\n' \
    4ded289e6a43e4e2bef660c74c8d833e00d87a9e30ad2c376468f41429a12614 \
    "$K1_RUST_DIST.tar.xz" | sha256sum --check -
  tar -xJf "$K1_RUST_DIST.tar.xz"
  sudo sh "$K1_RUST_DIST/install.sh" --prefix=/opt/microduck-rust-1.89.0 \
    --components=rustc,cargo,rust-std-riscv64gc-unknown-linux-gnu --disable-ldconfig
  printf 'Retained Rust download/extraction at %s\n' "$K1_RUST_STAGE"
)
```

这里只安装编译器、Cargo 和原生标准库，不修改 `/usr/bin/rustc`、apt 包、rustup 默认项或 shell 启动文件。
最小板端安装不包含 Rustfmt/Clippy；这些检查可在开发主机单独运行。

```sh
export PATH="/opt/microduck-rust-1.89.0/bin:$PATH"
rustc -Vv
cargo -V
/usr/bin/rustc -V
```

已测板上所选编译器输出 1.89.0，而 apt 的 `/usr/bin/rustc` 仍为 1.75.0。
两套工具链并存是有意设计。

<a id="4-build-and-run-software-regression"></a>
## 4. 构建与软件回归

在仓库根目录执行：

```sh
export CARGO_TARGET_DIR="$PWD/target"
export ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so
cargo k1 --locked --bins -j 2
K1_BIN="$CARGO_TARGET_DIR/riscv64gc-unknown-linux-gnu/release"
file "$K1_BIN/robotd"
```

预期得到 ELF64 RISC-V/LP64D 程序，而不是 aarch64。
修改共享配置 schema 后应构建**全部**二进制；只重建 `mediad` 会让旧版 `robotctl`、`robotd`、
`padd` 无法读取新的相机字段。`cargo board` 仍保留为 Radxa 别名。

```sh
sh scripts/k1-test.sh
```

脚本执行 release 模式 workspace 测试、构建二进制，并通过隔离运行目录中的 `--help` 检查九个程序的加载。
测试使用模拟机器人 IO，不启动舵机、麦克风、相机或系统服务。原生冷构建可能超过一小时；
默认两个构建任务限制 4 GB 板的内存占用。Cargo 依赖已缓存时也可用
`CARGO_NET_OFFLINE=true sh scripts/k1-test.sh`。

真实策略推理是可选独立检查。执行 `scripts/seed-policies.sh ROOT` 时使用**新的私有策略目录**，
不要仅为测试使用其生产默认目录。下载是尽力而为的，必须确认 `ROOT/current` 和所需 ONNX 文件存在，
再把目录传给 `sh scripts/k1-test.sh ROOT/current 200`。
[策略验证记录](../project/spacemit-k1_zh.md#validate-real-policy-inference)
说明模型、网络失败情况和该基准不能证明的内容。

<a id="5-install-developer-binaries-into-a-new-private-prefix"></a>
## 5. 安装到新的私有开发目录

这是**手动开发安装**，不是 OTA 发布。下面创建带修订标识的新目录，不替换旧安装或运行中的服务。
后续可选步骤继续使用 `K1_SDK_PREFIX`。

```sh
K1_INSTALL_BASE="$HOME/.local/opt/microduck-k1"
mkdir -p "$K1_INSTALL_BASE"
K1_SDK_PREFIX=$(mktemp -d "$K1_INSTALL_BASE/$(git rev-parse --short=12 HEAD).XXXXXX")
K1_PROGRAMS='robotd robotctl updaterd configd btd padd mediad tofd camera-check duck-bench sounds pet-detect'
(
  set -eu
  : "${K1_SDK_PREFIX:?Creating the private prefix must succeed first}"
  : "${K1_BIN:?Complete the native build step first}"
  test -d "$K1_SDK_PREFIX"
  install -d "$K1_SDK_PREFIX/bin" "$K1_SDK_PREFIX/config"
  for name in $K1_PROGRAMS; do
    install -m 755 "$K1_BIN/$name" "$K1_SDK_PREFIX/bin/$name"
  done
  install -m 644 deploy/k1/camera-usb-mono.toml deploy/k1/camera-usb-decxin-sbs.toml \
    deploy/k1/camera-usb-decxin-mpp.toml deploy/k1/camera-imx219.toml \
    deploy/k1/imx219-csi3-720p.json deploy/k1/robotd-audio.toml "$K1_SDK_PREFIX/config/"
  git rev-parse HEAD > "$K1_SDK_PREFIX/REVISION"
)
printf 'Developer SDK: %s\n' "$K1_SDK_PREFIX"
```

不会修改 `/opt/robot`、`/etc/robot`、`/usr/bin`、systemd 服务、默认配置或 `current` 符号链接。
这些配置文件是**独立示例**，不会自动合并成机器人完整配置。
本次复制不包含模型、音色库、固件或系统运行库。

在不联系运行中机器人的情况下检查安装程序：

```sh
K1_HELP_RUNTIME=$(mktemp -d "$K1_SDK_PREFIX/cli-runtime.XXXXXX")
(
  set -eu
  : "${K1_HELP_RUNTIME:?Creating the isolated runtime directory must succeed first}"
  for name in $K1_PROGRAMS; do
    DUCK_RUNTIME_DIR="$K1_HELP_RUNTIME" timeout 15 "$K1_SDK_PREFIX/bin/$name" --help >/dev/null
    printf '%s: executable OK\n' "$name"
  done
)
```

即使执行 `--help`，隔离运行目录也很重要：部分守护程序会在解析参数前发布身份。
检查时使用明确的二进制路径；直接运行 `robotctl` 可能调用旧安装。
需要时用 `export PATH="$K1_SDK_PREFIX/bin:$PATH"` 在当前 shell 选择该安装，
但这**不会**替换现有 socket 后面的守护进程。

<a id="6-optional-usb-camera-and-private-mpp-bundle"></a>
## 6. 可选 USB 相机与私有 MPP 组件

先用 `v4l2-ctl --list-devices` 和 `v4l2-ctl -d DEVICE --list-formats-ext`
（来自 `v4l-utils`）确认实际采集设备及支持模式。
检查复制的相机配置，不要把 DECXIN 预设用于其他相机；第二个 `/dev/video*` 节点是元数据，
不一定是另一只眼。

当前需求是**选一眼使用**：物理双目相机可以输出拼接帧，SDK 按 `camera.view = "left"`
或 `"right"` 裁剪。这种用法不要传 `--both-eyes`，无需双目标定、深度或两眼同时推理。

通用 USB 软件路径不需要 MPP/OpenCV 桥接。要启用加速，先审阅 `opencv-spacemit` 的 apt 模拟结果，
未安装时再安装匹配的 vendor 版本。还需要 C++17 编译器、CMake、可用的 VDEC/V2D/DMA-heap
设备及恰当权限，不要笼统赋予 `chmod 777`。
使用已有 SpaceMIT SDK MPP 工作副本，或把 [SpaceMIT MPP](https://github.com/spacemit-com/mpp)
克隆到新目录；其中必须含提交 `2b97ffe84c06071774301fad5542fbe76fa62774`。

```sh
# Substitute the actual MPP checkout path if different.
sh scripts/build-k1-camera.sh "$HOME/workspace/spacemit-sdk/components/multimedia/mpp"
(
  set -eu
  : "${K1_SDK_PREFIX:?Complete the private installation step first}"
  test -d "$K1_SDK_PREFIX"
  install -d "$K1_SDK_PREFIX/lib/k1-camera/lib"
  install -m 755 target/k1-camera/libmicroduck_k1_camera.so \
    target/k1-camera/camera-native-check target/k1-camera/camera-image-test \
    "$K1_SDK_PREFIX/lib/k1-camera/"
  cp -a target/k1-camera/lib/. "$K1_SDK_PREFIX/lib/k1-camera/lib/"
)
export MICRODUCK_K1_CAMERA_LIB="$K1_SDK_PREFIX/lib/k1-camera/libmicroduck_k1_camera.so"
ldd "$MICRODUCK_K1_CAMERA_LIB"
```

`ldd` 不应出现 `not found`。必须保持相对 `lib/` 布局：桥接从中查找私有 MPP，
MPP 再查找相邻 codec2 插件。系统 OpenCV 保留在 `/opt/opencv-spacemit`；
不要把私有 MPP 装到 `/usr/lib`，设置全局 `LD_LIBRARY_PATH`，构建 MPP 的 `all` 目标，
或运行 `cmake --install`。[相机后端记录](../project/k1-mpp-camera_zh.md)负责构建细节与限制。

板上 V2D 可用时，下列合成图像检查不会打开相机：

```sh
timeout 30 "$K1_SDK_PREFIX/lib/k1-camera/camera-image-test"
```

然后，**先确认相机空闲且配置匹配**，再打开选定的一眼；不启用舵机、音频、编码器或 WebRTC：

```sh
timeout 60 "$K1_SDK_PREFIX/bin/camera-check" \
  --config "$K1_SDK_PREFIX/config/camera-usb-decxin-mpp.toml" --warmup 5 --frames 30
```

软件采集改用已检查的 `camera-usb-decxin-sbs.toml` 或 `camera-usb-mono.toml`，
此时不加载桥接。MPP 与软件解码/缩放的像素不逐字节一致，因此加速仍需显式启用。

<a id="optional-imx219--k1-csi-input"></a>
### 可选 IMX219 / K1 CSI 输入

MUSE-Pi-Pro 在 vendor CSI3（`sensor_id=2`）接 IMX219 时，
显式 `camera.backend = "spacemit_csi"` 使用已安装的 `spacemitsrc` 和 vendor ISP JSON。
该路径不使用 USB MPP/OpenCV 桥接或 Rockchip sensor 控制。
打开相机前检查 `camera-imx219.toml` 和 `imx219-csi3-720p.json`，
尤其在使用上述私有安装时，修改 `camera.isp_config` 与模型的绝对路径。

[IMX219 指南](../project/k1-imx219_zh.md)包含已测板卡/运行时、有超时边界的采集与 ORT/EP 命令、
模型要求、实测结果和已知限制。
浏览器图传和同源推理继续见 [WebRTC 指南](../project/k1-webrtc_zh.md)。
其他 CSI 接口或未测模组不会因此自动获得支持。

<a id="optional-k1-csi-rtsp-preview"></a>
### 可选 K1 CSI RTSP 预览

[`camera-rtsp`](../project/k1-rtsp_zh.md)使用原生 SpaceMIT H.264 编码器和 RTSP/TCP
提供仅视频的 IMX219 预览。它**不是**完整 WebRTC 守护服务，不执行检测，目前不支持 USB 输入。
默认监听 loopback，可通过 SSH 隧道用 ffplay 或 VLC 观看。

安装可选开发包前，审阅 `apt-get -s install libgstrtspserver-1.0-dev` 是否匹配现有 vendor GStreamer。
然后在 K1 构建：

```sh
cargo k1 --locked --features mediad/rtsp --bin camera-rtsp -j 2
```

普通 SDK 构建不启用该 feature，也不依赖原生 RTSP-server 库。
私有安装步骤不会自动安装/启动此工具；按链接指南使用
`target/riscv64gc-unknown-linux-gnu/release/` 下的程序。

<a id="optional-imx219-webrtc-console"></a>
### 可选 IMX219 WebRTC 控制台

[K1 WebRTC 指南](../project/k1-webrtc_zh.md)说明固定版本的原生 `rswebrtc` / `rsrtp` 插件、
仅用于插件的独立 Rust 1.92 工具链，以及审阅 apt 后安装的 `gstreamer1.0-nice`。
SDK 本身仍使用 Rust 1.89。`mediad` 使用 `deploy/k1/webrtc-imx219.toml`
及私有 `GST_PLUGIN_PATH` / `GST_REGISTRY`；不要替换 vendor 库或运行 Radxa 安装脚本。
现有控制台显示视频，并经 control DataChannel 接收检测通知。
检测需显式启用且另行提供模型。遵循指南的 loopback/SSH 用法与局域网安全限制。

<a id="7-optional-detector-and-es8326"></a>
## 7. 可选检测器与 ES8326

- **视觉：**按 [ORT/EP 指南](../project/k1-duck-ort-ep_zh.md)提供已验证的浮点 opset 17
  `duck_detect.slim.onnx` 和显式 `[detect]` SpaceMIT 配置。该指南包含 SHA-256。
  EP 兼容 FP32 模型已在独立
  [models-duck-detect-v1 预发布](https://github.com/muggle-stack/microduck/releases/tag/models-duck-detect-v1)
  提供，同时包含来源/转换说明和模型许可声明。
  **需单独下载并校验；本安装过程不生成或自动下载模型。**
  原始 opset 12 ONNX、`.rknn` 文件和通用 YOLO11 COCO 模型不能替代它。
  无需 Python worker 或重复实现 Rust 前处理。
- 把检测参数合并到**所选相机配置的副本**，不要重复 TOML section。
  严格四核预算使用[无头检测命令](../project/k1-usb-camera_zh.md#configuration-and-headless-check)：
  cgroup `AllowedCPUs=0-2,4`，三个 EP worker 为 `0;1;2`，调用线程在 CPU 4，`--detect --hz 2`。
  已验证浮点模型保持关闭 FP16 epilogue。仅 `taskset` 不能约束此运行时的全进程 CPU 预算。
  经 `sudo` 或 service scope 启动时，把两个原生库环境变量都传入进程。
- **音频：**`sudo sh scripts/setup-k1-audio.sh` 安装显式启用的 ES8326 ALSA profile。
  与私有二进制复制不同，它会写 `/etc/alsa/conf.d`，并在不存在时创建仅含音频配置的
  `/etc/robot/robotd.toml`，已有配置则保留。不修改 Mixer 或启动服务。
  执行会打开麦克风的硬件测试前，先读
  [ES8326 安装与全双工验证](../project/spacemit-k1_zh.md#es8326-audio)；必要时安装 `alsa-utils`。

<a id="8-upstream-alignment-and-completion-boundary"></a>
## 8. 上游对齐与完成边界

上游版本和功能状态见[适配进度](../project/spacemit-k1-adaptation-zh.md)。维护 fork 时，
先 fetch `upstream` 并审阅变更，再在干净工作区执行 rebase。
改写共享分支历史前应协调，集成后重新执行 K1 回归。

移植保留运动控制核心、RKNN 实现和 Radxa 默认项，但也修改共享集成代码。
尤其无效的显式媒体配置现在会报错，不再静默选择 Rockchip 默认设备，ARM 同样受影响。
见 [USB 配置契约](../project/k1-usb-camera_zh.md#implemented-contract)；
这不是“行为完全不变”或已完成 RK3566 实物回归的声明。

原生构建/测试、策略推理、ES8326、USB 选眼与 EP 检查记录在
[K1 板级报告](../project/spacemit-k1_zh.md)。
IMX219 采集、单观众 WebRTC 图传与同源 ORT/EP 检测已可用，实测结果和剩余测试见
[当前适配进度](../project/spacemit-k1-adaptation-zh.md)。
真实 HAT/UART/舵机/IMU 反馈、ToF、蓝牙控制器/手柄、音视频集成、多观众、
量化长稳/并发负载、带标注检测精度及 RISC-V 部署/签名发布/OTA 仍需分别推进。
安装这些程序不代表 `robotctl health` 已成为硬件验收测试。
