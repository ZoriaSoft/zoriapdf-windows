#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Arc, Mutex},
};
use pdf_core::{output::OutputTransaction, scratch::ScratchDir, Engine, OperationRequest, OperationResult};
use serde_json::{json, Value};
use tauri::{Emitter, Manager, State};

#[cfg(windows)]
mod printing;
mod process_guard;

/// Opt-in diagnostics for the reader worker (`ZORIAPDF_DEBUG=1`).
fn reader_debug(message: impl std::fmt::Display) {
    if std::env::var_os("ZORIAPDF_DEBUG").is_none() { return; }
    let path = std::env::temp_dir().join("zoriapdf-reader.log");
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {}", now_seconds(), message);
    }
}

#[derive(Clone)]
struct OpenDocument { path: PathBuf, password: Option<String> }
enum JobPhase {
    Starting,
    Running {
        child: Arc<Mutex<Child>>,
        containment: Arc<process_guard::Containment>,
    },
}
struct ActiveJob {
    id: String,
    phase: JobPhase,
    cancelled: Arc<AtomicBool>,
}

enum JobFiles {
    Output(OutputTransaction),
    Component(ScratchDir),
}

impl JobFiles {
    fn new(request: &OperationRequest) -> Result<Self, String> {
        if request.kind.eq_ignore_ascii_case("install-ocr") {
            ScratchDir::new().map(Self::Component)
        } else {
            OutputTransaction::new(request).map(Self::Output)
        }.map_err(|error| format!("{error:#}"))
    }

    fn scratch_path(&self) -> &Path {
        match self {
            Self::Output(output) => output.scratch_path(),
            Self::Component(scratch) => scratch.path(),
        }
    }

    fn staged_request(&self, request: &OperationRequest) -> OperationRequest {
        match self {
            Self::Output(output) => output.staged_request(request),
            Self::Component(_) => request.clone(),
        }
    }

    fn publish(&mut self, result: OperationResult, cancel: &AtomicBool) -> Result<OperationResult, String> {
        match self {
            Self::Output(output) => output.commit(result, cancel).map_err(|error| format!("{error:#}")),
            Self::Component(_) if cancel.load(Ordering::Acquire) => Err("İşlem iptal edildi.".into()),
            Self::Component(_) => Ok(result),
        }
    }

    fn cleanup(&mut self) -> Result<(), String> {
        match self {
            Self::Output(output) => output.cleanup(),
            Self::Component(scratch) => scratch.cleanup(),
        }.map_err(|error| format!("Geçici işlem dosyaları temizlenemedi: {error:#}"))
    }
}

/// A long-lived PDF reader worker. Page rendering costs one process start plus PDFium
/// load; on Windows an unsigned executable is rescanned on every start, which dominated
/// page-flip latency. One persistent worker keeps that cost out of the interaction loop.
struct ReaderSession {
    child: Child,
    containment: process_guard::Containment,
    stdin: std::process::ChildStdin,
    responses: std::sync::mpsc::Receiver<(u64, Result<Value, String>)>,
    next_id: u64,
    last_used: Arc<AtomicU64>,
}

impl ReaderSession {
    fn spawn(resources: &Path) -> Result<Self, String> {
        let mut command = child_command(resources, "serve")?;
        // The reader reports failures as JSON on stdout; an undrained stderr pipe
        // could fill and deadlock the worker.
        command.stderr(Stdio::null());
        let mut child = command.spawn().map_err(|e| format!("PDF okuyucusu başlatılamadı: {e}"))?;
        reader_debug(format_args!("spawned reader pid={}", child.id()));
        let containment = process_guard::contain(&mut child)?;
        reader_debug("reader assigned to job");
        let stdin = child.stdin.take().ok_or("PDF okuyucusu kanalı açılamadı")?;
        let stdout = child.stdout.take().ok_or("PDF okuyucusu çıktısı açılamadı")?;
        let (sender, responses) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                reader_debug(format_args!("reader response bytes={}", line.len()));
                let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
                let id = value.get("id").and_then(Value::as_u64).unwrap_or(0);
                let outcome = match value.get("error").and_then(Value::as_str) {
                    Some(error) => Err(error.to_string()),
                    None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
                };
                if sender.send((id, outcome)).is_err() { break; }
            }
        });
        Ok(Self { child, containment, stdin, responses, next_id: 0, last_used: Arc::new(AtomicU64::new(now_seconds())) })
    }

    fn request(&mut self, action: &str, payload: Value) -> Result<Value, String> {
        self.last_used.store(now_seconds(), std::sync::atomic::Ordering::Relaxed);
        self.next_id += 1;
        let id = self.next_id;
        reader_debug(format_args!("request {id} action={action}"));
        let line = serde_json::to_string(&json!({"id": id, "action": action, "payload": payload}))
            .map_err(|e| e.to_string())?;
        writeln!(self.stdin, "{line}").map_err(|e| format!("PDF okuyucusu bağlantısı koptu: {e}"))?;
        self.stdin.flush().map_err(|e| format!("PDF okuyucusu bağlantısı koptu: {e}"))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() { return Err("PDF okuyucusu zaman aşımına uğradı.".into()); }
            match self.responses.recv_timeout(remaining) {
                Ok((received, outcome)) if received == id => return outcome,
                Ok(_) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err("PDF okuyucusu zaman aşımına uğradı.".into())
                }
                Err(_) => return Err("PDF okuyucusu kapandı.".into()),
            }
        }
    }
}

#[derive(Default)]
struct AppState {
    documents: Mutex<HashMap<String, OpenDocument>>,
    job: Mutex<Option<ActiveJob>>,
    // One reader worker serves all fast PDF calls; heavy operations use their own process.
    reader: Mutex<Option<ReaderSession>>,
    outputs: Mutex<Vec<PathBuf>>,
    initial: Mutex<Vec<String>>,
}

/// Runs a fast PDF call on the persistent reader, restarting a dead worker once.
fn reader_call(state: &AppState, resources: &Path, action: &str, payload: Value) -> Result<Value, String> {
    let mut slot = state.reader.lock().map_err(|_| "PDF okuyucu kilidi açılamadı")?;
    for attempt in 0..2 {
        if slot.is_none() {
            *slot = Some(ReaderSession::spawn(resources)?);
        }
        let reader = slot.as_mut().expect("reader slot filled above");
        match reader.request(action, payload.clone()) {
            Ok(value) => return Ok(value),
            Err(error) => {
                if error.contains("zaman aşımına") {
                    // A hung worker would block every later call; replace it.
                    if let Some(mut hung) = slot.take() { let _ = stop_child(&mut hung.child, &hung.containment); }
                    return Err(error);
                }
                let broken = error.starts_with("PDF okuyucusu bağlantısı koptu") || error.contains("kapandı");
                if attempt == 1 || !broken { return Err(error); }
                // The worker died: drop it and start a fresh one for the retry.
                if let Some(mut dead) = slot.take() { let _ = stop_child(&mut dead.child, &dead.containment); }
            }
        }
    }
    Err("PDF okuyucusu yanıt vermedi.".into())
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Releases the reader worker after an idle period so a parked app holds no PDFium memory.
fn start_reader_janitor(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        let state = app.state::<AppState>();
        let Ok(mut slot) = state.reader.lock() else { continue };
        let Some(reader) = slot.take() else { continue };
        let idle_seconds = now_seconds().saturating_sub(reader.last_used.load(std::sync::atomic::Ordering::Relaxed));
        if idle_seconds > 180 {
            let mut reader = reader;
            let _ = stop_child(&mut reader.child, &reader.containment);
        } else {
            *slot = Some(reader);
        }
    });
}

fn quiet_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW, no console flashes.
    }
    cmd
}
fn stop_child(child: &mut Child, containment: &process_guard::Containment) -> Result<(), String> {
    containment.terminate()?;
    if child.try_wait().map_err(|e| e.to_string())?.is_none() {
        child.kill().map_err(|e| e.to_string())?;
    }
    child.wait().map_err(|e| e.to_string())?;
    Ok(())
}

fn wait_child(child: &Mutex<Child>) -> Result<std::process::ExitStatus, String> {
    loop {
        if let Some(status) = child.lock().map_err(|_| "İşlem kanalı açılamadı")?
            .try_wait().map_err(|e| e.to_string())? {
            return Ok(status);
        }
        // Release the handle lock so cancellation can terminate a worker even
        // after it has closed stdout but before the process has exited.
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn resource_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let bundled = app.path().resource_dir().map_err(|e| e.to_string())?.join("resources");
    if bundled.join("pdfium.dll").is_file() { return Ok(bundled); }
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    if development.join("pdfium.dll").is_file() { return Ok(development); }
    Err("PDF motoru bulunamadı. Uygulamayı yeniden kurun (pdfium.dll).".into())
}
fn document(state: &AppState, id: &str) -> Result<OpenDocument, String> {
    state.documents.lock().map_err(|_| "Belge kilidi açılamadı")?
        .get(id).cloned().ok_or_else(|| "Belge kapatılmış. Yeniden açın.".into())
}

fn bind_input_passwords(state: &AppState, request: &mut OperationRequest) -> Result<(), String> {
    // An explicit source password wins over the host's cached fallback. Merge
    // still needs the password belonging to each selected input.
    if !request.kind.eq_ignore_ascii_case("merge") && request.password.is_some() {
        return Ok(());
    }
    for input in &request.inputs {
        let path = Path::new(input).canonicalize().map_err(|_| "Dosya bulunamadı veya erişilemiyor")?;
        let documents = state.documents.lock().map_err(|_| "Belge kaydı açılamadı")?;
        if let Some(password) = documents.values().find(|doc| doc.path == path).and_then(|doc| doc.password.as_ref()) {
            request.input_passwords.entry(input.clone()).or_insert_with(|| password.clone());
        }
    }
    if !request.kind.eq_ignore_ascii_case("merge") && request.password.is_none() {
        request.password = request.inputs.first().and_then(|input| request.input_passwords.get(input)).cloned();
    }
    Ok(())
}
fn child_command(resources: &Path, action: &str) -> Result<Command, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = quiet_command(exe);
    cmd.arg("--pdf-command").arg(action).arg(resources.join("pdfium.dll"))
        .env("ZORIAPDF_RESOURCES", resources)
        .env("TESSDATA_PREFIX", resources.join("tessdata"))
        .env("OMP_THREAD_LIMIT", "2")
        .env("PATH", format!("{};{}", resources.display(), std::env::var("PATH").unwrap_or_default()))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    Ok(cmd)
}
fn send_request(child: &mut Child, request: &Value) -> Result<(), String> {
    let mut stdin = child.stdin.take().ok_or("PDF işlem kanalı açılamadı")?;
    serde_json::to_writer(&mut stdin, request).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())
}
fn read_call(resources: PathBuf, action: &str, payload: Value) -> Result<Value, String> {
    let mut child = child_command(&resources, action)?.spawn().map_err(|e| e.to_string())?;
    let containment = process_guard::contain(&mut child)?;
    if let Err(e) = send_request(&mut child, &payload) { let _ = stop_child(&mut child, &containment); return Err(e); }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let response: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "PDF motoru belgeyi işleyemedi. Belge bozuk veya desteklenmeyen içerik taşıyor olabilir.".to_string())?;
    if let Some(error) = response.get("error").and_then(Value::as_str) { return Err(error.into()); }
    if !output.status.success() { return Err("PDF işlemi beklenmedik biçimde durdu.".into()); }
    response.get("result").cloned().ok_or_else(|| "PDF motorundan geçersiz yanıt".into())
}
#[tauri::command]
async fn open_documents(app: tauri::AppHandle, paths: Vec<String>, password: Option<String>) -> Result<Vec<Value>, String> {
    if paths.len() > 64 { return Err("Bir seferde en fazla 64 belge açabilirsiniz.".into()); }
    let resources = resource_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut opened = Vec::new();
        let mut pending: Vec<(String, OpenDocument)> = Vec::new();
        // All-or-nothing registry update: a password error cannot leak half-open tabs.
        for path in paths {
            let requested_path = path.clone();
            let path = PathBuf::from(path).canonicalize().map_err(|_| "Dosya bulunamadı veya erişilemiyor")?;
            if !path.is_file() { return Err("Bir PDF dosyası seçin.".into()); }
            let existing = state.documents.lock().map_err(|_| "Belge kaydı açılamadı")?
                .iter().find(|(_, doc)| doc.path == path).map(|(id, doc)| (id.clone(), doc.clone()))
                .or_else(|| pending.iter().find(|(_, doc)| doc.path == path).cloned());
            let effective_password = password.clone().or_else(|| existing.as_ref().and_then(|(_, doc)| doc.password.clone()));
            let mut info = reader_call(&state, &resources, "open", json!({"path":path,"password":effective_password}))
                .map_err(|message| json!({"path":requested_path,"message":message}).to_string())?;
            let id = existing.map(|(id, _)| id).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            info["id"] = json!(id);
            pending.push((id, OpenDocument { path, password: effective_password }));
            opened.push(info);
        }
        state.documents.lock().map_err(|_| "Belge kaydı açılamadı")?.extend(pending);
        Ok(opened)
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
fn close_document(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.documents.lock().map_err(|_| "Belge kaydı açılamadı")?.remove(&id);
    Ok(())
}
async fn document_call(app: tauri::AppHandle, id: String, action: &'static str, mut extra: Value) -> Result<Value, String> {
    let doc = document(&app.state::<AppState>(), &id)?;
    let resources = resource_dir(&app)?;
    extra["path"] = json!(doc.path);
    extra["password"] = json!(doc.password);
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if action == "print" {
            // Printing owns a modal dialog and blocks; keep it out of the shared reader.
            return read_call(resources, action, extra);
        }
        reader_call(&state, &resources, action, extra)
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn render_page(app: tauri::AppHandle, id: String, page: u32, width: u32) -> Result<Value, String> {
    if page == 0 || !(100..=2400).contains(&width) { return Err("Sayfa veya görüntü boyutu geçersiz.".into()); }
    document_call(app, id, "render", json!({"page":page,"width":width})).await
}
#[tauri::command]
async fn page_text(app: tauri::AppHandle, id: String, page: u32) -> Result<Value, String> {
    document_call(app, id, "text", json!({"page":page})).await
}
#[tauri::command]
async fn search_document(app: tauri::AppHandle, id: String, query: String) -> Result<Value, String> {
    if query.trim().is_empty() || query.len() > 512 { return Err("Arama metni 1–512 karakter olmalı.".into()); }
    document_call(app, id, "search", json!({"query":query})).await
}
#[tauri::command]
async fn run_operation(app: tauri::AppHandle, mut request: OperationRequest) -> Result<Value, String> {
    if request.id.is_empty() { return Err("İşlem kimliği eksik".into()); }
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<AppState>();
        let mut job = state.job.lock().map_err(|_| "İşlem kilidi açılamadı")?;
        if job.is_some() { return Err("Bir işlem zaten çalışıyor. Bitmesini bekleyin veya iptal edin.".into()); }
        // Reserve before any potentially blocking path resolution or staging.
        *job = Some(ActiveJob { id: request.id.clone(), phase: JobPhase::Starting,
            cancelled: cancelled.clone() });
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut files: Option<JobFiles> = None;
        let mut worker: Option<(Arc<Mutex<Child>>, Arc<process_guard::Containment>)> = None;
        let mut drain = None;
        let worker_result: Result<OperationResult, String> = (|| {
            if cancelled.load(Ordering::Acquire) { return Err("İşlem iptal edildi.".into()); }
            let _ = app.emit("job-progress", json!({"id":request.id,"completed":0,"total":1,"message":"İşlem hazırlanıyor..."}));
            let resources = resource_dir(&app)?;
            bind_input_passwords(&app.state::<AppState>(), &mut request)?;
            if cancelled.load(Ordering::Acquire) { return Err("İşlem iptal edildi.".into()); }
            files = Some(JobFiles::new(&request)?);
            let workspace = files.as_ref().ok_or("İşlem dosyaları açılamadı")?;
            let payload = serde_json::to_value(workspace.staged_request(&request)).map_err(|error| error.to_string())?;
            let mut command = child_command(&resources, "perform")?;
            command.env("TMP", workspace.scratch_path()).env("TEMP", workspace.scratch_path());
            if cancelled.load(Ordering::Acquire) { return Err("İşlem iptal edildi.".into()); }
            let mut child = command.spawn().map_err(|error| format!("PDF işlemi başlatılamadı: {error}"))?;
            let containment = Arc::new(process_guard::contain(&mut child)?);
            let stdin = child.stdin.take();
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let child = Arc::new(Mutex::new(child));
            worker = Some((child.clone(), containment.clone()));
            {
                let state = app.state::<AppState>();
                let mut job = state.job.lock().map_err(|_| "İşlem kilidi açılamadı")?;
                if cancelled.load(Ordering::Acquire) || !job.as_ref().is_some_and(|job| Arc::ptr_eq(&job.cancelled, &cancelled)) {
                    return Err("İşlem iptal edildi.".into());
                }
                job.as_mut().ok_or("İşlem kaydı bulunamadı")?.phase = JobPhase::Running {
                    child: child.clone(), containment,
                };
            }
            drain = Some(std::thread::spawn(move || {
                if let Some(mut stderr) = stderr { let _ = std::io::copy(&mut stderr, &mut std::io::sink()); }
            }));
            // No job/child mutex is held while writing a possibly full pipe.
            // Cancellation can terminate a worker still starting up under Windows.
            let mut stdin = stdin.ok_or("PDF işlem kanalı açılamadı")?;
            serde_json::to_writer(&mut stdin, &payload).map_err(|error| error.to_string())?;
            stdin.flush().map_err(|error| error.to_string())?;
            drop(stdin);
            let stdout = stdout.ok_or("İşlem çıktısı açılamadı")?;
            let mut final_result = None;
            for line in BufReader::new(stdout).lines() {
                let line = line.map_err(|error| error.to_string())?;
                let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
                if value.get("result").is_some() || value.get("error").is_some() { final_result = Some(value); }
                else if value.get("completed").is_some() { let _ = app.emit("job-progress", &value); }
            }
            let status = wait_child(&child)?;
            let value = final_result.ok_or("PDF motoru beklenmedik biçimde durdu; kaynak belge değiştirilmedi.")?;
            if let Some(error) = value.get("error").and_then(Value::as_str) { return Err(error.into()); }
            if !status.success() { return Err("PDF işlemi tamamlanamadı.".into()); }
            serde_json::from_value(value.get("result").cloned().ok_or("PDF çıktısı bulunamadı")?)
                .map_err(|error| error.to_string())
        })();
        // Descendants can inherit stderr. Stop the entire tree before joining
        // its drainer, including when the root reported a successful result.
        let stopped = match worker.as_ref() {
            Some((child, containment)) => child.lock().map_err(|_| "İşlem kanalı açılamadı".to_string())
                .and_then(|mut child| stop_child(&mut child, containment)),
            None => Ok(()),
        };
        if let Some(drain) = drain { let _ = drain.join(); }
        let state = app.state::<AppState>();
        let outcome = (|| {
            let mut job = state.job.lock().map_err(|_| "İşlem kaydı açılamadı")?;
            // A cancelled/retried request may reuse its string ID. Only the
            // particular reserved job is allowed to publish or clear this slot.
            let owns_slot = job.as_ref().is_some_and(|job| Arc::ptr_eq(&job.cancelled, &cancelled));
            let outcome = if cancelled.load(Ordering::Acquire) || !owns_slot {
                Err("İşlem iptal edildi.".into())
            } else {
                stopped.and_then(|()| worker_result).and_then(|result|
                    files.as_mut().ok_or_else(|| "İşlem dosyaları bulunamadı".to_string())?.publish(result, &cancelled))
            };
            if owns_slot { *job = None; }
            let result = outcome?;
            let mut outputs = state.outputs.lock().map_err(|_| "Çıktı kaydı açılamadı")?;
            outputs.extend(result.outputs.iter().map(PathBuf::from));
            serde_json::to_value(result).map_err(|error| error.to_string())
        })();
        let cleanup = match files.as_mut() { Some(files) => files.cleanup(), None => Ok(()) };
        match (outcome, cleanup) {
            (result, Ok(())) => result,
            (Ok(_), Err(cleanup)) => Err(cleanup),
            (Err(error), Err(cleanup)) => Err(format!("{error} {cleanup}")),
        }
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
fn cancel_operation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut job = state.job.lock().map_err(|_| "İşlem kilidi açılamadı")?;
    if let Some(active) = job.as_ref().filter(|job| job.id == id) {
        active.cancelled.store(true, Ordering::Release);
        match &active.phase {
            JobPhase::Starting => return Ok(()),
            JobPhase::Running { child, containment } => {
                let mut child = child.lock().map_err(|_| "İşlem kilidi açılamadı")?;
                stop_child(&mut child, containment)?;
            }
        }
    } else {
        return Err("İşlem kimliği eşleşmiyor.".into());
    }
    *job = None;
    Ok(())
}
/// Pinned optional OCR component.
const OCR_PACK_URL: &str = "https://www.zoriapdf.com/downloads/zoriapdf-ocr-1.0.0.zip";
const OCR_PACK_SHA256: &str = "935c6dc6f7dd03439e9e3c55597802b0983808f1d3692e7da12ce77ca7f1cfd4";
const OCR_PACK_SIZE_MB: u32 = 21;

#[tauri::command]
fn ocr_status(app: tauri::AppHandle) -> Result<Value, String> {
    let resources = resource_dir(&app)?;
    let (_, engine) = pdf_core::ocr::ocr_status().map_err(|error| format!("{error:#}"))?;
    let engine = engine.or_else(|| {
        let bundled = resources.join("ocr").join("tesseract.exe");
        bundled.is_file().then_some(bundled)
    });
    Ok(json!({
        "available": engine.is_some(),
        "enginePath": engine.map(|path| path.to_string_lossy().to_string()),
        "pack": {"url": OCR_PACK_URL, "sha256": OCR_PACK_SHA256, "sizeMb": OCR_PACK_SIZE_MB}
    }))
}

#[tauri::command]
fn initial_paths(state: State<'_, AppState>) -> Vec<String> {
    state.initial.lock().map(|mut p| std::mem::take(&mut *p)).unwrap_or_default()
}
#[tauri::command]
fn reveal_output(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let canonical = PathBuf::from(&path).canonicalize().map_err(|_| "Çıktı bulunamadı")?;
    let allowed = state.outputs.lock().map_err(|_| "Çıktı kaydı açılamadı")?.iter()
        .any(|p| p.canonicalize().ok().as_ref() == Some(&canonical));
    if !allowed { return Err("Yalnız bu oturumun ürettiği çıktılar gösterilebilir.".into()); }
    #[cfg(windows)] { quiet_command("explorer.exe").arg(format!("/select,{}", canonical.display())).spawn().map_err(|e| e.to_string())?; }
    Ok(())
}
#[tauri::command]
async fn print_document(app: tauri::AppHandle, id: String) -> Result<(), String> {
    document_call(app, id, "print", json!({})).await.map(|_| ())
}
fn pdf_arguments(args: impl Iterator<Item=String>) -> Vec<String> {
    args.filter(|p| Path::new(p).is_file() && Path::new(p).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))).collect()
}
fn worker_main(action: &str, pdfium: &Path) -> anyhow::Result<()> {
    let mut input = String::new();
    std::io::stdin().take(4 * 1024 * 1024).read_to_string(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    let engine = Engine::new(pdfium)?;
    let path = PathBuf::from(request.get("path").and_then(Value::as_str).unwrap_or(""));
    let password = request.get("password").and_then(Value::as_str);
    let page = request.get("page").and_then(Value::as_u64).unwrap_or(1) as u32;
    let result = match action {
        "open" => serde_json::to_value(engine.open(&path, password)?)?,
        "render" => serde_json::to_value(engine.render(&path, password, page, request.get("width").and_then(Value::as_u64).unwrap_or(1200) as u32)?)?,
        "text" => json!(engine.page_text(&path, password, page)?),
        "search" => serde_json::to_value(engine.search(&path, password, request.get("query").and_then(Value::as_str).unwrap_or(""))?)?,
        #[cfg(windows)]
        "print" => {
            printing::print(&engine, &path, password,
                request.get("printer").and_then(Value::as_str),
                request.get("output").and_then(Value::as_str))?;
            Value::Null
        },
        "perform" => {
            let operation: OperationRequest = serde_json::from_value(request)?;
            let cancel = AtomicBool::new(false);
            serde_json::to_value(engine.perform(&operation, &cancel, |progress| {
                if let Ok(line) = serde_json::to_string(&progress) { println!("{line}"); let _ = std::io::stdout().flush(); }
            })?)?
        },
        _ => anyhow::bail!("Bilinmeyen PDF işlemi"),
    };
    println!("{}", json!({"result":result}));
    Ok(())
}
/// Identity of a loaded document; a change in any field forces a reload.
#[derive(PartialEq)]
struct DocumentKey { path: PathBuf, password: Option<String>, size: u64, modified: Option<std::time::SystemTime> }
impl DocumentKey {
    fn of(path: &Path, password: Option<&str>) -> Self {
        let meta = std::fs::metadata(path).ok();
        Self {
            path: path.to_path_buf(),
            password: password.map(str::to_owned),
            size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: meta.and_then(|m| m.modified().ok()),
        }
    }
}

/// Long-lived reader mode used by the host: one JSON request per stdin line,
/// one JSON response per stdout line. Keeps PDFium warm without a process start
/// per page flip, while still isolating the engine from the UI process.
fn serve_main(pdfium: &Path) -> anyhow::Result<()> {
    let engine = Engine::new(pdfium)?;
    // Keep text/search documents warm. Raster calls own short-lived documents so
    // PDFium's decoded image streams cannot accumulate across viewport requests.
    let mut cached: Option<(DocumentKey, pdf_core::PdfDocument<'_>)> = None;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        // A malformed line must not take the whole reader down with it.
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            writeln!(stdout, "{}", json!({"id": Value::Null, "error": "Okuyucu isteği okunamadı"}))?;
            stdout.flush()?;
            continue;
        };
        // Echo the caller's id verbatim (it is a JSON number); coercing it to a
        // string here would make every response unmatchable.
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let action = request.get("action").and_then(Value::as_str).unwrap_or("").to_owned();
        let payload = request.get("payload").cloned().unwrap_or(Value::Null);
        let path = PathBuf::from(payload.get("path").and_then(Value::as_str).unwrap_or(""));
        let password = payload.get("password").and_then(Value::as_str);
        let page = payload.get("page").and_then(Value::as_u64).unwrap_or(1) as u32;
        let outcome: Result<Value, String> = match action.as_str() {
            "open" => {
                cached = None; // An explicit reopen must not retain an old PDFium handle.
                engine.open(&path, password).map_err(|error| error.to_string())
                    .and_then(|info| serde_json::to_value(info).map_err(|error| error.to_string()))
            },
            "render" => {
                cached = None;
                let width = payload.get("width").and_then(Value::as_u64).unwrap_or(1200) as u32;
                engine.render(&path, password, page, width.clamp(100, 2400))
                    .map_err(|e| format!("{e:#}"))
                    .and_then(|r| serde_json::to_value(r).map_err(|e| e.to_string()))
            },
            "text" | "search" => (|| -> Result<Value, String> {
                let key = DocumentKey::of(&path, password);
                if cached.as_ref().map(|(k, _)| k != &key).unwrap_or(true) {
                    cached = None; // release the old document before loading the next
                    let doc = engine.load(&path, password).map_err(|e| format!("{e:#}"))?;
                    cached = Some((key, doc));
                }
                let doc = &cached.as_ref().expect("document cached above").1;
                match action.as_str() {
                    "text" => Engine::page_text_loaded(doc, page).map(|t| json!(t)).map_err(|e| e.to_string()),
                    _ => {
                        let query = payload.get("query").and_then(Value::as_str).unwrap_or("");
                        serde_json::to_value(Engine::search_loaded(doc, query)).map_err(|e| e.to_string())
                    }
                }
            })(),
            other => Err(format!("Bilinmeyen okuyucu işlemi: '{}'", other)),
        };
        let response = match outcome {
            Ok(result) => json!({"id": id, "result": result}),
            Err(error) => json!({"id": id, "error": error}),
        };
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--pdf-command") {
        let action = args.get(2).map(String::as_str).unwrap_or("");
        let pdfium = args.get(3).map(PathBuf::from).unwrap_or_default();
        let outcome = if action == "serve" {
            serve_main(&pdfium)
        } else {
            worker_main(action, &pdfium)
        };
        if let Err(error) = outcome { println!("{}", json!({"error":format!("{error:#}")})); std::process::exit(1); }
        return;
    }
    let state = AppState::default();
    *state.initial.lock().unwrap() = pdf_arguments(args.into_iter().skip(1));
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let paths = pdf_arguments(argv.into_iter().skip(1));
            let _ = app.emit("open-paths", json!({"paths":paths}));
            if let Some(window) = app.get_webview_window("main") { let _ = window.unminimize(); let _ = window.set_focus(); }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(|_| {
            ScratchDir::recover_abandoned().map_err(|error|
                format!("Geçici işlem dosyaları temizlenemedi: {error:#}"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![open_documents,close_document,render_page,page_text,search_document,run_operation,cancel_operation,initial_paths,print_document,reveal_output,ocr_status])
        .build(tauri::generate_context!()).expect("ZoriaPDF başlatılamadı");
    start_reader_janitor(app.handle().clone());
    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            if let Ok(job) = app.state::<AppState>().job.lock() {
                if let Some(job) = job.as_ref() {
                    job.cancelled.store(true, Ordering::Release);
                    if let JobPhase::Running { child, containment } = &job.phase {
                        if let Ok(mut child) = child.lock() { let _ = stop_child(&mut child, containment); }
                    }
                }
            }
            if let Ok(mut slot) = app.state::<AppState>().reader.lock() {
                if let Some(mut reader) = slot.take() { let _ = stop_child(&mut reader.child, &reader.containment); }
            }
        }
    });
}
