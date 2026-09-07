//! Bounded USB/K1 CSI capture and detection, without WebRTC, audio, or a robot bus.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    about = "Check SDK USB mono/SBS or SpaceMIT CSI capture and detection without WebRTC",
    version
)]
struct Args {
    /// Explicit robotd TOML file; never falls back to another device/backend.
    #[arg(long)]
    config: PathBuf,
    /// Measured source frames (each contains two eyes with --both-eyes).
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=10000))]
    frames: u32,
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(0..=1000))]
    warmup: u32,
    #[arg(long, default_value_t = 5000, value_parser = clap::value_parser!(u32).range(1..=30000))]
    timeout_ms: u32,
    /// Keep the native SBS frame and extract both ROIs with the same source PTS.
    /// Otherwise use mediad's selected-eye, aspect-preserving media output path.
    #[arg(long)]
    both_eyes: bool,
    /// Physically rotate by camera.rotate (V2D for accelerated USB; videoflip for CSI).
    /// Normally only the detector rotates; native SBS extraction stays unrotated.
    #[arg(long, conflicts_with = "both_eyes")]
    flip_in_pipeline: bool,
    /// Run the existing detector on each returned view; no implicit CPU/precision fallback.
    #[arg(long)]
    detect: bool,
    /// Explicit model override for --detect; otherwise requires one configured model.
    #[arg(long, requires = "detect")]
    model: Option<PathBuf>,
    /// Optional native ORT profile prefix, for verifying provider assignment.
    #[arg(long, requires = "detect")]
    profile: Option<PathBuf>,
    /// Pace consumption at this rate (0 = unpaced). The latest-frame sink drops older frames.
    #[arg(long, default_value_t = 0.0)]
    hz: f64,
    /// Create a NEW directory and save the first measured UYVY frame/views plus metadata.
    #[arg(long)]
    dump_dir: Option<PathBuf>,
    /// Write SDK JSONL to a NEW file, separate from vendor camera logs on stdout.
    #[arg(long)]
    report: Option<PathBuf>,
}

#[cfg(target_os = "linux")]
fn run(args: Args) -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use mediad::camera::Capture;
    use mediad::detect::ImageDetector;
    use robotd_params::{CameraBackend, CameraLayout, CameraView, Params};
    use serde_json::json;
    use std::io::Write;
    use std::time::{Duration, Instant};

    ensure!(
        args.hz.is_finite() && (args.hz == 0.0 || (0.1..=120.0).contains(&args.hz)),
        "--hz must be 0 (unpaced), or finite and between 0.1 and 120"
    );
    let params = Params::load(&args.config, true)?;
    let camera = &params.camera;
    ensure!(
        matches!(
            camera.backend,
            CameraBackend::Usb | CameraBackend::SpacemitCsi
        ),
        "camera-check requires camera.backend = 'usb' or 'spacemit_csi'"
    );
    ensure!(
        !args.both_eyes || camera.layout == CameraLayout::StereoSbs,
        "--both-eyes requires camera.layout = 'stereo_sbs'"
    );
    let turn = duck_detect::Turn::from_degrees(if args.flip_in_pipeline {
        0
    } else {
        camera.rotation()
    })
    .context("invalid camera rotation")?;
    let mut engine = if args.detect {
        let mut options = mediad::detect::onnx_options(&params.detect);
        options.profile = args.profile.clone();
        let model = if let Some(model) = &args.model {
            model.clone()
        } else {
            let models = params.detect.models();
            ensure!(
                models.len() == 1,
                "set exactly one enabled detect.model, or use --model; camera-check does not try fallback models"
            );
            models[0].clone()
        };
        Some(ImageDetector::open(&model, &options)?)
    } else {
        None
    };
    // Vendor CSI libraries print directly to C stdout, outside Rust's log bridge.
    // A separate optional sink preserves JSONL without redirecting global FDs.
    let mut report_output: Box<dyn Write> = if let Some(path) = &args.report {
        Box::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .with_context(|| format!("report file must be new: {}", path.display()))?,
        )
    } else {
        Box::new(std::io::stdout())
    };
    if let Some(dir) = &args.dump_dir {
        std::fs::create_dir(dir)
            .with_context(|| format!("dump directory must be new: {}", dir.display()))?;
    }
    let started = Instant::now();
    let rotation = mediad::pipeline::Rotation::from_degrees(if args.flip_in_pipeline {
        camera.rotation()
    } else {
        0
    })?;
    let capture = Capture::start_with_rotation(
        camera,
        (!args.both_eyes).then_some(params.media.quality),
        rotation,
    )?;
    let timeout = Duration::from_millis(u64::from(args.timeout_ms));
    let period = (args.hz > 0.0).then(|| Duration::from_secs_f64(1.0 / args.hz));
    let mut next = Instant::now();
    let mut measured = None;
    let mut infer_ms = Vec::new();
    let mut pts = Vec::new();
    let mut nonincreasing_pts = 0;
    let mut shapes = Vec::new();
    let write_new = |path: PathBuf, bytes: &[u8]| -> anyhow::Result<()> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
            .write_all(bytes)?;
        Ok(())
    };
    for index in 0..args.warmup + args.frames {
        if let Some(period) = period {
            if next > Instant::now() {
                std::thread::sleep(next.saturating_duration_since(Instant::now()));
            }
            // No catch-up burst after a slow decode/inference.
            next = Instant::now() + period;
        }
        if index == args.warmup {
            measured = Some(Instant::now());
        }
        let acquired = capture.next_frame(timeout)?;
        let views = if args.both_eyes {
            Some(acquired.views(camera)?)
        } else {
            None
        };
        let images: Vec<(&str, &mediad::pipeline::Frame)> = if let Some(views) = &views {
            vec![
                ("left", &views.left),
                ("right", views.right.as_ref().context("missing right view")?),
            ]
        } else {
            vec![(
                if camera.layout == CameraLayout::Mono {
                    "mono"
                } else if camera.view == CameraView::Left {
                    "left"
                } else {
                    "right"
                },
                &acquired.image,
            )]
        };
        let warmup = index < args.warmup;
        let mut reports = Vec::new();
        for (eye, frame) in images {
            let sighting = engine
                .as_mut()
                .map(|detector| detector.infer(frame, turn, params.detect.threshold))
                .transpose()?;
            if !warmup {
                if let Some(sighting) = &sighting {
                    infer_ms.push(sighting.took_ms);
                }
                let report = json!({"eye": eye, "width": frame.width, "height": frame.height,
                    "format": frame.format, "bytes": frame.data.len(),
                    "sighting": sighting.as_ref().map(|s| serde_json::from_str::<serde_json::Value>(&mediad::detect::notification(s)).expect("sighting JSON"))});
                reports.push(report);
                if index == args.warmup {
                    shapes.push(json!({"eye":eye, "width":frame.width,"height":frame.height}));
                    if let Some(dir) = &args.dump_dir {
                        write_new(dir.join(format!("{eye}.uyvy")), &frame.data)?;
                    }
                }
            }
        }
        if !warmup {
            if let Some(t) = acquired.pts_ns {
                if pts.last().is_some_and(|previous| t <= *previous) {
                    nonincreasing_pts += 1;
                }
                pts.push(t);
            }
            let report = json!({"event":"frame", "index":index - args.warmup,
                "source_pts_ns": acquired.pts_ns, "buffer_offset":acquired.sequence, "views":reports});
            writeln!(report_output, "{report}")?;
            if index == args.warmup
                && let Some(dir) = &args.dump_dir
            {
                if args.both_eyes {
                    write_new(dir.join("native.uyvy"), &acquired.image.data)?;
                }
                write_new(
                    dir.join("frame.json"),
                    serde_json::to_string_pretty(&json!({
                        "camera":camera, "quality":params.media.quality, "physical_rotation":rotation.degrees(), "frame":report} ))?
                    .as_bytes(),
                )?;
            }
        }
    }
    let seconds = measured
        .context("no measured frames")?
        .elapsed()
        .as_secs_f64();
    let sum: f64 = infer_ms.iter().sum();
    infer_ms.sort_by(f64::total_cmp);
    let mean = (!infer_ms.is_empty()).then(|| sum / infer_ms.len() as f64);
    let p95 = (!infer_ms.is_empty())
        .then(|| infer_ms[((infer_ms.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)]);
    let source_span_fps = if pts.len() >= 2 && pts.last() > pts.first() {
        Some((pts.len() - 1) as f64 * 1e9 / (pts.last().unwrap() - pts.first().unwrap()) as f64)
    } else {
        None
    };
    drop(capture); // Release the camera before reporting success.
    drop(engine); // Flush an explicitly requested ORT profile.
    writeln!(
        report_output,
        "{}",
        json!({"event":"summary", "camera":camera, "both_eyes":args.both_eyes,
        "physical_rotation":rotation.degrees(),
        "views":shapes, "frames":args.frames, "warmup":args.warmup, "consumer_hz":args.hz,
        "measured_seconds":seconds, "consumed_fps":f64::from(args.frames)/seconds,
        "source_pts_span_fps":source_span_fps, "nonincreasing_pts":nonincreasing_pts,
        "inferences":infer_ms.len(), "inference_mean_ms":mean, "inference_p95_ms":p95,
        "total_capture_seconds":started.elapsed().as_secs_f64(),
        "note":"latest-frame consumption, not a transport loss test; SBS is not depth or hardware-sync validation"})
    )?;
    report_output.flush()?;
    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();
    #[cfg(target_os = "linux")]
    {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .init();
        match run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("camera-check: {error:#}");
                ExitCode::FAILURE
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        eprintln!("camera-check requires Linux and GStreamer");
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_and_explicit_cli() {
        assert!(
            Args::try_parse_from([
                "camera-check",
                "--config",
                "usb.toml",
                "--both-eyes",
                "--flip-in-pipeline"
            ])
            .is_err()
        );
        assert!(
            Args::try_parse_from(["camera-check", "--config", "usb.toml", "--flip-in-pipeline"])
                .unwrap()
                .flip_in_pipeline
        );
        assert!(Args::try_parse_from(["camera-check"]).is_err());
        assert!(
            Args::try_parse_from(["camera-check", "--config", "usb.toml", "--frames", "0"])
                .is_err()
        );
        assert!(
            Args::try_parse_from([
                "camera-check",
                "--config",
                "usb.toml",
                "--timeout-ms",
                "30001"
            ])
            .is_err()
        );
        let args = Args::try_parse_from([
            "camera-check",
            "--config",
            "usb.toml",
            "--both-eyes",
            "--frames",
            "2",
        ])
        .unwrap();
        assert!(args.both_eyes);
        assert!(!args.detect);
        assert_eq!(args.frames, 2);
    }
}
