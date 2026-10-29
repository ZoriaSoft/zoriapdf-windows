use pdfium_render::prelude::*;
use std::path::Path;

/// Embedded Manrope font bytes supporting Turkish Unicode and Latin glyphs.
pub const BUNDLED_MANROPE: &[u8] = include_bytes!("../resources/Manrope.ttf");

/// Attempts to obtain a Unicode-capable font for the document.
///
/// Priority:
/// 1. Path specified in `ZORIA_FONT_PATH` environment variable if present.
/// 2. Embedded `Manrope.ttf` TrueType font loaded with CID support.
/// 3. Built-in Helvetica font as fallback.
pub fn get_or_load_font<'a>(doc: &mut PdfDocument<'a>) -> PdfFontToken {
    if let Ok(custom_path) = std::env::var("ZORIA_FONT_PATH") {
        let path = Path::new(&custom_path);
        if path.is_file() {
            if let Ok(token) = doc.fonts_mut().load_true_type_from_file(path, true) {
                return token;
            }
        }
    }

    if let Ok(token) = doc.fonts_mut().load_true_type_from_bytes(BUNDLED_MANROPE, true) {
        return token;
    }

    doc.fonts_mut().helvetica()
}

/// Static Regular/Bold instances of Manrope (the variable font's default instance is
/// ExtraLight, which reads too thin for documents).
pub const BUNDLED_MANROPE_REGULAR: &[u8] = include_bytes!("../resources/Manrope-Regular.ttf");
pub const BUNDLED_MANROPE_BOLD: &[u8] = include_bytes!("../resources/Manrope-Bold.ttf");

/// Regular and bold fonts for document text. `None` for bold means no separate bold
/// face could be loaded and bold is simulated by the caller.
pub fn load_document_fonts(doc: &mut PdfDocument<'_>) -> (PdfFontToken, Option<PdfFontToken>) {
    if std::env::var("ZORIA_FONT_PATH").is_ok_and(|p| Path::new(&p).is_file()) {
        return (get_or_load_font(doc), None);
    }
    let regular = doc.fonts_mut().load_true_type_from_bytes(BUNDLED_MANROPE_REGULAR, true).ok();
    let bold = doc.fonts_mut().load_true_type_from_bytes(BUNDLED_MANROPE_BOLD, true).ok();
    match regular {
        Some(regular) => (regular, bold),
        None => (get_or_load_font(doc), None),
    }
}
