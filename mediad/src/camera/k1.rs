//! Explicit, optional native K1 MPP/OpenCV camera bridge. Its C ABI keeps native
//! headers/libraries out of Rust, Radxa, host builds, and ordinary USB capture.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::Mutex;

use anyhow::{Context, Result, ensure};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use libloading::Library;
use robotd_params::{CameraParams, CameraRect, Quality};

const ABI: u32 = 1;
const ERROR_SIZE: usize = 1024;
#[repr(C)]
struct Config {
    size: u32,
    abi: u32,
    device: *const c_char,
    width: u32,
    height: u32,
    fps: u32,
    crop_x: u32,
    crop_y: u32,
    crop_width: u32,
    crop_height: u32,
    output_width: u32,
    output_height: u32,
    rotation: u32,
}
#[repr(C)]
#[derive(Default)]
struct Metadata {
    pts_ns: u64,
    sequence: u64,
    wait_us: u64,
    image_us: u64,
    pack_us: u64,
    width: u32,
    height: u32,
    bytes: u32,
}
type Open = unsafe extern "C" fn(*const Config, *mut *mut c_void, *mut c_char, usize) -> i32;
type Read = unsafe extern "C" fn(
    *mut c_void,
    *mut u8,
    usize,
    u32,
    *mut Metadata,
    *mut c_char,
    usize,
) -> i32;
type Close = unsafe extern "C" fn(*mut c_void);

struct Native {
    handle: NonNull<c_void>,
    read: Read,
    close: Close,
    width: u32,
    height: u32,
    _device: CString,
    _library: Library,
}
// The bridge permits moving a context between threads, but not concurrent reads
// or close/read races. The AppSrc callback below serializes it through a Mutex.
unsafe impl Send for Native {}

fn error_message(bytes: &[c_char]) -> String {
    let bytes: Vec<u8> = bytes.iter().map(|b| *b as u8).collect();
    CStr::from_bytes_until_nul(&bytes)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "native bridge returned an unterminated error".into())
}

impl Native {
    fn open(camera: &CameraParams, output: Option<Quality>, rotation: u32) -> Result<Self> {
        ensure!(
            cfg!(target_arch = "riscv64"),
            "SpaceMIT camera acceleration requires a RISC-V K1"
        );
        let path = PathBuf::from(std::env::var_os("MICRODUCK_K1_CAMERA_LIB").context(
            "set MICRODUCK_K1_CAMERA_LIB to the absolute library path produced by scripts/build-k1-camera.sh",
        )?);
        ensure!(
            path.is_absolute(),
            "MICRODUCK_K1_CAMERA_LIB must be absolute"
        );
        let device = CString::new(camera.device.as_str()).context("NUL in camera device path")?;
        let (width, height) = output.map_or((camera.width, camera.height), |q| q.size());
        let r = match output {
            Some(_) => camera.selected_region().map_err(anyhow::Error::msg)?,
            None => CameraRect {
                x: 0,
                y: 0,
                width: camera.width,
                height: camera.height,
            },
        };
        let config = Config {
            size: std::mem::size_of::<Config>() as u32,
            abi: ABI,
            device: device.as_ptr(),
            width: camera.width,
            height: camera.height,
            fps: camera.fps,
            crop_x: r.x,
            crop_y: r.y,
            crop_width: r.width,
            crop_height: r.height,
            output_width: width,
            output_height: height,
            rotation,
        };
        // This is an operator-selected native library. Check the version and exact
        // symbols before passing any pointer; retain the library until after close.
        unsafe {
            let library = Library::new(&path)
                .with_context(|| format!("loading K1 camera bridge {}", path.display()))?;
            let version = library.get::<unsafe extern "C" fn() -> u32>(b"md_k1_camera_abi\0")?;
            ensure!(
                version() == ABI,
                "K1 camera bridge ABI mismatch; rebuild the private bridge"
            );
            let open: Open = *library.get(b"md_k1_camera_open\0")?;
            let read: Read = *library.get(b"md_k1_camera_read\0")?;
            let close: Close = *library.get(b"md_k1_camera_close\0")?;
            let build = library
                .get::<unsafe extern "C" fn() -> *const c_char>(b"md_k1_camera_build_info\0")?;
            let build = build();
            ensure!(!build.is_null(), "K1 bridge returned no build information");
            tracing::info!(bridge = %path.display(), native = %CStr::from_ptr(build).to_string_lossy(), "using opt-in K1 camera acceleration");
            let mut handle = std::ptr::null_mut();
            let mut error = [0; ERROR_SIZE];
            let rc = open(&config, &mut handle, error.as_mut_ptr(), error.len());
            ensure!(
                rc == 0,
                "K1 camera initialization: {}",
                error_message(&error)
            );
            let handle = NonNull::new(handle).context("K1 bridge returned a null handle")?;
            let (width, height) = if matches!(rotation, 90 | 270) {
                (height, width)
            } else {
                (width, height)
            };
            Ok(Self {
                handle,
                read,
                close,
                width,
                height,
                _device: device,
                _library: library,
            })
        }
    }

    fn frame(&mut self) -> Result<(Vec<u8>, Metadata)> {
        let mut data = vec![0; self.width as usize * self.height as usize * 2];
        let mut meta = Metadata::default();
        let mut error = [0; ERROR_SIZE];
        // The C ABI bounds writes by capacity; the context is exclusively borrowed.
        let rc = unsafe {
            (self.read)(
                self.handle.as_ptr(),
                data.as_mut_ptr(),
                data.len(),
                3000,
                &mut meta,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        ensure!(rc == 0, "K1 camera read: {}", error_message(&error));
        ensure!(
            (meta.width, meta.height) == (self.width, self.height)
                && meta.bytes as usize == data.len(),
            "K1 bridge returned unexpected output geometry/size"
        );
        Ok((data, meta))
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        // No C++ exception crosses this function. _library remains loaded here.
        unsafe { (self.close)(self.handle.as_ptr()) };
    }
}

pub(super) fn source(
    camera: &CameraParams,
    output: Option<Quality>,
    rotation: u32,
) -> Result<gst::Bin> {
    let native = Native::open(camera, output, rotation)?;
    let (width, height) = (native.width, native.height);
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", crate::pipeline::CAPTURE_FORMAT)
        .field("colorimetry", "bt601")
        .field("width", width as i32)
        .field("height", height as i32)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .field("framerate", gst::Fraction::new(camera.fps as i32, 1))
        .build();
    // A single AppSrc worker owns reads. No detached Rust capture thread survives
    // source teardown. The native feeder has its own bounded waits and is joined.
    let state = Mutex::new((native, false, None::<gst::ClockTime>));
    let fps = camera.fps;
    let src = gst_app::AppSrc::builder()
        .caps(&caps)
        .is_live(true)
        .format(gst::Format::Time)
        .block(true)
        .max_buffers(1)
        .callbacks(
            gst_app::AppSrcCallbacks::builder()
                .need_data(move |src, _| {
                    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                    if state.1 {
                        return;
                    }
                    match state.0.frame() {
                        Ok((data, meta)) => {
                            let base = *state.2.get_or_insert_with(|| {
                                src.current_running_time().unwrap_or(gst::ClockTime::ZERO)
                            });
                            let mut buffer = gst::Buffer::from_mut_slice(data);
                            let b = buffer.get_mut().expect("new exclusive buffer");
                            b.set_pts(
                                base.saturating_add(gst::ClockTime::from_nseconds(meta.pts_ns)),
                            );
                            b.set_duration(gst::ClockTime::from_nseconds(
                                1_000_000_000 / u64::from(fps),
                            ));
                            b.set_offset(meta.sequence);
                            if let Err(flow) = src.push_buffer(buffer) {
                                // Flushing during state NULL is normal; never turn shutdown into a retry loop.
                                state.1 = true;
                                tracing::debug!(?flow, "K1 camera AppSrc stopped accepting frames");
                            }
                        }
                        Err(error) => {
                            state.1 = true;
                            gst::element_error!(
                                src,
                                gst::ResourceError::Read,
                                ("K1 camera failed: {error:#}")
                            );
                            let _ = src.end_of_stream();
                        }
                    }
                })
                .build(),
        )
        .build();
    let bin = gst::Bin::new();
    bin.add(&src)?;
    let last: gst::Element = if let Some(q) = output.filter(|q| q.fps() < camera.fps) {
        let rate = gst::ElementFactory::make("videorate")
            .property("drop-only", true)
            .build()?;
        let mut caps = caps.clone();
        caps.make_mut()
            .structure_mut(0)
            .unwrap()
            .set("framerate", gst::Fraction::new(q.fps() as i32, 1));
        let filter = gst::ElementFactory::make("capsfilter")
            .property("caps", &caps)
            .build()?;
        bin.add_many([&rate, &filter])?;
        gst::Element::link_many([src.upcast_ref(), &rate, &filter])?;
        filter
    } else {
        src.upcast()
    };
    bin.add_pad(&gst::GhostPad::with_target(
        &last.static_pad("src").context("K1 source lacks src pad")?,
    )?)?;
    Ok(bin)
}
