use anyhow::{bail, Context, Result};
use pdf_core::{Engine, OperationRequest};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn read_input_payload(request_path: Option<&Path>) -> Result<String> {
    if let Some(path) = request_path {
        if path == Path::new("-") {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("Failed reading request from stdin")?;
            Ok(buf)
        } else {
            std::fs::read_to_string(path)
                .with_context(|| format!("Failed reading request file '{}'", path.display()))
        }
    } else {
        // Fallback: read up to 8MB from stdin
        let mut buf = String::new();
        std::io::stdin()
            .take(8 * 1024 * 1024)
            .read_to_string(&mut buf)
            .context("Failed reading request payload from stdin")?;
        Ok(buf)
    }
}

fn run_worker() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut pdfium_path: Option<PathBuf> = None;
    let mut request_file: Option<PathBuf> = None;
    let mut pdf_command_action: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pdfium" => {
                if let Some(val) = args.next() {
                    pdfium_path = Some(PathBuf::from(val));
                }
            }
            "--request" => {
                if let Some(val) = args.next() {
                    request_file = Some(PathBuf::from(val));
                }
            }
            "--pdf-command" => {
                if let Some(action) = args.next() {
                    pdf_command_action = Some(action);
                }
                if let Some(path) = args.next() {
                    pdfium_path = Some(PathBuf::from(path));
                }
            }
            _ => {}
        }
    }

    let pdfium = pdfium_path.unwrap_or_else(|| {
        if let Ok(res) = std::env::var("ZORIAPDF_RESOURCES") {
            PathBuf::from(res).join("pdfium.dll")
        } else {
            PathBuf::from("pdfium.dll")
        }
    });

    let raw_payload = read_input_payload(request_file.as_deref())?;

    // If an action was explicitly specified via --pdf-command (Tauri compatibility)
    if let Some(action) = pdf_command_action {
        let payload: Value = serde_json::from_str(&raw_payload)
            .context("Failed parsing JSON request payload")?;
        let engine = Engine::new(&pdfium)
            .with_context(|| format!("Failed initializing engine with Pdfium at '{}'", pdfium.display()))?;

        let path = PathBuf::from(payload.get("path").and_then(Value::as_str).unwrap_or(""));
        let password = payload.get("password").and_then(Value::as_str);
        let page = payload.get("page").and_then(Value::as_u64).unwrap_or(1) as u32;

        let result = match action.as_str() {
            "open" => serde_json::to_value(engine.open(&path, password)?)?,
            "render" => {
                let width = payload.get("width").and_then(Value::as_u64).unwrap_or(1200) as u32;
                serde_json::to_value(engine.render(&path, password, page, width)?)?
            }
            "text" => json!(engine.page_text(&path, password, page)?),
            "search" => {
                let query = payload.get("query").and_then(Value::as_str).unwrap_or("");
                serde_json::to_value(engine.search(&path, password, query)?)?
            }
            "perform" => {
                let operation: OperationRequest = serde_json::from_value(payload)?;
                let cancel = AtomicBool::new(false);
                serde_json::to_value(engine.perform(&operation, &cancel, |progress| {
                    if let Ok(line) = serde_json::to_string(&progress) {
                        println!("{line}");
                        let _ = std::io::stdout().flush();
                    }
                })?)?
            }
            other => bail!("Unknown worker command action: '{}'", other),
        };

        println!("{}", json!({ "result": result }));
        let _ = std::io::stdout().flush();
        return Ok(());
    }

    // Standard pdf-worker invocation: --pdfium PATH --request JSONFILE
    let operation: OperationRequest = serde_json::from_str(&raw_payload)
        .context("Failed deserializing OperationRequest from JSON payload")?;

    let engine = Engine::new(&pdfium)
        .with_context(|| format!("Failed initializing engine with Pdfium at '{}'", pdfium.display()))?;

    let cancel = AtomicBool::new(false);

    let result = engine.perform(&operation, &cancel, |progress| {
        if let Ok(line) = serde_json::to_string(&progress) {
            println!("{line}");
            let _ = std::io::stdout().flush();
        }
    })?;

    println!("{}", json!({ "result": result }));
    let _ = std::io::stdout().flush();
    Ok(())
}

fn main() {
    if let Err(err) = run_worker() {
        println!("{}", json!({ "error": format!("{err:#}") }));
        let _ = std::io::stdout().flush();
        std::process::exit(1);
    }
}
