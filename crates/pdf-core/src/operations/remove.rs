use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::range::parse_page_ranges;
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_remove(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Remove operation requires an input PDF document"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    let src_doc = pdfium
        .load_pdf_from_file(input_path, request.password.as_deref())
        .with_context(|| format!("Failed to open input document '{}'", input_path))?;

    let total_doc_pages = src_doc.pages().len() as u32;
    let range_str = request
        .pages
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Remove operation requires 'pages' parameter"))?;

    let remove_pages = parse_page_ranges(range_str, total_doc_pages)?;
    let remove_set: HashSet<u32> = remove_pages.into_iter().collect();

    let pages_to_keep: Vec<u32> = (1..=total_doc_pages)
        .filter(|p| !remove_set.contains(p))
        .collect();

    if pages_to_keep.is_empty() {
        bail!("Cannot remove all pages: resulting document would be empty");
    }

    let mut new_doc = pdfium
        .create_new_pdf()
        .context("Failed to create new PDF document for remove operation")?;

    let total = pages_to_keep.len() as u32;
    let mut completed: u32 = 0;

    for &page_num in &pages_to_keep {
        if cancel.load(Ordering::Relaxed) {
            bail!("Remove operation cancelled");
        }

        let src_idx = (page_num - 1) as PdfPageIndex;
        let dest_idx = new_doc.pages().len();
        new_doc
            .pages_mut()
            .copy_page_from_document(&src_doc, src_idx, dest_idx)
            .with_context(|| format!("Failed copying kept page {}", page_num))?;

        completed += 1;
        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Copied page {} ({} of {} kept)", page_num, completed, total),
        });
    }

    new_doc
        .save_to_file(&request.output)
        .with_context(|| format!("Failed to save modified PDF to '{}'", request.output))?;

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!(
            "Successfully removed {} pages ({} pages remaining)",
            remove_set.len(),
            pages_to_keep.len()
        ),
    })
}
