//! Camera input helpers. USB SBS pairs are crops of one frame, not depth estimates.

use anyhow::{Result, ensure};
use robotd_params::CameraRect;

#[cfg(target_os = "linux")]
pub mod usb;

/// Copy a UYVY rectangle without resampling or interpreting its colours.
/// The input stride may include padding; the result is tightly packed. Chroma pairs
/// must stay intact, so horizontal coordinates and widths are even.
pub fn copy_uyvy_rect(
    bytes: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    rect: CameraRect,
) -> Result<Vec<u8>> {
    ensure!(
        width > 0 && height > 0 && width.is_multiple_of(2),
        "invalid UYVY dimensions"
    );
    let row = (width as usize)
        .checked_mul(2)
        .ok_or_else(|| anyhow::anyhow!("row overflow"))?;
    ensure!(stride >= row, "UYVY stride is shorter than a row");
    let needed = (height as usize - 1)
        .checked_mul(stride)
        .and_then(|n| n.checked_add(row))
        .ok_or_else(|| anyhow::anyhow!("frame size overflow"))?;
    ensure!(
        bytes.len() >= needed,
        "short UYVY buffer: {} < {needed}",
        bytes.len()
    );
    ensure!(
        rect.width > 0
            && rect.height > 0
            && rect.x.is_multiple_of(2)
            && rect.width.is_multiple_of(2)
            && rect.x.checked_add(rect.width).is_some_and(|n| n <= width)
            && rect.y.checked_add(rect.height).is_some_and(|n| n <= height),
        "invalid UYVY crop"
    );
    let out_row = rect.width as usize * 2;
    let capacity = out_row
        .checked_mul(rect.height as usize)
        .ok_or_else(|| anyhow::anyhow!("crop size overflow"))?;
    let mut out = Vec::with_capacity(capacity);
    for y in rect.y..rect.y + rect.height {
        let start = y as usize * stride + rect.x as usize * 2;
        out.extend_from_slice(&bytes[start..start + out_row]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_row_padding_and_preserves_pixel_bytes_exactly() {
        let data: Vec<u8> = (0..24).collect();
        let full = CameraRect {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        };
        assert_eq!(
            copy_uyvy_rect(&data, 4, 2, 12, full).unwrap(),
            [0, 1, 2, 3, 4, 5, 6, 7, 12, 13, 14, 15, 16, 17, 18, 19]
        );
        let right = CameraRect {
            x: 2,
            width: 2,
            ..full
        };
        assert_eq!(
            copy_uyvy_rect(&data, 4, 2, 12, right).unwrap(),
            [4, 5, 6, 7, 16, 17, 18, 19]
        );
    }

    #[test]
    fn accepts_missing_final_padding_but_rejects_short_pixels_and_bad_crops() {
        let r = CameraRect {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        };
        assert!(copy_uyvy_rect(&[0; 20], 4, 2, 12, r).is_ok());
        assert!(copy_uyvy_rect(&[0; 19], 4, 2, 12, r).is_err());
        assert!(copy_uyvy_rect(&[0; 24], 4, 2, 7, r).is_err());
        assert!(
            copy_uyvy_rect(
                &[0; 24],
                4,
                2,
                12,
                CameraRect {
                    x: 1,
                    width: 2,
                    ..r
                }
            )
            .is_err()
        );
        assert!(copy_uyvy_rect(&[0; 24], 4, 2, 12, CameraRect { y: u32::MAX, ..r }).is_err());
    }
}
