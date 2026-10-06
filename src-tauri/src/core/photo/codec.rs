use super::{check_cancel, color, metadata, PhotoFrame, PhotoOutput, SourceInterpretation};
use image::{ImageDecoder, ImageEncoder};
use little_exif::{exif_tag::ExifTag, metadata::Metadata};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use tokio_util::sync::CancellationToken;

pub fn decode(
    path: &Path,
    source: &SourceInterpretation,
    cancel: &CancellationToken,
) -> Result<PhotoFrame, String> {
    check_cancel(cancel)?;
    let header = metadata::header(path)?;
    let icc = match source {
        SourceInterpretation::Assign { space } => metadata::assigned_icc(*space)?,
        SourceInterpretation::Embedded => match header.icc {
            Some(icc) if header.info.color_status == "embedded" => icc,
            Some(_) => return Err("照片 ICC 无效，请明确指定输入色彩空间".into()),
            None if header.declared_srgb => metadata::assigned_icc(super::PhotoSpace::Srgb)?,
            None => return Err("照片未标记色彩空间，请明确指定输入解释".into()),
        },
    };
    let decoder = metadata::reader(path)?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let orientation =
        image::metadata::Orientation::from_exif(header.info.orientation).ok_or("照片方向无效")?;
    check_cancel(cancel)?;
    let mut image =
        image::DynamicImage::from_decoder(decoder).map_err(|e| format!("照片解码失败：{e}"))?;
    check_cancel(cancel)?;
    image.apply_orientation(orientation);
    let rgba = image.into_rgba16();
    let (width, height) = rgba.dimensions();
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    let mut alpha = header
        .info
        .has_alpha
        .then(|| Vec::with_capacity(width as usize * height as usize));
    for (i, p) in rgba.pixels().enumerate() {
        if i % 16384 == 0 {
            check_cancel(cancel)?;
        }
        rgb.extend_from_slice(&p.0[..3]);
        if let Some(alpha) = &mut alpha {
            alpha.push(p.0[3]);
        }
    }
    drop(rgba);
    color::to_srgb(&mut rgb, &icc, header.gray, cancel)?;
    Ok(PhotoFrame {
        width,
        height,
        rgb,
        alpha,
    })
}

pub fn encode(
    frame: &PhotoFrame,
    output: &PhotoOutput,
    file: &mut File,
    cancel: &CancellationToken,
) -> Result<(), String> {
    output.validate(frame.alpha.is_some())?;
    check_cancel(cancel)?;
    let icc = color::profile(super::PhotoSpace::Srgb)?
        .icc()
        .map_err(|e| e.to_string())?;
    let bits = output.bit_depth();
    let mut values = Vec::with_capacity(frame.rgb.len() + frame.alpha.as_ref().map_or(0, Vec::len));
    for (i, p) in frame.rgb.chunks_exact(3).enumerate() {
        if i % 16384 == 0 {
            check_cancel(cancel)?;
        }
        values.extend_from_slice(p);
        if let Some(alpha) = &frame.alpha {
            values.push(alpha[i]);
        }
    }
    if matches!(output, PhotoOutput::Tiff { .. }) {
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut *file)
            .map_err(|e| e.to_string())?
            .with_compression(tiff::encoder::Compression::Lzw);
        if bits == 16 {
            let mut image = encoder
                .new_image::<tiff::encoder::colortype::RGB16>(frame.width, frame.height)
                .map_err(|e| e.to_string())?;
            image
                .encoder()
                .write_tag(tiff::tags::Tag::IccProfile, icc.as_slice())
                .map_err(|e| e.to_string())?;
            image
                .encoder()
                .write_tag(tiff::tags::Tag::Orientation, 1u16)
                .map_err(|e| e.to_string())?;
            image.write_data(&values).map_err(|e| e.to_string())?;
        } else {
            let values: Vec<u8> = values
                .iter()
                .map(|v| ((*v as u32 + 128) / 257) as u8)
                .collect();
            let mut image = encoder
                .new_image::<tiff::encoder::colortype::RGB8>(frame.width, frame.height)
                .map_err(|e| e.to_string())?;
            image
                .encoder()
                .write_tag(tiff::tags::Tag::IccProfile, icc.as_slice())
                .map_err(|e| e.to_string())?;
            image
                .encoder()
                .write_tag(tiff::tags::Tag::Orientation, 1u16)
                .map_err(|e| e.to_string())?;
            image.write_data(&values).map_err(|e| e.to_string())?;
        }
    } else {
        let alpha = frame.alpha.is_some();
        let color = match (bits, alpha) {
            (16, true) => image::ExtendedColorType::Rgba16,
            (16, false) => image::ExtendedColorType::Rgb16,
            (_, true) => image::ExtendedColorType::Rgba8,
            _ => image::ExtendedColorType::Rgb8,
        };
        let data: Vec<u8> = if bits == 16 {
            values.iter().flat_map(|v| v.to_ne_bytes()).collect()
        } else {
            values
                .iter()
                .map(|v| ((*v as u32 + 128) / 257) as u8)
                .collect()
        };
        match output {
            PhotoOutput::Jpeg { quality, .. } => {
                let mut encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut *file, *quality);
                encoder.set_icc_profile(icc).map_err(|e| e.to_string())?;
                encoder
                    .write_image(&data, frame.width, frame.height, color)
                    .map_err(|e| e.to_string())?;
            }
            _ => {
                let mut encoder = image::codecs::png::PngEncoder::new(&mut *file);
                encoder.set_icc_profile(icc).map_err(|e| e.to_string())?;
                encoder
                    .write_image(&data, frame.width, frame.height, color)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    check_cancel(cancel)
}

pub fn capture_metadata(path: &Path, preserve: bool, gps: bool) -> Result<Metadata, String> {
    let mut result = Metadata::new();
    if !preserve {
        return Ok(result);
    }
    let reader = metadata::reader(path)?;
    if reader.format() != Some(image::ImageFormat::Tiff) {
        let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
        if decoder
            .exif_metadata()
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Ok(result);
        }
    }
    let source = std::panic::catch_unwind(|| Metadata::new_from_path(path))
        .map_err(|_| "照片元数据解析失败")?
        .map_err(|e| format!("无法保留照片元数据：{e}"))?;
    for tag in &source {
        let id = tag.as_u16();
        let allowed = matches!(
            id,
            0x010f
                | 0x0110
                | 0x0132
                | 0x013b
                | 0x8298
                | 0x829a
                | 0x829d
                | 0x8827
                | 0x9003
                | 0x9004
                | 0x9010
                | 0x9011
                | 0x9012
                | 0x920a
                | 0xa432
                | 0xa433
                | 0xa434
                | 0x9290
                | 0x9291
                | 0x9292
                | 0x011a
                | 0x011b
                | 0x0128
        );
        if allowed || (gps && tag.get_group() == little_exif::ifd::ExifTagGroup::GPS) {
            result.set_tag(tag.clone());
        }
    }
    Ok(result)
}

pub fn write_metadata(
    path: &Path,
    mut metadata: Metadata,
    width: u32,
    height: u32,
) -> Result<(), String> {
    metadata.set_tag(ExifTag::Orientation(vec![1]));
    metadata.set_tag(ExifTag::ExifImageWidth(vec![width]));
    metadata.set_tag(ExifTag::ExifImageHeight(vec![height]));
    metadata.set_tag(ExifTag::ColorSpace(vec![1]));
    // TIFF EXIF writers replace the whole TIFF. Preserve the newly encoded image
    // container (not source EXIF), including strip payloads and the output ICC.
    let written = if metadata::reader(path)?.format() == Some(image::ImageFormat::Tiff) {
        let mut container = Metadata::new_from_path(path).map_err(|e| e.to_string())?;
        for tag in &metadata {
            container.set_tag(tag.clone());
        }
        container
    } else {
        metadata.clone()
    };
    if metadata::reader(path)?.format() == Some(image::ImageFormat::Png) {
        write_png_exif(path, &written)?;
    } else {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| written.write_to_file(path)))
            .map_err(|_| "照片元数据写入失败")?
            .map_err(|e| e.to_string())?;
        if metadata::reader(path)?.format() == Some(image::ImageFormat::Tiff) {
            let icc = color::profile(super::PhotoSpace::Srgb)?
                .icc()
                .map_err(|e| e.to_string())?;
            write_tiff_icc(path, &icc)?;
        }
    }
    let actual = Metadata::new_from_path(path).map_err(|e| e.to_string())?;
    for tag in &metadata {
        if actual.get_tag(tag).next() != Some(tag) {
            return Err(format!("照片元数据验证失败：{:04x}", tag.as_u16()));
        }
    }
    Ok(())
}

// The EXIF adapter doesn't retain unknown TIFF tags such as ICC. Append a new
// IFD referencing existing strip/EXIF data, rather than rewriting image pixels.
fn write_tiff_icc(path: &Path, icc: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let mut header = [0u8; 8];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    let little = match &header[..4] {
        b"II\x2a\0" => true,
        b"MM\0\x2a" => false,
        _ => return Err("不支持的 TIFF 结构".into()),
    };
    let u16_at = |bytes: &[u8]| {
        if little {
            u16::from_le_bytes(bytes.try_into().unwrap())
        } else {
            u16::from_be_bytes(bytes.try_into().unwrap())
        }
    };
    let u32_at = |bytes: &[u8]| {
        if little {
            u32::from_le_bytes(bytes.try_into().unwrap())
        } else {
            u32::from_be_bytes(bytes.try_into().unwrap())
        }
    };
    let word = |v: u16| {
        if little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    let long = |v: u32| {
        if little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    file.seek(SeekFrom::Start(u32_at(&header[4..8]) as u64))
        .map_err(|e| e.to_string())?;
    let mut count = [0u8; 2];
    file.read_exact(&mut count).map_err(|e| e.to_string())?;
    let count = u16_at(&count) as usize;
    let mut entries = vec![0u8; count.checked_mul(12).ok_or("TIFF 目录长度溢出")?];
    file.read_exact(&mut entries).map_err(|e| e.to_string())?;
    let mut next = [0u8; 4];
    file.read_exact(&mut next).map_err(|e| e.to_string())?;
    let mut entries: Vec<Vec<u8>> = entries
        .chunks_exact(12)
        .filter(|e| u16_at(&e[..2]) != 34675)
        .map(|e| e.to_vec())
        .collect();
    if entries.len() >= u16::MAX as usize {
        return Err("TIFF 目录过大".into());
    }
    let end = file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    if end % 2 != 0 {
        file.write_all(&[0]).map_err(|e| e.to_string())?;
    }
    let profile_offset = u32::try_from(file.stream_position().map_err(|e| e.to_string())?)
        .map_err(|_| "TIFF 超过标准文件长度")?;
    file.write_all(icc).map_err(|e| e.to_string())?;
    if icc.len() % 2 != 0 {
        file.write_all(&[0]).map_err(|e| e.to_string())?;
    }
    let ifd_offset = u32::try_from(file.stream_position().map_err(|e| e.to_string())?)
        .map_err(|_| "TIFF 超过标准文件长度")?;
    let mut entry = Vec::new();
    entry.extend(word(34675));
    entry.extend(word(7));
    entry.extend(long(u32::try_from(icc.len()).map_err(|_| "ICC 过大")?));
    entry.extend(long(profile_offset));
    entries.push(entry);
    entries.sort_by_key(|e| u16_at(&e[..2]));
    file.write_all(&word(entries.len() as u16))
        .map_err(|e| e.to_string())?;
    for entry in entries {
        file.write_all(&entry).map_err(|e| e.to_string())?;
    }
    file.write_all(&next).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(4)).map_err(|e| e.to_string())?;
    file.write_all(&long(ifd_offset))
        .map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    let mut decoder = tiff::decoder::Decoder::new(std::io::BufReader::new(
        File::open(path).map_err(|e| e.to_string())?,
    ))
    .map_err(|e| e.to_string())?;
    let actual = decoder
        .get_tag_u8_vec(tiff::tags::Tag::IccProfile)
        .map_err(|e| format!("TIFF ICC 标签验证失败：{e}"))?;
    if actual != icc {
        return Err("TIFF ICC 配置字节不匹配".into());
    }
    Ok(())
}

// little_exif's PNG writer emits legacy zTXt, which image viewers may ignore.
// Our newly encoded PNG has no old EXIF; insert a standards-based eXIf chunk.
pub(crate) fn write_png_exif(path: &Path, tags: &Metadata) -> Result<(), String> {
    let app1 = tags
        .as_u8_vec(little_exif::filetype::FileExtension::JPEG)
        .map_err(|e| e.to_string())?;
    if app1.len() < 10 || app1.len() > 65537 || &app1[4..10] != b"Exif\0\0" {
        return Err("照片 EXIF 无法安全编码".into());
    }
    let exif = &app1[10..];
    let mut png = std::fs::read(path).map_err(|e| e.to_string())?;
    if png.len() < 33 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return Err("无效 PNG 输出".into());
    }
    let mut chunk = Vec::with_capacity(exif.len() + 12);
    chunk.extend_from_slice(&(exif.len() as u32).to_be_bytes());
    chunk.extend_from_slice(b"eXIf");
    chunk.extend_from_slice(exif);
    let crc = crc32fast::hash(&chunk[4..]);
    chunk.extend_from_slice(&crc.to_be_bytes());
    png.splice(33..33, chunk);
    std::fs::write(path, png).map_err(|e| e.to_string())
}
