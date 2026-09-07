<a id="microduck-sdkspacemit-k1--risc-v-适配进度"></a>
# Microduck SDK: SpaceMIT K1 / RISC-V adaptation status

[English](spacemit-k1-adaptation.md) · [简体中文](spacemit-k1-adaptation-zh.md)

Updated **2026-09-07**. Fork: [muggle-stack/microduck](https://github.com/muggle-stack/microduck).
The development entry is this fork's `main`, including earlier adaptation (`896b34b`),
IMX219 (`ba2776c`), RTSP (`e73a4fa`), WebRTC (`229c7f3`), the detector model and usage guides.
There is no need to switch to old `spacemit-k1` or `feat/k1-*` branches.
The detector model is separately published as `models-duck-detect-v1`.
Source integration and model delivery are not SDK firmware or OTA publication.
The test results on this page correspond to upstream baseline `bc41fb5`.

This page owns supported platforms, verified functionality and remaining plans.
For build/install steps see the [K1 development installation guide](../robot/install-k1.md);
detailed evidence is in the [K1 bring-up record](spacemit-k1.md).
Each linked test record specifies its measurement dates and conditions.

<a id="1-我们适配的平台"></a>
## 1. Supported platform

This fork extends the original **Radxa Zero 3W / RK3566 / aarch64** SDK to
**SpaceMIT K1 / 64-bit RISC-V**. Verified environments:

| Item | Verified environment |
| --- | --- |
| Board | Original K1 4 GB; MUSE-Pi-Pro / K1 8 GB for IMX219 |
| OS | Original Bianbu 2.1.1; MUSE-Pi-Pro Bianbu 2.3.5 |
| Rust target | `riscv64gc-unknown-linux-gnu`; `uname -m` reports `riscv64` |
| Toolchain | SDK isolated 1.89.0; private WebRTC plugins separately use 1.92.0; apt compiler is not replaced |
| Inference | Native `spacemit-onnxruntime` 2.0.6-bpo1+1: ORT 1.24.2+spacemit.a1, SpaceMIT EP 2.0.6 |
| Multimedia | GStreamer 1.24.2; optional private SpaceMIT MPP / V2D bridge and `opencv-spacemit` 4.14.0-1bb3 |
| Peripherals | Original DECXIN USB camera (`1bcf:2d50`, one eye from packed stereo) and ES8326; MUSE-Pi-Pro CSI3 IMX219 |

This applies to those K1 software stacks and peripherals, not arbitrary RISC-V boards, cameras or images.
Mac builds/tests are development checks, not substitutes for K1 native execution and peripheral acceptance.

<a id="2-已经适配了多少功能"></a>
## 2. Feature progress

Using the original **ten feature categories**:

- **Five verified** by software or board tests within the scopes below, including IMX219 capture and detection.
- **Two partially complete**: working subpaths exist, but other parts of the category remain.
- **Three awaiting hardware integration or end-to-end validation**; this does not mean no reusable code exists.

Work continues on **five categories (two partial + three awaiting integration)** plus whole-robot qualification.
Categories differ in size; these counts do not mean “50% of engineering work complete” or a lines-of-code adaptation percentage.

| No. | Category | Status | Completed scope and remaining boundaries |
| --- | --- | --- | --- |
| 1 | Rust / RISC-V runtime foundation | Verified | Isolated Rust 1.89, full native SDK build, executable loading and regression tools; not all peripherals. |
| 2 | RL policies and control software | Verified | 9 official ONNX policies run on K1; about 50 Hz under fake IO. Real servo feedback belongs to category 3. |
| 3 | DXL / servos / IMU | Awaiting integration | Core/protocol code retained; connect HAT and verify K1 half-duplex UART, 1 Mbps, 15 servos, IMU and the physical closed loop. |
| 4 | Vision detection | Verified | Rust ORT + SpaceMIT EP integrated with SDK/config/tools; live selected-eye detection meets the current 2 Hz target. Labelled accuracy and whole-robot concurrency remain. |
| 5 | Camera / ISP | Verified | USB selected-eye and optional MPP/V2D verified; IMX219 K1 CSI/ISP → SDK → ORT/EP works, standalone capture about 26.7 fps and detection meets 2 Hz. Stress, abnormal recovery and other modules require separate tests. |
| 6 | H.264 / RTSP / WebRTC streaming | Partial | RTSP preview works; IMX219 → K1 H.264 → Mac browser WebRTC console video, DataChannel, same-source ORT/EP notifications and reconnect/shutdown verified. Combined audio/video, adaptive bitrate, multiple WebRTC viewers, USB streaming and whole-robot load remain. |
| 7 | ToF / I²C | Awaiting integration | Reusable driver; establish K1 bus mapping/permissions and validate sensor data acquisition/publication. |
| 8 | ES8326 audio | Verified | Optional ALSA config, SDK playback, microphone capture and simultaneous full duplex passed; not acoustic recognition accuracy for every enclosure/microphone. |
| 9 | Bluetooth / gamepad | Awaiting integration | Native builds work; controller, pairing, input, disconnect recovery and actual control still need acceptance. |
| 10 | Installation / release / OTA / CI | Partial | Native build, isolated installation and guides complete; compatible model on a separate GitHub Release for manual verified download. One-step dependency/Rust/build/check setup, automatic model retrieval, RISC-V deployment, signing, OTA and CI remain. |

**USB cameras support selected-eye capture.**
Two physical eyes do not require simultaneous two-eye inference; select left/right with `camera.view`.
Camera/ISP verification covers the tested USB path and specified MUSE-Pi-Pro / CSI3 IMX219,
not every module or image. Concurrent dual-eye inference, stereo depth and calibration are outside this page's verified scope.

<a id="3-已完成能力的实测依据"></a>
## 3. Measurement evidence

<a id="原生构建策略与控制软件"></a>
### Native builds, policies and control software

- Full SDK native build on K1 with Rust 1.89. A development-install recheck loaded all 12 installed
  daemon/diagnostic binaries via `--help`, with byte-identical copies of build artifacts.
  Loading is not hardware-function acceptance.
- Nine separately loaded policies, one ORT intra-op thread each, no pinning or concurrent workload:
  20 warm-ups and 200 measurements per model, mean **0.88–0.94 ms**.
  This excludes model loading, bus operations and full control-loop latency.
- Fake-IO control on real K1 ran about 30 seconds, **1,500 cycles at 49.986 Hz with zero missed deadlines**.
  It does not establish that the physical robot can walk on K1.

Conditions and evidence: [policy/control validation](spacemit-k1.md#validate-real-policy-inference).

<a id="usb-单眼与视觉检测"></a>
### USB selected-eye capture and detection

- DECXIN MJPEG 4000×1200 packed frame, one eye, aspect-preserving 1280×720 output.
  Capture-only under a hard four-CPU limit, five warm-ups and 120 measured frames:
  **30.05 fps**, versus about 5.39 fps for the same-condition software path.
  This is acquisition throughput, not browser frame rate.
- Floating-point opset 17 detection uses existing SDK pre/postprocessing and native ORT/EP.
  `AllowedCPUs=0-2,4`, three EP workers `0;1;2`, caller CPU 4; FP16 epilogues disabled.
  Five warm-ups and 60 measurements with final binaries: mean **192.8 ms**, P95 **209.4 ms**, consumed at 2 Hz.
  Detection includes preprocessing, inference and NMS, excluding acquisition/crop/scale.
- Hardware-path means varied **187–206 ms** across runs; the best run is not a guarantee.
  Real servos, audio, encoding and WebRTC were not concurrent in these runs.
- MPP/V2D and software decoder/scaler pixels are **not byte-identical**.
  EP performance does not establish equal accuracy. Acceleration remains explicit; INT8 is not the default.
  Labelled detection accuracy remains to be validated.
- EP-compatible FP32 `duck_detect.slim.onnx` is publicly delivered as the independent
  [models-duck-detect-v1 prerelease](https://github.com/muggle-stack/microduck/releases/tag/models-duck-detect-v1),
  with checksums, model card, license and reproducible conversion materials, not SDK firmware/OTA.
  Git clone and development installation do not generate or fetch it automatically.
  [Download and verify it manually](k1-imx219.md#模型和精度边界).
  Original opset 12 ONNX, RKNN or generic COCO models are not substitutes.

Configuration, checksums, repeated runs and failures:
[Rust ORT/EP](k1-duck-ort-ep.md), [USB camera](k1-usb-camera.md), [MPP/V2D/OpenCV](k1-mpp-camera.md).

### MUSE-Pi-Pro / IMX219

The `spacemit_csi` source delegates CSI3 / IMX219 ISP/CPP management to installed `spacemitsrc`,
then reuses SDK UYVY frames and ORT/EP. No sensor-driver rebuild, device-tree edit or RKNN/USB replacement.
The new board passed 719 library tests and three same-process open/read/close cycles.
Under four hard-limited CPUs, two 720p capture runs achieved about 26.7 fps.
Floating-point preprocessing + inference + NMS averaged 237.6–248.9 ms, P95 247.9–259.2 ms, consumed at 2 Hz.
A separate profile confirmed SpaceMIT subgraph execution.
No motion/audio/encoding overlapped; configured 30 fps is not substituted for measured throughput.

Occasional vendor CPP shutdown warnings from those development probes remain tracked;
no warning-free claim is made.
The results above verify capture and detection on the specified board.
Endurance, a complete lighting matrix, stress, abnormal recovery and other modules require independent tests.
Configuration and commands: [IMX219 capture and detection](k1-imx219.md).

<a id="imx219--webrtc-浏览器图传"></a>
### IMX219 / WebRTC browser streaming

The official `webrtcsink`, page and `control` DataChannel are retained.
The K1 video branch passes NV12 DMA-BUF to `spacemith264enc`; the detection branch maps on demand,
converts to BT.601 UYVY and reuses Rust ORT/EP.
A 20-second browser sample under a hard four-CPU cgroup without compilation added
**597 decoded frames (about 29.85 fps)** and **40 detection notifications**.
This is a board/scenario short test, not an end-to-end latency or whole-robot guarantee.
Eight rapid detector-enabled reconnects passed. Ctrl-C with the browser online yielded exit 0 and camera release.

That development scene was empty; it did not validate target-box hits.
The webpage keeps the official SVG overlay.
Mac browser video and same-source detection notifications passed functional checks; labelled accuracy requires separate tests.
The vendor encoder outputs Main Profile without bitrate controls, so congestion bitrate adaptation is explicitly disabled.
Installation, failures and limits: [K1 WebRTC](k1-webrtc.md).

<a id="es8326-音频"></a>
### ES8326 audio

SDK playback and microphone acquisition were verified simultaneously.
The ALSA profile fixes hardware at 48 kHz stereo and adapts the SDK-requested formats,
avoiding full-duplex clock conflicts. The original AIC3104 default remains.
This is not end-to-end audio latency or petting-recognition accuracy on a new microphone structure.
See [ES8326 integration](spacemit-k1.md#es8326-audio).

<a id="4-接下来继续适配的-5-类工作"></a>
## 4. Five categories of remaining work

The remaining integration tasks and test scopes are listed below. Hardware tests require the relevant peripherals;
tested USB selected-eye and IMX219 are available development inputs.

1. **Physical motion loop (3):** HAT power/voltage/half-duplex interface and K1 UART;
   1 Mbps with 15 servos and IMU ID 200, timeout/error handling, then a real 50 Hz feedback loop and safety behavior.
2. **Streaming remainder (6):** [RTSP](k1-rtsp.md) and [IMX219 WebRTC video/DataChannel/notifications](k1-webrtc.md) work.
   Validate real target overlays, audio/video, USB streaming, multiple viewers, adaptive bitrate,
   end-to-end delay, abnormal-network recovery and whole-robot concurrency.
   A working control channel does not establish physical motion-loop acceptance.
3. **ToF (7):** actual model and I²C wiring, initialization, sustained ranging, publication and error recovery.
4. **Bluetooth/gamepad (9):** controller/services, pairing/reconnect, buttons/sticks and control delivery,
   including disconnect safety; phone/network interaction still needs end-to-end tests.
5. **RISC-V deployment (10):** one entry point for dependencies, official prebuilt Rust, SDK build and checks;
   architecture-specific artifacts, automated board installation/model retrieval/runtime delivery,
   signing, OTA/rollback and CI. Manual model Release download/development installation are not substitutes.

Robot integration requires **combined-load testing** with simultaneous motion, camera, vision, audio and streaming:
scheduling/memory, control periods, temperature, endurance, labelled visual accuracy and enclosure-specific acoustic accuracy.
Individual module tests do not replace it.
Extended IMX219 tests should record duration/error statistics,
different-lighting exposure/white balance, network/camera abnormal recovery and existing vendor warnings.
The existing IMX219 capture, streaming and detection workflows remain available for development.

<a id="5-用户现在可以做什么"></a>
## 5. What users can do now

Develop natively on K1, debug, run policies and fake-IO tests, use selected-eye USB or specified IMX219,
[download and verify the model](https://github.com/muggle-stack/microduck/releases/tag/models-duck-detect-v1),
run ORT/EP detection, preview IMX219 with [RTSP](k1-rtsp.md) or the [WebRTC console](k1-webrtc.md),
receive detection notifications and use ES8326 recording/playback.
Standalone detection and RTSP currently run separately and must not compete for the camera.

**IMX219 browser streaming and same-source inference work; physical walking, automatic installation and OTA remain unfinished.
This is a development version.** Do not run Radxa `setup-board.sh`, `provision-board.sh` or `install.sh` on K1,
or install aarch64 release packages. Start with the [K1 development installation guide](../robot/install-k1.md).

<a id="6-与官方-sdk-的关系"></a>
## 6. Relationship to the official SDK

The original RKNN implementation, motion-control core, Rockchip camera and AIC3104 defaults are retained.
K1 support is selected through config/optional backends and also adjusts shared configuration, frame acquisition,
ORT wrapping and ALSA device-name handling.
For example, invalid explicit media config now errors instead of falling back to Rockchip, including on ARM.
K1 tests do not replace real RK3566 regression.
