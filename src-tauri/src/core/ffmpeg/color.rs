//! Explicit input interpretation. These transforms produce SDR Rec.709 before
//! the creative LUT. Auto preserves the source and never guesses camera Log.
use crate::types::{AppError, AppResult};

pub fn input_filter(space: &str) -> AppResult<String> {
    match space {
        "auto" => Ok("null".into()),
        "rec709" => Ok(
            "format=gbrp16le,zscale=pin=bt709:tin=bt709:min=gbr:p=bt709:t=bt709:m=gbr:r=full"
                .into(),
        ),
        "rec2020-pq" | "rec2020-hlg" => {
            let transfer = if space == "rec2020-pq" {
                "smpte2084"
            } else {
                "arib-std-b67"
            };
            Ok(format!("format=gbrp16le,zscale=pin=bt2020:tin={transfer}:min=gbr:rin=full:p=bt2020:t=linear:m=gbr:r=full:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=mobius:desat=2,zscale=t=bt709:m=gbr:r=full,format=gbrp16le"))
        }
        _ => Err(AppError::InvalidInput("不支持的输入色彩空间".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transforms_are_explicit_and_do_not_accept_filter_injection() {
        assert_eq!(input_filter("auto").unwrap(), "null");
        assert!(input_filter("rec2020-pq").unwrap().contains("tonemap"));
        assert!(input_filter("auto,scale=1:1").is_err());
    }
}
