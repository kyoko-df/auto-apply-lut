use crate::types::LutFormat;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConvertLutsRequest {
    pub paths: Vec<String>,
    pub target_format: LutFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConvertLutItemResult {
    pub source_path: String,
    pub target_path: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConvertLutsResponse {
    pub success_count: usize,
    pub failure_count: usize,
    pub results: Vec<BatchConvertLutItemResult>,
}
