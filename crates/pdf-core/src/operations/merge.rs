use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_merge(
    pdfium: &Pdfium,
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    if request.inputs.is_empty() {
        bail!("Merge operation requires at least one input PDF document");
    }

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    let mut new_doc = pdfium
        .create_new_pdf()
        .context("Failed to create new PDF document for merge")?;

    // Determine total page count across all inputs for accurate progress reporting
    let mut input_docs = Vec::new();
    let mut total_pages: u32 = 0;

    for inp in &request.inputs {
        if cancel.load(Ordering::Relaxed) {
            bail!("Merge operation cancelled");
        }
        let pwd = request
            .input_passwords
            .get(inp)
            .map(|s| s.as_str())
            .or(request.password.as_deref());
        let doc = pdfium
            .load_pdf_from_file(inp, pwd)
            .with_context(|| format!("Failed to open input document '{}'", inp))?;
        let count = doc.pages().len() as u32;
        total_pages += count;
        input_docs.push((inp, doc));
    }

    let mut completed: u32 = 0;
    progress(Progress {
        id: request.id.clone(),
        completed: 0,
        total: total_pages,
        message: format!("Starting merge of {} documents ({} pages)...", request.inputs.len(), total_pages),
    });

    for (inp_name, doc) in &input_docs {
        let page_count = doc.pages().len();
        for page_idx in 0..page_count {
            if cancel.load(Ordering::Relaxed) {
                bail!("Merge operation cancelled");
            }

            let dest_idx = new_doc.pages().len();
            new_doc
                .pages_mut()
                .copy_page_from_document(doc, page_idx, dest_idx)
                .with_context(|| {
                    format!(
                        "Failed copying page {} from '{}'",
                        page_idx + 1,
                        inp_name
                    )
                })?;

            completed += 1;
            progress(Progress {
                id: request.id.clone(),
                completed,
                total: total_pages,
                message: format!("Merged page {} of {}", completed, total_pages),
            });
        }
    }

    new_doc
        .save_to_file(&request.output)
        .with_context(|| format!("Failed to save merged PDF to '{}'", request.output))?;

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!(
            "Successfully merged {} documents ({} pages total)",
            request.inputs.len(),
            total_pages
        ),
    })
}
