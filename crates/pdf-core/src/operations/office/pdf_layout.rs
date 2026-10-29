//! Lays the document model out on A4 PDF pages with PDFium: styled runs (bold as
//! fill+stroke, italic as a slant, underline/strike as rules, colour, size),
//! alignment including justify, list indents, tables with borders, merged cells and
//! shading (rows split across pages), and PNG/JPEG images.
use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use super::docx_write::fitted_widths;
use super::model::{Align, Block, ImageFormat, ParagraphKind, TextStyle, VMerge, WordDocument, WordImage, WordParagraph, WordTable};

pub const A4_WIDTH: f32 = 595.28;
pub const A4_HEIGHT: f32 = 841.89;
pub const PAGE_MARGIN: f32 = 56.0;
const CONTENT_WIDTH: f32 = A4_WIDTH - 2.0 * PAGE_MARGIN;
const TAB_STOP: f32 = 24.0;
const LIST_INDENT: f32 = 18.0;
const CELL_PAD_X: f32 = 4.0;
const CELL_PAD_Y: f32 = 3.0;
const BORDER: f32 = 0.6;

#[derive(Clone, Debug)]
struct Piece {
    text: String,
    style: TextStyle,
    size: f32,
    width: f32,
    /// A space or tab: dropped at line ends, stretched when justifying.
    space: bool,
}

#[derive(Clone, Debug, Default)]
struct Line {
    pieces: Vec<Piece>,
    width: f32,
    max_size: f32,
    /// Ends with a forced break (or is the paragraph's last line): never justified.
    hard_end: bool,
}

impl Line {
    fn height(&self) -> f32 {
        self.max_size * 1.32
    }
}

struct LaidParagraph {
    lines: Vec<Line>,
    align: Align,
    indent: f32,
    space_before: f32,
    space_after: f32,
}

struct CellOwner {
    start: usize,
    span: usize,
    shading: Option<[u8; 3]>,
    lines: Vec<(Line, Align, f32)>,
    line_cursor: usize,
    remaining_height: f64,
    first_row: usize,
    last_row: usize,
}

#[derive(Clone, Copy)]
struct OwnerPageRect {
    top: f32,
    bottom: f32,
}


pub struct PdfComposer<'a> {
    document: PdfDocument<'a>,
    font: PdfFontToken,
    /// A real bold face; `None` simulates bold with fill + stroke.
    bold_font: Option<PdfFontToken>,
    page: PdfPage<'a>,
    cursor: f32,
    body_size: f32,
    widths: HashMap<(char, bool), f32>,
    /// Width of "xx" at 100 pt per face (regular, bold); the base for per-character measurement.
    xx_width: [Option<f32>; 2],
}

fn new_page<'a>(document: &mut PdfDocument<'a>) -> Result<PdfPage<'a>> {
    document
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(A4_WIDTH), PdfPoints::new(A4_HEIGHT)))
        .context("Failed creating PDF page")
}

impl<'a> PdfComposer<'a> {
    pub fn new(pdfium: &'a Pdfium, body_size: f32) -> Result<Self> {
        let mut document = pdfium.create_new_pdf().context("Failed creating output PDF document")?;
        let (font, bold_font) = crate::font::load_document_fonts(&mut document);
        let page = new_page(&mut document)?;
        Ok(PdfComposer {
            document,
            font,
            bold_font,
            page,
            cursor: A4_HEIGHT - PAGE_MARGIN,
            body_size: body_size.clamp(6.0, 36.0),
            widths: HashMap::new(),
            xx_width: [None, None],
        })
    }

    fn face(&self, bold: bool) -> PdfFontToken {
        if bold {
            self.bold_font.unwrap_or(self.font)
        } else {
            self.font
        }
    }

    fn text_width_100(&self, text: &str, bold: bool) -> Option<f32> {
        let object = PdfPageTextObject::new(&self.document, text, self.face(bold), PdfPoints::new(100.0)).ok()?;
        object.width().ok().map(|w| w.value).filter(|w| w.is_finite() && *w > 0.0)
    }

    /// Advance width of a character at 1 pt, measured once with the real font
    /// ("x?x" minus "xx" also captures spaces); falls back to an average width.
    fn char_width(&mut self, c: char, bold: bool) -> f32 {
        let bold = bold && self.bold_font.is_some();
        if let Some(width) = self.widths.get(&(c, bold)) {
            return *width;
        }
        let slot = usize::from(bold);
        if self.xx_width[slot].is_none() {
            self.xx_width[slot] = Some(self.text_width_100("xx", bold).unwrap_or(0.0));
        }
        let base = self.xx_width[slot].unwrap_or(0.0);
        let measured = if base > 0.0 {
            self.text_width_100(&format!("x{c}x"), bold).map(|w| (w - base) / 100.0).filter(|w| *w > 0.0)
        } else {
            None
        };
        let width = measured.unwrap_or(if c == ' ' { 0.26 } else { 0.56 });
        self.widths.insert((c, bold), width);
        width
    }

    /// Width in points; simulated bold is a little wider than the regular face.
    fn measure(&mut self, text: &str, size: f32, bold: bool) -> f32 {
        let raw: f32 = text.chars().map(|c| self.char_width(c, bold)).sum::<f32>() * size;
        if bold && self.bold_font.is_none() {
            raw * 1.04
        } else {
            raw
        }
    }

    fn default_size(&self, kind: ParagraphKind) -> f32 {
        match kind {
            ParagraphKind::Heading1 => (self.body_size * 1.8).max(18.0),
            ParagraphKind::Heading2 => (self.body_size * 1.45).max(15.0),
            ParagraphKind::Heading3 => (self.body_size * 1.2).max(12.5),
            ParagraphKind::Body => self.body_size,
        }
    }

    fn layout_paragraph(&mut self, paragraph: &WordParagraph, width: f32, in_table: bool) -> LaidParagraph {
        let default_size = self.default_size(paragraph.kind);
        let heading = paragraph.kind != ParagraphKind::Body;
        let indent = paragraph.list_level.map(|level| LIST_INDENT * (level as f32 + 1.0)).unwrap_or(0.0).min(width / 2.0);
        let width = (width - indent).max(20.0);

        // Tokens: words (possibly several styled pieces), spaces, tabs and forced breaks.
        enum Token {
            Word(Vec<Piece>),
            Space(Piece),
            Tab(TextStyle, f32),
            Break,
        }
        let mut tokens: Vec<Token> = Vec::new();
        let mut word: Vec<Piece> = Vec::new();
        let flush_word = |word: &mut Vec<Piece>, tokens: &mut Vec<Token>| {
            if !word.is_empty() {
                tokens.push(Token::Word(std::mem::take(word)));
            }
        };
        for run in &paragraph.runs {
            let mut style = run.style.clone();
            if heading {
                style.bold = true;
            }
            let size = style.size.unwrap_or(default_size).clamp(4.0, 96.0);
            let mut chunk = String::new();
            for c in run.text.chars() {
                match c {
                    ' ' => {
                        if !chunk.is_empty() {
                            let w = self.measure(&chunk, size, style.bold);
                            word.push(Piece { text: std::mem::take(&mut chunk), style: style.clone(), size, width: w, space: false });
                        }
                        flush_word(&mut word, &mut tokens);
                        let w = self.measure(" ", size, false);
                        tokens.push(Token::Space(Piece { text: " ".into(), style: style.clone(), size, width: w, space: true }));
                    }
                    '\t' | '\n' | '\r' => {
                        if !chunk.is_empty() {
                            let w = self.measure(&chunk, size, style.bold);
                            word.push(Piece { text: std::mem::take(&mut chunk), style: style.clone(), size, width: w, space: false });
                        }
                        flush_word(&mut word, &mut tokens);
                        tokens.push(if c == '\t' { Token::Tab(style.clone(), size) } else { Token::Break });
                    }
                    c if (c as u32) < 0x20 => {}
                    c => chunk.push(c),
                }
            }
            if !chunk.is_empty() {
                let w = self.measure(&chunk, size, style.bold);
                word.push(Piece { text: chunk, style: style.clone(), size, width: w, space: false });
            }
        }
        flush_word(&mut word, &mut tokens);

        let mut lines: Vec<Line> = Vec::new();
        let mut line = Line { max_size: default_size, ..Default::default() };
        let finish = |line: &mut Line, lines: &mut Vec<Line>, hard: bool, default_size: f32| {
            while line.pieces.last().is_some_and(|p| p.space) {
                let removed = line.pieces.pop().unwrap();
                line.width -= removed.width;
            }
            line.hard_end = hard;
            lines.push(std::mem::replace(line, Line { max_size: default_size, ..Default::default() }));
        };
        for token in tokens {
            match token {
                Token::Break => finish(&mut line, &mut lines, true, default_size),
                Token::Space(piece) => {
                    if !line.pieces.is_empty() {
                        line.width += piece.width;
                        line.pieces.push(piece);
                    }
                }
                Token::Tab(style, size) => {
                    let next = ((line.width / TAB_STOP).floor() + 1.0) * TAB_STOP;
                    if next > width {
                        finish(&mut line, &mut lines, false, default_size);
                    } else {
                        let advance = next - line.width;
                        line.pieces.push(Piece { text: String::new(), style, size, width: advance, space: true });
                        line.width = next;
                    }
                }
                Token::Word(pieces) => {
                    let word_width: f32 = pieces.iter().map(|p| p.width).sum();
                    if line.width + word_width > width && line.pieces.iter().any(|p| !p.space) {
                        finish(&mut line, &mut lines, false, default_size);
                    }
                    if word_width > width {
                        // A single word wider than the column is broken by characters.
                        for piece in pieces {
                            let mut part = String::new();
                            let mut part_width = 0.0;
                            for c in piece.text.chars() {
                                let mut buf = [0u8; 4];
                                let cw = self.measure(c.encode_utf8(&mut buf), piece.size, piece.style.bold);
                                if line.width + part_width + cw > width && (!part.is_empty() || !line.pieces.is_empty()) {
                                    if !part.is_empty() {
                                        line.max_size = line.max_size.max(piece.size);
                                        line.pieces.push(Piece { text: std::mem::take(&mut part), width: part_width, ..piece.clone() });
                                        line.width += part_width;
                                    }
                                    part_width = 0.0;
                                    finish(&mut line, &mut lines, false, default_size);
                                }
                                part.push(c);
                                part_width += cw;
                            }
                            if !part.is_empty() {
                                line.max_size = line.max_size.max(piece.size);
                                line.pieces.push(Piece { text: part, width: part_width, ..piece.clone() });
                                line.width += part_width;
                            }
                        }
                    } else {
                        for piece in pieces {
                            line.max_size = line.max_size.max(piece.size);
                            line.width += piece.width;
                            line.pieces.push(piece);
                        }
                    }
                }
            }
        }
        if !line.pieces.is_empty() || lines.is_empty() {
            finish(&mut line, &mut lines, true, default_size);
        } else if let Some(last) = lines.last_mut() {
            last.hard_end = true;
        }
        let (space_before, space_after) = match (in_table, paragraph.kind) {
            (true, _) => (0.0, 1.5),
            (false, ParagraphKind::Body) => (0.0, self.body_size * 0.55),
            (false, _) => (default_size * 0.5, default_size * 0.3),
        };
        LaidParagraph { lines, align: paragraph.align, indent, space_before, space_after }
    }

    fn break_page(&mut self) -> Result<()> {
        self.page.regenerate_content().ok();
        self.page = new_page(&mut self.document)?;
        self.cursor = A4_HEIGHT - PAGE_MARGIN;
        Ok(())
    }

    fn at_page_top(&self) -> bool {
        self.cursor >= A4_HEIGHT - PAGE_MARGIN - 0.5
    }

    fn color(rgb: Option<[u8; 3]>) -> PdfColor {
        match rgb {
            Some([r, g, b]) => PdfColor::new(r, g, b, 255),
            None => PdfColor::new(0, 0, 0, 255),
        }
    }

    fn place_text(&mut self, text: &str, style: &TextStyle, size: f32, x: f32, baseline: f32, width: f32) -> Result<()> {
        if !text.trim().is_empty() {
            let mut object = PdfPageTextObject::new(&self.document, text, self.face(style.bold), PdfPoints::new(size))
                .context("Failed creating text object")?;
            let color = Self::color(style.color);
            object.set_fill_color(color).ok();
            if style.bold && self.bold_font.is_none() {
                object.set_render_mode(PdfPageTextRenderMode::FilledThenStroked).ok();
                object.set_stroke_color(color).ok();
                object.set_stroke_width(PdfPoints::new(size * 0.035)).ok();
            }
            if style.italic {
                object.skew_degrees(0.0, 12.0).ok();
            }
            object.translate(PdfPoints::new(x), PdfPoints::new(baseline)).ok();
            self.page.objects_mut().add_text_object(object).context("Failed placing text on PDF page")?;
        }
        let color = Self::color(style.color);
        if style.underline && width > 0.0 {
            let y = baseline - size * 0.12;
            self.rule(x, y, x + width, y, size * 0.06, color)?;
        }
        if style.strike && width > 0.0 {
            let y = baseline + size * 0.28;
            self.rule(x, y, x + width, y, size * 0.06, color)?;
        }
        Ok(())
    }

    fn rule(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: PdfColor) -> Result<()> {
        let line = PdfPagePathObject::new_line(
            &self.document,
            PdfPoints::new(x1),
            PdfPoints::new(y1),
            PdfPoints::new(x2),
            PdfPoints::new(y2),
            color,
            PdfPoints::new(width.max(0.3)),
        )
        .context("Failed creating PDF line")?;
        self.page.objects_mut().add_path_object(line).context("Failed placing PDF line")?;
        Ok(())
    }

    fn fill_rect(&mut self, left: f32, bottom: f32, right: f32, top: f32, rgb: [u8; 3]) -> Result<()> {
        let rect = PdfRect::new(PdfPoints::new(bottom), PdfPoints::new(left), PdfPoints::new(top), PdfPoints::new(right));
        let object = PdfPagePathObject::new_rect(&self.document, rect, None, None, Some(PdfColor::new(rgb[0], rgb[1], rgb[2], 255)))
            .context("Failed creating PDF shape")?;
        self.page.objects_mut().add_path_object(object).context("Failed placing PDF shape")?;
        Ok(())
    }

    /// Draws one laid-out line whose top edge is at `top`, inside [left, left+width].
    fn draw_line(&mut self, line: &Line, align: Align, left: f32, width: f32, top: f32) -> Result<()> {
        let baseline = top - line.max_size * 1.02;
        let free = (width - line.width).max(0.0);
        let spaces = line.pieces.iter().filter(|p| p.space && !p.text.is_empty()).count();
        let (mut x, stretch) = match align {
            Align::Center => (left + free / 2.0, 0.0),
            Align::Right => (left + free, 0.0),
            Align::Justify if !line.hard_end && spaces > 0 => (left, free / spaces as f32),
            _ => (left, 0.0),
        };
        // Merge neighbouring pieces with the same style into one text object, except
        // when justifying, where every space is stretched separately.
        let mut i = 0;
        while i < line.pieces.len() {
            let piece = &line.pieces[i];
            if piece.space && piece.text.is_empty() {
                x += piece.width; // tab advance
                i += 1;
                continue;
            }
            let mut text = piece.text.clone();
            let mut w = piece.width + if piece.space { stretch } else { 0.0 };
            let style = piece.style.clone();
            let size = piece.size;
            let mut j = i + 1;
            while stretch == 0.0
                && j < line.pieces.len()
                && line.pieces[j].style == style
                && line.pieces[j].size == size
                && !(line.pieces[j].space && line.pieces[j].text.is_empty())
            {
                text.push_str(&line.pieces[j].text);
                w += line.pieces[j].width;
                j += 1;
            }
            self.place_text(&text, &style, size, x, baseline, w)?;
            x += w;
            i = j;
        }
        Ok(())
    }

    fn paragraph(&mut self, paragraph: &WordParagraph) -> Result<()> {
        if paragraph.page_break_before && !self.at_page_top() {
            self.break_page()?;
        }
        let laid = self.layout_paragraph(paragraph, CONTENT_WIDTH, false);
        if !self.at_page_top() {
            self.cursor -= laid.space_before;
        }
        for line in &laid.lines {
            if self.cursor - line.height() < PAGE_MARGIN {
                self.break_page()?;
            }
            self.draw_line(line, laid.align, PAGE_MARGIN + laid.indent, CONTENT_WIDTH - laid.indent, self.cursor)?;
            self.cursor -= line.height();
        }
        self.cursor -= laid.space_after;
        Ok(())
    }

    fn image(&mut self, image: &WordImage) -> Result<()> {
        if image.page_break_before && !self.at_page_top() {
            self.break_page()?;
        }
        let max_h = A4_HEIGHT - 2.0 * PAGE_MARGIN;
        let (mut w, mut h) = (image.width_pt.max(1.0), image.height_pt.max(1.0));
        let scale = (CONTENT_WIDTH / w).min(max_h / h).min(1.0);
        w *= scale;
        h *= scale;
        if self.cursor - h < PAGE_MARGIN {
            self.break_page()?;
        }
        let x = match image.align {
            Align::Center => PAGE_MARGIN + (CONTENT_WIDTH - w) / 2.0,
            Align::Right => PAGE_MARGIN + CONTENT_WIDTH - w,
            _ => PAGE_MARGIN,
        };
        let y = self.cursor - h;
        let mut object = match image.format {
            ImageFormat::Jpeg => {
                let mut object = PdfPageImageObject::new_from_jpeg_reader(&self.document, std::io::Cursor::new(image.data.clone()))
                    .context("Failed embedding JPEG image")?;
                object.scale(w, h).context("Failed sizing JPEG image")?;
                object
            }
            ImageFormat::Png => {
                let reader = image::ImageReader::with_format(
                    std::io::Cursor::new(image.data.as_slice()), image::ImageFormat::Png);
                let decoded = crate::geometry::decode_bounded_image(reader).context("Failed decoding PNG image")?;
                // Flatten transparency onto white so transparent areas never turn black.
                let rgba = decoded.into_rgba8();
                let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
                for (x, y, p) in rgba.enumerate_pixels() {
                    let a = p[3] as u32;
                    let blend = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
                    rgb.put_pixel(x, y, image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])]));
                }
                PdfPageImageObject::new_with_size(&self.document, &image::DynamicImage::ImageRgb8(rgb), PdfPoints::new(w), PdfPoints::new(h))
                    .context("Failed embedding PNG image")?
            }
        };
        object.translate(PdfPoints::new(x), PdfPoints::new(y)).context("Failed positioning Word image")?;
        self.page.objects_mut().add_image_object(object).context("Failed placing image on PDF page")?;
        self.cursor = y - self.body_size * 0.6;
        Ok(())
    }

    fn flush_table_page(
        &mut self,
        col_x: &[f32],
        borderless: bool,
        page_rects: &mut [Option<OwnerPageRect>],
        page_owners: &mut Vec<usize>,
        owners: &mut [CellOwner],
    ) -> Result<()> {
        for &id in page_owners.iter() {
            let owner = &owners[id];
            let rect = page_rects[id].expect("registered page owner");
            if let Some(rgb) = owner.shading {
                self.fill_rect(col_x[owner.start], rect.bottom, col_x[owner.start + owner.span], rect.top, rgb)?;
            }
        }
        for &id in page_owners.iter() {
            let owner = &mut owners[id];
            let rect = page_rects[id].expect("registered page owner");
            let left = col_x[owner.start];
            let right = col_x[owner.start + owner.span];
            let mut y = (rect.top - CELL_PAD_Y) as f64;
            let bottom_limit = (rect.bottom + CELL_PAD_Y) as f64;
            while owner.line_cursor < owner.lines.len() {
                let (line, align, indent) = &owner.lines[owner.line_cursor];
                let height = line.height() as f64;
                if y - height < bottom_limit - 1e-3 { break; }
                self.draw_line(line, *align, left + CELL_PAD_X + indent,
                    (right - left - 2.0 * CELL_PAD_X - indent).max(10.0), y as f32)?;
                y -= height;
                owner.line_cursor += 1;
                owner.remaining_height = (owner.remaining_height - height).max(0.0);
            }
        }
        if !borderless {
            let black = PdfColor::new(0, 0, 0, 255);
            for &id in page_owners.iter() {
                let owner = &owners[id];
                let rect = page_rects[id].expect("registered page owner");
                let left = col_x[owner.start];
                let right = col_x[owner.start + owner.span];
                self.rule(left, rect.top, left, rect.bottom, BORDER, black)?;
                self.rule(right, rect.top, right, rect.bottom, BORDER, black)?;
                self.rule(left, rect.top, right, rect.top, BORDER, black)?;
                self.rule(left, rect.bottom, right, rect.bottom, BORDER, black)?;
            }
        }
        for id in page_owners.drain(..) { page_rects[id] = None; }
        Ok(())
    }

    fn table(&mut self, table: &WordTable, cancel: &AtomicBool) -> Result<()> {
        if table.rows.is_empty() {
            return Ok(());
        }
        if table.page_break_before && !self.at_page_top() {
            self.break_page()?;
        }
        let columns = table.grid_columns();
        if columns == 0 {
            return Ok(());
        }
        let widths = fitted_widths(&table.col_widths_pt, columns, CONTENT_WIDTH);
        let mut col_x = vec![PAGE_MARGIN];
        for w in &widths {
            col_x.push(col_x.last().unwrap() + w);
        }
        let max_page_room = A4_HEIGHT - 2.0 * PAGE_MARGIN;

        // 1. Resolve cell owners with exact (start, span) matching for VMerge::Continue
        let mut owners: Vec<CellOwner> = Vec::new();
        let mut row_owners: Vec<Vec<usize>> = Vec::with_capacity(table.rows.len());

        for (r, row) in table.rows.iter().enumerate() {
            let mut current_row_owners = Vec::new();
            let mut col = 0usize;

            for cell in row {
                if col >= columns {
                    break;
                }
                let span = (cell.span.max(1) as usize).min(columns - col);

                let mut matched_owner_id = None;
                if cell.vmerge == VMerge::Continue {
                    if let Some(previous) = row_owners.last() {
                        matched_owner_id = previous.iter().copied().find(|&id| {
                            owners[id].start == col && owners[id].span == span
                        });
                        if let Some(id) = matched_owner_id { owners[id].last_row = r; }
                    }
                }

                if let Some(id) = matched_owner_id {
                    if owners[id].shading.is_none() && cell.shading.is_some() {
                        owners[id].shading = cell.shading;
                    }
                    current_row_owners.push(id);
                } else {
                    let owner_id = owners.len();
                    let width = col_x[col + span] - col_x[col] - 2.0 * CELL_PAD_X;
                    let mut lines = Vec::new();
                    let mut remaining_height = 0.0;
                    for paragraph in cell.paragraphs.iter().filter(|paragraph| !paragraph.is_blank()) {
                        let laid = self.layout_paragraph(paragraph, width.max(10.0), true);
                        for line in laid.lines {
                            let height = line.height();
                            if !height.is_finite() || height <= 0.0 || height + 2.0 * CELL_PAD_Y > max_page_room {
                                bail!("Table line cannot fit within a PDF page");
                            }
                            remaining_height += height as f64;
                            lines.push((line, laid.align, laid.indent));
                        }
                    }
                    owners.push(CellOwner {
                        start: col,
                        span,
                        shading: cell.shading,
                        lines,
                        line_cursor: 0,
                        remaining_height,
                        first_row: r,
                        last_row: r,
                    });
                    current_row_owners.push(owner_id);
                }
                col += span;
            }

            // Outline / filler for short rows where cells do not cover all columns
            if col < columns {
                let span = columns - col;
                let owner_id = owners.len();
                owners.push(CellOwner {
                    start: col,
                    span,
                    shading: None,
                    lines: Vec::new(),
                    line_cursor: 0,
                    remaining_height: 0.0,
                    first_row: r,
                    last_row: r,
                });
                current_row_owners.push(owner_id);
            }

            row_owners.push(current_row_owners);
        }

        // 2. Compute minimum row heights (as minimums, not ceilings)
        let min_row = self.body_size * 1.32 + 2.0 * CELL_PAD_Y;
        let mut min_row_heights = vec![min_row; table.rows.len()];

        for owner in &owners {
            let content_h = owner.remaining_height as f32 + 2.0 * CELL_PAD_Y;
            if owner.first_row == owner.last_row {
                let r = owner.first_row;
                min_row_heights[r] = min_row_heights[r].max(content_h);
            }
        }

        let mut multi_row_indices: Vec<usize> = (0..owners.len()).filter(|&i| owners[i].first_row < owners[i].last_row).collect();
        multi_row_indices.sort_by_key(|&i| owners[i].last_row);
        for i in multi_row_indices {
            let owner = &owners[i];
            let content_h = owner.remaining_height as f32 + 2.0 * CELL_PAD_Y;
            let available: f32 = min_row_heights[owner.first_row..=owner.last_row].iter().sum();
            if content_h > available {
                min_row_heights[owner.last_row] += content_h - available;
            }
        }

        let num_rows = table.rows.len();
        let mut page_rects: Vec<Option<OwnerPageRect>> = vec![None; owners.len()];
        let mut page_owners = Vec::new();
        let mut r = 0usize;
        let mut row_rem_min_height = min_row_heights[0];

        // 3. Pagination loop
        while r < num_rows {
            if cancel.load(Ordering::Relaxed) {
                bail!("Word-to-PDF cancelled");
            }

            let page_room = self.cursor - PAGE_MARGIN;
            let has_page_content = !page_owners.is_empty();

            // If on a partially occupied page with insufficient room for a minimal row, break to fresh page
            if !self.at_page_top() && page_room < min_row && !has_page_content {
                self.break_page()?;
                continue;
            }

            // If page room is exhausted, flush current page and break to fresh page
            if page_room <= 1e-4 {
                self.flush_table_page(&col_x, table.borderless, &mut page_rects, &mut page_owners, &mut owners)?;
                self.break_page()?;
                continue;
            }

            // Step 1: Allocate current row's remaining minimum height into available page room
            if row_rem_min_height > 1e-4 {
                let alloc = row_rem_min_height.min(page_room);
                let top = self.cursor;
                let bottom = top - alloc;
                self.cursor = bottom;
                row_rem_min_height -= alloc;

                // Step 2: Extend every active owner's page-local rectangle by that allocation
                for &owner_id in &row_owners[r] {
                    if let Some(rect) = &mut page_rects[owner_id] {
                        rect.bottom = bottom;
                    } else {
                        page_rects[owner_id] = Some(OwnerPageRect { top, bottom });
                        page_owners.push(owner_id);
                    }
                }
                continue;
            }

            // Row height is a minimum. Extend its terminal fragment until every
            // owner ending here has room for all remaining whole lines.
            let mut max_extra_needed = 0.0f32;
            for &id in &row_owners[r] {
                let owner = &owners[id];
                if owner.last_row == r && owner.line_cursor < owner.lines.len() {
                    let needed = owner.remaining_height + (2.0 * CELL_PAD_Y) as f64;
                    let current = page_rects[id].map(|rect| (rect.top - rect.bottom) as f64).unwrap_or(0.0);
                    max_extra_needed = max_extra_needed.max((needed - current).max(0.0) as f32);
                }
            }

            // Step 4: If ending owners do not fit, extend this logical row
            if max_extra_needed > 1e-4 {
                let alloc = max_extra_needed.min(page_room);
                let top = self.cursor;
                let bottom = top - alloc;
                self.cursor = bottom;

                for &owner_id in &row_owners[r] {
                    if let Some(rect) = &mut page_rects[owner_id] {
                        rect.bottom = bottom;
                    } else {
                        page_rects[owner_id] = Some(OwnerPageRect { top, bottom });
                        page_owners.push(owner_id);
                    }
                }
                continue;
            }

            // Step 6: Advance logical row only after min height is consumed and all ending owners exhausted
            r += 1;
            if r < num_rows {
                row_rem_min_height = min_row_heights[r];
            }
        }

        // Flush any remaining fragments on the final page
        if !page_owners.is_empty() {
            self.flush_table_page(&col_x, table.borderless, &mut page_rects, &mut page_owners, &mut owners)?;
        }
        if owners.iter().any(|owner| owner.line_cursor != owner.lines.len()) {
            bail!("Table pagination did not emit every content line");
        }

        self.cursor -= self.body_size * 0.7;
        Ok(())
    }

    pub fn compose(
        mut self,
        doc: &WordDocument,
        output: &str,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u32, u32),
    ) -> Result<()> {
        let total = doc.blocks.len() as u32;
        for (i, block) in doc.blocks.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                bail!("Word-to-PDF cancelled");
            }
            match block {
                Block::Paragraph(p) => self.paragraph(p)?,
                Block::Table(t) => self.table(t, cancel)?,
                Block::Image(img) => self.image(img)?,
            }
            progress(i as u32 + 1, total);
        }
        self.page.regenerate_content().ok();
        self.document
            .save_to_file(output)
            .with_context(|| format!("Failed saving PDF to '{output}'"))?;
        Ok(())
    }
}
