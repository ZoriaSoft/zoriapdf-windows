//! Optional OCR component: download, verify and unpack the local OCR engine.
//!
//! The download uses the Windows HTTP stack (WinHTTP/Schannel), so the app needs no
//! bundled TLS library and the core installer stays small. The archive is verified
//! against a pinned SHA-256 before anything is written to disk.
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::scratch::ScratchDir;
use crate::types::{OperationRequest, OperationResult, Progress};

/// Default download location for the optional component.
pub fn default_ocr_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("ZoriaPDF").join("ocr")
}


/// Checks whether an OCR installation directory contains a valid engine and language models.
fn is_valid_ocr_installation(dir: &Path) -> bool {
    let engine = dir.join("tesseract.exe");
    if !engine.is_file() {
        return false;
    }
    let tessdata = dir.join("tessdata");
    if !tessdata.is_dir() {
        return false;
    }
    verify_engine(dir).is_ok()
}

/// Restores the default OCR installation from its backup if the target is absent and backup is valid.
pub fn restore_default_backup_if_needed() -> Result<bool> {
    restore_backup_for(&default_ocr_dir())
}

pub(crate) fn restore_backup_for(target_dir: &Path) -> Result<bool> {
    let parent = match target_dir.parent() {
        Some(p) => p,
        None => return Ok(false),
    };
    let backup = parent.join("zoriapdf-ocr-backup");
    if target_dir.exists() || !backup.is_dir() { return Ok(false); }
    let lease = install_lock_file(parent)?;
    match lease.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(false),
        Err(std::fs::TryLockError::Error(error)) => return Err(error).context("Cannot lock OCR recovery"),
    }
    if !target_dir.exists() && backup.is_dir() {
        if is_valid_ocr_installation(&backup) {
            fs::rename(&backup, target_dir)
                .with_context(|| format!("OCR yedeği geri yüklenemedi: '{}'", backup.display()))?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn install_lock_file(parent: &Path) -> Result<fs::File> {
    fs::OpenOptions::new().create(true).truncate(false).read(true).write(true)
        .open(parent.join(".ocr-install.lock")).context("Cannot open OCR installation lock")
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("Cannot open '{}'", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(windows)]
mod download {
    use anyhow::{bail, Context, Result};
    use std::ffi::c_void;
    use std::fs;
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
        WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
        WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
        WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    };

    fn wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// Downloads `url` into `target`, reporting transferred bytes through `on_progress`.
    pub fn download(
        url: &str,
        target: &Path,
        cancel: &std::sync::atomic::AtomicBool,
        on_progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<()> {
        let without_scheme = url
            .strip_prefix("https://")
            .ok_or_else(|| anyhow::anyhow!("OCR indirme adresi https olmalı"))?;
        let (host, path) = match without_scheme.find('/') {
            Some(index) => (&without_scheme[..index], &without_scheme[index..]),
            None => (without_scheme, "/"),
        };

        let session: *mut c_void = unsafe {
            WinHttpOpen(
                wide("ZoriaPDF").as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                0,
            )
        };
        if session.is_null() {
            bail!("Windows HTTP istemcisi başlatılamadı");
        }
        unsafe {
            WinHttpSetTimeouts(session, 30_000, 30_000, 60_000, 120_000);
        }

        let connect: *mut c_void = unsafe { WinHttpConnect(session, wide(host).as_ptr(), 443, 0) };
        if connect.is_null() {
            unsafe { WinHttpCloseHandle(session) };
            bail!("OCR indirme sunucusuna bağlanılamadı");
        }

        let request: *mut c_void = unsafe {
            WinHttpOpenRequest(
                connect,
                wide("GET").as_ptr(),
                wide(path).as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        };
        if request.is_null() {
            unsafe {
                WinHttpCloseHandle(connect);
                WinHttpCloseHandle(session);
            }
            bail!("OCR indirme isteği oluşturulamadı");
        }

        let outcome = (|| -> Result<()> {
            if unsafe { WinHttpSendRequest(request, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) } == 0 {
                bail!("OCR paketi isteği gönderilemedi");
            }
            if unsafe { WinHttpReceiveResponse(request, std::ptr::null_mut()) } == 0 {
                bail!("OCR paketi yanıtı alınamadı");
            }

            let mut status: u32 = 0;
            let mut status_size = std::mem::size_of::<u32>() as u32;
            let ok = unsafe {
                WinHttpQueryHeaders(
                    request,
                    (WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER) as u32,
                    std::ptr::null(),
                    (&mut status as *mut u32).cast(),
                    &mut status_size,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 || status != 200 {
                bail!("OCR paketi indirilemedi (sunucu {} yanıtı verdi)", status);
            }

            let mut total: u32 = 0;
            let mut total_size = std::mem::size_of::<u32>() as u32;
            unsafe {
                WinHttpQueryHeaders(
                    request,
                    (WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER) as u32,
                    std::ptr::null(),
                    (&mut total as *mut u32).cast(),
                    &mut total_size,
                    std::ptr::null_mut(),
                )
            };

            let file = fs::File::create(target)
                .with_context(|| format!("Cannot create '{}'", target.display()))?;
            let mut writer = std::io::BufWriter::new(file);
            let mut buffer = vec![0u8; 64 * 1024];
            let mut received: u64 = 0;
            let mut reported: u64 = 0;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    bail!("OCR indirmesi iptal edildi");
                }
                let mut available: u32 = 0;
                if unsafe { WinHttpQueryDataAvailable(request, &mut available) } == 0 {
                    bail!("OCR paketi verisi alınamadı");
                }
                if available == 0 {
                    break;
                }
                let mut read: u32 = 0;
                let chunk = available.min(buffer.len() as u32);
                if unsafe { WinHttpReadData(request, buffer.as_mut_ptr().cast(), chunk, &mut read) } == 0 {
                    bail!("OCR paketi okunamadı");
                }
                if read == 0 {
                    break;
                }
                writer.write_all(&buffer[..read as usize])?;
                received += read as u64;
                // One report per megabyte keeps the UI responsive without flooding it.
                if received - reported >= 1024 * 1024 {
                    reported = received;
                    on_progress(received, if total > 0 { Some(total as u64) } else { None });
                }
            }
            writer.flush()?;
            Ok(())
        })();

        unsafe {
            WinHttpCloseHandle(request);
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
        }
        outcome
    }
}

#[cfg(not(windows))]
mod download {
    use anyhow::{bail, Result};
    use std::path::Path;
    pub fn download(
        _url: &str,
        _target: &Path,
        _cancel: &std::sync::atomic::AtomicBool,
        _on_progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<()> {
        bail!("OCR bileşeni yalnız Windows'ta kurulabilir")
    }
}

/// Installs the optional OCR component into `target_dir`.
pub fn install_ocr_pack(
    id: &str,
    url: &str,
    expected_sha256: &str,
    target_dir: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    // Attempt recovery of any previous backup before downloading
    restore_backup_for(target_dir)?;

    let parent = target_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("OCR hedef dizini geçersiz"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Cannot create '{}'", parent.display()))?;
    let parent = parent.canonicalize().context("Cannot resolve OCR installation parent")?;

    // Move OCR download and unpack intermediates into scratch so host forced cancel
    // can remove its owned TMP/TEMP tree and Drop cleans up on failure.
    let mut scratch = ScratchDir::new()?;
    let archive = scratch.path().join("zoriapdf-ocr-download.zip");
    progress(Progress {
        id: id.to_owned(),
        completed: 0,
        total: 100,
        message: "OCR paketi indiriliyor...".into(),
    });

    let mut on_bytes = |received: u64, total: Option<u64>| {
        let percent = total
            .filter(|t| *t > 0)
            .map(|t| ((received as f64 / t as f64) * 50.0) as u32)
            .unwrap_or(0);
        progress(Progress {
            id: id.to_owned(),
            completed: percent,
            total: 100,
            message: format!(
                "OCR paketi indiriliyor... {:.1} MB",
                received as f64 / 1048576.0
            ),
        });
    };
    download::download(url, &archive, cancel, &mut on_bytes)?;

    let actual = sha256_file(&archive)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = fs::remove_file(&archive);
        bail!(
            "OCR paketi doğrulanamadı (beklenen {}, gelen {}). Dosya silindi.",
            expected_sha256,
            actual
        );
    }

    progress(Progress {
        id: id.to_owned(),
        completed: 60,
        total: 100,
        message: "OCR paketi kuruluyor...".into(),
    });

    // Extraction and final publication stay on one volume; never copy a
    // half-installed component into its live directory.
    let stage_root = scratch.output_stage(&parent)?;
    crate::fs_security::make_private(&stage_root)?;
    let staging = stage_root.join("ocr");
    fs::create_dir(&staging)?;
    {
        use zip::ZipArchive;
        let file = fs::File::open(&archive)?;
        let mut archive = ZipArchive::new(file)?;
        for index in 0..archive.len() {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                drop(archive);
                bail!("OCR kurulumu iptal edildi");
            }
            let mut entry = archive.by_index(index)?;
            let Some(relative) = entry.enclosed_name() else {
                bail!("OCR paketi güvenli olmayan bir yol içeriyor");
            };
            let destination = staging.join(&relative);
            if entry.is_dir() {
                fs::create_dir_all(&destination)?;
                continue;
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = fs::File::create(&destination)?;
            std::io::copy(&mut entry, &mut output)?;
        }
    }
    let _ = fs::remove_file(&archive);

    let engine = staging.join("tesseract.exe");
    if !engine.is_file() {
        bail!("OCR paketi beklenen motor dosyasını içermiyor");
    }

    progress(Progress {
        id: id.to_owned(),
        completed: 95,
        total: 100,
        message: "OCR motoru doğrulanıyor...".into(),
    });
    verify_engine(&staging)?;
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        bail!("OCR kurulumu iptal edildi");
    }

    let lease = install_lock_file(&parent)?;
    lease.lock().context("Cannot lock OCR installation")?;
    let backup = parent.join("zoriapdf-ocr-backup");
    if backup.exists() {
        if target_dir.exists() || !is_valid_ocr_installation(&backup) {
            fs::remove_dir_all(&backup).context("Cannot remove previous OCR backup")?;
        } else {
            fs::rename(&backup, target_dir).context("Cannot recover previous OCR installation")?;
        }
    }

    let had_previous = target_dir.exists();
    if had_previous {
        fs::rename(target_dir, &backup)?;
    }

    if let Err(error) = fs::rename(&staging, target_dir) {
        if had_previous {
            fs::rename(&backup, target_dir)
                .with_context(|| format!("OCR installation failed ({error}); previous component could not be restored"))?;
        }
        return Err(error).context("OCR bileşeni yerleştirilemedi");
    }
    if had_previous {
        fs::remove_dir_all(&backup).context("Cannot remove previous OCR component backup")?;
    }
    scratch.cleanup()?;

    progress(Progress {
        id: id.to_owned(),
        completed: 100,
        total: 100,
        message: "Yerel OCR hazır".into(),
    });
    Ok(OperationResult {
        outputs: vec![target_dir.to_string_lossy().to_string()],
        message: "Yerel OCR motoru kuruldu (Türkçe + İngilizce)".into(),
    })
}

/// Checks that the installed engine starts and exposes both language models.
pub fn verify_engine(target_dir: &Path) -> Result<()> {
    let engine = target_dir.join("tesseract.exe");
    let mut command = std::process::Command::new(&engine);
    command.arg("--list-langs");
    command.env("TESSDATA_PREFIX", target_dir.join("tessdata"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command
        .output()
        .with_context(|| format!("Cannot start '{}'", engine.display()))?;
    if !output.status.success() {
        bail!("OCR motoru başlatılamadı");
    }
    let languages = String::from_utf8_lossy(&output.stdout);
    for required in ["eng", "tur"] {
        if !languages.lines().any(|line| line.trim() == required) {
            bail!("OCR kurulumunda '{required}' dil modeli yok");
        }
    }
    Ok(())
}

/// Operation entry point used by the host command.
pub fn op_install_ocr(
    request: &OperationRequest,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress),
) -> Result<OperationResult> {
    let url = request
        .download_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("OCR indirme adresi eksik"))?;
    let expected = request
        .sha256
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("OCR paketi doğrulama anahtarı eksik"))?;
    if url != "https://www.zoriapdf.com/downloads/zoriapdf-ocr-1.0.0.zip"
        || expected != "935c6dc6f7dd03439e9e3c55597802b0983808f1d3692e7da12ce77ca7f1cfd4"
        || request.target_dir.is_some()
    {
        bail!("Yalnız uygulamanın sabitlenmiş OCR bileşeni kurulabilir");
    }
    install_ocr_pack(&request.id, url, expected, &default_ocr_dir(), cancel, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_backup_is_preserved_without_being_promoted() {
        let temp = ScratchDir::new().unwrap();
        let target = temp.path().join("ocr");
        let backup = temp.path().join("zoriapdf-ocr-backup");
        fs::create_dir(&backup).unwrap();
        fs::write(backup.join("notes.txt"), b"incomplete component diagnostics").unwrap();
        assert!(!restore_backup_for(&target).unwrap());
        assert!(!target.exists());
        assert_eq!(fs::read(backup.join("notes.txt")).unwrap(), b"incomplete component diagnostics");
    }

    #[test]
    fn recovery_does_not_replace_an_existing_component_directory() {
        let temp = ScratchDir::new().unwrap();
        let target = temp.path().join("ocr");
        let backup = temp.path().join("zoriapdf-ocr-backup");
        fs::create_dir(&target).unwrap();
        fs::create_dir(&backup).unwrap();
        fs::write(target.join("keep.txt"), b"current installation").unwrap();
        fs::write(backup.join("keep.txt"), b"previous installation").unwrap();
        assert!(!restore_backup_for(&target).unwrap());
        assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"current installation");
        assert_eq!(fs::read(backup.join("keep.txt")).unwrap(), b"previous installation");
    }
}
