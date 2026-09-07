# Private K1 WebRTC plugins

`rswebrtc-k1.patch` applies to GStreamer's `gst-plugins-rs` **0.15.3**, commit
`6302bea23b53e6461c104f9543df887a63b2d6ec`:
<https://github.com/GStreamer/gst-plugins-rs/tree/0.15.3>.
The patched files are covered by upstream's **MPL-2.0** license; retain that license
and make these modifications available when redistributing the plugin binaries.
This repository does not redistribute the upstream sources or generated binaries.

The original Microduck plugin pack v3
(`pollen-robotics/microduck-gst-plugins`, `a9a839f274fb20698d3abc2639a28d75421c5471`)
also uses 0.15.3. Its `mpph264enc` identity-converter workaround is retained.
K1 additionally needs NV12 DMA-BUF passthrough and truthful H.264 Main-profile
negotiation: the installed vendor encoder does not expose a profile selector.
The patch does not rewrite SPS bytes or label Main-profile video as Baseline.
Retired K1 consumer pipelines enqueue EOS and wait up to two seconds for it to
leave the encoder before NULL. Other encoders retain upstream's teardown path.
An EOS timeout remains a warning, not a claim that the encoder was drained.

Build with `sh scripts/build-k1-webrtc.sh` on K1 using Rust 1.92+.
It builds only `gst-plugin-webrtc` and `gst-plugin-rtp`, without optional
Janus/WHIP/WHEP/web-server features. The SDK continues to serve its own console
and retains its Rust 1.89 minimum. Only set `GST_PLUGIN_PATH` for the opt-in K1 run;
do not install these libraries over system/vendor/Radxa plugins.

User setup and measured limitations: [K1 WebRTC guide](../../docs/project/k1-webrtc.md).
