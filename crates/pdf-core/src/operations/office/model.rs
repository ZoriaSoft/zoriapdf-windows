//! Document model shared by the Word reader/writer and the PDF layout/extraction code.
//!
//! It keeps what survives a conversion in both directions: paragraphs with styled runs
//! (bold, italic, underline, strike, colour, size), headings, alignment, list prefixes,
//! tables (column widths, horizontal and vertical merges, cell shading) and images.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ParagraphKind {
    #[default]
    Body,
    Heading1,
    Heading2,
    Heading3,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// `None` means the default (black) text colour.
    pub color: Option<[u8; 3]>,
    /// Font size in points; `None` uses the paragraph's default size.
    pub size: Option<f32>,
}

/// A piece of text with one style. `'\n'` inside the text is a line break, `'\t'` a tab.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: TextStyle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WordParagraph {
    pub runs: Vec<Run>,
    pub kind: ParagraphKind,
    pub align: Align,
    /// List nesting level (0 = top level) when the paragraph is a list item.
    pub list_level: Option<u8>,
    /// Starts a new page in the output document.
    pub page_break_before: bool,
}

impl WordParagraph {
    pub fn plain(text: impl Into<String>) -> Self {
        WordParagraph {
            runs: vec![Run { text: text.into(), style: TextStyle::default() }],
            ..Default::default()
        }
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }

    pub fn is_blank(&self) -> bool {
        self.runs.iter().all(|run| run.text.trim().is_empty())
    }

    /// Appends text, merging with the last run when the style is identical.
    pub fn push(&mut self, text: &str, style: &TextStyle) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.runs.last_mut() {
            if last.style == *style {
                last.text.push_str(text);
                return;
            }
        }
        self.runs.push(Run { text: text.to_string(), style: style.clone() });
    }

    /// Removes trailing whitespace from the paragraph's last runs.
    pub fn trim_end(&mut self) {
        while let Some(last) = self.runs.last_mut() {
            let trimmed = last.text.trim_end().len();
            last.text.truncate(trimmed);
            if last.text.is_empty() {
                self.runs.pop();
            } else {
                break;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn sniff(data: &[u8]) -> Option<ImageFormat> {
        if data.starts_with(&[0x89, b'P', b'N', b'G']) {
            Some(ImageFormat::Png)
        } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(ImageFormat::Jpeg)
        } else {
            None
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WordImage {
    pub data: Vec<u8>,
    pub format: ImageFormat,
    /// Display size in points.
    pub width_pt: f32,
    pub height_pt: f32,
    pub align: Align,
    pub page_break_before: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VMerge {
    /// Not part of a vertical merge.
    #[default]
    None,
    /// First cell of a vertically merged group.
    Restart,
    /// Covered by the cell above; carries no content of its own.
    Continue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableCell {
    pub paragraphs: Vec<WordParagraph>,
    /// Number of grid columns this cell covers (horizontal merge).
    pub span: u16,
    pub vmerge: VMerge,
    pub shading: Option<[u8; 3]>,
}

impl TableCell {
    #[cfg(test)]
    pub fn new(paragraphs: Vec<WordParagraph>) -> Self {
        TableCell { paragraphs, span: 1, vmerge: VMerge::None, shading: None }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct WordTable {
    pub rows: Vec<Vec<TableCell>>,
    /// Grid column widths in points; may be empty (equal widths).
    pub col_widths_pt: Vec<f32>,
    pub page_break_before: bool,
    /// Drawn without cell borders (layout tables, column-aligned text).
    pub borderless: bool,
}

impl WordTable {
    /// Number of grid columns: the widest row (spans counted) or the declared grid.
    pub fn grid_columns(&self) -> usize {
        let widest = self
            .rows
            .iter()
            .map(|row| row.iter().map(|cell| cell.span.max(1) as usize).sum::<usize>())
            .max()
            .unwrap_or(0);
        widest.max(self.col_widths_pt.len()).max(1)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Paragraph(WordParagraph),
    Table(WordTable),
    Image(WordImage),
}

impl Block {
    pub fn set_page_break_before(&mut self, value: bool) {
        match self {
            Block::Paragraph(p) => p.page_break_before = value,
            Block::Table(t) => t.page_break_before = value,
            Block::Image(i) => i.page_break_before = value,
        }
    }
}

/// Document-level settings plus the content blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct WordDocument {
    pub blocks: Vec<Block>,
    /// Body text size in points (written as the Normal style size).
    pub body_size: f32,
}

impl Default for WordDocument {
    fn default() -> Self {
        WordDocument { blocks: Vec::new(), body_size: 11.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub paragraphs: usize,
    pub tables: usize,
    pub images: usize,
}

impl WordDocument {
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for block in &self.blocks {
            match block {
                Block::Paragraph(_) => counts.paragraphs += 1,
                Block::Table(_) => counts.tables += 1,
                Block::Image(_) => counts.images += 1,
            }
        }
        counts
    }

    pub fn has_content(&self) -> bool {
        self.blocks.iter().any(|block| match block {
            Block::Paragraph(p) => !p.is_blank(),
            Block::Table(t) => !t.rows.is_empty(),
            Block::Image(_) => true,
        })
    }
}

/// Parses a Word/HTML style hex colour ("FF0000"); "auto" and black give `None`.
pub fn parse_hex_color(value: &str) -> Option<[u8; 3]> {
    let hex = value.trim().trim_start_matches('#');
    let bytes = hex.as_bytes();
    if bytes.len() != 6 || !bytes.iter().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let rgb = [byte(0)?, byte(2)?, byte(4)?];
    if rgb == [0, 0, 0] {
        None
    } else {
        Some(rgb)
    }
}

pub fn hex_color(rgb: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}
