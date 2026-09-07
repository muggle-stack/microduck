<a id="k1--imx219rtsp-实时预览"></a>
# K1 / IMX219: live RTSP preview

[English](k1-rtsp.md) · [简体中文](k1-rtsp_zh.md)

> Dated test results and known issues are recorded below. See [adaptation status](spacemit-k1-adaptation.md) for current functionality, and the [IMX219](k1-imx219.md) / [WebRTC](k1-webrtc.md) guides for the integrated camera workflows.

`camera-rtsp` is an SDK preview tool that is **built optionally and run separately**.
It reuses `spacemit_csi` configuration and ISP-profile validation without starting `robotd`,
servos, audio, detection or WebRTC. Ordinary `cargo k1 --bins` and Radxa builds gain no native RTSP-server dependency.

```text
IMX219 → K1 ISP / CPP → NV12 DMA-BUF
       → spacemith264enc (MPP / VPU)→ H.264 → RTP → RTSP/TCP
       → SSH tunnel → desktop ffplay / VLC
```

Scope: MUSE-Pi-Pro CSI3 IMX219, 1280×720, configured 30 fps, mono, rotation 0.
`media.quality` must match camera dimensions/rate. The tool does not silently scale, rotate or fall back to software encoding.
USB capture and original Rockchip WebRTC code are unchanged; neither input is supported by this tool yet.

<a id="1-在-k1-编译"></a>
## 1. Build on K1

Complete the [K1 installation guide](../robot/install-k1.md) and [IMX219 setup](k1-imx219.md).
The additional dependency is `libgstrtspserver-1.0-dev`.
**Simulate installation first** to avoid replacing vendor GStreamer:

```sh
sudo apt-get -s install libgstrtspserver-1.0-dev
```

After confirming no existing vendor camera packages would be upgraded or removed:

```sh
sudo apt-get install libgstrtspserver-1.0-dev
pkg-config --modversion gstreamer-rtsp-server-1.0
gst-inspect-1.0 spacemitsrc
gst-inspect-1.0 spacemith264enc
```

Only three 1.24.2-1 packages were added on the tested board:
`libgstrtspserver-1.0-0`, `libgstrtspserver-1.0-dev` and `gir1.2-gst-rtsp-server-1.0`.
No installed packages were upgraded or removed.
The Rust tool does not require Python GI; GI was an associated apt package used for development probes.

```sh
cd ~/workspace/microduck
export PATH=/opt/microduck-rust-1.89.0/bin:$PATH
export CARGO_TARGET_DIR="$PWD/target"
cargo k1 --locked --features mediad/rtsp --bin camera-rtsp -j 2
```

First enabling `rtsp` downloads/builds its Rust bindings, not system GStreamer.
Another full `k1-test.sh` run is unnecessary. The existing development installer does not automatically install or start this optional tool.

<a id="2-启动相机预览"></a>
## 2. Start camera preview

Stop other consumers of the same camera, including `camera-check` and `mediad`.
Check that the absolute TOML `camera.isp_config` fits the current user/checkout location.
On K1:

```sh
cd ~/workspace/microduck
target/riscv64gc-unknown-linux-gnu/release/camera-rtsp \
  --config deploy/k1/camera-imx219.toml --duration 600
```

By default it listens only on `127.0.0.1:8554` at `/camera`, exiting after ten minutes.
Ctrl-C stops early; `--duration 0` runs continuously.
`RTSP listening` means only that the listener is ready: **the camera opens when a client connects**.
Use a player/decoder to establish that frames actually arrive.
Concurrent clients share one sensor and encoder.

<a id="3-在电脑看实时画面"></a>
## 3. View from a computer

Create the tunnel in one computer terminal, replacing `musepi` with your SSH alias or `USER@BOARD_IP`:

```sh
ssh -N -L 127.0.0.1:8554:127.0.0.1:8554 musepi
```

Open a player from another terminal:

```sh
ffplay -rtsp_transport tcp -fflags nobuffer -flags low_delay -framedrop \
  rtsp://127.0.0.1:8554/camera
```

VLC can open the same URL with RTP over RTSP/TCP enabled; the tool does not enable UDP transport.
A browser address bar cannot directly play RTSP.
A phone also needs an RTSP/TCP player; this is not instant webpage playback.
Quit the player, press Ctrl-C on K1 to stop the service, then Ctrl-C the SSH tunnel.

For deliberate direct access from a phone on a trusted LAN, add `--listen BOARD_LAN_IP` on K1
and open `rtsp://BOARD_LAN_IP:8554/camera` without SSH.
**There is no RTSP authentication or video encryption in this mode.**
Do not expose it publicly or add router port forwarding.

<a id="边界与已知问题"></a>
## Boundaries and known issues

- This previews live source images without boxes, ORT loading, audio or motion control.
  Boxes require a detection branch from **the same capture source**;
  another process must not compete for the sensor.
- This H.264/RTSP subpath is not evidence that complete `mediad` WebRTC, browser control
  or audio/video synchronization has been implemented.
- Hardware encoding requires `close-dmabuf=false` and `video/x-raw(memory:DMABuf)`.
  With `close-dmabuf=true` directly feeding the encoder, the test reported
  `Failed to queue buffer ... Cannot allocate memory`, wrote zero bytes and stalled at EOS until timeout.
  The DMA-BUF version produced 90 valid H.264 frames and exited cleanly.
  The original CPU detector capture still uses `close-dmabuf=true` and was not changed by this fix.
- The tested encoder plugin exposes no bitrate/GOP properties.
  The tool does not apply Rockchip properties or claim adjustable bitrate.
  Hardware H.264 is lossy, not byte-equivalent to NV12/UYVY; detection was unchanged.
- Early camera probes showed tint/highlight-overexposure issues requiring AE/AWB/PQ review by the camera team;
  no filter conceals them. Cold start, player buffering and keyframe intervals affect first-frame delay.
  End-to-end low latency and quantified endurance need dedicated measurements.
- Normal client exit can release media; an abrupt network loss may wait for RTSP session timeout.
  The last client sends EOS to drain encoding. A new connection waits for the old stream to close,
  then creates a new stream instead of reusing a finished vendor encoder instance.
  A shutdown wait over five seconds logs an error and may fail the new connection.
  On duration expiry or Ctrl-C, the listener closes and camera cleanup is attempted.
  Cleanup exceeding five seconds reports failure and exits unsuccessfully:
  **a timeout is not clean release**. An abnormal sensor may still require hardware reset.
  Vendor CPP shutdown warnings need upstream investigation.

<a id="2026-09-07-开发验证记录"></a>
## Development validation: 2026-09-07

- `camera-rtsp` built natively with Rust 1.89; a Python demo was not substituted for SDK integration.
- Final Linux software regression:
  `cargo test --locked --release --workspace --lib --bin camera-rtsp
  --features mediad/rtsp --target riscv64gc-unknown-linux-gnu -j 2 -- --test-threads=2`:
  **724 passed, zero failed, two hardware tests ignored by default**.
  Tests include loopback defaults, argument rejection, URL sensor sharing, replacing the shared key
  after asynchronous shutdown and simulated vendor cleanup errors/timeouts.
  The ignored CSI test was explicitly run afterward: three same-process open/read-10/close cycles,
  one pass in 2.05 seconds. USB hardware was not rerun.
  Mac mediad/params 32 + 98 tests and one CLI test also passed.
  The default ARM dependency tree excludes the new RTSP-server bindings; this is not an ARM hardware regression claim.
- Mac decoded SDK 720p H.264 through SSH: one run about ten seconds / 300 frames.
  ffplay displayed live images.
- With ffplay still connected, a second decoder using `?viewer=2` received about eight seconds / 238 frames.
  One factory key prevents URL queries from opening a second sensor.
- The SDK used hard cgroup `AllowedCPUs=0-2,4`; all sampled userspace threads were constrained.
  Test compilation overlapped, so **these are functional checks, not performance or end-to-end latency baselines**.
- A busy port caused exit 1 before another sensor opened.
- Around 17:02, a run encountered ISP interrupt timeouts and waited in cleanup.
  Around 17:03 the board rebooted, interrupting the last compilation/test and SSH.
  The reboot cause was not established; wiring needs investigation, and
  the journal cannot independently establish the cause.
  It is neither attributed directly to the SDK nor ignored as normal exit.
  This run is not a successful shutdown/stability acceptance; the cleanup deadline remains.
- After reboot, a short run with unchanged capture/encoding settings produced frames again.
  At 15-second expiry, it disconnected the active client and exited 0 with
  `sensor stream off` / `finish power off` logs and unchanged boot ID during that run.
  First-frame startup consumed part of the run, leaving only about 2.3 seconds / 70 decoded frames,
  not a claimed full four-second acceptance.

<a id="重连和退出修复"></a>
### Reconnect and shutdown fixes

The wiring clue did not replace software investigation: early code still reproduced cold-reconnect
failures while boot ID stayed unchanged. Retained failures and fixes:

- Going directly to NULL without EOS blocked shutdown for about 37 seconds;
  the next client's OPTIONS request timed out.
- Enabling EOS alone let an immediate reconnect obtain cached media still shutting down, causing RTSP 503.
- Reusing a finished vendor GstBin negotiated RTSP on the second connection but produced no valid frames;
  that approach was rejected.
- The final approach uses **EOS drain + shutdown-signal wait + a new shared cache key / GstBin per generation**.
  Concurrent clients still share one sensor, regardless of URL query.
  Service shutdown explicitly removes sessions to release lingering references.

Final native Rust tool acceptance:

- Three same-process “play three seconds → disconnect → immediately reconnect” cycles:
  91 decoded frames each, all exit 0.
- No compilation/other workload, hard cgroup `AllowedCPUs=0-2,4`, all sampled userspace threads constrained.
  A single 720p client received **599 frames** over a 20-second pull;
  decoded PTS span 19.975056 seconds, average **29.937 fps**.
  The client used passthrough frame rate and demux timebase.
  The first two PTS values were both zero, then increased.
  That startup timestamp issue is not resolved; no zero-timestamp-error, zero-loss or end-to-end-latency claim is made.
- Two simultaneous TCP clients, one with `?viewer=2`, decoded 891 / 829 frames.
  SIGINT while both remained connected produced client EOF and SDK **exit 0** with sensor stream-off/power-off.
  Compilation overlapped; this validates multi-client/shutdown function, not performance.
- Reopening with `--duration 12` disconnected the client at expiry after 360 decoded frames,
  exited 0 and logged stream-off/power-off. Expiry and SIGINT were independently tested.
- All these runs retained boot ID `385fa665-52e8-423f-96b4-7cfd0af98659`; no further reboot was used to recover capture.

Board logs: `/root/workspace/microduck/target/imx219-rtsp.59nbn6/`, including `generation-acceptance.log`.
Mac records: `target/rtsp-rust-generation-pass-*.log`,
`target/rtsp-generation-final-frames.md5` and `target/rtsp-generation-overlap-*.log`.
These bounded runs do not replace endurance, hot-plug, abnormal-network or whole-robot-load tests.
