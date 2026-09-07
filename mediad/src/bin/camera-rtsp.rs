//! Opt-in RTSP preview. No robot bus, detector, audio, or WebRTC service is started.

use clap::Parser;
use std::{net::IpAddr, path::PathBuf, process::ExitCode};

#[derive(Debug, Parser)]
#[command(
    about = "Preview K1 CSI video over RTSP/TCP (no detection or robot control)",
    version
)]
struct Args {
    /// Explicit robotd TOML; the camera and ISP profile must agree.
    #[arg(long)]
    config: PathBuf,
    /// Loopback by default. Non-loopback exposes unauthenticated camera video.
    #[arg(long, default_value = "127.0.0.1")]
    listen: IpAddr,
    #[arg(long, default_value_t = 8554, value_parser = clap::value_parser!(u16).range(1..))]
    port: u16,
    /// Stop after this many seconds; 0 runs until Ctrl-C. Camera opens on demand.
    #[arg(long, default_value_t = 600, value_parser = clap::value_parser!(u32).range(0..=86400))]
    duration: u32,
}

#[cfg(target_os = "linux")]
fn run(args: Args) -> anyhow::Result<()> {
    let params = robotd_params::Params::load(&args.config, true)?;
    mediad::rtsp::serve(&params, args.listen, args.port, args.duration)
}

#[cfg(not(target_os = "linux"))]
fn run(_args: Args) -> anyhow::Result<()> {
    anyhow::bail!("camera-rtsp requires Linux and the K1 vendor camera/codec plugins")
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("camera-rtsp: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_local_and_bounded() {
        let args = Args::try_parse_from(["camera-rtsp", "--config", "camera.toml"]).unwrap();
        assert!(args.listen.is_loopback());
        assert_eq!(args.port, 8554);
        assert_eq!(args.duration, 600);
        assert!(Args::try_parse_from(["camera-rtsp"]).is_err());
        for (key, value) in [
            ("--port", "0"),
            ("--duration", "86401"),
            ("--listen", "bad"),
        ] {
            assert!(
                Args::try_parse_from(["camera-rtsp", "--config", "camera.toml", key, value])
                    .is_err()
            );
        }
    }
}
