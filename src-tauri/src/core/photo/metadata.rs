use super::PhotoSpace;
use image::{ImageDecoder, ImageFormat, ImageReader};
use little_exif::{exif_tag::ExifTag, metadata::Metadata};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
    time::UNIX_EPOCH,
};

pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 64_000_000;
pub const MAX_ICC_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoInfo {
    pub path: String,
    pub filename: String,
    pub size: u64,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub stored_width: u32,
    pub stored_height: u32,
    pub bit_depth: u8,
    pub has_alpha: bool,
    pub orientation: u8,
    pub color_profile: String,
    pub color_status: String,
    pub source_version: String,
}
pub struct PhotoHeader {
    pub info: PhotoInfo,
    pub icc: Option<Vec<u8>>,
    pub declared_srgb: bool,
    pub gray: bool,
}

pub fn version(path: &Path) -> Result<String, String> {
    let m = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() || m.len() > MAX_FILE_BYTES {
        return Err("照片必须是小于 512 MiB 的普通文件".into());
    }
    let modified = m
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return Ok(format!("{}:{}:{}:{modified}", m.dev(), m.ino(), m.len()));
    }
    #[cfg(not(unix))]
    {
        Ok(format!("{}:{modified}", m.len()))
    }
}

pub fn reader(path: &Path) -> Result<ImageReader<BufReader<File>>, String> {
    let mut reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Tiff)
    ) {
        return Err("首版照片模式支持 JPEG、PNG、单页 TIFF".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader)
}

fn png_srgb(path: &Path) -> Result<bool, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(8)).map_err(|e| e.to_string())?;
    let mut metadata_bytes = 0usize;
    let mut srgb = false;
    loop {
        let mut header = [0u8; 8];
        file.read_exact(&mut header).map_err(|e| e.to_string())?;
        let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let name = &header[4..];
        if name == b"IDAT" || name == b"IEND" {
            break;
        }
        if name == b"acTL" {
            return Err("首版照片模式不支持 APNG 动画".into());
        }
        if name == b"cICP" {
            if size != 4 {
                return Err("PNG 色彩声明无效".into());
            }
            let mut color = [0u8; 4];
            file.read_exact(&mut color).map_err(|e| e.to_string())?;
            if matches!(color[1], 16 | 18) {
                return Err("首版照片模式不支持 PQ 或 HLG HDR 照片".into());
            }
            file.seek(SeekFrom::Current(4)).map_err(|e| e.to_string())?;
            continue;
        }
        metadata_bytes = metadata_bytes
            .checked_add(size)
            .ok_or("PNG 元数据长度溢出")?;
        if metadata_bytes > MAX_METADATA_BYTES {
            return Err("照片元数据超过 16 MiB".into());
        }
        srgb |= name == b"sRGB";
        file.seek(SeekFrom::Current(size as i64 + 4))
            .map_err(|e| e.to_string())?;
    }
    Ok(srgb)
}

pub fn header(path: &Path) -> Result<PhotoHeader, String> {
    let source_version = version(path)?;
    let reader = reader(path)?;
    let format = reader.format().ok_or("无法识别照片格式")?;
    let declared_png_srgb = if format == ImageFormat::Png {
        png_srgb(path)?
    } else {
        false
    };
    let mut tiff_icc = None;
    if format == ImageFormat::Tiff {
        let mut magic = [0u8; 4];
        File::open(path)
            .map_err(|e| e.to_string())?
            .read_exact(&mut magic)
            .map_err(|e| e.to_string())?;
        if matches!(&magic, b"II\x2b\0" | b"MM\0\x2b") {
            return Err("首版照片模式不支持 BigTIFF".into());
        }
        let mut raw = tiff::decoder::Decoder::new(BufReader::new(
            File::open(path).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?;
        if raw.more_images() {
            return Err("首版不支持多页 TIFF，请先导出单张照片".into());
        }
        if !matches!(
            raw.colortype().map_err(|e| e.to_string())?,
            tiff::ColorType::RGB(8 | 16) | tiff::ColorType::Gray(8 | 16)
        ) {
            return Err("首版 TIFF 支持 8/16 位 RGB 或灰度，不支持透明、CMYK 或浮点数据".into());
        }
        // image ties TIFF tag allocation to pixel-buffer bytes, which can hide
        // an ICC larger than a tiny image. Read tags with independent limits.
        let mut limits = tiff::decoder::Limits::default();
        limits.ifd_value_size = MAX_METADATA_BYTES;
        limits.decoding_buffer_size = MAX_METADATA_BYTES;
        raw = raw.with_limits(limits);
        tiff_icc = raw
            .find_tag(tiff::tags::Tag::IccProfile)
            .map_err(|e| e.to_string())?
            .map(|v| v.into_u8_vec().map_err(|e| e.to_string()))
            .transpose()?;
    }
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (stored_width, stored_height) = decoder.dimensions();
    let pixels = (stored_width as u64)
        .checked_mul(stored_height as u64)
        .ok_or("照片尺寸溢出")?;
    if pixels == 0 || pixels > MAX_PIXELS {
        return Err("照片像素数量必须在 1 到 6400 万之间".into());
    }
    let original = decoder.original_color_type();
    if matches!(original, image::ExtendedColorType::Cmyk8) {
        return Err("首版不支持 CMYK 照片".into());
    }
    let color = decoder.color_type();
    if !matches!(
        color,
        image::ColorType::L8
            | image::ColorType::La8
            | image::ColorType::Rgb8
            | image::ColorType::Rgba8
            | image::ColorType::L16
            | image::ColorType::La16
            | image::ColorType::Rgb16
            | image::ColorType::Rgba16
    ) {
        return Err("照片必须是 8 位或 16 位整数图像".into());
    }
    let icc = if format == ImageFormat::Tiff {
        tiff_icc
    } else {
        decoder
            .icc_profile()
            .map_err(|e| format!("照片 ICC 无法读取：{e}"))?
    };
    if icc.as_ref().is_some_and(|v| v.len() > MAX_ICC_BYTES) {
        return Err("ICC 配置超过 4 MiB".into());
    }
    let exif = decoder.exif_metadata().map_err(|e| e.to_string())?;
    if exif.as_ref().is_some_and(|v| v.len() > MAX_METADATA_BYTES) {
        return Err("照片 EXIF 超过 16 MiB".into());
    }
    let orientation = exif
        .as_ref()
        .and_then(|v| image::metadata::Orientation::from_exif_chunk(v))
        .unwrap_or(
            decoder
                .orientation()
                .map_err(|e| format!("照片方向无法读取：{e}"))?,
        )
        .to_exif();
    let declared_exif_srgb = exif.and_then(|v| Metadata::new_from_vec(&v, little_exif::filetype::FileExtension::TIFF).ok()).is_some_and(|m| matches!(m.get_tag_by_hex(0xa001, None).next(), Some(ExifTag::ColorSpace(v)) if v.first() == Some(&1)));
    let gray = matches!(
        color,
        image::ColorType::L8
            | image::ColorType::La8
            | image::ColorType::L16
            | image::ColorType::La16
    );
    let (color_profile, color_status) = match &icc {
        Some(data) => match lcms2::Profile::new_icc_context(lcms2::ThreadContext::new(), data) {
            Ok(profile)
                if profile.color_space() == lcms2::ColorSpaceSignature::RgbData
                    || (gray && profile.color_space() == lcms2::ColorSpaceSignature::GrayData) =>
            {
                (
                    profile
                        .info(lcms2::InfoType::Description, lcms2::Locale::new("en_US"))
                        .unwrap_or_else(|| "嵌入 ICC".into())
                        .chars()
                        .take(256)
                        .collect(),
                    "embedded",
                )
            }
            _ => ("无效或通道不匹配的 ICC".into(), "invalid"),
        },
        None if declared_png_srgb || declared_exif_srgb => ("sRGB".into(), "srgb"),
        None => ("未标记色彩空间".into(), "unknown"),
    };
    let (width, height) = if orientation >= 5 {
        (stored_height, stored_width)
    } else {
        (stored_width, stored_height)
    };
    Ok(PhotoHeader {
        info: PhotoInfo {
            path: path.to_string_lossy().into(),
            filename: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            size: std::fs::metadata(path).map_err(|e| e.to_string())?.len(),
            format: format.extensions_str()[0].into(),
            width,
            height,
            stored_width,
            stored_height,
            bit_depth: if matches!(
                color,
                image::ColorType::L16
                    | image::ColorType::La16
                    | image::ColorType::Rgb16
                    | image::ColorType::Rgba16
            ) {
                16
            } else {
                8
            },
            has_alpha: color.has_alpha(),
            orientation,
            color_profile,
            color_status: color_status.into(),
            source_version,
        },
        icc,
        declared_srgb: declared_png_srgb || declared_exif_srgb,
        gray,
    })
}

pub fn assigned_icc(space: PhotoSpace) -> Result<Vec<u8>, String> {
    super::color::profile(space)?
        .icc()
        .map_err(|e| e.to_string())
}
