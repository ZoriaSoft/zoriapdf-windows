use anyhow::{bail, Context, Result};
use lopdf::Document;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_compress(
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Compress operation requires an input PDF document"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    if cancel.load(Ordering::Relaxed) {
        bail!("Compress operation cancelled");
    }

    progress(Progress {
        id: request.id.clone(),
        completed: 0,
        total: 3,
        message: "Loading document structure...".into(),
    });

    let password = request
        .input_passwords
        .get(input_path)
        .map(|s| s.as_str())
        .or(request.password.as_deref())
        .unwrap_or("");

    let mut doc = Document::load_with_password(input_path, password)
        .with_context(|| format!("Failed opening input document '{}'", input_path))?;

    if doc.trailer.has(b"Encrypt") {
        bail!("Input PDF requires a valid password");
    }

    if cancel.load(Ordering::Relaxed) {
        bail!("Compress operation cancelled");
    }

    progress(Progress {
        id: request.id.clone(),
        completed: 1,
        total: 3,
        message: "Pruning unused and orphaned PDF objects...".into(),
    });

    let pruned = doc.prune_objects();
    doc.renumber_objects();

    if cancel.load(Ordering::Relaxed) {
        bail!("Compress operation cancelled");
    }

    progress(Progress {
        id: request.id.clone(),
        completed: 2,
        total: 3,
        message: "Applying Flate stream compression...".into(),
    });

    doc.compress();

    doc.save(&request.output)
        .with_context(|| format!("Failed saving compressed PDF to '{}'", request.output))?;

    let orig_size = std::fs::metadata(input_path).map(|m| m.len()).unwrap_or(0);
    let new_size = std::fs::metadata(&request.output).map(|m| m.len()).unwrap_or(0);

    let saved_pct = if orig_size > 0 && new_size < orig_size {
        ((orig_size - new_size) as f64 / orig_size as f64) * 100.0
    } else {
        0.0
    };

    progress(Progress {
        id: request.id.clone(),
        completed: 3,
        total: 3,
        message: format!(
            "Compressed from {} KB to {} KB ({:.1}% reduction)",
            orig_size / 1024,
            new_size / 1024,
            saved_pct
        ),
    });

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: format!(
            "Lossless compression complete (pruned {} unreferenced objects, size: {} KB -> {} KB, {:.1}% saved)",
            pruned.len(),
            orig_size / 1024,
            new_size / 1024,
            saved_pct
        ),
    })
}
