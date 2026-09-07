<a id="k1--imx219webrtc-浏览器图传"></a>
# K1 / IMX219: WebRTC browser streaming

[English](k1-webrtc.md) · [简体中文](k1-webrtc_zh.md)

This is the explicit K1 CSI path in `mediad`, reusing official signalling, the web console and
the `control` DataChannel, not a separate Python streaming application.
RTSP remains an independent tool; do not run it concurrently or compete for the sensor.

```text
IMX219 → K1 ISP / CPP → NV12 DMA-BUF → tee
    ├─ webrtcsink → spacemith264enc → H.264 Main → RTP / WebRTC → browser video
    └─ map on demand DMA-BUF → native GstVideoConverter → UYVY BT.601 → Rust ORT / SpaceMIT EP
                               → media.detections → DataChannel → browser SVG boxes
```

Current scope: MUSE-Pi-Pro, CSI3 IMX219, 1280×720, 30 fps configuration and a trusted LAN.
Detection defaults to off. The Rockchip, USB capture and `camera-rtsp` paths are not replaced.
Retaining the web control API does not mean the real K1 servo/IMU loop has been validated.

<a id="1-系统依赖"></a>
## 1. System dependencies

Complete [native SDK build preparation](../robot/install-k1.md) and [IMX219 capture validation](k1-imx219.md) first.
Do not run Radxa's `setup-gstreamer.sh` on K1 or copy aarch64 plugins.

WebRTC also needs the GStreamer ICE plugin. Simulate installation and ensure no vendor multimedia
packages would be upgraded or removed before proceeding:

```sh
sudo apt-get -s install gstreamer1.0-nice
sudo apt-get install gstreamer1.0-nice
gst-inspect-1.0 nice
gst-inspect-1.0 webrtcbin
gst-inspect-1.0 spacemitsrc
gst-inspect-1.0 spacemith264enc
```

The tested MUSE-Pi uses GStreamer 1.24.2. Only `gstreamer1.0-nice` 0.1.21-2build3 was added;
system GStreamer, camera drivers, device tree and MPP were not replaced.
Finding `webrtcbin` does not mean `webrtcsink` is installed; the latter is built separately.

<a id="2-给插件准备独立-rust-192"></a>
## 2. A separate Rust 1.92 for the plugins

The SDK remains on Rust **1.89**. Pinned `gst-plugins-rs 0.15.3` requires **1.92+**.
Skip this section if `/opt/microduck-gst-rust-1.92.0/bin/rustc -V` already exists and reports the expected version.
This downloads an official prebuilt toolchain, not Rust source for compilation,
and replaces neither apt Rust nor the SDK toolchain:

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

Checksum source: [official Rust 1.92 RISC-V distribution checksum](https://static.rust-lang.org/dist/rust-1.92.0-riscv64gc-unknown-linux-gnu.tar.xz.sha256).
Neither separate prefix requires shell-startup edits. Select the toolchain only for the relevant build command.

<a id="3-编译私有-webrtc-插件和-sdk"></a>
## 3. Build the private WebRTC plugins and SDK

In the SDK checkout on K1:

```sh
cd ~/workspace/microduck
PATH=/opt/microduck-gst-rust-1.92.0/bin:$PATH sh scripts/build-k1-webrtc.sh

export CARGO_TARGET_DIR="$PWD/target"
PATH=/opt/microduck-rust-1.89.0/bin:$PATH cargo k1 --locked --bin mediad -j 2
```

The script pins the upstream commit and repository patch, builds only `rswebrtc` / `rsrtp`,
and writes `target/k1-webrtc/plugins`, not `/usr/lib` or `/usr/local/lib`.
It has a separate Cargo cache. The first run still downloads/builds Rust crates;
it is not rebuilding system GStreamer.

The measured build used an 8 GB board and the default four build jobs.
Upstream release builds enable LTO, making the first native build slow.
For a 4 GB board, start with `K1_BUILD_JOBS=1` to limit peak memory.
The 8 GB result is not a 4 GB acceptance result.
Source, license and patch details: [native/k1-webrtc](../../native/k1-webrtc/README.md).

<a id="4-启动并在电脑浏览器查看"></a>
## 4. Start and view from a desktop browser

Check the absolute `camera.isp_config` in [webrtc-imx219.toml](../../deploy/k1/webrtc-imx219.toml).
Exit other camera applications, then on K1:

```sh
cd ~/workspace/microduck
export GST_PLUGIN_PATH="$PWD/target/k1-webrtc/plugins"
export GST_REGISTRY="$PWD/target/k1-webrtc/registry.bin"
export ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so
target/riscv64gc-unknown-linux-gnu/release/mediad \
  --config deploy/k1/webrtc-imx219.toml --host 127.0.0.1
```

In another terminal on the computer:

```sh
ssh -N -L 127.0.0.1:8080:127.0.0.1:8080 \
  -L 127.0.0.1:8443:127.0.0.1:8443 musepi
```

Open **http://127.0.0.1:8080/** and click **connect**.
The webpage and signalling use SSH; **video ICE/UDP still requires direct access to the board's LAN address**.
SSH access through a jump host alone is insufficient. Public STUN / TURN is not configured.

For a phone on the same trusted LAN, set K1 `--host` to its LAN IP and open
`http://BOARD_IP:8080/`; no SSH tunnel is needed.
**The official console has no user authentication; a connected peer can call control APIs.**
Do not listen on the public Internet, forward router ports or run on an unknown network.
The page/signalling use HTTP/WS; WebRTC media encryption is not access control.

Disconnecting the page does not stop `mediad`; capture can continue for the detector.
Press Ctrl-C on K1 to stop the service, then close the SSH tunnel.
The program attempts EOS draining before releasing the camera.
A cleanup timeout reports an error and fails rather than pretending shutdown succeeded.

<a id="5-开启检测和框"></a>
## 5. Enable detection and boxes

First verify preview, then set the example `[detect] enabled` to `true` and restart `mediad`.
Download `duck_detect.slim.onnx` from the
[models-duck-detect-v1 Release](https://github.com/muggle-stack/microduck/releases/tag/models-duck-detect-v1),
verify the [SHA256 in the IMX219 guide](k1-imx219.md#模型和精度边界), and adjust the absolute model path.
Ordinary git clone does not fetch Release assets; the SDK does not download them automatically.
This is a FP32 model prerelease with a separate model license, not SDK firmware or experimental INT8.

Inference reuses Rust ORT + EP and existing pre/postprocessing.
`media.detections` travels over the same DataChannel to the webpage.
The official SVG draws boxes; they are not burned into H.264, so a downloaded raw video does not automatically include them.
Boxes appear only when a target is detected. Missing boxes alone do not prove streaming or inference stopped;
check the detection count and service log.
This model detects the Microduck robot, not arbitrary duck photographs or generic COCO objects.

<a id="6-当前边界"></a>
## 6. Current boundaries

- Original DMA-BUF feeds hardware encoding inside `webrtcsink`, without software H.264 fallback
  or implicit rotation/scaling. `media.quality` must match camera dimensions/rate.
  Browser rotation can change display orientation.
- The board encoder actually outputs **H.264 Main**. The patch negotiates its real profile,
  not a disguised Baseline stream. Baseline-only clients are outside this validation.
- This vendor encoder version exposes no bitrate, GOP or profile controls.
  Configure `congestion_control="disabled"`; `media.bitrate` does not control its actual output.
  Adaptive bitrate, fast packet-loss recovery and end-to-end latency are not accepted here.
- Only video and DataChannel were tested; microphone, speaker, motion services and servos were not started.
  `robot.*` / `system.*` errors from absent daemons must be distinguished from video failures.
- `webrtcsink` creates encoding sessions per viewer, unlike the RTSP tool's shared encoder.
  This phase tests one viewer and reconnects; multi-viewer resource limits and whole-robot concurrency need separate tests.
  The webpage RTT is network round-trip time, not sensor-to-screen video latency.
- Single-viewer IMX219 video with same-source detection has been verified.
  Early ceiling-scene tint/highlight-overexposure evidence is retained; PQ/exposure under other lighting still needs regression.
  The tests used no additional filter.
  Lossy H.264, YUV conversion and differing EP outputs are not claimed byte-identical; model precision was not changed.

<a id="7-2026-09-07-开发过程记录"></a>
## 7. Development record: 2026-09-07

- Private plugins pinned to 0.15.3 built natively with Rust 1.92; the first build took about 44 minutes.
  `gst-inspect-1.0` confirmed loading from the private path while runtime GStreamer remained vendor 1.24.2.
  SDK compilation stayed on Rust 1.89. Mac related tests passed 130; K1 library tests passed 726,
  with two hardware tests ignored by default. These software results do not replace camera/WebRTC tests.
- Retained first failure: `videoconvert` could not negotiate DMA-BUF NV12 to ordinary UYVY;
  the SDK rejected `Noformat` before sensor startup.
  The fix accepts real DMA-BUF at appsink and maps only requested frames, using native `GstVideoConverter`
  to produce ordinary BT.601 UYVY. No caps-label trick or second sensor opening is used.
- Initial browser frames and DataChannel requests succeeded; `media.video` returned 1280×720, rotate 0.
  ICE selected the board's LAN host / UDP candidate; webpage/signalling used local SSH forwarding.
  H.264 offer `profile-level-id=4d401f` and Chromium answer `4d001f` both indicated Main.
  Eight seconds added 237 decoded frames; compilation overlapped, so this is functional evidence only.
- With ORT / EP on the same source, 15 seconds added 447 decoded frames and 30 detection notifications.
  The ceiling scene produced `boxes=[]`; it is not target-detection or accuracy acceptance.
  Plugin compilation overlapped, so these timings are not compared with independent `camera-check` latency.
- Retained reconnect failure: the first two rapid reconnects added 149 / 152 frames, but the third had only its first frame,
  then zero new frames over five seconds while DataChannel stayed functional. Board capture later recovered.
  Another detector-enabled shutdown hit the five-second cleanup deadline and exit 1.
  A sensor-power-off log alone did not qualify as clean release.
  The fix drains the K1 encoder before setting retired consumer sessions to NULL,
  retaining EOS failure logs and the SDK's overall shutdown deadline.

Original board logs are under `target/k1-webrtc/`; browser logs under host `target/webrtc-*.log`.

<a id="修复后的验收"></a>
### Acceptance after the fixes

- No compilation or other SDK business workload ran concurrently.
  Hard cgroup `AllowedCPUs=0-2,4` constrained every sampled userspace thread;
  three EP workers used `0;1;2`, with model and precision unchanged.
- Detection enabled: eight consecutive “play three seconds → disconnect → immediately reconnect” cycles added
  **89 / 89 / 89 / 90 / 89 / 89 / 91 / 89 frames**, each with **six detection notifications**.
  All eight logs confirmed encoder drain without EOS timeout.
- A subsequent 20-second sample in the visible Chromium console added **597 decoded frames**,
  about **29.85 fps** by browser timestamps, with **40 notifications** and zero incremental packet loss in that window.
  This was a static ceiling scene, one viewer and a warmed model, not a complex-scene bitrate or endurance guarantee.
- Sending SIGINT while the browser and detector remained active closed the control channel;
  SDK **exit 0**, with `K1 media stopped`, sensor stream-off and power-off logs.
- Reopening with detection off passed six immediate reconnects:
  **90 / 90 / 89 / 89 / 90 / 91 frames**. SIGTERM to the idle service yielded **exit 0** and camera release.
  This repeats the original failing detector-off configuration; detector-on tests alone are insufficient.
- Boot ID throughout these runs was `385fa665-52e8-423f-96b4-7cfd0af98659`;
  recovery did not depend on rebooting.
  Some disconnects still logged a lower-level SCTP association error, but subsequent video/control reconnects passed.
  This is not a no-warning or complete abnormal-network-recovery claim.

Final board logs: `run-d.log` / `run-e.log` and `library-tests-final.log`.
Host logs: `webrtc-final-reconnect.log` / `webrtc-final-combined.log` /
`webrtc-active-stop.log` / `webrtc-final-video-only.log`.
The services, browser and SSH tunnel used for this development test were stopped.

Plugin artifact checksums from this build (rebuilding is not guaranteed byte-reproducible):

```text
555c37dc09a370210c27f905e3fc915bd4779b90f93a6083d39cc36858042d6c  libgstrswebrtc.so
59eb4c1c09dd016b8beb525584cec06c0a38e64780c07b09a9f59ba263b9178f  libgstrsrtp.so
```

<a id="后续用户实测确认2026-09-07"></a>
<a id="subsequent-user-acceptance-2026-09-07"></a>
<a id="浏览器图传与检测2026-09-07"></a>
### Browser video and detection: 2026-09-07

A Mac browser can receive single-viewer IMX219 video and same-source ORT / EP detection notifications.
The tests above record decoded frame counts, detection notification counts, reconnection and shutdown results.

Those measurements retain their original four-CPU short-test conditions.
End-to-end latency, labelled detection accuracy and quantified endurance have not been measured.
Audio/video, multiple viewers, network/camera abnormal recovery and whole-robot concurrency require dedicated tests.
