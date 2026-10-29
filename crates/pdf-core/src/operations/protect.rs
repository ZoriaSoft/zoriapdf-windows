use anyhow::{bail, Context, Result};
use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{Document, EncryptionState, EncryptionVersion, Permissions};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::{check_collision, ensure_parent_dir};
use crate::types::{OperationRequest, OperationResult, Progress};

pub fn op_protect(
    request: &OperationRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let input_path = request
        .inputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("Protect operation requires an input PDF document"))?;

    let new_pwd = request
        .new_password
        .as_deref()
        .or(request.password.as_deref())
        .filter(|p| !p.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Protect operation requires a non-empty 'newPassword'"))?;

    check_collision(&request.inputs, &request.output, false)?;
    ensure_parent_dir(Path::new(&request.output))?;

    if cancel.load(Ordering::Relaxed) {
        bail!("Protect operation cancelled");
    }

    progress(Progress {
        id: request.id.clone(),
        completed: 0,
        total: 1,
        message: "Loading document for encryption...".into(),
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
        bail!("Protect operation cancelled");
    }

    progress(Progress {
        id: request.id.clone(),
        completed: 0,
        total: 1,
        message: "Applying AES-256 PDF encryption...".into(),
    });

    ensure_file_id(&mut doc)?;
    let state = aes256_state(new_pwd)?;

    doc.encrypt(&state)
        .map_err(|e| anyhow::anyhow!("Failed applying encryption to document objects: {:?}", e))?;

    doc.save(&request.output)
        .with_context(|| format!("Failed saving encrypted PDF to '{}'", request.output))?;

    progress(Progress {
        id: request.id.clone(),
        completed: 1,
        total: 1,
        message: "Document encrypted successfully".into(),
    });

    Ok(OperationResult {
        outputs: vec![request.output.clone()],
        message: "Successfully protected document with password".into(),
    })
}

/// Builds an AES-256 (PDF 2.0, V5/R6) encryption state with a fresh random file key.
///
/// RC4-128 (V2) is obsolete and trivially attacked; R6 is what current readers
/// (Acrobat, PDFium, qpdf, pdf.js) expect for new documents.
pub fn aes256_state(password: &str) -> Result<EncryptionState> {
    let mut file_key = [0u8; 32];
    getrandom::fill(&mut file_key)
        .map_err(|e| anyhow::anyhow!("Secure random source unavailable: {e}"))?;
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let version = EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &file_key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: password,
        user_password: password,
        permissions: Permissions::PRINTABLE
            | Permissions::COPYABLE
            | Permissions::COPYABLE_FOR_ACCESSIBILITY
            | Permissions::PRINTABLE_IN_HIGH_QUALITY,
    };
    EncryptionState::try_from(version)
        .map_err(|e| anyhow::anyhow!("Failed initializing PDF encryption parameters: {:?}", e))
}

/// Readers and older revisions key off the trailer /ID; give documents without one a random ID.
pub fn ensure_file_id(doc: &mut Document) -> Result<()> {
    if doc.trailer.get(b"ID").is_ok() {
        return Ok(());
    }
    let mut id = [0u8; 16];
    getrandom::fill(&mut id).map_err(|e| anyhow::anyhow!("Secure random source unavailable: {e}"))?;
    let part = lopdf::Object::String(id.to_vec(), lopdf::StringFormat::Hexadecimal);
    doc.trailer.set("ID", lopdf::Object::Array(vec![part.clone(), part]));
    Ok(())
}
