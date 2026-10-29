use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::range::parse_page_ranges;
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_rotate(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Rotate operation requires an input PDF document"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    let degrees = request.rotation.unwrap_or(90);
    // Normalize rotation to 0, 90, 180, 270
    let norm_deg = ((degrees % 360) + 360) % 360;
    let quarter_turns = norm_deg / 90;

    let mut doc = pdfium
        .load_pdf_from_file(input_path, request.password.as_deref())
        .with_context(|| format!("Failed to open input document '{}'", input_path))?;

    let total_doc_pages = doc.pages().len() as u32;
    let target_pages = parse_page_ranges(
        request.pages.as_deref().unwrap_or(""),
        total_doc_pages,
    )?;
    let target_set: HashSet<u32> = target_pages.into_iter().collect();

    let total = total_doc_pages;
    let mut completed: u32 = 0;

    for page_idx in 0..total_doc_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("Rotate operation cancelled");
        }

        let page_num = page_idx + 1;
        if target_set.contains(&page_num) {
            let mut page = doc
                .pages_mut()
                .get(page_idx as PdfPageIndex)
                .with_context(|| format!("Failed getting page {}", page_num))?;

            let cur_deg = page
                .rotation()
                .unwrap_or(PdfPageRenderRotation::None)
                .as_degrees() as i32;

            let new_deg = (cur_deg + norm_deg) % 360;
            let new_rot = match new_deg {
                90 => PdfPageRenderRotation::Degrees90,
                180 => PdfPageRenderRotation::Degrees180,
                270 => PdfPageRenderRotation::Degrees270,
                _ => PdfPageRenderRotation::None,
            };

            page.set_rotation(new_rot);
        }

        completed += 1;
        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Processed page {} of {}", completed, total),
        });
    }

    doc.save_to_file(&request.output)
        .with_context(|| format!("Failed to save rotated PDF to '{}'", request.output))?;

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!(
            "Successfully rotated {} pages by {} degrees ({} turns)",
            target_set.len(),
            norm_deg,
            quarter_turns
        ),
    })
}
