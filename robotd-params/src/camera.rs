//! Camera input settings, separate from the size of the outgoing media stream.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraBackend {
    #[default]
    Rockchip,
    Usb,
    /// K1's installed spacemitsrc plugin and an explicit vendor ISP JSON profile.
    SpacemitCsi,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraAcceleration {
    #[default]
    Software,
    /// Explicit K1 MPP/OpenCV bridge; missing hardware/library is an error, not a fallback.
    Spacemit,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraFormat {
    #[default]
    Mjpeg,
    Yuyv,
    Uyvy,
    Nv12,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraLayout {
    #[default]
    Mono,
    StereoSbs,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraView {
    #[default]
    Left,
    Right,
}

/// Pixel coordinates in the native, unrotated input image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CameraParams {
    pub backend: CameraBackend,
    pub acceleration: CameraAcceleration,
    /// Required for USB. Prefer /dev/v4l/by-id/...-video-index0 over a changing number.
    pub device: String,
    /// Only for spacemit_csi. Never auto-probe sensors or reuse Rockchip controls.
    pub isp_config: Option<std::path::PathBuf>,
    pub input_format: CameraFormat,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Stereo here means two views in ONE USB frame, not two independently clocked devices.
    pub layout: CameraLayout,
    /// The one eye supplied to the existing video/detector consumers.
    pub view: CameraView,
    /// Empty means the full mono image, or the left half of a conventional SBS image.
    /// Otherwise [x, y, width, height]; explicit ROIs exclude vendor data/border columns.
    pub left_roi: Vec<u32>,
    /// Empty means the right half of a conventional SBS image. Unused in mono mode.
    pub right_roi: Vec<u32>,
    /// Unset preserves Radxa's 90-degree mount; USB and K1 CSI default to unrotated.
    pub rotate: Option<u32>,
}

impl Default for CameraParams {
    fn default() -> Self {
        Self {
            backend: CameraBackend::Rockchip,
            acceleration: CameraAcceleration::Software,
            device: String::new(),
            isp_config: None,
            input_format: CameraFormat::Mjpeg,
            width: 1280,
            height: 720,
            fps: 30,
            layout: CameraLayout::Mono,
            view: CameraView::Left,
            left_roi: Vec::new(),
            right_roi: Vec::new(),
            rotate: None,
        }
    }
}

impl CameraParams {
    pub fn rotation(&self) -> u32 {
        self.rotate.unwrap_or(match self.backend {
            CameraBackend::Rockchip => 90,
            CameraBackend::Usb | CameraBackend::SpacemitCsi => 0,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.rotation(), 0 | 90 | 180 | 270) {
            return Err("camera.rotate must be 0, 90, 180 or 270 degrees".into());
        }
        if self.backend == CameraBackend::SpacemitCsi {
            if self.isp_config.as_ref().is_none_or(|p| !p.is_absolute()) {
                return Err("spacemit_csi requires an absolute camera.isp_config JSON path".into());
            }
            if !self.device.is_empty() || self.acceleration != CameraAcceleration::Software {
                return Err("spacemit_csi uses the ISP profile, not camera.device or the USB acceleration bridge".into());
            }
            if self.input_format != CameraFormat::Nv12
                || self.layout != CameraLayout::Mono
                || self.view != CameraView::Left
                || !self.left_roi.is_empty()
                || !self.right_roi.is_empty()
            {
                return Err(
                    "spacemit_csi currently requires NV12 mono, left view and no ROIs".into(),
                );
            }
            if self.width == 0
                || self.height == 0
                || self.width > 4096
                || self.height > 2160
                || !self.width.is_multiple_of(2)
                || !self.height.is_multiple_of(2)
                || !(1..=120).contains(&self.fps)
            {
                return Err(
                    "spacemit_csi requires even dimensions up to 4096x2160 and 1..120 fps".into(),
                );
            }
            return Ok(());
        }
        if self.isp_config.is_some() {
            return Err("camera.isp_config requires backend = 'spacemit_csi'".into());
        }
        if self.backend != CameraBackend::Usb {
            if self.acceleration != CameraAcceleration::Software {
                return Err("camera.acceleration = 'spacemit' requires backend = 'usb'".into());
            }
            return Ok(());
        }
        if self.device.trim().is_empty() || !self.device.starts_with('/') {
            return Err("USB camera.device must be an explicit absolute device path".into());
        }
        if self.width == 0
            || self.height == 0
            || self.width > 8192
            || self.height > 8192
            || u64::from(self.width) * u64::from(self.height) > 16_777_216
            || !self.width.is_multiple_of(2)
        {
            return Err(
                "USB dimensions need a positive even width, at most 8192 per side and 16 MP".into(),
            );
        }
        if self.input_format == CameraFormat::Nv12 && !self.height.is_multiple_of(2) {
            return Err("NV12 input height must be even".into());
        }
        if !(1..=120).contains(&self.fps) {
            return Err("USB camera.fps must be between 1 and 120".into());
        }
        let (left, right) = self.regions()?;
        if self.acceleration == CameraAcceleration::Spacemit {
            if self.input_format != CameraFormat::Mjpeg
                || self.width > 4096
                || self.height > 2160
                || !self.height.is_multiple_of(2)
                || self.device.len() >= 128
            {
                return Err("SpaceMIT camera currently requires MJPEG, even dimensions up to 4096x2160, and a device path shorter than 128 bytes".into());
            }
            for r in std::iter::once(left).chain(right) {
                if !r.y.is_multiple_of(2) || !r.height.is_multiple_of(2) {
                    return Err("SpaceMIT NV12 camera ROI y/height must be even".into());
                }
            }
        }
        Ok(())
    }

    pub fn regions(&self) -> Result<(CameraRect, Option<CameraRect>), String> {
        let full = CameraRect {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        };
        let rect = |values: &[u32], fallback: CameraRect| -> Result<CameraRect, String> {
            let r = match values {
                [] => fallback,
                [x, y, width, height] => CameraRect {
                    x: *x,
                    y: *y,
                    width: *width,
                    height: *height,
                },
                _ => return Err("camera ROIs must be empty or [x, y, width, height]".into()),
            };
            if r.width == 0
                || r.height == 0
                || !r.x.is_multiple_of(2)
                || !r.width.is_multiple_of(2)
                || r.x.checked_add(r.width).is_none_or(|n| n > self.width)
                || r.y.checked_add(r.height).is_none_or(|n| n > self.height)
            {
                return Err(
                    "camera ROI is out of bounds or has an odd x/width (UYVY chroma pairs)".into(),
                );
            }
            Ok(r)
        };
        if self.layout == CameraLayout::Mono {
            if self.view != CameraView::Left || !self.right_roi.is_empty() {
                return Err(
                    "mono camera has no right eye; use view = 'left' and an empty right_roi".into(),
                );
            }
            return Ok((rect(&self.left_roi, full)?, None));
        }
        let left = rect(
            &self.left_roi,
            CameraRect {
                width: self.width / 2,
                ..full
            },
        )?;
        let right = rect(
            &self.right_roi,
            CameraRect {
                x: self.width / 2,
                width: self.width / 2,
                ..full
            },
        )?;
        if left.x < right.x + right.width
            && right.x < left.x + left.width
            && left.y < right.y + right.height
            && right.y < left.y + left.height
        {
            return Err("left and right camera ROIs must not overlap".into());
        }
        Ok((left, Some(right)))
    }

    pub fn selected_region(&self) -> Result<CameraRect, String> {
        let (left, right) = self.regions()?;
        match self.view {
            CameraView::Left => Ok(left),
            CameraView::Right => right.ok_or_else(|| "camera has no right view".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csi_is_explicit_and_cannot_silently_select_usb_or_a_second_eye() {
        let valid = CameraParams {
            backend: CameraBackend::SpacemitCsi,
            isp_config: Some("/etc/microduck/imx219.json".into()),
            input_format: CameraFormat::Nv12,
            ..Default::default()
        };
        valid.validate().unwrap();
        assert_eq!(valid.rotation(), 0);
        let mut invalid = vec![];
        let mut p = valid.clone();
        p.isp_config = None;
        invalid.push(p);
        let mut p = valid.clone();
        p.isp_config = Some("relative.json".into());
        invalid.push(p);
        let mut p = valid.clone();
        p.device = "/dev/video0".into();
        invalid.push(p);
        let mut p = valid.clone();
        p.acceleration = CameraAcceleration::Spacemit;
        invalid.push(p);
        let mut p = valid.clone();
        p.layout = CameraLayout::StereoSbs;
        invalid.push(p);
        let mut p = valid.clone();
        p.view = CameraView::Right;
        invalid.push(p);
        let mut p = valid.clone();
        p.left_roi = vec![0, 0, 640, 480];
        invalid.push(p);
        let mut p = valid.clone();
        p.input_format = CameraFormat::Mjpeg;
        invalid.push(p);
        let mut p = valid.clone();
        p.height = 721;
        invalid.push(p);
        let mut p = valid.clone();
        p.fps = 0;
        invalid.push(p);
        let mut p = valid;
        p.backend = CameraBackend::Rockchip;
        invalid.push(p);
        for p in invalid {
            assert!(p.validate().is_err(), "{p:?}");
        }
    }

    fn usb() -> CameraParams {
        CameraParams {
            backend: CameraBackend::Usb,
            device: "/dev/video-test".into(),
            ..Default::default()
        }
    }

    #[test]
    fn defaults_preserve_radxa_and_usb_has_no_implicit_device_or_rotation() {
        assert_eq!(CameraParams::default().rotation(), 90);
        assert_eq!(
            CameraParams::default().acceleration,
            CameraAcceleration::Software
        );
        assert_eq!(usb().rotation(), 0);
        assert!(
            CameraParams {
                backend: CameraBackend::Usb,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn spacemit_is_explicit_mjpeg_only_and_validates_nv12_chroma_alignment() {
        let mut p = usb();
        p.acceleration = CameraAcceleration::Spacemit;
        p.validate().unwrap();
        p.left_roi = vec![0, 1, 640, 480];
        assert!(p.validate().is_err());
        p.acceleration = CameraAcceleration::Software;
        p.validate().unwrap(); // UYVY software path still permits odd vertical origins.
        p.acceleration = CameraAcceleration::Spacemit;
        p.left_roi.clear();
        p.input_format = CameraFormat::Yuyv;
        assert!(p.validate().is_err());
        p.input_format = CameraFormat::Mjpeg;
        p.backend = CameraBackend::Rockchip;
        assert!(p.validate().is_err());
    }

    #[test]
    fn mono_and_standard_side_by_side_regions() {
        let mut p = usb();
        assert_eq!(p.regions().unwrap().0.width, 1280);
        assert!(p.regions().unwrap().1.is_none());
        p.layout = CameraLayout::StereoSbs;
        p.view = CameraView::Right;
        assert_eq!(
            p.selected_region().unwrap(),
            CameraRect {
                x: 640,
                y: 0,
                width: 640,
                height: 720
            }
        );
    }

    #[test]
    fn explicit_regions_exclude_a_vendor_prefix_without_guessing() {
        let p = CameraParams {
            width: 4000,
            height: 1200,
            layout: CameraLayout::StereoSbs,
            left_roi: vec![160, 0, 1920, 1200],
            right_roi: vec![2080, 0, 1920, 1200],
            ..usb()
        };
        p.validate().unwrap();
        assert_eq!(p.regions().unwrap().0.x, 160);
        assert_eq!(p.regions().unwrap().1.unwrap().x, 2080);
    }

    #[test]
    fn invalid_geometry_is_rejected_before_any_device_is_opened() {
        for roi in [
            vec![1, 0, 10, 10],
            vec![0, 0, 3, 10],
            vec![0, 0, 0, 10],
            vec![0, 0, 1282, 720],
            vec![0, u32::MAX, 2, 2],
            vec![1, 2],
        ] {
            assert!(
                CameraParams {
                    left_roi: roi,
                    ..usb()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            CameraParams {
                layout: CameraLayout::StereoSbs,
                left_roi: vec![0, 0, 1000, 720],
                ..usb()
            }
            .validate()
            .is_err()
        );
        assert!(
            CameraParams {
                width: 8192,
                height: 8192,
                ..usb()
            }
            .validate()
            .is_err()
        );
        assert!(CameraParams { fps: 0, ..usb() }.validate().is_err());
        assert!(
            CameraParams {
                rotate: Some(1),
                ..usb()
            }
            .validate()
            .is_err()
        );
        assert!(
            CameraParams {
                rotate: Some(360),
                ..usb()
            }
            .validate()
            .is_err()
        );
        assert!(
            CameraParams {
                view: CameraView::Right,
                ..usb()
            }
            .validate()
            .is_err()
        );
    }
}
