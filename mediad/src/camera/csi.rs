//! Opt-in K1 CSI capture through the installed spacemitsrc plugin.
//! Sensor/ISP setup remains vendor-owned; the detector still receives ordinary UYVY frames.

use anyhow::{Context, Result, ensure};
use gstreamer as gst;
use gstreamer::prelude::*;
use robotd_params::{CameraBackend, CameraParams, Quality};
use serde_json::Value;

use crate::pipeline::Rotation;

/// Reject geometry drift and profiles that start a tuning server or save frames.
/// This checks the profile, not sensor support: only a live board can prove that.
fn validate_profile(bytes: &[u8], camera: &CameraParams) -> Result<()> {
    let profile: Value = serde_json::from_slice(bytes).context("invalid ISP JSON")?;
    for key in [
        "tuning_server_enable",
        "auto_detect",
        "gpu_render",
        "save_yuv",
    ] {
        ensure!(
            profile[key].as_u64() == Some(0),
            "CSI profile requires {key} = 0"
        );
    }
    ensure!(
        profile["dump_one_frame"].as_i64() == Some(-1),
        "CSI profile requires dump_one_frame = -1 (no implicit frame dumps)"
    );
    for (key, name) in [("isp_node", "isp0"), ("cpp_node", "cpp0")] {
        let nodes = profile[key]
            .as_array()
            .with_context(|| format!("missing {key} array"))?;
        ensure!(!nodes.is_empty() && nodes.len() <= 2, "invalid {key} count");
        ensure!(
            nodes
                .iter()
                .all(|n| matches!(n["enable"].as_u64(), Some(0 | 1))),
            "invalid {key} enable flag"
        );
        let enabled: Vec<_> = nodes
            .iter()
            .filter(|n| n["enable"].as_u64() == Some(1))
            .collect();
        ensure!(
            enabled.len() == 1 && enabled[0]["name"].as_str() == Some(name),
            "CSI profile requires only {name} enabled"
        );
        let node = enabled[0];
        ensure!(
            node["format"].as_str() == Some("NV12"),
            "{name} must output NV12"
        );
        let (w, h) = if key == "isp_node" {
            ("out_width", "out_height")
        } else {
            ("size_width", "size_height")
        };
        ensure!(
            node[w].as_u64() == Some(u64::from(camera.width))
                && node[h].as_u64() == Some(u64::from(camera.height)),
            "{name} dimensions differ from camera.width/height"
        );
        if key == "isp_node" {
            ensure!(
                node["work_mode"].as_str() == Some("online"),
                "CSI requires online sensor input"
            );
            ensure!(
                node["fps"].as_f64() == Some(f64::from(camera.fps)),
                "ISP fps differs from camera.fps"
            );
            ensure!(
                node["sensor_name"].as_str().is_some_and(|s| !s.is_empty())
                    && node["sensor_id"].as_u64().is_some_and(|id| id <= 2),
                "CSI requires an explicit sensor name and ID 0..2"
            );
        } else {
            ensure!(
                node["src_from_file"].as_u64() == Some(0),
                "CSI requires live CPP input, not a file"
            );
        }
    }
    Ok(())
}

/// The encoder needs DMA-BUF; CPU capture keeps its existing system-memory caps.
pub(crate) fn native_source(camera: &CameraParams, dmabuf: bool) -> Result<gst::Element> {
    camera.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        camera.backend == CameraBackend::SpacemitCsi,
        "CSI source requires spacemit_csi backend"
    );
    ensure!(
        cfg!(target_arch = "riscv64"),
        "spacemit_csi requires the K1 RISC-V vendor camera stack"
    );
    let path = camera
        .isp_config
        .as_ref()
        .context("missing camera.isp_config")?;
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("cannot read ISP profile {}", path.display()))?;
    ensure!(
        metadata.is_file() && metadata.len() <= 65536,
        "ISP profile must be a regular JSON file of at most 64 KiB"
    );
    validate_profile(&std::fs::read(path)?, camera)?;
    let location = path.to_str().context("ISP profile path must be UTF-8")?;
    ensure!(!location.contains('\0'), "invalid ISP profile path");
    gst::ElementFactory::make("spacemitsrc")
        .property("location", location)
        .property("close-dmabuf", !dmabuf)
        .build()
        .context("missing spacemitsrc; use the matching Bianbu vendor GStreamer camera plugin")
}

pub(super) fn source(
    camera: &CameraParams,
    output: Option<Quality>,
    rotation: Rotation,
) -> Result<gst::Bin> {
    if let Some(q) = output {
        ensure!(
            q.fps() <= camera.fps,
            "media frame rate exceeds CSI capture rate"
        );
    }
    let src = native_source(camera, false)?;
    let input = super::usb::build(camera, output, src)?;
    let Some(direction) = rotation.video_direction() else {
        return Ok(input);
    };
    let flip = gst::ElementFactory::make("videoflip")
        .property_from_str("video-direction", direction)
        .build()?;
    let bin = gst::Bin::new();
    bin.add_many([input.upcast_ref::<gst::Element>(), &flip])?;
    input.link(&flip)?;
    bin.add_pad(&gst::GhostPad::with_target(
        &flip
            .static_pad("src")
            .context("CSI rotation lacks src pad")?,
    )?)?;
    Ok(bin)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = include_str!("../../../deploy/k1/imx219-csi3-720p.json");

    fn camera() -> CameraParams {
        CameraParams {
            backend: CameraBackend::SpacemitCsi,
            input_format: robotd_params::CameraFormat::Nv12,
            isp_config: Some("/unused/imx219.json".into()),
            ..Default::default()
        }
    }

    #[test]
    fn profile_matches_real_source_contract_and_rejects_unsafe_or_mismatched_inputs() {
        validate_profile(PROFILE.as_bytes(), &camera()).unwrap();
        let original: Value = serde_json::from_str(PROFILE).unwrap();
        for (pointer, value) in [
            ("/auto_detect", Value::from(1)),
            ("/tuning_server_enable", Value::from(1)),
            ("/save_yuv", Value::from(1)),
            ("/dump_one_frame", Value::from(90)),
            ("/isp_node/0/work_mode", Value::from("offline")),
            ("/isp_node/0/out_width", Value::from(1920)),
            ("/cpp_node/0/size_height", Value::from(1080)),
            ("/isp_node/0/fps", Value::from(60)),
            ("/isp_node/0/sensor_name", Value::from("")),
            ("/isp_node/0/sensor_id", Value::from(-1)),
            ("/isp_node/1/enable", Value::from(1)),
            ("/cpp_node/0/format", Value::from("I420")),
            ("/cpp_node/0/src_from_file", Value::from(1)),
        ] {
            let mut bad = original.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(
                validate_profile(&serde_json::to_vec(&bad).unwrap(), &camera()).is_err(),
                "{pointer}"
            );
        }
        assert!(validate_profile(b"{}", &camera()).is_err());
        assert!(validate_profile(b"broken", &camera()).is_err());
    }

    /// Never opened by CI. The opt-in test exercises teardown and a new ISP
    /// session in the same process, rather than hiding leaks behind process exit.
    #[test]
    #[ignore = "requires a free K1 CSI camera and MICRODUCK_K1_CSI_TEST_CONFIG"]
    fn repeated_open_read_drop() {
        let path = std::env::var("MICRODUCK_K1_CSI_TEST_CONFIG").expect("explicit CSI config");
        let params = robotd_params::Params::load(std::path::Path::new(&path), true).unwrap();
        assert_eq!(params.camera.backend, CameraBackend::SpacemitCsi);
        for _ in 0..3 {
            let capture =
                crate::camera::Capture::start(&params.camera, Some(params.media.quality)).unwrap();
            let mut previous = None;
            for _ in 0..10 {
                let frame = capture
                    .next_frame(std::time::Duration::from_secs(5))
                    .unwrap();
                let pts = frame.pts_ns.expect("CSI PTS");
                assert!(previous.is_none_or(|old| pts > old));
                previous = Some(pts);
            }
            drop(capture);
        }
    }
}
