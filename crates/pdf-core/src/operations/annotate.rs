use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::font::get_or_load_font;
use crate::geometry::{display_to_page_matrix, load_bounded_image};
use crate::types::{Annotation, OperationRequest, OperationResult, Progress};


pub fn op_annotate(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Annotate operation requires an input PDF document"))?;

    let annotations: Vec<Annotation> = request
        .annotation_list
        .clone()
        .filter(|list| !list.is_empty())
        .or_else(|| request.annotation.clone().map(|a| vec![a]))
        .ok_or_else(|| anyhow::anyhow!("Annotate operation requires at least one 'annotation' payload"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    if cancel.load(Ordering::Relaxed) {
        bail!("Annotate operation cancelled");
    }

    let mut doc = pdfium
        .load_pdf_from_file(input_path, request.password.as_deref())
        .with_context(|| format!("Failed opening input document '{}'", input_path))?;

    let total_pages = doc.pages().len() as u32;
    for annotation in &annotations {
        if annotation.page == 0 || annotation.page > total_pages {
            bail!(
                "Annotation page {} is out of bounds (document has {} pages)",
                annotation.page,
                total_pages
            );
        }
    }

    let total = annotations.len() as u32;
    progress(Progress {
        id: request.id.clone(),
        completed: 0,
        total,
        message: format!("Applying {} annotations...", total),
    });

    let font_token = get_or_load_font(&mut doc);

    for (index, annotation) in annotations.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            bail!("Annotate operation cancelled");
        }

        if !annotation.x.is_finite()
            || !annotation.y.is_finite()
            || !annotation.width.is_finite()
            || !annotation.height.is_finite()
        {
            bail!("Annotation geometry contains non-finite values");
        }

        let page_idx = (annotation.page - 1) as PdfPageIndex;
        let mut page = doc
            .pages_mut()
            .get(page_idx)
            .with_context(|| format!("Failed getting page {}", annotation.page))?;

        let page_w = page.width().value;
        let page_h = page.height().value;
        if !page_w.is_finite() || !page_h.is_finite() || page_w <= 0.0 || page_h <= 0.0 {
            bail!("Page dimensions for page {} are invalid", annotation.page);
        }

        let matrix = display_to_page_matrix(&page)
            .with_context(|| format!("Failed computing display transform for page {}", annotation.page))?;

        let left_pts = (annotation.x * page_w as f64) as f32;
        let right_pts = ((annotation.x + annotation.width) * page_w as f64) as f32;
        let top_pts = ((1.0 - annotation.y) * page_h as f64) as f32;
        let bottom_pts = ((1.0 - annotation.y - annotation.height) * page_h as f64) as f32;

        match annotation.kind.to_lowercase().as_str() {
            "highlight" => {
                if annotation.width <= 0.0 || annotation.height <= 0.0 {
                    bail!("Highlight annotation width and height must be positive");
                }
                let rect = PdfRect::new_from_values(bottom_pts, left_pts, top_pts, right_pts);
                // Semi-transparent yellow highlight (Origami / highlighter yellow)
                let yellow_tint = PdfColor::new(255, 235, 59, 115);
                let mut rect_obj = page.objects_mut()
                    .create_path_object_rect(rect, None, None, Some(yellow_tint))
                    .with_context(|| "Failed creating highlight rectangle")?;
                rect_obj.apply_matrix(matrix)
                    .with_context(|| "Failed applying transform matrix to highlight rectangle")?;
            }
            "text" => {
                let text_content = annotation
                    .text
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Note");

                let height_pts = (annotation.height * page_h as f64) as f32;
                let font_size = if height_pts > 4.0 && height_pts < 40.0 {
                    PdfPoints::new(height_pts * 0.8)
                } else {
                    PdfPoints::new(14.0)
                };

                let mut text_obj = PdfPageTextObject::new(&doc, text_content, font_token, font_size)
                    .with_context(|| "Failed creating text annotation object")?;

                // Ink color (#1C1F1E)
                text_obj.set_fill_color(PdfColor::new(28, 31, 30, 255))
                    .with_context(|| "Failed setting text annotation fill color")?;
                text_obj.translate(PdfPoints::new(left_pts), PdfPoints::new(bottom_pts))
                    .with_context(|| "Failed translating text annotation")?;
                text_obj.apply_matrix(matrix)
                    .with_context(|| "Failed applying transform matrix to text annotation")?;

                page.objects_mut()
                    .add_text_object(text_obj)
                    .with_context(|| "Failed adding text annotation to page")?;
            }
            "ink" => {
                let points = annotation
                    .points
                    .as_ref()
                    .filter(|pts| pts.len() >= 2)
                    .ok_or_else(|| anyhow::anyhow!("Ink annotation requires at least 2 coordinate points"))?;

                let ink_color = PdfColor::new(28, 31, 30, 255); // Origami ink
                let stroke_width = PdfPoints::new(2.0);

                for window in points.windows(2) {
                    let p1 = &window[0];
                    let p2 = &window[1];
                    if p1.len() < 2 || p2.len() < 2 {
                        bail!("Ink point must contain at least 2 coordinates");
                    }
                    if !p1[0].is_finite() || !p1[1].is_finite() || !p2[0].is_finite() || !p2[1].is_finite() {
                        bail!("Ink coordinates must be finite numbers");
                    }
                    let x1 = (p1[0] * page_w as f64) as f32;
                    let y1 = ((1.0 - p1[1]) * page_h as f64) as f32;
                    let x2 = (p2[0] * page_w as f64) as f32;
                    let y2 = ((1.0 - p2[1]) * page_h as f64) as f32;

                    let mut line_obj = page.objects_mut().create_path_object_line(
                        PdfPoints::new(x1),
                        PdfPoints::new(y1),
                        PdfPoints::new(x2),
                        PdfPoints::new(y2),
                        ink_color,
                        stroke_width,
                    ).with_context(|| "Failed creating ink path object line")?;
                    line_obj.apply_matrix(matrix)
                        .with_context(|| "Failed applying transform matrix to ink line")?;
                }
            }
            "signature" => {
                let img_path_str = annotation
                    .image_path
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Signature annotation requires 'imagePath' parameter"))?;

                let sig_img = load_bounded_image(Path::new(img_path_str))?;

                let render_w = if annotation.width > 0.0 {
                    (annotation.width * page_w as f64) as f32
                } else {
                    120.0
                };
                let render_h = if annotation.height > 0.0 {
                    (annotation.height * page_h as f64) as f32
                } else {
                    60.0
                };
                let sig_bottom_pts = top_pts - render_h;

                let mut img_obj = page.objects_mut()
                    .create_image_object(
                        PdfPoints::new(left_pts),
                        PdfPoints::new(sig_bottom_pts),
                        &sig_img,
                        Some(PdfPoints::new(render_w)),
                        Some(PdfPoints::new(render_h)),
                    )
                    .with_context(|| "Failed adding signature image to page")?;

                img_obj.apply_matrix(matrix)
                    .with_context(|| "Failed applying transform matrix to signature image")?;
            }
            other => {
                bail!("Unsupported annotation kind: '{}'", other);
            }
        }

        page.regenerate_content()
            .with_context(|| "Failed regenerating content after annotation")?;

        progress(Progress {
            id: request.id.clone(),
            completed: (index + 1) as u32,
            total,
            message: format!("Applied '{}' annotation to page {}", annotation.kind, annotation.page),
        });
    }

    doc.save_to_file(&request.output)
        .with_context(|| format!("Failed saving annotated PDF to '{}'", request.output))?;

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!("Successfully applied {} annotations", total),
    })
}
