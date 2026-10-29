use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::check_collision;
use crate::range::parse_page_ranges;
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_split(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Split operation requires an input PDF document"))?;

    check_collision(&request.inputs, &request.output, true)?;

    let out_dir = Path::new(&request.output);
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed to create output directory '{}'", request.output))?;

    let src_doc = pdfium
        .load_pdf_from_file(input_path, request.password.as_deref())
        .with_context(|| format!("Failed to open input document '{}'", input_path))?;

    let total_doc_pages = src_doc.pages().len() as u32;
    let range_str = request.pages.as_deref().unwrap_or("");
    let target_pages = parse_page_ranges(range_str, total_doc_pages)?;

    let total = target_pages.len() as u32;
    let mut completed: u32 = 0;
    let mut outputs = Vec::new();

    let input_stem = Path::new(input_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");

    for &page_num in &target_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("Split operation cancelled");
        }

        let mut page_doc = pdfium
            .create_new_pdf()
            .context("Failed to create single-page document during split")?;

        let src_idx = (page_num - 1) as PdfPageIndex;
        page_doc
            .pages_mut()
            .copy_page_from_document(&src_doc, src_idx, 0)
            .with_context(|| format!("Failed copying page {}", page_num))?;

        let out_filename = format!("{}_page_{:04}.pdf", input_stem, page_num);
        let out_file_path = out_dir.join(out_filename);

        page_doc
            .save_to_file(&out_file_path)
            .with_context(|| format!("Failed to save split page to '{}'", out_file_path.display()))?;

        outputs.push(out_file_path.to_string_lossy().to_string());
        completed += 1;

        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Split page {} of {}", completed, total),
        });
    }

    Ok(OperationResult {
        outputs,
        message: format!("Successfully split document into {} individual PDF files", total),
    })
}
