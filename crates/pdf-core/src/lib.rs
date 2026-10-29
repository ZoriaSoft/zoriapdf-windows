pub mod collision;
pub mod engine;
pub mod font;
mod fs_security;
pub mod geometry;
pub mod ocr;
pub mod ocr_install;
pub mod operations;
pub mod range;
mod raster;
pub mod output;
pub mod scratch;
pub mod types;

pub use collision::{check_collision, ensure_parent_dir, normalize_path};
pub use engine::{find_snippets, Engine, MAX_SEARCH_HITS};
pub use pdfium_render::prelude::PdfDocument;
pub use font::{get_or_load_font, BUNDLED_MANROPE};
pub use ocr::{find_tesseract, run_tesseract_ocr};
pub use range::parse_page_ranges;
pub use types::{
    Annotation, DocumentInfo, OperationRequest, OperationResult, Progress, RedactRect,
    RenderedPage, SearchHit,
};
