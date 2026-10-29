use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Document information returned when opening a PDF file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    pub id: String,
    pub path: String,
    pub name: String,
    pub pages: u32,
    pub size: u64,
    /// Content fingerprint used to bind document-bound UI state after an explicit reopen.
    pub revision: String,
    /// Per-page [width, height] in PDF points (rotation applied), for lazy layout
    /// of thumbnails and continuous scroll without rendering every page.
    #[serde(default)]
    pub page_sizes: Vec<[u32; 2]>,
}

/// Rendered raster representation of a single PDF page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedPage {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

/// A search hit in a document page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub page: u32,
    pub text: String,
}

/// Normalized bounding rectangle for redaction (coordinates 0..1 from top-left).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactRect {
    pub page: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Normalized annotation request payload (coordinates 0..1 from top-left).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub page: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 'highlight' | 'text' | 'ink' | 'signature'
    pub kind: String,
    pub text: Option<String>,
    pub points: Option<Vec<Vec<f64>>>,
    pub image_path: Option<String>,
}

/// Request for a batch or discrete PDF modification/generation operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRequest {
    pub id: String,
    pub kind: String,
    pub inputs: Vec<String>,
    pub output: String,
    pub pages: Option<String>,
    pub password: Option<String>,
    /// Credentials bound by the host to the exact selected input paths.
    #[serde(default)]
    pub input_passwords: HashMap<String, String>,
    pub new_password: Option<String>,
    pub text: Option<String>,
    pub rotation: Option<i32>,
    pub dpi: Option<u32>,
    pub image_paths: Option<Vec<String>>,
    pub rects: Option<Vec<RedactRect>>,
    /// Enables local OCR for scanned pages (PDF-to-Word).
    #[serde(default)]
    pub ocr: Option<bool>,
    /// Download URL of an optional component (local OCR pack).
    #[serde(default)]
    pub download_url: Option<String>,
    /// Pinned SHA-256 of the downloaded component.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Installation directory of an optional component.
    #[serde(default)]
    pub target_dir: Option<String>,
    pub annotation: Option<Annotation>,
    #[serde(default, alias = "annotations")]
    pub annotation_list: Option<Vec<Annotation>>,
}

/// Result returned after executing an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub outputs: Vec<String>,
    pub message: String,
}

/// Incremental progress event emitted while executing an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: String,
    pub completed: u32,
    pub total: u32,
    pub message: String,
}
