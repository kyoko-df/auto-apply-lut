//! Portable full-image inputs: explicitly configured FFmpeg and prebuilt LibRaw.
use super::{check_cancel, color, metadata, PhotoFrame, RawWhiteBalance, SourceInterpretation};
use std::{
    ffi::{c_char, c_void, CStr},
    path::Path,
};
use tokio_util::sync::CancellationToken;

pub fn format(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "hif" | "heif" | "heic" => Some("heif"),
        "arw" => Some("arw"),
        _ => None,
    }
}
#[repr(C)]
struct RawInfo {
    width: u32,
    height: u32,
    orientation: u32,
    iso: f32,
    shutter: f32,
    aperture: f32,
    focal: f32,
    date_time: [c_char; 20],
    make: [c_char; 64],
    model: [c_char; 64],
    lens: [c_char; 128],
}
extern "C" {
    fn lutlab_raw_version() -> *const c_char;
    fn lutlab_raw(
        data: *const u8,
        length: usize,
        rgb: *mut u16,
        samples: usize,
        exposure_ev: f32,
        white_balance: i32,
        info: *mut RawInfo,
        check: unsafe extern "C" fn(*mut c_void) -> i32,
        context: *mut c_void,
        error: *mut c_char,
        error_size: usize,
    ) -> i32;
}
unsafe extern "C" fn cancelled(context: *mut c_void) -> i32 {
    unsafe { (&*context.cast::<CancellationToken>()).is_cancelled() as i32 }
}
pub fn raw_version() -> String {
    unsafe {
        CStr::from_ptr(lutlab_raw_version())
            .to_string_lossy()
            .into_owned()
    }
}
fn raw(
    path: &Path,
    pixels: &mut [u16],
    token: &CancellationToken,
    source: &SourceInterpretation,
) -> Result<RawInfo, String> {
    source.validate(path)?;
    let (exposure, balance) = match source {
        SourceInterpretation::Raw {
            exposure_ev,
            white_balance,
        } => (
            *exposure_ev,
            match white_balance {
                RawWhiteBalance::Camera => 0,
                RawWhiteBalance::Daylight => 1,
                RawWhiteBalance::Auto => 2,
            },
        ),
        _ => (0.0, 0),
    };
    check_cancel(token)?;
    let version = metadata::version(path)?;
    // Rust opens paths (including Unicode on Windows); LibRaw receives a bounded buffer.
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(metadata::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > metadata::MAX_FILE_BYTES {
        return Err("照片超过 512 MiB".into());
    }
    check_cancel(token)?;
    let mut info: RawInfo = unsafe { std::mem::zeroed() };
    let mut error = [0 as c_char; 512];
    let ok = unsafe {
        lutlab_raw(
            bytes.as_ptr(),
            bytes.len(),
            if pixels.is_empty() {
                std::ptr::null_mut()
            } else {
                pixels.as_mut_ptr()
            },
            pixels.len(),
            exposure,
            balance,
            &mut info,
            cancelled,
            token as *const _ as *mut c_void,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    check_cancel(token)?;
    if ok == 0 {
        return Err(format!("LibRaw {} 无法完整显影 Sony ARW：{}；请使用支持该相机/压缩格式的 RAW 显影工具导出 16 位 sRGB TIFF",raw_version(),unsafe{CStr::from_ptr(error.as_ptr())}.to_string_lossy()));
    }
    if metadata::version(path)? != version {
        return Err("源照片在显影期间变化".into());
    }
    Ok(info)
}
fn info(
    path: &Path,
    width: u32,
    height: u32,
    orientation: u8,
    bits: u8,
    profile: String,
    status: &str,
) -> Result<metadata::PhotoInfo, String> {
    Ok(metadata::PhotoInfo {
        path: path.to_string_lossy().into(),
        filename: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        size: std::fs::metadata(path).map_err(|e| e.to_string())?.len(),
        format: format(path).unwrap().into(),
        width: if orientation >= 5 { height } else { width },
        height: if orientation >= 5 { width } else { height },
        stored_width: width,
        stored_height: height,
        bit_depth: bits,
        has_alpha: false,
        orientation,
        color_profile: profile,
        color_status: status.into(),
        source_version: metadata::version(path)?,
    })
}
pub(super) fn make_header(
    path: &Path,
    width: u32,
    height: u32,
    orientation: u8,
    bits: u8,
    profile: String,
    status: &str,
    icc: Option<Vec<u8>>,
    srgb: bool,
) -> Result<metadata::PhotoHeader, String> {
    Ok(metadata::PhotoHeader {
        info: info(path, width, height, orientation, bits, profile, status)?,
        icc,
        declared_srgb: srgb,
        gray: false,
    })
}
pub fn header_with_engine(
    path: &Path,
    engine: Option<&Path>,
    token: &CancellationToken,
) -> Result<metadata::PhotoHeader, String> {
    if format(path) == Some("heif") {
        return super::heif_decode::header(path, engine, token);
    }
    let version = metadata::version(path)?;
    let report = raw(path, &mut [], token, &SourceInterpretation::Embedded)?;
    let header = make_header(
        path,
        report.width,
        report.height,
        report.orientation as u8,
        16,
        format!("LibRaw {} sensor RAW → sRGB", raw_version()),
        "srgb",
        None,
        true,
    )?;
    if header.info.source_version != version {
        return Err("源照片在读取期间变化".into());
    }
    Ok(header)
}
pub(super) fn oriented(
    width: u32,
    height: u32,
    rgb: Vec<u16>,
    orientation: u8,
) -> Result<PhotoFrame, String> {
    let buffer = image::ImageBuffer::<image::Rgb<u16>, _>::from_raw(width, height, rgb)
        .ok_or("照片像素长度不正确")?;
    let mut image = image::DynamicImage::ImageRgb16(buffer);
    image.apply_orientation(
        image::metadata::Orientation::from_exif(orientation).ok_or("照片方向无效")?,
    );
    let image = image.into_rgb16();
    Ok(PhotoFrame {
        width: image.width(),
        height: image.height(),
        rgb: image.into_raw(),
        alpha: None,
    })
}
pub fn decode_with_engine(
    path: &Path,
    source: &SourceInterpretation,
    token: &CancellationToken,
    engine: Option<&Path>,
) -> Result<PhotoFrame, String> {
    if format(path) == Some("heif") {
        return super::heif_decode::decode(path, source, token, engine);
    }
    source.validate(path)?;
    let header = header_with_engine(path, engine, token)?;
    let mut rgb =
        vec![0u16; header.info.stored_width as usize * header.info.stored_height as usize * 3];
    let report = raw(path, &mut rgb, token, source)?;
    if (report.width, report.height, report.orientation)
        != (
            header.info.stored_width,
            header.info.stored_height,
            header.info.orientation as u32,
        )
        || metadata::version(path)? != header.info.source_version
    {
        return Err("源照片在显影期间变化".into());
    }
    let linear = color::linear_srgb()?.icc().map_err(|e| e.to_string())?;
    color::to_srgb(&mut rgb, &linear, false, token)?;
    check_cancel(token)?;
    oriented(report.width, report.height, rgb, report.orientation as u8)
}
pub fn capture_metadata(path: &Path, gps: bool) -> Result<little_exif::metadata::Metadata, String> {
    use little_exif::{exif_tag::ExifTag, metadata::Metadata, rational::uR64};
    if format(path) == Some("heif") {
        return super::heif::capture_metadata(path, gps);
    }
    let r = raw(
        path,
        &mut [],
        &CancellationToken::new(),
        &SourceInterpretation::Embedded,
    )?;
    let mut m = Metadata::new();
    let string = |value: &[c_char]| {
        unsafe { CStr::from_ptr(value.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    };
    m.set_tag(ExifTag::Make(string(&r.make).to_uppercase()));
    m.set_tag(ExifTag::Model(string(&r.model)));
    if r.lens[0] != 0 {
        m.set_tag(ExifTag::LensModel(string(&r.lens)));
    }
    if r.iso.is_finite() && r.iso > 0. && r.iso <= 65535. {
        m.set_tag(ExifTag::ISO(vec![r.iso.round() as u16]));
    }
    for (v, tag) in [
        (r.shutter, ExifTag::ExposureTime as fn(Vec<uR64>) -> ExifTag),
        (r.aperture, ExifTag::FNumber),
        (r.focal, ExifTag::FocalLength),
    ] {
        if v.is_finite() && v > 0. {
            m.set_tag(tag(vec![uR64::from(v as f64)]));
        }
    }
    let date = string(&r.date_time);
    if !date.is_empty() {
        m.set_tag(ExifTag::DateTimeOriginal(date));
    }
    // GPS and MakerNote are not copied from RAW; HEIF EXIF uses the same allowlist as JPEG.
    Ok(m)
}
