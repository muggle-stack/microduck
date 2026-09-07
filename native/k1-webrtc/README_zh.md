<a id="private-k1-webrtc-plugins"></a>
# K1 私有 WebRTC 插件

[English](README.md) · [简体中文](README_zh.md)

`rswebrtc-k1.patch` 适用于 GStreamer `gst-plugins-rs` **0.15.3**，
提交 `6302bea23b53e6461c104f9543df887a63b2d6ec`：
[上游源码](https://github.com/GStreamer/gst-plugins-rs/tree/0.15.3)。

修改的文件受上游 **MPL-2.0** 约束；再分发插件二进制时保留许可证，并提供这些修改。
本仓库不再分发上游源码或生成的二进制。

原 Microduck 插件包 v3（pollen-robotics/microduck-gst-plugins，
`a9a839f274fb20698d3abc2639a28d75421c5471`）同样使用 0.15.3。
其 mpph264enc identity-converter 绕过方案保留。
K1 另需 NV12 DMA-BUF 直通和按真实码流协商 H.264 Main；
板上编码器不提供 profile 选择器。补丁不重写 SPS，也不把 Main 冒充 Baseline。

退出的 K1 消费者先入队 EOS，最多等两秒通过编码器，再设置 NULL。
其他编码器保留上游退出路径。
EOS 超时仍告警，不声称编码器已经排空。

在 K1 用 Rust 1.92+ 执行 `sh scripts/build-k1-webrtc.sh`。
只构建 gst-plugin-webrtc、gst-plugin-rtp，
不启用可选 Janus/WHIP/WHEP/web-server features。
SDK 自身仍提供网页控制台，最低 Rust 1.89 不变。
只有运行可选 K1 mediad 时才设置 GST_PLUGIN_PATH，
不要覆盖 system/vendor/Radxa 插件。

使用方法和实测限制见 [K1 WebRTC 指南](../../docs/project/k1-webrtc_zh.md)。
