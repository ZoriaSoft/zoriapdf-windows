//! Local Word interchange: PDF -> DOCX, DOCX -> PDF, image -> DOCX.
//!
//! Both directions carry text styling (bold, italic, underline, strike, colour,
//! size), headings, alignment, lists, page breaks, tables (column widths, merged
//! cells, shading) and PNG/JPEG images. PDF writing reuses the embedded Unicode font,
//! so Turkish characters survive without system font assumptions. No network access.
use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::ocr::{find_tesseract, run_tesseract_ocr};
use crate::types::{OperationRequest, OperationResult, Progress};

mod docx_read;
mod docx_write;
mod model;
mod pdf_extract;
mod pdf_layout;

use model::{Block, Counts, WordDocument, WordParagraph};

fn plain_blocks(text: &str) -> Vec<Block> {
    text.split("\n\n")
        .map(str::trim)
        .filter(|block| !block.is_empty())
        .map(|block| Block::Paragraph(WordParagraph::plain(block.replace('\r', ""))))
        .collect()
}

/// "4 paragraph(s), 1 table(s), 2 image(s)"; tables and images only when present.
fn describe(counts: Counts) -> String {
    let mut text = format!("{} paragraph(s)", counts.paragraphs);
    if counts.tables > 0 {
        text.push_str(&format!(", {} table(s)", counts.tables));
    }
    if counts.images > 0 {
        text.push_str(&format!(", {} image(s)", counts.images));
    }
    text
}

pub fn op_pdf_to_word(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("PDF-to-Word requires an input PDF document"))?;
    if !request.output.to_lowercase().ends_with(".docx") {
        bail!("PDF-to-Word output must end with .docx");
    }
    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    let document = pdfium
        .load_pdf_from_file(input_path, request.password.as_deref())
        .with_context(|| format!("Failed opening input PDF '{}'", input_path))?;
    let total_pages = document.pages().len() as u32;
    let mut blocks: Vec<Block> = Vec::new();
    let mut ocr_pages = 0u32;
    let mut ocr_pages_used = 0u32;
    let ocr_available = find_tesseract()?;

    for page_index in 0..total_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("PDF-to-Word cancelled");
        }
        let page = document
            .pages()
            .get(page_index as PdfPageIndex)
            .with_context(|| format!("Failed reading page {}", page_index + 1))?;
        let has_text = page.text().map(|layer| !layer.all().trim().is_empty()).unwrap_or(false);
        let mut page_blocks: Vec<Block>;
        let mut recognised = false;
        if has_text {
            page_blocks = pdf_extract::extract_page(pdfium, &document, &page, page_index as PdfPageIndex)?;
        } else {
            // A page without a text layer: pictures that leave most of the page free stay
            // pictures; a page-sized image is a scan and goes to OCR, so the user never has
            // to know (or choose) whether the document is "image or text".
            let areas = pdf_extract::image_areas(&page)?;
            let page_area = page.width().value * page.height().value;
            let scan_like = areas.is_empty() || areas.iter().any(|a| *a >= page_area * 0.8);
            page_blocks = Vec::new();
            if !scan_like || !request.ocr.unwrap_or(true) {
                page_blocks = pdf_extract::collect_images(pdfium, &document, &page, page_index as PdfPageIndex, false)?
                    .into_iter()
                    .map(|(_, _, img)| Block::Image(img))
                    .collect();
            }
            if scan_like {
                ocr_pages += 1;
            }
            if scan_like && request.ocr.unwrap_or(true) {
                let Some(tesseract) = ocr_available.as_ref() else {
                    bail!(
                        "This page is a scan and the local OCR component is not installed. Install it from the 'Yerel OCR' panel (one-off download)."
                    );
                };
                let (render_w, render_h) = crate::geometry::bounded_render_size(
                    page.width().value,
                    page.height().value,
                    2000,
                    2400,
                )?;
                let image = crate::raster::render_fixed(pdfium, Path::new(input_path),
                    request.password.as_deref(), page_index as PdfPageIndex, (render_w, render_h), false)
                    .with_context(|| format!("Failed rasterising page {} for OCR", page_index + 1))?;
                let mut scratch = crate::scratch::ScratchDir::new()?;
                let temp_png = scratch.path().join("page.png");
                image.save(&temp_png)?;
                let base = scratch.path().join("tess");
                let recognised_text = run_tesseract_ocr(tesseract, &temp_png, &base);
                let text_path = recognised_text?;
                let text = std::fs::read_to_string(&text_path).context("Cannot read recognised page text")?;
                scratch.cleanup()?;
                page_blocks = plain_blocks(&text);
                ocr_pages_used += 1;
                recognised = true;
            }
        }
        // Each PDF page starts on a new Word page, so page layout survives a round trip.
        if !blocks.is_empty() {
            if let Some(first) = page_blocks.first_mut() {
                first.set_page_break_before(true);
            }
        }
        let page_kind = if recognised {
            "scanned page, local OCR"
        } else if page_blocks.is_empty() {
            "empty page"
        } else {
            "text layer"
        };
        blocks.extend(page_blocks);
        progress(Progress {
            id: request.id.clone(),
            completed: page_index + 1,
            total: total_pages,
            message: format!("Page {} of {} ({})", page_index + 1, total_pages, page_kind),
        });
    }

    let mut word = WordDocument { blocks, body_size: 11.0 };
    if !word.has_content() {
        if ocr_pages > 0 && !request.ocr.unwrap_or(true) {
            bail!(
                "This PDF is a scan ({} page(s) with no text layer). Local OCR is disabled for this request.",
                ocr_pages
            );
        }
        bail!("This PDF contains no readable text");
    }
    word.body_size = pdf_extract::finish_styles(&mut word.blocks);
    docx_write::write_docx(Path::new(&request.output), &word)?;
    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!(
            "Created Word document with {} from {} page(s){}",
            describe(word.counts()),
            total_pages,
            if ocr_pages_used > 0 {
                format!("; {} page(s) recognised with local OCR", ocr_pages_used)
            } else {
                String::new()
            }
        ),
    })
}

pub fn op_word_to_pdf(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Word-to-PDF requires an input .docx document"))?;
    if !request.output.to_lowercase().ends_with(".pdf") {
        bail!("Word-to-PDF output must end with .pdf");
    }
    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;
    let read = docx_read::read_docx(Path::new(input_path))?;
    let counts = read.document.counts();

    let composer = pdf_layout::PdfComposer::new(pdfium, read.document.body_size)?;
    composer.compose(&read.document, &request.output, cancel, |completed, total| {
        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Writing block {} of {}", completed, total),
        });
    })?;
    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!(
            "Converted Word document ({}) to PDF{}",
            describe(counts),
            if read.skipped_images > 0 {
                format!("; {} unsupported image(s) skipped", read.skipped_images)
            } else {
                String::new()
            }
        ),
    })
}

pub fn op_image_to_word(
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let image = request
        .image_paths
        .as_ref()
        .and_then(|list| list.first())
        .or_else(|| request.inputs.first())
        .ok_or_else(|| anyhow::anyhow!("Image-to-Word requires an input image"))?;
    if !request.output.to_lowercase().ends_with(".docx") {
        bail!("Image-to-Word output must end with .docx");
    }
    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;
    if cancel.load(Ordering::Relaxed) {
        bail!("Image-to-Word cancelled");
    }
    let tesseract = find_tesseract()?.ok_or_else(|| {
        anyhow::anyhow!(
            "The local OCR component is not installed. Install it from the 'Yerel OCR' panel (one-off download)."
        )
    })?;
    progress(Progress {
        id: request.id.clone(),
        completed: 0,
        total: 2,
        message: "Running local OCR on the image...".into(),
    });
    let scratch = crate::scratch::ScratchDir::new()?;
    let base = scratch.path().join("tess");
    let text_path = run_tesseract_ocr(&tesseract, Path::new(image), &base)?;
    let text = std::fs::read_to_string(&text_path).unwrap_or_default();
    if text.trim().is_empty() {
        bail!("No readable text found in the image");
    }
    if cancel.load(Ordering::Relaxed) {
        bail!("Image-to-Word cancelled");
    }
    let word = WordDocument { blocks: plain_blocks(&text), body_size: 11.0 };
    docx_write::write_docx(Path::new(&request.output), &word)?;
    progress(Progress {
        id: request.id.clone(),
        completed: 2,
        total: 2,
        message: "Word document created".into(),
    });
    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!(
            "Created Word document from image ({} paragraph(s), local OCR)",
            word.blocks.len()
        ),
    })
}

#[cfg(test)]
mod tests;
