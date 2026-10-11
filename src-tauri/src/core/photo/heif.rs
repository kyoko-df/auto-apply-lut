//! Read primary-item color properties before FFmpeg can supply defaults.
//! Parsing is bounded to the HEIF meta box; compressed image payloads are skipped.
use super::metadata::{MAX_ICC_BYTES, MAX_METADATA_BYTES};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Color {
    Profile(Vec<u8>),
    Invalid,
    Nclx { primaries: u16, transfer: u16 },
    Unknown,
}

fn boxes(data: &[u8]) -> Result<Vec<(&[u8], &[u8])>, String> {
    let mut result = Vec::new();
    let mut offset = 0usize;
    while offset < data.len() {
        if result.len() >= 65536 || data.len() - offset < 8 {
            return Err("HEIF 元数据结构无效".into());
        }
        let mut header = 8;
        let mut length = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as u64;
        if length == 1 {
            if data.len() - offset < 16 {
                return Err("HEIF 扩展长度无效".into());
            }
            length = u64::from_be_bytes(data[offset + 8..offset + 16].try_into().unwrap());
            header = 16;
        } else if length == 0 {
            length = (data.len() - offset) as u64;
        }
        let length = usize::try_from(length).map_err(|_| "HEIF 长度溢出")?;
        if length < header || length > data.len() - offset {
            return Err("HEIF 元数据长度无效".into());
        }
        result.push((
            &data[offset + 4..offset + 8],
            &data[offset + header..offset + length],
        ));
        offset += length;
    }
    Ok(result)
}
fn number(data: &[u8], cursor: &mut usize, bytes: usize) -> Result<u32, String> {
    let end = cursor
        .checked_add(bytes)
        .filter(|v| *v <= data.len())
        .ok_or("HEIF 属性关联无效")?;
    let value = data[*cursor..end]
        .iter()
        .fold(0, |n, b| (n << 8) | u32::from(*b));
    *cursor = end;
    Ok(value)
}
fn parse(meta: &[u8]) -> Result<Color, String> {
    if meta.len() < 4 {
        return Err("HEIF meta 无效".into());
    }
    let children = boxes(&meta[4..])?;
    let pitm = children
        .iter()
        .find(|(kind, _)| *kind == b"pitm")
        .ok_or("HEIF 缺少主图像标识")?
        .1;
    if pitm.len() < 4 || pitm[0] > 1 {
        return Err("HEIF 主图像标识无效".into());
    }
    let mut cursor = 4;
    let primary = number(pitm, &mut cursor, if pitm[0] == 0 { 2 } else { 4 })?;
    // Sony grid items inherit color from their dimg tiles. Follow only the
    // primary image's derivation graph; unrelated thmb/cdsc items are excluded.
    let mut ids = std::collections::HashSet::from([primary]);
    if let Some(iref) = children
        .iter()
        .find(|(kind, _)| *kind == b"iref")
        .map(|v| v.1)
    {
        if iref.len() < 4 || iref[0] > 1 {
            return Err("HEIF 图像引用版本无效".into());
        }
        for (kind, value) in boxes(&iref[4..])? {
            if kind != b"auxl" {
                continue;
            }
            let bytes = if iref[0] == 0 { 2 } else { 4 };
            let mut cursor = 0;
            number(value, &mut cursor, bytes)?;
            let count = number(value, &mut cursor, 2)?;
            for _ in 0..count {
                if number(value, &mut cursor, bytes)? == primary {
                    return Err(
                        "首版 HEIF 不支持透明或辅助主图像；请先转换为不透明 SDR RGB 照片".into(),
                    );
                }
            }
        }
    }

    if let Some(iref) = children
        .iter()
        .find(|(kind, _)| *kind == b"iref")
        .map(|v| v.1)
    {
        if iref.len() < 4 || iref[0] > 1 {
            return Err("HEIF 图像引用版本无效".into());
        }
        let mut graph = std::collections::HashMap::<u32, Vec<u32>>::new();
        for (kind, value) in boxes(&iref[4..])? {
            if kind != b"dimg" {
                continue;
            }
            let bytes = if iref[0] == 0 { 2 } else { 4 };
            let mut cursor = 0;
            let from = number(value, &mut cursor, bytes)?;
            let count = number(value, &mut cursor, 2)?;
            for _ in 0..count {
                graph
                    .entry(from)
                    .or_default()
                    .push(number(value, &mut cursor, bytes)?);
            }
        }
        let mut queue = vec![primary];
        while let Some(id) = queue.pop() {
            for to in graph.get(&id).into_iter().flatten() {
                if ids.insert(*to) {
                    queue.push(*to);
                }
                if ids.len() > 513 {
                    return Err("HEIF 主图像引用过多".into());
                }
            }
        }
    }
    let Some(iprp) = children
        .iter()
        .find(|(kind, _)| *kind == b"iprp")
        .map(|v| v.1)
    else {
        return Ok(Color::Unknown);
    };
    let properties = boxes(iprp)?;
    let Some(ipco) = properties
        .iter()
        .find(|(kind, _)| *kind == b"ipco")
        .map(|v| v.1)
    else {
        return Ok(Color::Unknown);
    };
    let properties_table = boxes(ipco)?;
    let mut color = Color::Unknown;
    let mut seen = std::collections::HashSet::new();
    for (_, ipma) in properties.iter().filter(|(kind, _)| *kind == b"ipma") {
        if ipma.len() < 8 || ipma[0] > 1 {
            return Err("HEIF 属性关联版本无效".into());
        }
        let wide = ipma[3] & 1 != 0;
        let mut cursor = 4;
        let entries = number(ipma, &mut cursor, 4)?;
        if entries > 65536 {
            return Err("HEIF 属性关联过多".into());
        }
        for _ in 0..entries {
            let id = number(ipma, &mut cursor, if ipma[0] == 0 { 2 } else { 4 })?;
            let count = number(ipma, &mut cursor, 1)?;
            for _ in 0..count {
                let index = number(ipma, &mut cursor, if wide { 2 } else { 1 })?
                    & if wide { 0x7fff } else { 0x7f };
                if !ids.contains(&id) || index == 0 {
                    continue;
                }
                let (kind, value) = properties_table
                    .get(index as usize - 1)
                    .ok_or("HEIF 主图像属性索引无效")?;
                if *kind != b"colr" || !seen.insert(index) {
                    continue;
                }
                if value.len() < 4 {
                    return Err("HEIF 色彩属性无效".into());
                }
                match &value[..4] {
                    b"nclx" => {
                        if value.len() != 11 {
                            return Err("HEIF nclx 声明无效".into());
                        }
                        let primaries = u16::from_be_bytes(value[4..6].try_into().unwrap());
                        let transfer = u16::from_be_bytes(value[6..8].try_into().unwrap());
                        // HDR must win even if a second ICC property claims SDR.
                        if matches!(transfer, 16 | 18) {
                            return Ok(Color::Nclx {
                                primaries,
                                transfer,
                            });
                        }
                        if !matches!(color, Color::Profile(_) | Color::Invalid) {
                            let next = Color::Nclx {
                                primaries,
                                transfer,
                            };
                            if matches!(color, Color::Nclx { .. }) && color != next {
                                return Err("HEIF 主图像网格瓦片的色彩声明不一致".into());
                            }
                            color = next;
                        }
                    }
                    b"prof" | b"rICC" => {
                        if value.len() - 4 > MAX_ICC_BYTES {
                            return Err("ICC 配置超过 4 MiB".into());
                        }
                        color = if lcms2::Profile::new_icc_context(
                            lcms2::ThreadContext::new(),
                            &value[4..],
                        )
                        .is_ok_and(|p| p.color_space() == lcms2::ColorSpaceSignature::RgbData)
                        {
                            let profile = lcms2::Profile::new_icc_context(
                                lcms2::ThreadContext::new(),
                                &value[4..],
                            )
                            .map_err(|e| e.to_string())?;
                            if matches!(profile.read_tag(lcms2::TagSignature::CicpTag), lcms2::Tag::VideoSignal(signal) if matches!(signal.TransferCharacteristics, 16 | 18))
                            {
                                return Err(
                                    "HLG/PQ HDR ICC 照片尚不支持；请先明确转换为 SDR sRGB".into()
                                );
                            }
                            if let Color::Profile(previous) = &color {
                                if previous.as_slice() != &value[4..] {
                                    return Err("HEIF 主图像瓦片 ICC 不一致".into());
                                }
                            }
                            if matches!(color, Color::Invalid) {
                                Color::Invalid
                            } else {
                                Color::Profile(value[4..].to_vec())
                            }
                        } else {
                            Color::Invalid
                        };
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(color)
}
fn read_meta(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let total = file.metadata().map_err(|e| e.to_string())?.len();
    let mut offset = 0u64;
    while offset < total {
        if total - offset < 8 {
            return Err("HEIF 文件结构无效".into());
        }
        let mut header = [0u8; 8];
        file.read_exact(&mut header).map_err(|e| e.to_string())?;
        let mut head = 8;
        let mut length = u32::from_be_bytes(header[..4].try_into().unwrap()) as u64;
        if length == 1 {
            let mut extended = [0u8; 8];
            file.read_exact(&mut extended).map_err(|e| e.to_string())?;
            length = u64::from_be_bytes(extended);
            head = 16;
        } else if length == 0 {
            length = total - offset;
        }
        if length < head || length > total - offset {
            return Err("HEIF 文件长度无效".into());
        }
        if &header[4..] == b"meta" {
            if length - head > MAX_METADATA_BYTES as u64 {
                return Err("HEIF meta 超过 16 MiB".into());
            }
            let mut data = vec![0; (length - head) as usize];
            file.read_exact(&mut data).map_err(|e| e.to_string())?;
            return Ok(data);
        }
        offset += length;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
    }
    Err("HEIF 缺少主图像元数据".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn boxed(kind: &[u8], value: &[u8]) -> Vec<u8> {
        [
            ((value.len() + 8) as u32).to_be_bytes().as_slice(),
            kind,
            value,
        ]
        .concat()
    }
    fn meta(transfer: u16, primary_association: u8) -> Vec<u8> {
        let srgb = boxed(
            b"colr",
            &[
                b"nclx".as_slice(),
                &[0, 1],
                &13u16.to_be_bytes(),
                &[0, 1, 128],
            ]
            .concat(),
        );
        let hdr = boxed(
            b"colr",
            &[
                b"nclx".as_slice(),
                &[0, 9],
                &transfer.to_be_bytes(),
                &[0, 9, 128],
            ]
            .concat(),
        );
        let ipco = boxed(b"ipco", &[srgb, hdr].concat());
        // Item 1 is a thumbnail; item 2 is the primary.
        let ipma = boxed(
            b"ipma",
            &[
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                2,
                0,
                1,
                1,
                1,
                0,
                2,
                1,
                primary_association,
            ],
        );
        [
            vec![0; 4],
            boxed(b"pitm", &[0, 0, 0, 0, 0, 2]),
            boxed(b"iprp", &[ipco, ipma].concat()),
        ]
        .concat()
    }
    #[test]
    fn primary_hdr_cannot_be_hidden_by_sdr_thumbnail_or_missing_color() {
        assert_eq!(
            parse(&meta(18, 2)).unwrap(),
            Color::Nclx {
                primaries: 9,
                transfer: 18
            }
        );
        assert_eq!(
            parse(&meta(16, 2)).unwrap(),
            Color::Nclx {
                primaries: 9,
                transfer: 16
            }
        );
        assert_eq!(parse(&meta(18, 0)).unwrap(), Color::Unknown);
        assert_eq!(
            parse(&meta(18, 1)).unwrap(),
            Color::Nclx {
                primaries: 1,
                transfer: 13
            }
        );
        assert!(parse(&meta(18, 3)).is_err());
        assert!(parse(&[0, 0, 0, 0, 0]).is_err());
    }
    #[test]
    fn sony_grid_inherits_color_only_from_its_full_image_tiles() {
        let mut data = meta(18, 0);
        // Primary item 2 references item 1 as a dimg tile; item 1 carries sRGB.
        let mut refs = vec![0; 4];
        refs.extend(boxed(b"dimg", &[0, 2, 0, 1, 0, 1]));
        data.extend(boxed(b"iref", &refs));
        assert_eq!(
            parse(&data).unwrap(),
            Color::Nclx {
                primaries: 1,
                transfer: 13
            }
        );
        let mut hdr = meta(18, 0);
        // Move the tile association from SDR property 1 to HLG property 2.
        let association = hdr.windows(4).rposition(|v| v == [0, 1, 1, 1]).unwrap();
        hdr[association + 3] = 2;
        hdr.extend(boxed(b"iref", &refs));
        assert_eq!(
            parse(&hdr).unwrap(),
            Color::Nclx {
                primaries: 9,
                transfer: 18
            }
        );
    }
}

pub fn primary_color(path: &Path) -> Result<Color, String> {
    parse(&read_meta(path)?)
}
pub fn primary_id(path: &Path) -> Result<u32, String> {
    let meta = read_meta(path)?;
    let children = boxes(&meta[4..])?;
    let value = children
        .iter()
        .find(|(k, _)| *k == b"pitm")
        .ok_or("HEIF 缺少主图像标识")?
        .1;
    if value.len() < 4 || value[0] > 1 {
        return Err("HEIF 主图像标识无效".into());
    }
    let mut c = 4;
    number(value, &mut c, if value[0] == 0 { 2 } else { 4 })
}
pub fn capture_metadata(path: &Path, gps: bool) -> Result<little_exif::metadata::Metadata, String> {
    use little_exif::metadata::Metadata;
    let meta = read_meta(path)?;
    let children = boxes(&meta[4..])?;
    let Some(iinf) = children.iter().find(|(k, _)| *k == b"iinf").map(|v| v.1) else {
        return Ok(Metadata::new());
    };
    if iinf.len() < 6 {
        return Err("HEIF iinf 无效".into());
    }
    let start = if iinf[0] == 0 { 6 } else { 8 };
    if iinf.len() < start {
        return Err("HEIF iinf 无效".into());
    }
    let mut exif_ids = Vec::new();
    for (kind, value) in boxes(&iinf[start..])? {
        if kind != b"infe" || value.len() < 12 || ![2, 3].contains(&value[0]) {
            continue;
        }
        let mut cursor = 4;
        let id = number(value, &mut cursor, if value[0] == 2 { 2 } else { 4 })?;
        number(value, &mut cursor, 2)?;
        if value.get(cursor..cursor + 4) == Some(b"Exif") {
            exif_ids.push(id);
        }
    }
    if exif_ids.is_empty() {
        return Ok(Metadata::new());
    }
    // Require metadata to describe the primary image, excluding thumbnail EXIF.
    let primary = primary_id(path)?;
    let mut selected = None;
    if let Some(iref) = children.iter().find(|(k, _)| *k == b"iref").map(|v| v.1) {
        if iref.len() < 4 || iref[0] > 1 {
            return Err("HEIF 元数据引用无效".into());
        }
        for (kind, value) in boxes(&iref[4..])? {
            if kind != b"cdsc" {
                continue;
            }
            let bytes = if iref[0] == 0 { 2 } else { 4 };
            let mut cursor = 0;
            let from = number(value, &mut cursor, bytes)?;
            let count = number(value, &mut cursor, 2)?;
            for _ in 0..count {
                if number(value, &mut cursor, bytes)? == primary && exif_ids.contains(&from) {
                    selected = Some(from);
                }
            }
        }
    }
    let Some(selected) = selected else {
        return Ok(Metadata::new());
    };
    let value = children
        .iter()
        .find(|(k, _)| *k == b"iloc")
        .ok_or("HEIF EXIF 缺少位置")?
        .1;
    if value.len() < 8 || value[0] > 2 {
        return Err("HEIF iloc 无效".into());
    }
    let offset_size = (value[4] >> 4) as usize;
    let length_size = (value[4] & 15) as usize;
    let base_size = (value[5] >> 4) as usize;
    let index_size = if value[0] > 0 {
        (value[5] & 15) as usize
    } else {
        0
    };
    let mut cursor = 6;
    let count = number(value, &mut cursor, if value[0] < 2 { 2 } else { 4 })?;
    if count > 65536 {
        return Err("HEIF iloc 项目过多".into());
    }
    fn wide(data: &[u8], cursor: &mut usize, n: usize) -> Result<u64, String> {
        if n > 8 || cursor.checked_add(n).is_none_or(|end| end > data.len()) {
            return Err("HEIF EXIF 位置无效".into());
        }
        let result = data[*cursor..*cursor + n]
            .iter()
            .fold(0u64, |a, b| (a << 8) | u64::from(*b));
        *cursor += n;
        Ok(result)
    }
    for _ in 0..count {
        let id = number(value, &mut cursor, if value[0] < 2 { 2 } else { 4 })?;
        let method = if value[0] > 0 {
            number(value, &mut cursor, 2)? & 15
        } else {
            0
        };
        let reference = number(value, &mut cursor, 2)?;
        let base = wide(value, &mut cursor, base_size)?;
        let extents = number(value, &mut cursor, 2)?;
        if extents > 256 {
            return Err("HEIF EXIF 区块过多".into());
        }
        let mut result = Vec::new();
        for _ in 0..extents {
            wide(value, &mut cursor, index_size)?;
            let offset = wide(value, &mut cursor, offset_size)?;
            let length = wide(value, &mut cursor, length_size)?;
            if id != selected {
                continue;
            }
            if reference != 0
                || method > 1
                || length == 0
                || length > MAX_METADATA_BYTES as u64
                || result.len() as u64 + length > MAX_METADATA_BYTES as u64
            {
                return Err("HEIF EXIF 区块无效/过大".into());
            }
            let offset = base.checked_add(offset).ok_or("HEIF EXIF 偏移溢出")?;
            if method == 1 {
                let idat = children
                    .iter()
                    .find(|(k, _)| *k == b"idat")
                    .ok_or("HEIF EXIF 缺少 idat")?
                    .1;
                let end = offset
                    .checked_add(length)
                    .filter(|v| *v <= idat.len() as u64)
                    .ok_or("HEIF EXIF 越界")?;
                result.extend_from_slice(&idat[offset as usize..end as usize]);
            } else {
                let mut file = File::open(path).map_err(|e| e.to_string())?;
                if offset
                    .checked_add(length)
                    .is_none_or(|end| end > file.metadata().map(|m| m.len()).unwrap_or(0))
                {
                    return Err("HEIF EXIF 越界".into());
                }
                file.seek(SeekFrom::Start(offset))
                    .map_err(|e| e.to_string())?;
                file.take(length)
                    .read_to_end(&mut result)
                    .map_err(|e| e.to_string())?;
            }
        }
        if id == selected {
            if result.len() < 4 {
                return Err("HEIF EXIF 数据无效".into());
            }
            let start = 4usize
                .checked_add(u32::from_be_bytes(result[..4].try_into().unwrap()) as usize)
                .filter(|n| *n < result.len())
                .ok_or("HEIF EXIF TIFF 偏移无效")?;
            let tiff = result[start..].to_vec();
            let source = std::panic::catch_unwind(|| {
                Metadata::new_from_vec(&tiff, little_exif::filetype::FileExtension::TIFF)
            })
            .map_err(|_| "HEIF EXIF 解析失败")?
            .map_err(|e| e.to_string())?;
            return Ok(super::codec::metadata_allowlist(&source, gps));
        }
    }
    Err("HEIF EXIF 项目缺失".into())
}
