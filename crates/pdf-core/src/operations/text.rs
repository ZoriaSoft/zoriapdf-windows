use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::range::parse_page_ranges;
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_extract_text(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Extract-text operation requires an input PDF document"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    let doc = pdfium
        .load_pdf_from_file(input_path, request.password.as_deref())
        .with_context(|| format!("Failed opening input document '{}'", input_path))?;

    let total_doc_pages = doc.pages().len() as u32;
    let target_pages = parse_page_ranges(
        request.pages.as_deref().unwrap_or(""),
        total_doc_pages,
    )?;

    let total = target_pages.len() as u32;
    let mut completed: u32 = 0;

    let mut out_file = File::create(&request.output)
        .with_context(|| format!("Failed creating output text file '{}'", request.output))?;

    for &page_num in &target_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("Extract-text operation cancelled");
        }

        let page_idx = (page_num - 1) as PdfPageIndex;
        let page = doc
            .pages()
            .get(page_idx)
            .with_context(|| format!("Failed getting page {}", page_num))?;

        let text_content = page
            .text()
            .map(|t| t.all())
            .unwrap_or_default();

        writeln!(out_file, "--- Sayfa {} ---", page_num)?;
        writeln!(out_file, "{}", text_content)?;
        writeln!(out_file)?;

        completed += 1;
        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Extracted text from page {} ({} of {})", page_num, completed, total),
        });
    }

    out_file.flush()?;

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!("Successfully extracted text from {} pages to '{}'", total, request.output),
    })
}
