use anyhow::{bail, Context, Result};
use image::{DynamicImage, Rgba};
use pdfium_render::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::geometry::bounded_render_size;
use crate::types::{OperationRequest, OperationResult, Progress, RedactRect};

pub fn op_redact(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Redact operation requires an input PDF document"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    let src_doc = pdfium
        .load_pdf_from_file(input_path, request.password.as_deref())
        .with_context(|| format!("Failed opening input document '{}'", input_path))?;

    let total_doc_pages = src_doc.pages().len() as u32;
    if total_doc_pages == 0 {
        bail!("Input document has 0 pages");
    }

    // Group redactions by 1-based page number
    let mut page_redactions: HashMap<u32, Vec<&RedactRect>> = HashMap::new();
    if let Some(ref rects) = request.rects {
        for r in rects {
            if r.page >= 1 && r.page <= total_doc_pages {
                page_redactions.entry(r.page).or_default().push(r);
            }
        }
    }

    let mut new_doc = pdfium
        .create_new_pdf()
        .context("Failed creating new PDF document for redaction")?;

    let total = total_doc_pages;
    let mut completed: u32 = 0;
    let mut burned_count: usize = 0;

    for page_idx in 0..total_doc_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("Redact operation cancelled");
        }

        let page_num = page_idx + 1;
        let rects_opt = page_redactions.get(&page_num);

        if let Some(rects) = rects_opt {
            // Selected page has redactions: perform true raster burn to obliterate underlying text/vectors
            burned_count += 1;

            let src_page = src_doc
                .pages()
                .get(page_idx as PdfPageIndex)
                .with_context(|| format!("Failed getting page {} for redaction", page_num))?;

            let orig_w_pts = src_page.width();
            let orig_h_pts = src_page.height();

            // High resolution raster target: requested width between 1400 and 2400 (or 4x points)
            let requested_px = 2400.min(((orig_w_pts.value * 4.0).round() as u32).max(1400));
            let (render_w, render_h) = bounded_render_size(
                orig_w_pts.value,
                orig_h_pts.value,
                requested_px,
                2400,
            )?;

            let mut img = crate::raster::render_fixed(pdfium, Path::new(input_path),
                request.password.as_deref(), page_idx as PdfPageIndex, (render_w, render_h), true)
                .with_context(|| format!("Failed rendering page {} for raster burn", page_num))?
                .into_rgba8();

            let img_w = img.width() as f64;
            let img_h = img.height() as f64;
            let solid_black = Rgba([0u8, 0u8, 0u8, 255u8]);

            // Burn redactions directly into the raster pixels (normalized 0..1 from top-left)
            for r in rects {
                if !r.x.is_finite() || !r.y.is_finite() || !r.width.is_finite() || !r.height.is_finite() {
                    continue;
                }
                let x_start = (r.x.clamp(0.0, 1.0) * img_w).floor() as u32;
                let y_start = (r.y.clamp(0.0, 1.0) * img_h).floor() as u32;
                let x_end = (((r.x + r.width).clamp(0.0, 1.0)) * img_w).ceil() as u32;
                let y_end = (((r.y + r.height).clamp(0.0, 1.0)) * img_h).ceil() as u32;

                let x_end = x_end.min(img.width());
                let y_end = y_end.min(img.height());

                for py in y_start..y_end {
                    for px in x_start..x_end {
                        img.put_pixel(px, py, solid_black);
                    }
                }
            }

            // The raster already includes the source rotation; rebuild geometry from the
            // source dimensions (which are already rotation-aware) with NO rotation.
            let paper_size = PdfPagePaperSize::Custom(orig_w_pts, orig_h_pts);
            let mut new_page = new_doc
                .pages_mut()
                .create_page_at_end(paper_size)
                .with_context(|| format!("Failed creating page {} in redacted document", page_num))?;

            let dyn_img = DynamicImage::ImageRgba8(img);
            new_page
                .objects_mut()
                .create_image_object(
                    PdfPoints::ZERO,
                    PdfPoints::ZERO,
                    &dyn_img,
                    Some(orig_w_pts),
                    Some(orig_h_pts),
                )
                .with_context(|| format!("Failed embedding burned raster on page {}", page_num))?;
            new_page
                .regenerate_content()
                .with_context(|| format!("Failed regenerating content on page {}", page_num))?;
        } else {
            // Unredacted pages preserved as original vector content
            let dest_idx = new_doc.pages().len();
            new_doc
                .pages_mut()
                .copy_page_from_document(&src_doc, page_idx as PdfPageIndex, dest_idx)
                .with_context(|| format!("Failed preserving unredacted page {}", page_num))?;
        }

        completed += 1;
        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Processed page {} of {} (raster burned: {})", completed, total, burned_count),
        });
    }

    new_doc
        .save_to_file(&request.output)
        .with_context(|| format!("Failed saving redacted PDF to '{}'", request.output))?;

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!(
            "Successfully applied redactions: {} pages permanently raster-burned, {} vector pages preserved",
            burned_count,
            total_doc_pages as usize - burned_count
        ),
    })
}
