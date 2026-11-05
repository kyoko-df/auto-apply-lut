//! Scopes sample the graded RGB16 preview before display encoding.
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
pub struct FrameScopes {
    pub channels: [Vec<u32>; 3],
    pub clipped_channels: [u32; 3],
    pub sampled: u32,
    pub clipped: u32,
    pub mask: Option<String>,
}

impl FrameScopes {
    pub fn bytes(&self) -> usize {
        3 * 256 * 4 + self.mask.as_ref().map_or(0, String::len)
    }
}

pub fn analyze(
    width: u32,
    height: u32,
    rgb: &[u16],
    alpha: Option<&[u16]>,
    token: &CancellationToken,
) -> Result<FrameScopes, String> {
    if width == 0 || height == 0 || width.max(height) > 1920 {
        return Err("Invalid preview dimensions".into());
    }
    let pixels = (width * height) as usize;
    if rgb.len() != pixels * 3 || alpha.is_some_and(|a| a.len() != pixels) {
        return Err("Invalid preview pixels".into());
    }
    let mut scopes = FrameScopes {
        channels: std::array::from_fn(|_| vec![0; 256]),
        clipped_channels: [0; 3],
        sampled: 0,
        clipped: 0,
        mask: None,
    };
    let mut overlay = vec![0u8; pixels * 4];
    for (i, pixel) in rgb.chunks_exact(3).enumerate() {
        if i % 4096 == 0 {
            crate::core::photo::check_cancel(token)?;
        }
        if alpha.is_some_and(|a| a[i] == 0) {
            continue;
        }
        scopes.sampled += 1;
        for (channel, &value) in pixel.iter().enumerate() {
            scopes.channels[channel][usize::from(value >> 8)] += 1;
            if value == u16::MAX {
                scopes.clipped_channels[channel] += 1;
            }
        }
        if pixel.contains(&u16::MAX) {
            scopes.clipped += 1;
            overlay[i * 4..i * 4 + 4].copy_from_slice(&[255, 0, 160, 180]);
        }
    }
    if scopes.clipped > 0 {
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&overlay, width, height, ExtendedColorType::Rgba8)
            .map_err(|e| e.to_string())?;
        scopes.mask = Some(format!("data:image/png;base64,{}", STANDARD.encode(png)));
    }
    crate::core::photo::check_cancel(token)?;
    Ok(scopes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_buffers_and_cancelled_work() {
        let token = CancellationToken::new();
        assert!(analyze(0, 1, &[], None, &token).is_err());
        assert!(analyze(1921, 1, &[], None, &token).is_err());
        assert!(analyze(1, 1, &[0, 0], None, &token).is_err());
        assert!(analyze(1, 1, &[0; 3], Some(&[]), &token).is_err());
        let scopes = analyze(1, 1, &[0; 3], None, &token).unwrap();
        assert_eq!((scopes.sampled, scopes.clipped), (1, 0));
        assert!(scopes.mask.is_none());
        token.cancel();
        assert!(analyze(1, 1, &[0; 3], None, &token).is_err());
    }

    #[test]
    fn separates_last_bin_from_clipping_and_excludes_transparent_pixels() {
        let s = analyze(
            3,
            1,
            &[65534, 0, 256, 0, 65535, 65535, 65535, 0, 0],
            Some(&[65535, 300, 0]),
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!((s.sampled, s.clipped), (2, 1));
        assert_eq!(s.channels[0][255], 1);
        assert_eq!(s.clipped_channels, [0, 1, 1]);
        assert_eq!(s.channels[2][1], 1);
        for bins in s.channels {
            assert_eq!(bins.iter().sum::<u32>(), 2);
        }
        let png = STANDARD
            .decode(s.mask.unwrap().split(',').nth(1).unwrap())
            .unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert!(image.get_pixel(1, 0)[3] > 0);
        assert_eq!(image.get_pixel(2, 0)[3], 0);
    }
}
