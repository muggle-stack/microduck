//! A single, shared K1 CSI sensor feeding the native H.264 encoder and RTSP/TCP.
//! This preview is independent of mediad's Rockchip/WebRTC daemon.

use anyhow::{Context, Result, ensure};
use glib::subclass::prelude::*;
use gstreamer as gst;
use gstreamer_rtsp as rtsp;
use gstreamer_rtsp_server::{self as server, prelude::*, subclass::prelude::*};
use robotd_params::{CameraBackend, CameraParams, Params};
use std::{
    net::IpAddr,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn validate(params: &Params) -> Result<()> {
    let camera = &params.camera;
    camera.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        camera.backend == CameraBackend::SpacemitCsi,
        "RTSP preview currently requires camera.backend = 'spacemit_csi'"
    );
    ensure!(
        params.media.camera,
        "media.camera must be enabled for RTSP preview"
    );
    ensure!(
        camera.rotation() == 0,
        "native RTSP preview requires camera.rotate = 0"
    );
    let q = params.media.quality;
    ensure!(
        (q.width(), q.height(), q.fps()) == (camera.width, camera.height, camera.fps),
        "native RTSP preview requires media.quality to match camera width/height/fps (no implicit resizing)"
    );
    Ok(())
}

fn payload_bin(camera: &CameraParams) -> Result<gst::Bin> {
    let src = crate::camera::csi::native_source(camera, true)?;
    // ISP-mapped pointers passed as USERPTR fail in this vendor encoder.
    // Keep the DMA-BUF feature explicit across this link.
    let caps = gst::Caps::builder("video/x-raw")
        .features(["memory:DMABuf"])
        .field("format", "NV12")
        .field("width", camera.width as i32)
        .field("height", camera.height as i32)
        .field("framerate", gst::Fraction::new(camera.fps as i32, 1))
        .build();
    let filter = gst::ElementFactory::make("capsfilter")
        .property("caps", caps)
        .build()?;
    let enc = gst::ElementFactory::make("spacemith264enc")
        .build()
        .context("missing spacemith264enc; install the matching vendor codec plugin")?;
    let parse = gst::ElementFactory::make("h264parse").build()?;
    let pay = gst::ElementFactory::make("rtph264pay")
        .name("pay0")
        .property("pt", 96_u32)
        // A late client needs SPS/PPS again with the next IDR.
        .property("config-interval", -1_i32)
        .build()?;
    let bin = gst::Bin::new();
    bin.add_many([&src, &filter, &enc, &parse, &pay])?;
    gst::Element::link_many([&src, &filter, &enc, &parse, &pay])?;
    Ok(bin)
}

mod imp {
    use super::*;
    #[derive(Clone)]
    pub struct TrackedMedia {
        pub media: glib::WeakRef<server::RTSPMedia>,
        pub closed: Arc<AtomicBool>,
    }
    #[derive(Default)]
    pub struct CameraFactory {
        pub camera: OnceLock<CameraParams>,
        pub media: Mutex<Vec<TrackedMedia>>,
        pub generation: Arc<AtomicU64>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for CameraFactory {
        const NAME: &'static str = "MicroduckCsiRtspFactory";
        type Type = super::CameraFactory;
        type ParentType = server::RTSPMediaFactory;
    }
    impl ObjectImpl for CameraFactory {}
    impl RTSPMediaFactoryImpl for CameraFactory {
        fn gen_key(&self, _url: &rtsp::RTSPUrl) -> Option<glib::GString> {
            // EOS shutdown is asynchronous. A new DESCRIBE must wait for the
            // old sensor to close, but must not reuse that vendor GstBin after
            // EOS. gen_key runs before GStreamer's factory cache lock; don't
            // hold our own lock while waiting for the media's signal either.
            let deadline = Instant::now() + Duration::from_secs(5);
            let sessions = self.media.lock().unwrap().clone();
            for tracked in sessions {
                if let Some(media) = tracked.media.upgrade() {
                    while !tracked.closed.load(Ordering::Acquire)
                        && matches!(
                            media.status(),
                            server::RTSPMediaStatus::Unpreparing
                                | server::RTSPMediaStatus::Unprepared
                        )
                    {
                        if Instant::now() >= deadline {
                            // Keep the existing cache key on failure, rather
                            // than opening a second sensor over stuck hardware.
                            eprintln!(
                                "camera-rtsp: previous media is still closing; reconnect may fail"
                            );
                            return Some(self.cache_key());
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
            // Query strings / different hostnames share one active generation.
            Some(self.cache_key())
        }
        fn create_element(&self, _url: &rtsp::RTSPUrl) -> Option<gst::Element> {
            match self
                .camera
                .get()
                .context("missing camera configuration")
                .and_then(payload_bin)
            {
                Ok(bin) => Some(bin.upcast()),
                Err(error) => {
                    eprintln!("camera-rtsp: cannot build media: {error:#}");
                    None
                }
            }
        }
        fn media_configure(&self, media: &server::RTSPMedia) {
            self.parent_media_configure(media);
            let closed = Arc::new(AtomicBool::new(false));
            let done = closed.clone();
            let generation = self.generation.clone();
            media.connect_unprepared(move |_| {
                // UNPREPARED status is set just before this signal and cache
                // eviction. A fresh key also avoids racing that eviction.
                generation.fetch_add(1, Ordering::AcqRel);
                done.store(true, Ordering::Release);
            });
            let mut sessions = self.media.lock().unwrap();
            sessions.retain(|m| m.media.upgrade().is_some());
            sessions.push(TrackedMedia {
                media: media.downgrade(),
                closed,
            });
        }
    }
    impl CameraFactory {
        fn cache_key(&self) -> glib::GString {
            format!("microduck-csi-{}", self.generation.load(Ordering::Acquire)).into()
        }
    }
}

glib::wrapper! {
    pub struct CameraFactory(ObjectSubclass<imp::CameraFactory>) @extends server::RTSPMediaFactory;
}

/// Runs until SIGINT, SIGTERM, or the optional time limit. Listening does not
/// start capture; the first client opens the shared camera.
pub fn serve(params: &Params, listen: IpAddr, port: u16, duration: u32) -> Result<()> {
    validate(params)?;
    ensure!(port > 0, "RTSP port must be nonzero");
    gst::init()?;
    // Check profile, plugins and links before announcing a URL. NULL-state
    // construction deliberately does not open the physical sensor.
    drop(payload_bin(&params.camera)?);
    let factory: CameraFactory = glib::Object::new();
    factory.imp().camera.set(params.camera.clone()).unwrap();
    factory.set_shared(true);
    factory.set_protocols(rtsp::RTSPLowerTrans::TCP);
    factory.set_suspend_mode(server::RTSPSuspendMode::None);
    // The vendor encoder otherwise spends tens of seconds draining during
    // NULL, blocking the RTSP request thread and the next client's OPTIONS.
    factory.set_eos_shutdown(true);
    factory.set_stop_on_disconnect(false);
    factory.set_latency(0);
    let srv = server::RTSPServer::new();
    srv.set_address(&listen.to_string());
    srv.set_service(&port.to_string());
    srv.mount_points()
        .context("missing RTSP mount points")?
        .add_factory("/camera", factory.clone());
    let listener = srv
        .attach(None)
        .context("cannot bind RTSP listener (address/port already in use?)")?;
    let main_loop = glib::MainLoop::new(None, false);
    let mut sources = Vec::new();
    // Linux SIGINT/SIGTERM. Remove sources exactly once after run(); returning
    // Break from these callbacks would already destroy their source IDs.
    for signum in [2, 15] {
        let stop = main_loop.clone();
        sources.push(glib::unix_signal_add_local(signum, move || {
            stop.quit();
            glib::ControlFlow::Continue
        }));
    }
    if duration > 0 {
        let stop = main_loop.clone();
        sources.push(glib::timeout_add_seconds_local(duration, move || {
            stop.quit();
            glib::ControlFlow::Continue
        }));
    }
    let pool = srv.session_pool().context("missing RTSP session pool")?;
    sources.push(glib::timeout_add_seconds_local(5, move || {
        pool.cleanup();
        glib::ControlFlow::Continue
    }));
    if !listen.is_loopback() {
        eprintln!(
            "WARNING: unauthenticated camera video exposed on {listen}; use a trusted LAN only."
        );
    }
    let address = std::net::SocketAddr::new(listen, port);
    eprintln!(
        "RTSP listening: rtsp://{address}/camera (TCP only; video starts when a client connects)"
    );
    eprintln!(
        "Video only: no detection boxes, audio, WebRTC, or robot control. Stop with Ctrl-C; duration={duration}s (0=unlimited)."
    );
    main_loop.run();
    listener.remove();
    for source in sources {
        source.remove();
    }
    // A disconnected/faulted vendor sensor can block even a NULL transition.
    // Move ALL media-owning refs into this worker, including their destructors:
    // returning a timeout must not block again while dropping them on this thread.
    finish_with_timeout(
        move || {
            eprintln!("RTSP shutdown: closing clients");
            // Do not close clients while holding the server's filter lock.
            for client in srv.client_filter(None) {
                client.close();
            }
            // Closing TCP alone does not remove RTSP sessions: they normally
            // survive until timeout, retaining media prepare counts and streams.
            // On server shutdown remove them explicitly, outside the pool lock.
            eprintln!("RTSP shutdown: removing sessions");
            if let Some(pool) = srv.session_pool() {
                for session in pool.filter(None) {
                    pool.remove(&session)
                        .context("cannot remove RTSP session")?;
                }
            }
            eprintln!("RTSP shutdown: releasing media");
            let media: Vec<_> = factory
                .imp()
                .media
                .lock()
                .unwrap()
                .iter()
                .filter_map(|m| m.media.upgrade())
                .collect();
            for session in media {
                session.unprepare().context("cannot stop RTSP camera")?;
                // EOS shutdown is asynchronous. Its RTSP media thread must
                // finish before we report success or drop the server.
                while session.status() != server::RTSPMediaStatus::Unprepared {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            eprintln!("RTSP shutdown: releasing server");
            drop(srv);
            drop(factory);
            Ok(())
        },
        Duration::from_secs(5),
    )?;
    eprintln!("RTSP stopped; camera released.");
    Ok(())
}

fn finish_with_timeout(
    shutdown: impl FnOnce() -> Result<()> + Send + 'static,
    timeout: Duration,
) -> Result<()> {
    let (done, result) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("rtsp-shutdown".into())
        .spawn(move || {
            let _ = done.send(shutdown());
        })
        .context("cannot start RTSP cleanup")?;
    result.recv_timeout(timeout).context(
        "vendor camera shutdown did not finish; exit this process before reopening the camera (hardware reset may be needed)"
    )?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_reports_failure_and_cannot_wait_forever_on_vendor_code() {
        finish_with_timeout(|| Ok(()), Duration::from_secs(1)).unwrap();
        assert!(
            finish_with_timeout(|| anyhow::bail!("vendor error"), Duration::from_secs(1)).is_err()
        );
        let (release, wait) = mpsc::channel();
        let result = finish_with_timeout(
            move || {
                let _ = wait.recv();
                Ok(())
            },
            Duration::from_millis(20),
        );
        release.send(()).unwrap();
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("shutdown did not finish")
        );
    }

    #[test]
    fn urls_share_one_sensor_even_with_different_queries() {
        gst::init().unwrap();
        let factory: CameraFactory = glib::Object::new();
        let (status, a) = rtsp::RTSPUrl::parse("rtsp://127.0.0.1:8554/camera");
        assert_eq!(status, rtsp::RTSPResult::Ok);
        let (status, b) = rtsp::RTSPUrl::parse("rtsp://localhost:8554/camera?viewer=2");
        assert_eq!(status, rtsp::RTSPResult::Ok);
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(factory.imp().gen_key(&a), factory.imp().gen_key(&b));
    }

    #[test]
    fn closed_media_gets_a_fresh_shared_key_without_reusing_the_vendor_bin() {
        gst::init().unwrap();
        let factory: CameraFactory = glib::Object::new();
        let (_, url) = rtsp::RTSPUrl::parse("rtsp://127.0.0.1:8554/camera");
        let url = url.unwrap();
        let first_key = factory.imp().gen_key(&url);
        let media = server::RTSPMedia::new(gst::Bin::new());
        factory.imp().media_configure(&media);
        assert!(!media.is_reusable());
        // Model the async EOS completion, without opening camera hardware.
        // UNPREPARED alone must not reuse the old cache entry before its signal.
        let completion = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            media.emit_by_name::<()>("unprepared", &[]);
        });
        let next_key = factory.imp().gen_key(&url);
        completion.join().unwrap();
        assert_ne!(first_key, next_key);
        assert_eq!(next_key, factory.imp().gen_key(&url));
    }

    #[test]
    fn rejects_silent_backend_rotation_and_geometry_changes() {
        let mut p = camera_params();
        validate(&p).unwrap();
        p.camera.rotate = Some(90);
        assert!(validate(&p).is_err());
        p = camera_params();
        p.media.camera = false;
        assert!(validate(&p).is_err());
        p = camera_params();
        p.camera.fps = 15;
        assert!(validate(&p).is_err());
        assert!(validate(&Params::default()).is_err());
    }
    fn camera_params() -> Params {
        // Load the shipped config, but never open a sensor in unit tests.
        Params::load(
            std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../deploy/k1/camera-imx219.toml"
            )),
            true,
        )
        .unwrap()
    }
}
