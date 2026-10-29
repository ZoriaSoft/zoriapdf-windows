//! Writes the document model as a minimal but valid OOXML package
//! (Word, WPS and LibreOffice open it): styled runs, headings, alignment,
//! list indents, tables with merges and shading, and inline images.
use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::model::{hex_color, Align, Block, ParagraphKind, VMerge, WordDocument, WordImage, WordParagraph, WordTable};

/// Printable width of the A4 page we write (11906 twips minus 2 x 1134 margins), in points.
pub const CONTENT_WIDTH_PT: f32 = (11906.0 - 2.0 * 1134.0) / 20.0;
const EMU_PER_PT: f64 = 12700.0;

pub(crate) fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("    "),
            // Control characters are illegal in XML 1.0.
            c if (c as u32) < 0x20 && c != '\n' && c != '\r' => {}
            // Noncharacters that are also illegal in XML 1.0 (Word refuses the file).
            '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

struct Writer<'a> {
    body: String,
    images: Vec<&'a WordImage>,
}

impl<'a> Writer<'a> {
    fn paragraph(&mut self, paragraph: &WordParagraph, extra_ppr: &str) {
        let style = match paragraph.kind {
            ParagraphKind::Heading1 => "Heading1",
            ParagraphKind::Heading2 => "Heading2",
            ParagraphKind::Heading3 => "Heading3",
            ParagraphKind::Body => "Normal",
        };
        // Child order follows the OOXML schema (pStyle, spacing, ind, jc); Word is strict.
        let mut ppr = format!("<w:pStyle w:val=\"{style}\"/>");
        ppr.push_str(extra_ppr);
        if let Some(level) = paragraph.list_level {
            let left = 360 * (level as u32 + 1);
            ppr.push_str(&format!("<w:ind w:left=\"{left}\" w:hanging=\"360\"/>"));
        }
        ppr.push_str(align_xml(paragraph.align));
        self.body.push_str(&format!("<w:p><w:pPr>{ppr}</w:pPr>"));
        // The break belongs to the start of this paragraph ("page_break_before").
        if paragraph.page_break_before {
            self.body.push_str("<w:r><w:br w:type=\"page\"/></w:r>");
        }
        for run in &paragraph.runs {
            let mut rpr = String::new();
            let s = &run.style;
            if s.bold {
                rpr.push_str("<w:b/><w:bCs/>");
            }
            if s.italic {
                rpr.push_str("<w:i/><w:iCs/>");
            }
            if s.strike {
                rpr.push_str("<w:strike/>");
            }
            if let Some(rgb) = s.color {
                rpr.push_str(&format!("<w:color w:val=\"{}\"/>", hex_color(rgb)));
            }
            if let Some(size) = s.size {
                let half_points = (size * 2.0).round().clamp(2.0, 3276.0) as u32;
                rpr.push_str(&format!("<w:sz w:val=\"{half_points}\"/><w:szCs w:val=\"{half_points}\"/>"));
            }
            if s.underline {
                rpr.push_str("<w:u w:val=\"single\"/>");
            }
            let rpr = if rpr.is_empty() { String::new() } else { format!("<w:rPr>{rpr}</w:rPr>") };
            let mut lines = run.text.split('\n').peekable();
            while let Some(line) = lines.next() {
                if !line.is_empty() {
                    self.body.push_str(&format!(
                        "<w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
                        xml_escape(line)
                    ));
                }
                if lines.peek().is_some() {
                    self.body.push_str(&format!("<w:r>{rpr}<w:br/></w:r>"));
                }
            }
        }
        self.body.push_str("</w:p>");
    }

    fn image(&mut self, image: &'a WordImage) {
        self.images.push(image);
        let index = self.images.len();
        // Never wider than the printable width, keeping the aspect ratio.
        let (mut w, mut h) = (image.width_pt.max(1.0), image.height_pt.max(1.0));
        if w > CONTENT_WIDTH_PT {
            h *= CONTENT_WIDTH_PT / w;
            w = CONTENT_WIDTH_PT;
        }
        let cx = (w as f64 * EMU_PER_PT).round() as u64;
        let cy = (h as f64 * EMU_PER_PT).round() as u64;
        let page_break = if image.page_break_before { "<w:r><w:br w:type=\"page\"/></w:r>" } else { "" };
        self.body.push_str(&format!(
            "<w:p><w:pPr>{align}</w:pPr>{page_break}<w:r><w:drawing>\
<wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
<wp:docPr id=\"{index}\" name=\"Resim {index}\"/>\
<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:pic><pic:nvPicPr><pic:cNvPr id=\"{index}\" name=\"image{index}.{ext}\"/><pic:cNvPicPr/></pic:nvPicPr>\
<pic:blipFill><a:blip r:embed=\"rIdImg{index}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
</a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
            align = align_xml(image.align),
            ext = image.format.extension(),
        ));
    }

    fn table(&mut self, table: &'a WordTable) {
        let columns = table.grid_columns();
        let widths = fitted_widths(&table.col_widths_pt, columns, CONTENT_WIDTH_PT);
        if table.page_break_before {
            // A table cannot hold the break itself; an empty paragraph before it does.
            self.body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
        }
        let borders = if table.borderless {
            ""
        } else {
            "<w:tblBorders>\
<w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
<w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
<w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
<w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
<w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
<w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"/>\
</w:tblBorders>"
        };
        self.body.push_str(&format!(
            "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/>{borders}<w:tblLayout w:type=\"fixed\"/>\
<w:tblCellMar><w:left w:w=\"80\" w:type=\"dxa\"/><w:right w:w=\"80\" w:type=\"dxa\"/></w:tblCellMar>\
</w:tblPr><w:tblGrid>"
        ));
        for width in &widths {
            self.body.push_str(&format!("<w:gridCol w:w=\"{}\"/>", twips(*width)));
        }
        self.body.push_str("</w:tblGrid>");
        for row in &table.rows {
            self.body.push_str("<w:tr>");
            let mut col = 0usize;
            for cell in row {
                let span = (cell.span.max(1) as usize).min(columns.saturating_sub(col).max(1));
                let width: f32 = widths.iter().skip(col).take(span).sum();
                col += span;
                let mut tcpr = format!("<w:tcW w:w=\"{}\" w:type=\"dxa\"/>", twips(width));
                if span > 1 {
                    tcpr.push_str(&format!("<w:gridSpan w:val=\"{span}\"/>"));
                }
                match cell.vmerge {
                    VMerge::Restart => tcpr.push_str("<w:vMerge w:val=\"restart\"/>"),
                    VMerge::Continue => tcpr.push_str("<w:vMerge/>"),
                    VMerge::None => {}
                }
                if let Some(rgb) = cell.shading {
                    tcpr.push_str(&format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>", hex_color(rgb)));
                }
                self.body.push_str(&format!("<w:tc><w:tcPr>{tcpr}</w:tcPr>"));
                let paragraphs: Vec<&WordParagraph> = if cell.vmerge == VMerge::Continue {
                    Vec::new()
                } else {
                    cell.paragraphs.iter().filter(|p| !p.is_blank()).collect()
                };
                if paragraphs.is_empty() {
                    // Every cell needs at least one paragraph to be valid OOXML.
                    self.body.push_str("<w:p><w:pPr><w:spacing w:after=\"0\"/></w:pPr></w:p>");
                }
                for paragraph in paragraphs {
                    let mut p = paragraph.clone();
                    p.page_break_before = false;
                    self.paragraph(&p, "<w:spacing w:after=\"0\"/>");
                }
                self.body.push_str("</w:tc>");
            }
            // Pad short rows so every row covers the whole grid (Word repairs otherwise).
            while col < columns {
                let width = widths[col];
                col += 1;
                self.body.push_str(&format!(
                    "<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/></w:tcPr><w:p/></w:tc>",
                    twips(width)
                ));
            }
            self.body.push_str("</w:tr>");
        }
        self.body.push_str("</w:tbl>");
        // Two consecutive tables would merge into one in Word; keep them apart.
        self.body.push_str("<w:p><w:pPr><w:spacing w:after=\"0\"/></w:pPr></w:p>");
    }
}

fn align_xml(align: Align) -> &'static str {
    match align {
        Align::Left => "",
        Align::Center => "<w:jc w:val=\"center\"/>",
        Align::Right => "<w:jc w:val=\"right\"/>",
        Align::Justify => "<w:jc w:val=\"both\"/>",
    }
}

fn twips(points: f32) -> u32 {
    (points * 20.0).round().max(1.0) as u32
}

/// Column widths that fill at most `available` points: declared widths scaled down
/// when they overflow, equal widths when none (or a wrong number) are declared.
pub fn fitted_widths(declared: &[f32], columns: usize, available: f32) -> Vec<f32> {
    let columns = columns.max(1);
    let mut widths: Vec<f32> = if declared.len() == columns && declared.iter().all(|w| *w > 1.0) {
        declared.to_vec()
    } else {
        vec![available / columns as f32; columns]
    };
    let total: f32 = widths.iter().sum();
    if total > available {
        let scale = available / total;
        widths.iter_mut().for_each(|w| *w *= scale);
    }
    widths
}

pub fn docx_document_xml(document: &WordDocument) -> (String, Vec<&WordImage>) {
    let mut writer = Writer { body: String::with_capacity(document.blocks.len() * 160), images: Vec::new() };
    for block in &document.blocks {
        match block {
            Block::Paragraph(p) => writer.paragraph(p, ""),
            Block::Table(t) => writer.table(t),
            Block::Image(i) => writer.image(i),
        }
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<w:body>{}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
<w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"567\" w:footer=\"567\" w:gutter=\"0\"/></w:sectPr>\
</w:body></w:document>",
        writer.body
    );
    (xml, writer.images)
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Default Extension=\"png\" ContentType=\"image/png\"/>\
<Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
</Types>";

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";

fn document_rels(images: &[&WordImage]) -> String {
    let mut rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
    );
    for (i, image) in images.iter().enumerate() {
        rels.push_str(&format!(
            "<Relationship Id=\"rIdImg{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{n}.{ext}\"/>",
            n = i + 1,
            ext = image.format.extension()
        ));
    }
    rels.push_str("</Relationships>");
    rels
}

fn styles_xml(body_size: f32) -> String {
    let body = (body_size * 2.0).round().clamp(12.0, 96.0) as u32;
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/>\
<w:sz w:val=\"{body}\"/><w:szCs w:val=\"{body}\"/><w:lang w:val=\"tr-TR\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"120\" w:line=\"264\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/>\
<w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"0\"/></w:pPr>\
<w:rPr><w:b/><w:sz w:val=\"40\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/>\
<w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before=\"200\" w:after=\"100\"/><w:outlineLvl w:val=\"1\"/></w:pPr>\
<w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading3\"><w:name w:val=\"heading 3\"/>\
<w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before=\"160\" w:after=\"80\"/><w:outlineLvl w:val=\"2\"/></w:pPr>\
<w:rPr><w:b/><w:sz w:val=\"26\"/></w:rPr></w:style>\
</w:styles>"
    )
}

pub fn write_docx(path: &Path, document: &WordDocument) -> Result<()> {
    let (xml, images) = docx_document_xml(document);
    let file = std::fs::File::create(path)
        .with_context(|| format!("Failed creating Word document '{}'", path.display()))?;
    let mut archive = ZipWriter::new(file);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut put = |name: &str, data: &[u8], options: SimpleFileOptions| -> Result<()> {
        archive.start_file(name, options)?;
        archive.write_all(data)?;
        Ok(())
    };
    put("[Content_Types].xml", CONTENT_TYPES.as_bytes(), deflated)?;
    put("_rels/.rels", ROOT_RELS.as_bytes(), deflated)?;
    put("word/document.xml", xml.as_bytes(), deflated)?;
    put("word/_rels/document.xml.rels", document_rels(&images).as_bytes(), deflated)?;
    put("word/styles.xml", styles_xml(document.body_size).as_bytes(), deflated)?;
    for (i, image) in images.iter().enumerate() {
        // PNG/JPEG are already compressed.
        put(&format!("word/media/image{}.{}", i + 1, image.format.extension()), &image.data, stored)?;
    }
    archive.finish()?.sync_all().ok();
    Ok(())
}
