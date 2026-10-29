use anyhow::{bail, Context, Result};
use image::GenericImageView;
use pdfium_render::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::geometry::{bounded_render_size, load_bounded_image};
use crate::range::parse_page_ranges;
use crate::types::{OperationRequest, OperationResult, Progress};


pub fn op_images_to_pdf(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let images: Vec<String> = if let Some(ref list) = request.image_paths {
        if !list.is_empty() {
            list.clone()
        } else {
            request.inputs.clone()
        }
    } else {
        request.inputs.clone()
    };

    if images.is_empty() {
        bail!("Images-to-PDF operation requires at least one input image file");
    }

    check_collision(&images, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    let mut doc = pdfium
        .create_new_pdf()
        .context("Failed to create new PDF document for images conversion")?;

    let total = images.len() as u32;
    let mut completed: u32 = 0;

    for img_path_str in &images {
        if cancel.load(Ordering::Relaxed) {
            bail!("Images-to-PDF operation cancelled");
        }

        let img_path = Path::new(img_path_str);
        let dyn_img = load_bounded_image(img_path)?;

        let (w_px, h_px) = dyn_img.dimensions();
        // 72 DPI mapping: 1 px = 1 pt
        let w_pts = PdfPoints::new(w_px as f32);
        let h_pts = PdfPoints::new(h_px as f32);

        let paper_size = PdfPagePaperSize::Custom(w_pts, h_pts);
        let mut page = doc
            .pages_mut()
            .create_page_at_end(paper_size)
            .with_context(|| format!("Failed creating page for image '{}'", img_path_str))?;

        page.objects_mut()
            .create_image_object(
                PdfPoints::ZERO,
                PdfPoints::ZERO,
                &dyn_img,
                Some(w_pts),
                Some(h_pts),
            )
            .with_context(|| format!("Failed placing image object on page for '{}'", img_path_str))?;

        page.regenerate_content()
            .with_context(|| format!("Failed regenerating page content for image '{}'", img_path_str))?;

        completed += 1;
        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Converted image {} of {}", completed, total),
        });
    }

    doc.save_to_file(&request.output)
        .with_context(|| format!("Failed to save converted PDF to '{}'", request.output))?;

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!("Successfully created PDF from {} images", total),
    })
}

pub fn op_pdf_to_images(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("PDF-to-images operation requires an input PDF document"))?;

    check_collision(&request.inputs, &request.output, true)?;

    let out_dir = Path::new(&request.output);
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Failed creating output directory '{}'", request.output))?;

    let doc = pdfium
        .load_pdf_from_file(input_path, request.password.as_deref())
        .with_context(|| format!("Failed opening input document '{}'", input_path))?;

    let total_doc_pages = doc.pages().len() as u32;
    let target_pages = parse_page_ranges(
        request.pages.as_deref().unwrap_or(""),
        total_doc_pages,
    )?;

    let dpi = request.dpi.unwrap_or(150).clamp(72, 600);
    let total = target_pages.len() as u32;
    let mut completed: u32 = 0;
    let mut outputs = Vec::new();

    let input_stem = Path::new(input_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("page");

    for &page_num in &target_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("PDF-to-images operation cancelled");
        }

        let page_idx = (page_num - 1) as PdfPageIndex;
        let page = doc
            .pages()
            .get(page_idx)
            .with_context(|| format!("Failed getting page {}", page_num))?;

        let page_w_pts = page.width().value;
        let page_h_pts = page.height().value;

        // 72 points per inch in PDF spec
        let width_inches = page_w_pts / 72.0;
        let target_px = ((width_inches * dpi as f32).round() as u32).clamp(100, 4800);

        let (render_w, render_h) = bounded_render_size(
            page_w_pts,
            page_h_pts,
            target_px,
            4800,
        )?;

        let img = crate::raster::render_fixed(pdfium, Path::new(input_path),
            request.password.as_deref(), page_idx, (render_w, render_h), true)
            .with_context(|| format!("Failed rendering page {}", page_num))?;

        let out_filename = format!("{}_page_{:04}.png", input_stem, page_num);
        let out_file_path = out_dir.join(out_filename);

        img.save(&out_file_path)
            .with_context(|| format!("Failed saving image file '{}'", out_file_path.display()))?;

        outputs.push(out_file_path.to_string_lossy().to_string());
        completed += 1;

        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Exported page {} of {} to PNG", completed, total),
        });
    }

    Ok(OperationResult {
        outputs,
        message: format!("Successfully exported {} pages to PNG images at {} DPI", total, dpi),
    })
}
