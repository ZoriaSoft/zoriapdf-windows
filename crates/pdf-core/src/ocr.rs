use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locates the Tesseract OCR executable.
///
/// Search order (first hit wins):
/// 1. `ZORIAPDF_OCR_DIR` — explicit override.
/// 2. `%LOCALAPPDATA%\ZoriaPDF\ocr` — the optional component the user installed.
/// 3. The bundled `resources/ocr` folder (offline installer variant).
/// 4. A system-wide Tesseract installation.
pub fn find_tesseract() -> Result<Option<PathBuf>> {
    if let Ok(path_str) = std::env::var("ZORIAPDF_OCR_DIR") {
        let candidate = PathBuf::from(path_str).join("tesseract.exe");
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    // Engine-discovery/startup recovery:
    // If a previous installation swap was interrupted between renames, recover
    // the valid backup before checking the default installation location.
    crate::ocr_install::restore_default_backup_if_needed()?;

    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let candidate = PathBuf::from(local)
            .join("ZoriaPDF")
            .join("ocr")
            .join("tesseract.exe");
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    if let Ok(resources) = std::env::var("ZORIAPDF_RESOURCES") {
        let bundled = PathBuf::from(resources).join("ocr").join("tesseract.exe");
        if bundled.is_file() {
            return Ok(Some(bundled));
        }
    }

    // System-wide installations.
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let win_path = dir.join("tesseract.exe");
            if win_path.is_file() {
                return Ok(Some(win_path));
            }
        }
    }

    #[cfg(windows)]
    {
        for candidate in [
            r"C:\Program Files\Tesseract-OCR\tesseract.exe",
            r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
        ] {
            let path = PathBuf::from(candidate);
            if path.is_file() {
                return Ok(Some(path));
            }
        }
    }

    Ok(None)
}

/// Reports whether local OCR is available and where it was found.
pub fn ocr_status() -> Result<(bool, Option<PathBuf>)> {
    let path = find_tesseract()?;
    Ok((path.is_some(), path))
}

/// Directory that holds the language models for the given engine.
pub fn tessdata_dir_for(engine: &Path) -> Option<PathBuf> {
    let candidate = engine.parent()?.join("tessdata");
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

/// Path Tesseract writes for `output_base`: it appends `.txt` verbatim, so a base
/// such as `rapor.v2_ocr_0001` must not have its "extension" replaced.
pub fn tesseract_text_path(output_base: &Path) -> PathBuf {
    let mut name = output_base.as_os_str().to_owned();
    name.push(".txt");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::tesseract_text_path;
    use std::path::Path;

    #[test]
    fn text_path_keeps_dotted_stems() {
        assert_eq!(tesseract_text_path(Path::new("out/rapor.v2_ocr_0001")), Path::new("out/rapor.v2_ocr_0001.txt"));
        assert_eq!(tesseract_text_path(Path::new("page")), Path::new("page.txt"));
    }
}

/// Executes Tesseract on an input image, saving OCR text to `{output_base}.txt`.
pub fn run_tesseract_ocr(
    tesseract_exe: &Path,
    image_path: &Path,
    output_base: &Path,
) -> Result<PathBuf> {
    let mut cmd = Command::new(tesseract_exe);
    cmd.arg(image_path).arg(output_base);

    // Turkish + English with a graceful fallback to the default language set.
    cmd.args(["-l", "tur+eng"]);

    // Keep the models next to the engine when the install carries its own tessdata.
    if let Some(models) = tessdata_dir_for(tesseract_exe) {
        cmd.env("TESSDATA_PREFIX", models);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = cmd.output();
    let success = matches!(&output, Ok(o) if o.status.success());

    if !success {
        let mut retry_cmd = Command::new(tesseract_exe);
        retry_cmd.arg(image_path).arg(output_base);
        if let Some(models) = tessdata_dir_for(tesseract_exe) {
            retry_cmd.env("TESSDATA_PREFIX", models);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            retry_cmd.creation_flags(0x08000000);
        }

        let retry_out = retry_cmd
            .output()
            .with_context(|| format!("Failed to run Tesseract executable at '{}'", tesseract_exe.display()))?;

        if !retry_out.status.success() {
            let stderr = String::from_utf8_lossy(&retry_out.stderr);
            bail!("Tesseract OCR failed: {}", stderr.trim());
        }
    }

    let txt_path = tesseract_text_path(output_base);
    if txt_path.is_file() {
        Ok(txt_path)
    } else {
        bail!(
            "Tesseract ran but expected output file '{}' was not created",
            txt_path.display()
        );
    }
}
