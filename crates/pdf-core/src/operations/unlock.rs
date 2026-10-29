use anyhow::{bail, Context, Result};
use lopdf::Document;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_unlock(
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Unlock operation requires an input PDF document"))?;

    let pwd = request
        .input_passwords
        .get(input_path)
        .map(|s| s.as_str())
        .or(request.password.as_deref())
        .ok_or_else(|| anyhow::anyhow!("Unlock operation requires 'password' parameter"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    if cancel.load(Ordering::Relaxed) {
        bail!("Unlock operation cancelled");
    }

    progress(Progress {
        id: request.id.clone(),
        completed: 0,
        total: 1,
        message: "Decrypting document with password...".into(),
    });

    let mut doc = Document::load_with_password(input_path, pwd)
        .with_context(|| format!("Failed to unlock PDF document with provided password: '{}'", input_path))?;

    if doc.trailer.has(b"Encrypt") {
        bail!("Input PDF requires a valid password");
    }

    if cancel.load(Ordering::Relaxed) {
        bail!("Unlock operation cancelled");
    }

    doc.save(&request.output)
        .with_context(|| format!("Failed to save unlocked PDF to '{}'", request.output))?;

    progress(Progress {
        id: request.id.clone(),
        completed: 1,
        total: 1,
        message: "Document unlocked and saved without encryption".into(),
    });

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: "Successfully removed password protection from document".into(),
    })
}
