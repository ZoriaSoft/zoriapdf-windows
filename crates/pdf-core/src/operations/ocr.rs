use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::check_collision;
use crate::geometry::bounded_render_size;
use crate::ocr::{find_tesseract, run_tesseract_ocr};
use crate::range::parse_page_ranges;
use crate::scratch::ScratchDir;
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_ocr(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("OCR operation requires an input PDF document"))?;

    check_collision(&request.inputs, &request.output, true)?;

    let tesseract_exe = find_tesseract()?.ok_or_else(|| {
        anyhow::anyhow!(
            "Tesseract OCR motoru bulunamadı. Tesseract kurun veya uygulama kaynaklarına ekleyin. (Tesseract executable was not found on PATH or in application resources)"
        )
    })?;

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

    let mut scratch = ScratchDir::new()
        .context("Failed creating temporary scratch directory for OCR")?;

    let total = target_pages.len() as u32;
    let mut completed: u32 = 0;
    let mut outputs = Vec::new();
    let mut combined_text = String::new();

    let input_stem = Path::new(input_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("page");

    for &page_num in &target_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("OCR operation cancelled");
        }

        let page_idx = (page_num - 1) as PdfPageIndex;
        let page = doc
            .pages()
            .get(page_idx)
            .with_context(|| format!("Failed getting page {}", page_num))?;

        let page_w_pts = page.width().value;
        let page_h_pts = page.height().value;

        // Render at 300 DPI for high OCR recognition accuracy (max 3600)
        let width_inches = page_w_pts / 72.0;
        let target_px = ((width_inches * 300.0).round() as u32).clamp(600, 3600);

        let (render_w, render_h) = bounded_render_size(
            page_w_pts,
            page_h_pts,
            target_px,
            3600,
        )?;

        let img = crate::raster::render_fixed(pdfium, Path::new(input_path),
            request.password.as_deref(), page_idx, (render_w, render_h), true)
            .with_context(|| format!("Failed rendering page {} for OCR", page_num))?;

        // Standalone OCR temporary PNG lives in scoped ScratchDir, not final out_dir
        let temp_img_path = scratch.path().join(format!("page-{page_num:04}.png"));
        img.save(&temp_img_path)
            .with_context(|| format!("Failed saving temporary image for page {}", page_num))?;

        let out_base = out_dir.join(format!("{}_ocr_{:04}", input_stem, page_num));
        let ocr_txt_path_res = run_tesseract_ocr(&tesseract_exe, &temp_img_path, &out_base);

        // Always clean up temp raster image
        let _ = std::fs::remove_file(&temp_img_path);

        let txt_path = ocr_txt_path_res?;
        if let Ok(content) = std::fs::read_to_string(&txt_path) {
            combined_text.push_str(&format!("--- Sayfa {} ---\n", page_num));
            combined_text.push_str(&content);
            combined_text.push_str("\n\n");
        }

        outputs.push(txt_path.to_string_lossy().to_string());
        completed += 1;

        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("OCR recognized page {} of {}", completed, total),
        });
    }

    // Save full combined OCR file if multiple pages
    if target_pages.len() > 1 {
        let combined_file = out_dir.join(format!("{}_ocr_full.txt", input_stem));
        if let Ok(mut f) = File::create(&combined_file) {
            let _ = f.write_all(combined_text.as_bytes());
            outputs.push(combined_file.to_string_lossy().to_string());
        }
    }
    scratch.cleanup().context("Failed cleaning temporary OCR files")?;

    Ok(OperationResult {
        outputs,
        message: format!("Successfully performed OCR on {} pages", total),
    })
}
