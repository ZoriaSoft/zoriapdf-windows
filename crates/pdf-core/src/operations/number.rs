use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::font::get_or_load_font;
use crate::range::parse_page_ranges;
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_number(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Page numbering operation requires an input PDF document"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    let template = request
        .text
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or("{page} / {total}");

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
    let font_size = PdfPoints::new(10.0);

    let total = total_doc_pages;
    let mut completed: u32 = 0;

    for page_idx in 0..total_doc_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("Page numbering operation cancelled");
        }

        let page_num = page_idx + 1;
        if target_set.contains(&page_num) {
            let label = page_label(template, page_num, total_doc_pages);

            let mut page = doc
                .pages_mut()
                .get(page_idx as PdfPageIndex)
                .with_context(|| format!("Failed getting page {}", page_num))?;

            let page_w = page.width();
            let mut text_obj = PdfPageTextObject::new(&doc, &label, font_token, font_size)
                .with_context(|| format!("Failed creating page number object for page {}", page_num))?;

            let _ = text_obj.set_fill_color(PdfColor::new(28, 31, 30, 220));

            // Position at bottom center
            let x_pos = (page_w / 2.0) - PdfPoints::new(20.0);
            let y_pos = PdfPoints::new(20.0);
            let _ = text_obj.translate(x_pos, y_pos);

            page.objects_mut()
                .add_text_object(text_obj)
                .with_context(|| format!("Failed adding page number to page {}", page_num))?;

            page.regenerate_content()
                .with_context(|| format!("Failed regenerating content on page {}", page_num))?;
        }

        completed += 1;
        progress(Progress {
            id: request.id.clone(),
            completed,
            total,
            message: format!("Numbered page {} of {}", completed, total),
        });
    }

    doc.save_to_file(&request.output)
        .with_context(|| format!("Failed to save numbered PDF to '{}'", request.output))?;

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!("Successfully added page numbers to {} pages", target_set.len()),
    })
}

/// Fills a page-number template. The UI offers `{n}`; `{page}` is kept for older callers.
pub fn page_label(template: &str, page: u32, total: u32) -> String {
    template
        .replace("{n}", &page.to_string())
        .replace("{page}", &page.to_string())
        .replace("{total}", &total.to_string())
}

#[cfg(test)]
mod tests {
    use super::page_label;

    #[test]
    fn every_offered_template_gets_real_numbers() {
        assert_eq!(page_label("Sayfa {n} / {total}", 2, 4), "Sayfa 2 / 4");
        assert_eq!(page_label("{n} / {total}", 1, 9), "1 / 9");
        assert_eq!(page_label("Sayfa {n}", 3, 3), "Sayfa 3");
        assert_eq!(page_label("- {n} -", 7, 10), "- 7 -");
        assert_eq!(page_label("{page} / {total}", 5, 6), "5 / 6");
    }
}
