//! 错误类型定义

use serde::{Deserialize, Serialize};
use std::fmt;

/// 应用程序错误类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppError {
    /// 文件系统错误
    FileSystem(String),
    /// 数据库错误
    Database(String),
    /// FFmpeg处理错误
    FFmpeg(String),
    /// LUT处理错误
    LutProcessing(String),
    /// GPU相关错误
    Gpu(String),
    /// 配置错误
    Config(String),
    /// 配置错误（别名）
    Configuration(String),
    /// 网络错误
    Network(String),
    /// 验证错误
    Validation(String),
    /// IO错误
    Io(String),
    /// 序列化错误
    Serialization(String),
    /// 解析错误
    Parse(String),
    /// 无效输入
    InvalidInput(String),
    /// 未找到
    NotFound(String),
    /// 内部错误
    Internal(String),
    /// 超时错误
    Timeout(String),
    /// 未知错误
    Unknown(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::FileSystem(m)
            | Self::Database(m)
            | Self::FFmpeg(m)
            | Self::LutProcessing(m)
            | Self::Gpu(m)
            | Self::Config(m)
            | Self::Configuration(m)
            | Self::Network(m)
            | Self::Validation(m)
            | Self::Io(m)
            | Self::Serialization(m)
            | Self::Parse(m)
            | Self::InvalidInput(m)
            | Self::NotFound(m)
            | Self::Internal(m)
            | Self::Timeout(m)
            | Self::Unknown(m) => m,
        };
        // Keep UI messages parseable after passing through a processing error.
        if parse_ui_error(message).is_some() {
            return f.write_str(message);
        }
        match self {
            AppError::FileSystem(msg) => write!(f, "文件系统错误: {}", msg),
            AppError::Database(msg) => write!(f, "数据库错误: {}", msg),
            AppError::FFmpeg(msg) => write!(f, "FFmpeg处理错误: {}", msg),
            AppError::LutProcessing(msg) => write!(f, "LUT处理错误: {}", msg),
            AppError::Gpu(msg) => write!(f, "GPU错误: {}", msg),
            AppError::Config(msg) => write!(f, "配置错误: {}", msg),
            AppError::Configuration(msg) => write!(f, "配置错误: {}", msg),
            AppError::Timeout(msg) => write!(f, "超时错误: {}", msg),
            AppError::Network(msg) => write!(f, "网络错误: {}", msg),
            AppError::Validation(msg) => write!(f, "验证错误: {}", msg),
            AppError::Io(msg) => write!(f, "IO错误: {}", msg),
            AppError::Serialization(msg) => write!(f, "序列化错误: {}", msg),
            AppError::Parse(msg) => write!(f, "解析错误: {}", msg),
            AppError::InvalidInput(msg) => write!(f, "无效输入错误: {}", msg),
            AppError::NotFound(msg) => write!(f, "未找到: {}", msg),
            AppError::Internal(msg) => write!(f, "内部错误: {}", msg),
            AppError::Unknown(msg) => write!(f, "未知错误: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

/// Marker-prefixed structured error for the frontend, encoded inside the
/// ordinary String error channel: `\x1f{code}\x1f{params-json}\x1f{message}`.
///
/// The backend error plumbing is `Result<T, String>` end to end (commands,
/// DB, persisted snapshots); this keeps signatures untouched while letting
/// the frontend translate `code` with `params` and fall back to `message`.
pub const UI_ERROR_MARK: char = '\u{1f}';

pub fn ui_err(code: &str, message: impl AsRef<str>) -> String {
    format!(
        "{mark}{code}{mark}{mark}{msg}",
        mark = UI_ERROR_MARK,
        code = code,
        msg = message.as_ref()
    )
}

pub fn ui_err_p(code: &str, params: serde_json::Value, message: impl AsRef<str>) -> String {
    format!(
        "{mark}{code}{mark}{params}{mark}{msg}",
        mark = UI_ERROR_MARK,
        code = code,
        params = params,
        msg = message.as_ref()
    )
}

/// Reads a structured error back into (code, params, message).
pub fn parse_ui_error(text: &str) -> Option<(&str, Option<&str>, &str)> {
    let rest = text.strip_prefix(UI_ERROR_MARK)?;
    let (code, rest) = rest.split_once(UI_ERROR_MARK)?;
    let (params, message) = rest.split_once(UI_ERROR_MARK)?;
    if code.is_empty()
        || !code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return None;
    }
    Some((
        code,
        if params.is_empty() {
            None
        } else {
            Some(params)
        },
        message,
    ))
}

// 从标准库错误类型转换
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Serialization(err.to_string())
    }
}

/// 应用程序结果类型
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_errors_preserve_structured_ui_messages_and_plain_diagnostics() {
        let message = ui_err("photo.input_profile_unknown", "照片未标记色彩空间");
        assert_eq!(AppError::InvalidInput(message.clone()).to_string(), message);
        assert_eq!(AppError::FFmpeg(message.clone()).to_string(), message);
        assert_eq!(
            AppError::InvalidInput("bad file".into()).to_string(),
            "无效输入错误: bad file"
        );
    }

    #[test]
    fn ui_error_roundtrips_code_params_and_message() {
        let plain = ui_err("preview.cancelled", "预览已取消");
        assert_eq!(
            parse_ui_error(&plain),
            Some(("preview.cancelled", None, "预览已取消"))
        );
        let with_params = ui_err_p(
            "fs.read_file",
            serde_json::json!({ "path": "/a.mov", "error": "denied" }),
            "无法读取 /a.mov：denied",
        );
        let (code, params, message) = parse_ui_error(&with_params).unwrap();
        assert_eq!(code, "fs.read_file");
        let params: serde_json::Value = serde_json::from_str(params.unwrap()).unwrap();
        assert_eq!(params["path"], "/a.mov");
        assert_eq!(message, "无法读取 /a.mov：denied");
    }

    #[test]
    fn plain_strings_and_malformed_markers_are_not_ui_errors() {
        assert_eq!(parse_ui_error("plain error"), None);
        assert_eq!(parse_ui_error(""), None);
        assert_eq!(parse_ui_error("\u{1f}"), None);
        assert_eq!(parse_ui_error("\u{1f}code\u{1f}no-second-mark"), None);
        assert_eq!(parse_ui_error("\u{1f}\u{1f}\u{1f}empty code"), None);
        assert_eq!(
            parse_ui_error("\u{1f}bad code!\u{1f}\u{1f}msg"),
            None,
            "codes are limited to ASCII alphanumerics, dots and underscores"
        );
        // A message may itself contain the marker; it survives intact.
        let nested = ui_err("x.y", "a\u{1f}b");
        assert_eq!(parse_ui_error(&nested), Some(("x.y", None, "a\u{1f}b")));
    }
}
