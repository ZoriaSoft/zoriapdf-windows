use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::font::get_or_load_font;
use crate::geometry::display_to_page_matrix;
use crate::range::parse_page_ranges;
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_watermark(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Watermark operation requires an input PDF document"))?;

    let watermark_text = request
        .text
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Watermark operation requires non-empty 'text'"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    let mut doc = pdfium
        .load_pdf_from_file(input_path, request.password.as_deref())
        .with_context(|| format!("Failed to open input document '{}'", input_path))?;

    let total_doc_pages = doc.pages().len() as u32;
    let target_pages = parse_page_ranges(
        request.pages.as_deref().unwrap_or(""),
        total_doc_pages,
    )?;
    let target_set: HashSet<u32> = target_pages.into_iter().collect();

    let font_token = get_or_load_font(&mut doc);

    let total = total_doc_pages;
    let mut completed: u32 = 0;

    for page_idx in 0..total_doc_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("Watermark operation cancelled");
        }

        let page_num = page_idx + 1;
        if target_set.contains(&page_num) {
            let mut page = doc
                .pages_mut()
                .get(page_idx as PdfPageIndex)
                .with_context(|| format!("Failed getting page {}", page_num))?;

            let page_w = page.width();
            let page_h = page.height();

            // Font size proportional to page dimensions
            let font_size = PdfPoints::new((page_w.value.min(page_h.value) / 10.0).clamp(24.0, 72.0));

            let mut text_obj = PdfPageTextObject::new(&doc, watermark_text, font_token, font_size)
                .with_context(|| format!("Failed creating watermark text for page {}", page_num))?;

            text_obj.set_fill_color(PdfColor::new(128, 128, 128, 90))
                .context("Failed setting watermark color")?;
            text_obj.rotate_counter_clockwise_degrees(45.0)
                .context("Failed rotating watermark")?;
            let bounds = text_obj.bounds()?.to_rect();
            let scale = (page_w.value * 0.8 / bounds.width().value)
                .min(page_h.value * 0.8 / bounds.height().value).min(1.0);
            if !scale.is_finite() || scale <= 0.0 {
                bail!("Watermark text has invalid display bounds");
            }
            text_obj.scale(scale, scale).context("Failed fitting watermark to page")?;
            let bounds = text_obj.bounds()?.to_rect();
            text_obj.translate(
                PdfPoints::new((page_w.value - bounds.width().value) * 0.5 - bounds.left().value),
                PdfPoints::new((page_h.value - bounds.height().value) * 0.5 - bounds.bottom().value),
            ).context("Failed centering watermark")?;
            text_obj.apply_matrix(display_to_page_matrix(&page)?)
                .context("Failed mapping watermark to displayed page")?;

            page.objects_mut()
                .add_text_object(text_obj)
                .with_context(|| format!("Failed adding watermark to page {}", page_num))?;

            page.regenerate_content()
                .with_context(|| format!("Failed regenerating content on page {}", page_num))?;
        }

        completed += 1;
        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Applied watermark to page {} of {}", completed, total),
        });
    }

    doc.save_to_file(&request.output)
        .with_context(|| format!("Failed to save watermarked PDF to '{}'", request.output))?;

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!("Successfully watermarked {} pages", target_set.len()),
    })
}
