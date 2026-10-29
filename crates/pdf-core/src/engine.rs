use anyhow::{bail, Context, Result};
use base64::Engine as _;
use image::ImageFormat;
use pdfium_render::prelude::*;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use uuid::Uuid;
use sha2::{Digest, Sha256};

use crate::collision::normalize_path;
use crate::operations::*;
use crate::output::OutputTransaction;
use crate::types::{
    DocumentInfo, OperationRequest, OperationResult, Progress, RenderedPage, SearchHit,
};

/// Upper bound on search hits returned for one query.
pub const MAX_SEARCH_HITS: usize = 200;

fn content_revision(file: &mut std::fs::File) -> Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let bytes = file.read(&mut buffer).context("Cannot fingerprint PDF contents")?;
        if bytes == 0 { break; }
        digest.update(&buffer[..bytes]);
    }
    Ok(format!("sha256:{:x}", digest.finalize()))
}

/// Case folding rule used when matching.
#[derive(Clone, Copy)]
enum Fold {
    /// Turkish: `İ`→`i`, `I`→`ı`.
    Turkish,
    /// Default Unicode lowercasing: `I`→`i`, `İ`→`i`.
    Default,
}

fn fold_char(ch: char, rule: Fold, out: &mut String) {
    match (ch, rule) {
        ('\u{130}', _) => out.push('i'),
        ('I', Fold::Turkish) => out.push('\u{131}'),
        ('\u{DF}' | '\u{1E9E}', _) => out.push_str("ss"),
        ('\u{3C2}', _) => out.push('\u{3C3}'),
        // Drop the combining dot that full lowercasing would leave behind.
        ('\u{307}', _) => {}
        _ => out.extend(ch.to_lowercase()),
    }
}

/// Folds `text`, returning the folded string and, for each folded byte, the source char index.
fn fold_with_origin(chars: &[char], rule: Fold) -> (String, Vec<usize>) {
    let mut folded = String::with_capacity(chars.len());
    let mut origin = Vec::with_capacity(chars.len());
    for (index, &ch) in chars.iter().enumerate() {
        let before = folded.len();
        fold_char(ch, rule, &mut folded);
        origin.extend(std::iter::repeat(index).take(folded.len() - before));
    }
    (folded, origin)
}

fn fold_str(text: &str, rule: Fold) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        fold_char(ch, rule, &mut out);
    }
    out
}

/// Case-insensitive search of `text` for `query`, returning context snippets.
///
/// Matching is Turkish-aware: both Turkish (`I`/`ı`, `İ`/`i`) and default Unicode
/// folding are tried and their matches merged, so `istanbul` finds `İSTANBUL` and
/// `ISTANBUL`, while `ışık` finds `IŞIK`. Works on characters, never raw byte
/// offsets, so multibyte text cannot cause a slicing panic.
pub fn find_snippets(text: &str, query: &str, limit: usize) -> Vec<String> {
    let query = query.trim();
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for rule in [Fold::Turkish, Fold::Default] {
        let needle = fold_str(query, rule);
        if needle.is_empty() {
            continue;
        }
        let (folded, origin) = fold_with_origin(&chars, rule);
        let mut from = 0;
        while let Some(found) = folded[from..].find(&needle) {
            let start = from + found;
            let end = start + needle.len();
            spans.push((origin[start], origin[end - 1] + 1));
            from = end;
        }
    }
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for span in spans {
        match merged.last() {
            Some(last) if span.0 < last.1 => {}
            _ => merged.push(span),
        }
    }
    merged
        .into_iter()
        .take(limit)
        .map(|(first, last)| {
            let lo = first.saturating_sub(40);
            let hi = (last + 60).min(chars.len());
            let window: String = chars[lo..hi].iter().collect();
            let clean = window.replace(['\r', '\n'], " ");
            format!(
                "{}{}{}",
                if lo > 0 { "..." } else { "" },
                clean.trim(),
                if hi < chars.len() { "..." } else { "" }
            )
        })
        .collect()
}

#[cfg(test)]
mod search_tests {
    use super::find_snippets;

    #[test]
    fn turkish_text_does_not_panic_and_matches_case_insensitively() {
        let text = "İSTANBUL şehrinde ğüşıöç İstanbul ve istanbul".repeat(3);
        let hits = find_snippets(&text, "istanbul", 50);
        assert!(hits.len() >= 3);
        assert_eq!(find_snippets(&text, "Şehrinde", 50).len(), 3);
    }

    #[test]
    fn multibyte_context_window_is_char_safe() {
        let text = format!("{}hedef{}", "ç".repeat(45), "ğ".repeat(70));
        let hits = find_snippets(&text, "HEDEF", 10);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].starts_with("...") && hits[0].ends_with("..."));
    }

    #[test]
    fn turkish_dotted_and_dotless_i() {
        assert_eq!(find_snippets("İSTANBUL", "istanbul", 9).len(), 1);
        assert_eq!(find_snippets("ISTANBUL", "istanbul", 9).len(), 1);
        assert_eq!(find_snippets("IŞIK", "ışık", 9).len(), 1);
        assert_eq!(find_snippets("ışık", "IŞIK", 9).len(), 1);
        assert_eq!(find_snippets("istanbul", "İSTANBUL", 9).len(), 1);
        // Different Turkish letters stay distinct.
        assert!(find_snippets("irmak", "ırmak", 9).is_empty());
    }

    #[test]
    fn unicode_folding_and_no_double_counting() {
        assert_eq!(find_snippets("Straße", "STRASSE", 9).len(), 1);
        assert_eq!(find_snippets("ΟΔΟΣ οδος", "οδοσ", 9).len(), 2);
        // Both fold rules match "abc"; it must be reported once.
        assert_eq!(find_snippets("abc", "ABC", 9).len(), 1);
    }

    #[test]
    fn limit_is_respected() {
        assert_eq!(find_snippets(&"ab ".repeat(500), "ab", 7).len(), 7);
        assert!(find_snippets("abc", "  ", 7).is_empty());
    }
}

/// Core PDF engine managing Pdfium bindings and high-level operations.
pub struct Engine {
    pdfium: Pdfium,
}

impl Engine {
    /// Initializes the engine by binding to the Pdfium library.
    ///
    /// Accepts a path directly to `pdfium.dll` / `libpdfium.so` or to a directory containing it.
    pub fn new(pdfium_path: &Path) -> Result<Self> {
        let candidate_path = if pdfium_path.is_file() {
            pdfium_path.to_path_buf()
        } else if pdfium_path.is_dir() {
            Pdfium::pdfium_platform_library_name_at_path(pdfium_path)
        } else if pdfium_path.as_os_str().is_empty() {
            PathBuf::from(Pdfium::pdfium_platform_library_name())
        } else {
            pdfium_path.to_path_buf()
        };

        let pdfium = match Pdfium::bind_to_library(&candidate_path) {
            Ok(bindings) => Pdfium::new(bindings),
            Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Pdfium::default(),
            Err(e) => {
                // If direct binding failed and candidate was relative, try standard directory lookup
                if let Ok(bindings) = Pdfium::bind_to_system_library() {
                    Pdfium::new(bindings)
                } else {
                    bail!(
                        "Failed binding to Pdfium dynamic library at '{}': {}",
                        candidate_path.display(),
                        e
                    );
                }
            }
        };

        Ok(Self { pdfium })
    }

    /// Opens a PDF document and extracts its metadata and page count.
    pub fn open(&self, path: &Path, password: Option<&str>) -> Result<DocumentInfo> {
        if !path.exists() {
            bail!("PDF file not found: '{}'", path.display());
        }

        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)] {
            use std::os::windows::fs::OpenOptionsExt;
            // Keep the explicit-open fingerprint and PDFium metadata on one
            // unchanged file, including replacements that preserve timestamps.
            options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
        }
        let mut source = options.open(path)
            .with_context(|| format!("Failed opening PDF file '{}'", path.display()))?;
        let size = source.metadata()?.len();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document.pdf")
            .to_string();

        let doc = self
            .pdfium
            .load_pdf_from_file(path, password)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed opening PDF document '{}'. The document may be password-protected or corrupted: {}",
                    path.display(),
                    e
                )
            })?;
        let revision = content_revision(&mut source)?;

        let pages = doc.pages().len() as u32;
        let page_sizes = doc
            .pages()
            .page_sizes()
            .map(|sizes| {
                sizes
                    .iter()
                    .map(|rect| [rect.width().value.round().max(1.0) as u32, rect.height().value.round().max(1.0) as u32])
                    .collect()
            })
            .unwrap_or_default();
        let id = Uuid::new_v4().to_string();
        let path_str = normalize_path(path).to_string_lossy().to_string();

        Ok(DocumentInfo {
            id,
            path: path_str,
            name,
            pages,
            size,
            revision,
            page_sizes,
        })
    }

    /// Renders a single 1-based page of a PDF document to a PNG data URL.
    /// Clamps rendering width to a maximum of 2400 pixels while preserving aspect ratio.
    pub fn render(
        &self,
        path: &Path,
        password: Option<&str>,
        page: u32,
        width: u32,
    ) -> Result<RenderedPage> {
        if page == 0 { bail!("Page number must be 1-based (got 0)"); }
        let img = crate::raster::render_requested(&self.pdfium, path, password, page, width, 2400, true)
            .with_context(|| format!("Failed rendering page {}", page))?;
        let (rendered_width, rendered_height) = (img.width(), img.height());
        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
            .context("Failed encoding rendered page to PNG format")?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        Ok(RenderedPage { data_url: format!("data:image/png;base64,{}", b64), width: rendered_width, height: rendered_height })
    }

    /// Loads a document for the persistent reader's cached text and search calls.
    pub fn load<'a>(&'a self, path: &Path, password: Option<&str>) -> Result<PdfDocument<'a>> {
        self.pdfium
            .load_pdf_from_file(path, password)
            .with_context(|| format!("Failed opening PDF document: '{}'", path.display()))
    }


    /// Extracts the full text content of a single 1-based page.
    pub fn page_text(&self, path: &Path, password: Option<&str>, page: u32) -> Result<String> {
        if page == 0 {
            bail!("Page number must be 1-based (got 0)");
        }

        let doc = self.load(path, password)?;
        Self::page_text_loaded(&doc, page)
    }

    /// Extracts the text of a page of an already loaded document.
    pub fn page_text_loaded(doc: &PdfDocument<'_>, page: u32) -> Result<String> {
        if page == 0 {
            bail!("Page number must be 1-based (got 0)");
        }
        let page_idx = (page - 1) as PdfPageIndex;
        if page_idx >= doc.pages().len() {
            bail!(
                "Page {} is out of bounds (document has {} pages)",
                page,
                doc.pages().len()
            );
        }

        let pdf_page = doc
            .pages()
            .get(page_idx)
            .with_context(|| format!("Failed retrieving page {}", page))?;

        let text = pdf_page
            .text()
            .map(|t| t.all())
            .unwrap_or_default();

        Ok(text)
    }

    /// Searches the document for the specified query string, returning matched pages and snippets.
    pub fn search(&self, path: &Path, password: Option<&str>, query: &str) -> Result<Vec<SearchHit>> {
        let trimmed_query = query.trim();
        if trimmed_query.is_empty() {
            return Ok(Vec::new());
        }

        let doc = self
            .pdfium
            .load_pdf_from_file(path, password)
            .with_context(|| format!("Failed opening PDF document for search: '{}'", path.display()))?;
        Ok(Self::search_loaded(&doc, trimmed_query))
    }

    /// Searches an already loaded document (see [`Engine::search`]).
    pub fn search_loaded(doc: &PdfDocument<'_>, query: &str) -> Vec<SearchHit> {
        let trimmed_query = query.trim();
        if trimmed_query.is_empty() {
            return Vec::new();
        }
        let total_pages = doc.pages().len();
        let mut hits = Vec::new();

        for idx in 0..total_pages {
            let page_num = (idx + 1) as u32;
            let page = match doc.pages().get(idx) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let page_text = match page.text() {
                Ok(t) => t.all(),
                Err(_) => continue,
            };

            let remaining = MAX_SEARCH_HITS - hits.len();
            for snippet in find_snippets(&page_text, trimmed_query, remaining) {
                hits.push(SearchHit { page: page_num, text: snippet });
            }
            if hits.len() >= MAX_SEARCH_HITS {
                break;
            }
        }

        hits
    }

    pub fn perform<F>(
        &self,
        request: &OperationRequest,
        cancel: &AtomicBool,
        mut progress: F,
    ) -> Result<OperationResult>
    where
        F: FnMut(Progress),
    {
        progress(Progress {
            id: request.id.clone(),
            completed: 0,
            total: 1,
            message: "Checking output destination...".into(),
        });
        if request.kind.eq_ignore_ascii_case("install-ocr") {
            return crate::ocr_install::op_install_ocr(request, cancel, progress);
        }
        let mut transaction = OutputTransaction::new(request)?;
        let staged = transaction.staged_request(request);
        let result = self.perform_staged(&staged, cancel, progress)
            .and_then(|result| transaction.commit(result, cancel));
        let cleanup = transaction.cleanup();
        match (result, cleanup) {
            (result, Ok(())) => result,
            (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(cleanup)) => Err(error.context(format!("Temporary job cleanup failed: {cleanup:#}"))),
        }
    }

    fn perform_staged<F>(
        &self,
        request: &OperationRequest,
        cancel: &AtomicBool,
        progress: F,
    ) -> Result<OperationResult>
    where
        F: FnMut(Progress),
    {
        match request.kind.to_lowercase().as_str() {
            "merge" => op_merge(&self.pdfium, request, cancel, progress),
            "extract" => op_extract(&self.pdfium, request, cancel, progress),
            "split" => op_split(&self.pdfium, request, cancel, progress),
            "remove" => op_remove(&self.pdfium, request, cancel, progress),
            "reorder" => op_reorder(&self.pdfium, request, cancel, progress),
            "rotate" => op_rotate(&self.pdfium, request, cancel, progress),
            "watermark" => op_watermark(&self.pdfium, request, cancel, progress),
            "number" => op_number(&self.pdfium, request, cancel, progress),
            "protect" => op_protect(request, cancel, progress),
            "unlock" => op_unlock(request, cancel, progress),
            "compress" => op_compress(request, cancel, progress),
            "images-to-pdf" => op_images_to_pdf(&self.pdfium, request, cancel, progress),
            "pdf-to-images" => op_pdf_to_images(&self.pdfium, request, cancel, progress),
            "redact" => op_redact(&self.pdfium, request, cancel, progress),
            "annotate" => op_annotate(&self.pdfium, request, cancel, progress),
            "extract-text" => op_extract_text(&self.pdfium, request, cancel, progress),
            "ocr" => op_ocr(&self.pdfium, request, cancel, progress),
            "pdf-to-word" => op_pdf_to_word(&self.pdfium, request, cancel, progress),
            "word-to-pdf" => op_word_to_pdf(&self.pdfium, request, cancel, progress),
            "image-to-word" => op_image_to_word(request, cancel, progress),
            other => bail!("Unknown or unsupported PDF operation kind: '{}'", other),
        }
    }
}

#[cfg(test)]
mod revision_tests {
    use super::*;
    use std::fs::{FileTimes, OpenOptions};
    use std::time::{Duration, SystemTime};

    #[test]
    fn content_revision_detects_same_size_same_timestamp_replacements() {
        let scratch = crate::scratch::ScratchDir::new().unwrap();
        let path = scratch.path().join("document.pdf");
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        std::fs::write(&path, b"OLD document").unwrap();
        let mut file = OpenOptions::new().read(true).write(true).open(&path).unwrap();
        file.set_times(FileTimes::new().set_modified(time)).unwrap();
        let old = content_revision(&mut file).unwrap();
        drop(file);
        std::fs::write(&path, b"NEW document").unwrap();
        let mut file = OpenOptions::new().read(true).write(true).open(&path).unwrap();
        file.set_times(FileTimes::new().set_modified(time)).unwrap();
        assert_eq!(file.metadata().unwrap().modified().unwrap(), time);
        let new = content_revision(&mut file).unwrap();
        assert_ne!(new, old);
        file.set_times(FileTimes::new().set_modified(time + Duration::from_secs(1))).unwrap();
        drop(file);
        let mut file = OpenOptions::new().read(true).open(&path).unwrap();
        assert_eq!(content_revision(&mut file).unwrap(), new);
    }
}
