//! What this daemon streams and what it looks for, out of the config file `robotd` already reads.
//!
//! `[media]` in `/etc/robot/robotd.toml` — camera or test pattern, frame size, rate, bitrate — and
//! `[detect]` beside it, which is this daemon's too because the frames are on this daemon's tee.
//! The schema, the defaults and the validation are `robotd_params`'s, which is the point: the crate
//! read here is the one `robotctl configure` writes through, so the editor cannot offer a value
//! this daemon would not understand.
//!
//! Its own module rather than four lines in `main`, for one reason: `main` is Linux-only, so
//! anything living there is not compiled — let alone tested — on the machine it is written on.

use std::path::{Path, PathBuf};

use robotd_params::Params;

/// The file, when `--config` said nothing.
pub fn default_path() -> PathBuf {
    PathBuf::from(robotd_params::DEFAULT_PATH)
}

/// Read the file, refusing invalid configuration before any camera is opened.
/// A broken USB configuration must not fall back to Rockchip's /dev/video0 and sensor writes.
///
/// A *missing* file is not even a warning at the default path: an unprovisioned board has none and
/// streams its camera at 720p30 like every other. A path named on the command line must exist,
/// which is `Params::load`'s own rule and the reason `explicit` is passed through.
pub fn load(path: &Path, explicit: bool) -> Result<Params, robotd_params::ParamsError> {
    Params::load(path, explicit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use robotd_params::MediaParams;

    fn write(dir: &Path, text: &str) -> PathBuf {
        let path = dir.join("robotd.toml");
        std::fs::write(&path, text).expect("writes");
        path
    }

    /// The section is read, and one key set does not disturb the rest.
    #[test]
    fn a_quality_in_the_file_is_what_gets_streamed() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "[media]\nquality = \"360p30\"\n");
        let media = load(&path, true).unwrap().media;
        assert_eq!(media.quality.size(), (640, 360));
        assert_eq!(media.quality.fps(), 30);
        assert_eq!(media.bitrate_resolved(), media.quality.default_bitrate());
        assert!(media.camera, "untouched keys keep their defaults");
    }

    /// A robot with no file at the default path streams its camera, and says nothing about it.
    #[test]
    fn a_missing_file_streams_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let media = load(&dir.path().join("absent.toml"), false).unwrap().media;
        assert_eq!(media.quality, MediaParams::default().quality);
        assert!(media.camera);
    }

    /// No implicit backend/device fallback after a typo or a missing explicit file.
    #[test]
    fn a_broken_file_or_missing_explicit_file_cannot_select_another_camera() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "[media\nquality = ");
        assert!(load(&path, true).is_err());
        assert!(load(&dir.path().join("absent.toml"), true).is_err());
        let path = write(dir.path(), "[camera]\nbackend = 'usb'\n");
        assert!(load(&path, true).is_err());
    }

    /// A `[media]` section from a build that had a key this one does not is ignored key by key,
    /// not section by section — the same rule that stopped a `[chorale]` from a branch keeping a
    /// robot down. What this build *does* understand still applies.
    #[test]
    fn a_key_from_another_build_does_not_cost_the_ones_this_build_has() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "[media]\nquality = \"720p15\"\nchroma_subsampling = \"4:4:4\"\n",
        );
        let media = load(&path, true).unwrap().media;
        assert_eq!(media.quality.fps(), 15);
    }
}
