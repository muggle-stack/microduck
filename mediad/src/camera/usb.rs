//! UVC/V4L2 camera input shared by mediad and the headless camera-check tool.
//! No Rockchip sensor controls, ISP startup, serial ports, or signalling servers.

use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use gst_video::prelude::*;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use robotd_params::{
    CameraAcceleration, CameraBackend, CameraFormat, CameraParams, CameraRect, Quality,
};

use crate::pipeline::{CAPTURE_FORMAT, Frame, Rotation};

fn element(name: &str) -> Result<gst::Element> {
    gst::ElementFactory::make(name)
        .build()
        .with_context(|| format!("missing GStreamer element {name}"))
}

fn raw_caps(width: u32, height: u32, fps: u32) -> gst::Caps {
    // The existing detector consumes BT.601 limited-range UYVY. videoconvert maps
    // the source's negotiated colourimetry/range instead of relabelling MJPEG bytes.
    gst::Caps::builder("video/x-raw")
        .field("format", CAPTURE_FORMAT)
        .field("colorimetry", "bt601")
        .field("width", width as i32)
        .field("height", height as i32)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .field("framerate", gst::Fraction::new(fps as i32, 1))
        .build()
}

fn filter(caps: &gst::Caps) -> Result<gst::Element> {
    Ok(gst::ElementFactory::make("capsfilter")
        .property("caps", caps)
        .build()?)
}

/// Native USB mode is independent from the outgoing media quality. `None` keeps
/// both eyes in the native frame; `Some` selects one ROI and letterboxes to that quality.
pub fn source(camera: &CameraParams, output: Option<Quality>) -> Result<gst::Bin> {
    source_with_rotation(camera, output, Rotation::None)
}

/// Physical output rotation, independent of mount metadata. K1 uses V2D;
/// portable USB uses videoflip. Ordinary consumers should leave this at None.
pub fn source_with_rotation(
    camera: &CameraParams,
    output: Option<Quality>,
    rotation: Rotation,
) -> Result<gst::Bin> {
    camera.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        camera.backend == CameraBackend::Usb,
        "USB capture requires camera.backend = 'usb'"
    );
    if let Some(q) = output {
        ensure!(
            q.fps() <= camera.fps,
            "media frame rate exceeds native USB rate; choose a lower media.quality"
        );
    }
    if camera.acceleration == CameraAcceleration::Spacemit {
        return super::k1::source(camera, output, rotation.degrees());
    }
    let src = gst::ElementFactory::make("v4l2src")
        .property("device", &camera.device)
        .build()?;
    let input = build(camera, output, src)?;
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
        &flip.static_pad("src").context("videoflip has no src pad")?,
    )?)?;
    Ok(bin)
}

pub(super) fn build(
    camera: &CameraParams,
    output: Option<Quality>,
    src: gst::Element,
) -> Result<gst::Bin> {
    let mut input = gst::Caps::builder(if camera.input_format == CameraFormat::Mjpeg {
        "image/jpeg"
    } else {
        "video/x-raw"
    })
    .field("width", camera.width as i32)
    .field("height", camera.height as i32)
    .field("framerate", gst::Fraction::new(camera.fps as i32, 1));
    if camera.input_format != CameraFormat::Mjpeg {
        input = input.field(
            "format",
            match camera.input_format {
                CameraFormat::Yuyv => "YUY2",
                CameraFormat::Uyvy => "UYVY",
                CameraFormat::Nv12 => "NV12",
                CameraFormat::Mjpeg => unreachable!(),
            },
        );
    }
    let mut elements = vec![src, filter(&input.build())?];
    if camera.input_format == CameraFormat::Mjpeg {
        // The portable default. Hardware acceleration is selected explicitly
        // before building this path; there is no automatic backend fallback.
        elements.push(element("jpegdec")?);
    }
    let convert = element("videoconvert")?;
    if camera.backend == CameraBackend::SpacemitCsi {
        convert.set_property("n-threads", 1u32);
    }
    elements.push(convert);
    elements.push(filter(&raw_caps(camera.width, camera.height, camera.fps))?);
    if let Some(quality) = output {
        let r = camera.selected_region().map_err(anyhow::Error::msg)?;
        elements.push(
            gst::ElementFactory::make("videocrop")
                .property("left", r.x as i32)
                .property("top", r.y as i32)
                .property("right", (camera.width - r.x - r.width) as i32)
                .property("bottom", (camera.height - r.y - r.height) as i32)
                .build()?,
        );
        // Preserve aspect ratio: a 16:10 eye must not be stretched into a 16:9 stream.
        elements.push(
            gst::ElementFactory::make("videoscale")
                .property("add-borders", true)
                .build()?,
        );
        elements.push(
            gst::ElementFactory::make("videorate")
                .property("drop-only", true)
                .build()?,
        );
        ensure!(
            quality.fps() <= camera.fps,
            "media frame rate exceeds native USB rate; choose a lower media.quality"
        );
        elements.push(filter(&raw_caps(
            quality.width(),
            quality.height(),
            quality.fps(),
        ))?);
    }
    let bin = gst::Bin::new();
    for el in &elements {
        bin.add(el)?;
    }
    gst::Element::link_many(elements.iter())?;
    let pad = elements
        .last()
        .unwrap()
        .static_pad("src")
        .context("USB source has no src pad")?;
    bin.add_pad(&gst::GhostPad::with_target(&pad)?)?;
    Ok(bin)
}

/// Read the negotiated layout, including GstVideoMeta stride/offset, before copying.
pub fn frame_from_sample(sample: &gst::Sample) -> Result<Frame> {
    let caps = sample.caps().context("camera sample has no caps")?;
    let info = gst_video::VideoInfo::from_caps(caps)?;
    ensure!(
        info.format() == gst_video::VideoFormat::Uyvy,
        "expected UYVY, got {}",
        info.format()
    );
    let buffer = sample.buffer().context("camera sample has no buffer")?;
    let video = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)?;
    let stride =
        usize::try_from(video.plane_stride()[0]).context("negative UYVY stride is unsupported")?;
    let (width, height) = (video.width(), video.height());
    ensure!(
        width <= 8192 && height <= 8192 && u64::from(width) * u64::from(height) <= 16_777_216,
        "camera sample exceeds the supported dimensions"
    );
    let data = super::copy_uyvy_rect(
        video.plane_data(0)?,
        width,
        height,
        stride,
        CameraRect {
            x: 0,
            y: 0,
            width,
            height,
        },
    )?;
    Ok(Frame {
        width,
        height,
        format: CAPTURE_FORMAT,
        data,
    })
}

#[derive(Debug)]
pub struct CameraFrame {
    pub image: Frame,
    pub pts_ns: Option<u64>,
    pub sequence: Option<u64>,
}

/// Both crops carry the same source timestamp. This does not assert calibrated
/// geometry or hardware exposure synchronisation between the two sensors.
#[derive(Debug)]
pub struct CameraViews {
    pub left: Frame,
    pub right: Option<Frame>,
    pub pts_ns: Option<u64>,
    pub sequence: Option<u64>,
}

impl CameraFrame {
    pub fn views(&self, camera: &CameraParams) -> Result<CameraViews> {
        ensure!(
            self.image.format == CAPTURE_FORMAT,
            "camera views require UYVY"
        );
        ensure!(
            self.image.width == camera.width && self.image.height == camera.height,
            "SBS regions require the native input dimensions"
        );
        let (left, right) = camera.regions().map_err(anyhow::Error::msg)?;
        let crop = |r: CameraRect| -> Result<Frame> {
            Ok(Frame {
                width: r.width,
                height: r.height,
                format: CAPTURE_FORMAT,
                data: super::copy_uyvy_rect(
                    &self.image.data,
                    self.image.width,
                    self.image.height,
                    self.image.width as usize * 2,
                    r,
                )?,
            })
        };
        Ok(CameraViews {
            left: crop(left)?,
            right: right.map(crop).transpose()?,
            pts_ns: self.pts_ns,
            sequence: self.sequence,
        })
    }
}

/// A bounded-pull, latest-frame headless source. Drop releases the camera.
pub struct Capture {
    pipeline: gst::Pipeline,
    sink: gst_app::AppSink,
    width: u32,
    height: u32,
}

impl Capture {
    pub fn start(camera: &CameraParams, output: Option<Quality>) -> Result<Self> {
        Self::start_with_rotation(camera, output, Rotation::None)
    }

    pub fn start_with_rotation(
        camera: &CameraParams,
        output: Option<Quality>,
        rotation: Rotation,
    ) -> Result<Self> {
        gst::init()?;
        let src = super::source_with_rotation(camera, output, rotation)?;
        let (width, height) = output.map_or((camera.width, camera.height), |q| q.size());
        let (width, height) = rotation.output(width, height);
        Self::from_source(src, width, height)
    }

    fn from_source(src: gst::Bin, width: u32, height: u32) -> Result<Self> {
        let pipeline = gst::Pipeline::new();
        let sink = gst_app::AppSink::builder()
            .sync(false)
            .max_buffers(1)
            .drop(true)
            .wait_on_eos(false)
            .build();
        pipeline.add_many([src.upcast_ref::<gst::Element>(), sink.upcast_ref()])?;
        src.link(&sink)?;
        let capture = Self {
            pipeline,
            sink,
            width,
            height,
        };
        capture
            .pipeline
            .set_state(gst::State::Playing)
            .context("camera would not start")?;
        Ok(capture)
    }

    fn check_bus(&self) -> Result<()> {
        if let Some(bus) = self.pipeline.bus() {
            for message in bus.iter_filtered(&[gst::MessageType::Error]) {
                if let gst::MessageView::Error(error) = message.view() {
                    bail!(
                        "camera pipeline error: {} ({:?})",
                        error.error(),
                        error.debug()
                    );
                }
            }
        }
        Ok(())
    }

    pub fn next_frame(&self, timeout: Duration) -> Result<CameraFrame> {
        self.check_bus()?;
        let timeout =
            gst::ClockTime::from_nseconds(timeout.as_nanos().min(u64::MAX as u128 - 1) as u64);
        let Some(sample) = self.sink.try_pull_sample(timeout) else {
            self.check_bus()?;
            bail!("camera produced no frame before the timeout (or reached EOS)");
        };
        let image = frame_from_sample(&sample)?;
        ensure!(
            (image.width, image.height) == (self.width, self.height),
            "camera negotiated unexpected dimensions"
        );
        let buffer = sample.buffer().context("camera sample has no buffer")?;
        Ok(CameraFrame {
            image,
            pts_ns: buffer.pts().map(|t| t.nseconds()),
            sequence: (buffer.offset() != u64::MAX).then_some(buffer.offset()),
        })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Err(error) = self.pipeline.set_state(gst::State::Null) {
            tracing::warn!(%error, "camera cleanup failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use robotd_params::{CameraLayout, CameraView};

    /// Explicit hardware test, never run by ordinary CI or with an inferred device.
    #[test]
    #[ignore = "needs K1, a free MJPEG UVC camera, and explicit bridge/config environment"]
    fn k1_repeated_open_read_drop() {
        let path = std::env::var("MICRODUCK_K1_CAMERA_TEST_CONFIG").expect("explicit test config");
        let params = robotd_params::Params::load(std::path::Path::new(&path), true).unwrap();
        assert_eq!(params.camera.acceleration, CameraAcceleration::Spacemit);
        for _ in 0..3 {
            let capture = Capture::start(&params.camera, Some(params.media.quality)).unwrap();
            let mut previous = None;
            for _ in 0..5 {
                let frame = capture.next_frame(Duration::from_secs(5)).unwrap();
                let pts = frame.pts_ns.expect("capture PTS");
                assert!(previous.is_none_or(|old| pts > old));
                previous = Some(pts);
            }
            drop(capture); // Next open must work in this SAME process.
        }
    }

    #[test]
    fn sample_mapping_honors_video_meta_offset_and_row_padding() {
        gst::init().unwrap();
        let mut bytes = vec![255u8; 32];
        bytes[4..12].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        bytes[16..24].copy_from_slice(&[11, 12, 13, 14, 15, 16, 17, 18]);
        let mut buffer = gst::Buffer::from_mut_slice(bytes);
        gst_video::VideoMeta::add_full(
            buffer.get_mut().unwrap(),
            gst_video::VideoFrameFlags::empty(),
            gst_video::VideoFormat::Uyvy,
            4,
            2,
            &[4],
            &[12],
        )
        .unwrap();
        let sample = gst::Sample::builder()
            .buffer(&buffer)
            .caps(&raw_caps(4, 2, 30))
            .build();
        let frame = frame_from_sample(&sample).unwrap();
        assert_eq!(
            frame.data,
            [1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 13, 14, 15, 16, 17, 18]
        );
    }

    #[test]
    fn selected_eye_pixels_and_aspect_borders_are_not_swapped_or_stretched() {
        gst::init().unwrap();
        for (view, expected_y) in [(CameraView::Left, 32), (CameraView::Right, 192)] {
            let config = CameraParams {
                backend: CameraBackend::Usb,
                device: "/not-opened".into(),
                input_format: CameraFormat::Uyvy,
                width: 8,
                height: 4,
                layout: CameraLayout::StereoSbs,
                view,
                ..Default::default()
            };
            let src = gst_app::AppSrc::builder()
                .caps(&raw_caps(8, 4, 30))
                .format(gst::Format::Time)
                .build();
            let mut bytes = Vec::new();
            for _ in 0..4 {
                for pair in 0..4 {
                    let y = if pair < 2 { 32 } else { 192 };
                    bytes.extend_from_slice(&[128, y, 128, y]);
                }
            }
            let mut buffer = gst::Buffer::from_mut_slice(bytes);
            buffer.get_mut().unwrap().set_pts(gst::ClockTime::ZERO);
            src.push_buffer(buffer).unwrap();
            src.end_of_stream().unwrap();
            let capture = Capture::from_source(
                build(&config, Some(Quality::Q360p30), src.upcast()).unwrap(),
                640,
                360,
            )
            .unwrap();
            let frame = capture.next_frame(Duration::from_secs(3)).unwrap();
            assert_eq!(frame.image.data[(180 * 640 + 320) * 2 + 1], expected_y);
            // A square eye in a 16:9 output has black side borders, not horizontal stretch.
            assert_eq!(frame.image.data[(180 * 640 + 20) * 2 + 1], 16);
        }
    }

    #[test]
    fn synthetic_nv12_source_normalises_and_splits_one_timestamped_frame() {
        gst::init().unwrap();
        let config = CameraParams {
            backend: CameraBackend::Usb,
            device: "/not-opened".into(),
            input_format: CameraFormat::Nv12,
            width: 8,
            height: 4,
            layout: CameraLayout::StereoSbs,
            ..Default::default()
        };
        let src = gst::ElementFactory::make("videotestsrc")
            .property("num-buffers", 2i32)
            .build()
            .unwrap();
        let capture = Capture::from_source(build(&config, None, src).unwrap(), 8, 4).unwrap();
        let frame = capture.next_frame(Duration::from_secs(3)).unwrap();
        let views = frame.views(&config).unwrap();
        assert_eq!(views.left.data.len(), 4 * 4 * 2);
        assert_eq!(views.right.as_ref().unwrap().data.len(), 4 * 4 * 2);
        assert_eq!(views.pts_ns, frame.pts_ns);
        for y in 0..4 {
            assert_eq!(
                &views.left.data[y * 8..y * 8 + 8],
                &frame.image.data[y * 16..y * 16 + 8]
            );
            assert_eq!(
                &views.right.as_ref().unwrap().data[y * 8..y * 8 + 8],
                &frame.image.data[y * 16 + 8..y * 16 + 16]
            );
        }
    }

    #[test]
    fn synthetic_selected_right_view_uses_the_daemon_source_builder() {
        gst::init().unwrap();
        let config = CameraParams {
            backend: CameraBackend::Usb,
            device: "/not-opened".into(),
            input_format: CameraFormat::Yuyv,
            width: 1280,
            height: 720,
            layout: CameraLayout::StereoSbs,
            view: CameraView::Right,
            ..Default::default()
        };
        let src = gst::ElementFactory::make("videotestsrc")
            .property("num-buffers", 1i32)
            .build()
            .unwrap();
        let capture = Capture::from_source(
            build(&config, Some(Quality::Q360p30), src).unwrap(),
            640,
            360,
        )
        .unwrap();
        let frame = capture.next_frame(Duration::from_secs(3)).unwrap();
        assert_eq!((frame.image.width, frame.image.height), (640, 360));
        assert_eq!(frame.image.data.len(), 640 * 360 * 2);
    }

    #[test]
    fn missing_device_fails_without_starting_another_backend() {
        let config = CameraParams {
            backend: CameraBackend::Usb,
            device: "/no-such-microduck-camera".into(),
            ..Default::default()
        };
        if let Ok(capture) = Capture::start(&config, None) {
            assert!(capture.next_frame(Duration::from_millis(200)).is_err());
        }
    }
}
