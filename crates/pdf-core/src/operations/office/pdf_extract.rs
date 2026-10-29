//! Rebuilds editable content from a PDF page: styled text runs (bold, italic,
//! colour, size), paragraphs and their alignment, tables drawn with ruling lines
//! (including merged cells and cell shading) and embedded images.
//!
//! Tables are found from the page's vector lines: connected horizontal and vertical
//! rules form a grid, missing inner rules mean merged cells, and the characters inside
//! each cell become that cell's paragraphs. Tables drawn without any lines cannot be
//! told apart from ordinary columns of text and stay as paragraphs.
use anyhow::{bail, Context, Result};
use image::GenericImageView;
use crate::geometry::display_to_page_matrix;
use crate::raster::{checked_matrix, image_bounds, image_source_size, visit_page_objects};
use pdfium_render::prelude::*;

use super::model::{Align, Block, ImageFormat, ParagraphKind, TableCell, TextStyle, VMerge, WordImage, WordParagraph, WordTable};

#[derive(Clone, Debug)]
pub(crate) struct Ch {
    pub c: char,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    pub size: f32,
    pub style: TextStyle,
}

impl Ch {
    fn is_newline(&self) -> bool {
        self.c == '\n' || self.c == '\r'
    }
    fn cx(&self) -> f32 {
        (self.left + self.right) / 2.0
    }
    fn cy(&self) -> f32 {
        (self.bottom + self.top) / 2.0
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TextLine {
    pub chars: Vec<Ch>,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub size: f32,
}

impl TextLine {
    fn text(&self) -> String {
        self.chars.iter().map(|c| c.c).collect()
    }
    fn width(&self) -> f32 {
        self.right - self.left
    }
}

fn near_black(color: PdfColor) -> bool {
    color.red() < 40 && color.green() < 40 && color.blue() < 40
}

pub(crate) fn collect_chars(text: &PdfPageText) -> Vec<Ch> {
    let mut out = Vec::new();
    for ch in text.chars().iter() {
        let Some(c) = ch.unicode_char() else { continue };
        let generated = ch.is_generated().unwrap_or(false);
        let (left, right, bottom, top) = match ch.loose_bounds() {
            Ok(r) => (r.left().value, r.right().value, r.bottom().value, r.top().value),
            Err(_) => (0.0, 0.0, 0.0, 0.0),
        };
        let size = ch.scaled_font_size().value;
        let mut style = TextStyle::default();
        if !generated && !c.is_whitespace() {
            let name = ch.font_name().to_ascii_lowercase();
            let weight_bold = matches!(
                ch.font_weight(),
                Some(PdfFontWeight::Weight600) | Some(PdfFontWeight::Weight700Bold) | Some(PdfFontWeight::Weight800) | Some(PdfFontWeight::Weight900)
            );
            let fake_bold = matches!(ch.render_mode(), Ok(PdfPageTextRenderMode::FilledThenStroked));
            style.bold = weight_bold
                || fake_bold
                || ["bold", "black", "heavy", "semibold", "demi"].iter().any(|k| name.contains(k));
            style.italic = ch.font_is_italic() || name.contains("italic") || name.contains("oblique");
            if let Ok(color) = ch.fill_color() {
                if !near_black(color) {
                    style.color = Some([color.red(), color.green(), color.blue()]);
                }
            }
            if size > 0.5 {
                style.size = Some((size * 2.0).round() / 2.0);
            }
        }
        out.push(Ch { c, left, right, bottom, top, size, style });
    }
    out
}

/// Splits characters (in PDFium reading order) into lines.
pub(crate) fn build_lines(chars: &[Ch]) -> Vec<TextLine> {
    let mut lines: Vec<TextLine> = Vec::new();
    let mut current: Vec<Ch> = Vec::new();
    let finish = |current: &mut Vec<Ch>, lines: &mut Vec<TextLine>| {
        let chars: Vec<Ch> = std::mem::take(current);
        let visible: Vec<&Ch> = chars.iter().filter(|c| !c.c.is_whitespace() && c.right > c.left).collect();
        if visible.is_empty() {
            return;
        }
        let mut line = TextLine {
            left: visible.iter().map(|c| c.left).fold(f32::MAX, f32::min),
            right: visible.iter().map(|c| c.right).fold(f32::MIN, f32::max),
            top: visible.iter().map(|c| c.top).fold(f32::MIN, f32::max),
            bottom: visible.iter().map(|c| c.bottom).fold(f32::MAX, f32::min),
            size: visible.iter().map(|c| c.size).fold(0.0, f32::max),
            chars,
        };
        // Trim surrounding whitespace characters.
        while line.chars.last().is_some_and(|c| c.c.is_whitespace()) {
            line.chars.pop();
        }
        let lead = line.chars.iter().take_while(|c| c.c.is_whitespace()).count();
        line.chars.drain(..lead);
        lines.push(line);
    };
    for ch in chars {
        if ch.is_newline() {
            finish(&mut current, &mut lines);
            continue;
        }
        // Safety net when PDFium did not insert a newline: a jump to another baseline
        // together with a move back to the left starts a new line.
        if let Some(last) = current.iter().rev().find(|c| !c.c.is_whitespace() && c.right > c.left) {
            if !ch.c.is_whitespace() && ch.right > ch.left {
                let size = last.size.max(ch.size).max(1.0);
                if (last.bottom - ch.bottom).abs() > size * 0.6 && ch.left < last.left {
                    finish(&mut current, &mut lines);
                }
            }
        }
        current.push(ch.clone());
    }
    finish(&mut current, &mut lines);
    lines
}

fn starts_list_item(text: &str) -> bool {
    let t = text.trim_start();
    let mut chars = t.chars();
    match chars.next() {
        Some('•' | '◦' | '▪' | '▫' | '●' | '○' | '■' | '□' | '–' | '·' | '➢' | '✓' | '-') => {
            chars.next().is_some_and(char::is_whitespace)
        }
        Some(c) if c.is_ascii_digit() => {
            let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
            let rest = &t[digits.len()..];
            digits.len() <= 3 && (rest.starts_with(". ") || rest.starts_with(") "))
        }
        _ => false,
    }
}

/// A paragraph rebuilt from lines, with its position (for ordering) and dominant size.
#[derive(Clone, Debug)]
pub(crate) struct RawParagraph {
    pub paragraph: WordParagraph,
    pub top: f32,
}

/// Groups lines into paragraphs. `region` is (left, right) of the text area the
/// lines belong to (the page's text extent or a table cell) for alignment checks.
pub(crate) fn build_paragraphs(lines: &[TextLine], region: (f32, f32)) -> Vec<RawParagraph> {
    let (region_left, region_right) = region;
    let region_width = (region_right - region_left).max(1.0);
    let mut groups: Vec<Vec<&TextLine>> = Vec::new();
    for line in lines {
        let start_new = match groups.last().and_then(|g| g.last()) {
            None => true,
            Some(prev) => {
                let size = prev.size.max(line.size).max(1.0);
                let baseline_gap = prev.bottom - line.bottom;
                let below = baseline_gap > size * 0.3;
                let tight = baseline_gap <= size * 1.5;
                let same_size = (prev.size - line.size).abs() <= 1.0;
                let prev_short = prev.right < region_right - region_width * 0.15 - size;
                let indented = line.left > prev.left + size * 1.5;
                let prev_text = prev.text();
                let ends_sentence = prev_text.trim_end().ends_with([':', '.', '!', '?']);
                let style_switch = prev.chars.iter().all(|c| c.c.is_whitespace() || c.style.bold)
                    != line.chars.iter().all(|c| c.c.is_whitespace() || c.style.bold);
                !(below && tight && same_size)
                    || starts_list_item(&line.text())
                    || indented
                    || (prev_short && (ends_sentence || style_switch || prev.width() < region_width * 0.5))
                    || style_switch
            }
        };
        if start_new {
            groups.push(vec![line]);
        } else {
            groups.last_mut().unwrap().push(line);
        }
    }

    groups
        .into_iter()
        .map(|group| {
            let mut paragraph = WordParagraph::default();
            for (i, line) in group.iter().enumerate() {
                if i > 0 {
                    // Rejoin a word hyphenated across lines; otherwise lines join with a space.
                    let prev_ends_hyphen = group[i - 1].chars.last().is_some_and(|c| c.c == '-' || c.c == '\u{AD}');
                    let next_lower = line.chars.first().is_some_and(|c| c.c.is_lowercase());
                    if prev_ends_hyphen && next_lower {
                        if let Some(run) = paragraph.runs.last_mut() {
                            run.text.pop();
                        }
                    } else {
                        let style = paragraph.runs.last().map(|r| r.style.clone()).unwrap_or_default();
                        paragraph.push(" ", &style);
                    }
                }
                let mut last_style = paragraph.runs.last().map(|r| r.style.clone()).unwrap_or_default();
                for ch in &line.chars {
                    if ch.c.is_whitespace() {
                        // Spaces take the surrounding style to avoid needless run splits.
                        paragraph.push(" ", &last_style);
                    } else if ch.c != '\u{FFFE}' && ch.c != '\u{FFFF}' && (ch.c as u32) >= 0x20 {
                        let mut buf = [0u8; 4];
                        paragraph.push(ch.c.encode_utf8(&mut buf), &ch.style);
                        last_style = ch.style.clone();
                    }
                }
            }
            // Collapse runs of spaces PDFium produced from wide gaps.
            for run in &mut paragraph.runs {
                while run.text.contains("  ") {
                    run.text = run.text.replace("  ", " ");
                }
            }
            paragraph.trim_end();
            let left = group.iter().map(|l| l.left).fold(f32::MAX, f32::min);
            let right = group.iter().map(|l| l.right).fold(f32::MIN, f32::max);
            let tolerance = region_width * 0.05;
            paragraph.align = if group.len() == 1 {
                let left_gap = left - region_left;
                let right_gap = region_right - right;
                if left_gap > region_width * 0.08 && (left_gap - right_gap).abs() < tolerance.max(6.0) {
                    Align::Center
                } else if left_gap > region_width * 0.35 && right_gap < tolerance.max(4.0) {
                    Align::Right
                } else {
                    Align::Left
                }
            } else {
                let full = group[..group.len() - 1].iter().all(|l| region_right - l.right < 3.0 && l.left - region_left < 3.0);
                if full && group.len() >= 3 {
                    Align::Justify
                } else {
                    Align::Left
                }
            };
            RawParagraph { paragraph, top: group[0].top }
        })
        .filter(|raw| !raw.paragraph.is_blank())
        .collect()
}

// ---------------------------------------------------------------- tables

#[derive(Clone, Copy, Debug)]
pub(crate) struct HRule {
    pub y: f32,
    pub x1: f32,
    pub x2: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VRule {
    pub x: f32,
    pub y1: f32,
    pub y2: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FilledBox {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    pub color: [u8; 3],
}

const THIN: f32 = 3.0;
const AXIS_TOLERANCE: f32 = 0.8;

fn is_white(color: PdfColor) -> bool {
    color.red() > 245 && color.green() > 245 && color.blue() > 245
}


/// Horizontal/vertical rules and filled boxes drawn on the page.
pub(crate) fn collect_rules(page: &PdfPage) -> Result<(Vec<HRule>, Vec<VRule>, Vec<FilledBox>)> {
    let mut hs = Vec::new();
    let mut vs = Vec::new();
    let mut boxes = Vec::new();
    visit_page_objects(page, |object, ancestor, _| {
        let Some(path) = object.as_path_object() else { return Ok(()) };
        let filled = !matches!(path.fill_mode(), Ok(PdfPathFillMode::None) | Err(_));
        let stroked = path.is_stroked().unwrap_or(false);
        if !filled && !stroked {
            return Ok(());
        }
        let fill = path.fill_color().unwrap_or(PdfColor::new(0, 0, 0, 255));
        let stroke = path.stroke_color().unwrap_or(PdfColor::new(0, 0, 0, 255));
        let visible_fill = filled && !is_white(fill) && fill.alpha() > 0;
        let visible_stroke = stroked && !is_white(stroke) && stroke.alpha() > 0;
        let matrix = checked_matrix(path.matrix()?.multiply(ancestor))?;
        let segments = path.segments().transform(matrix);
        // Walk sub-paths, collecting straight edges.
        let mut subpaths: Vec<(Vec<(f32, f32)>, bool)> = Vec::new();
        let mut points: Vec<(f32, f32)> = Vec::new();
        let mut straight = true;
        for segment in segments.iter() {
            let (x, y) = segment.point();
            let point = (x.value, y.value);
            match segment.segment_type() {
                PdfPathSegmentType::MoveTo => {
                    if points.len() > 1 {
                        subpaths.push((std::mem::take(&mut points), straight));
                    }
                    points.clear();
                    straight = true;
                    points.push(point);
                }
                PdfPathSegmentType::LineTo => points.push(point),
                _ => {
                    straight = false;
                    points.push(point);
                }
            }
            if segment.is_close() && points.len() > 1 {
                let first = points[0];
                points.push(first);
            }
        }
        if points.len() > 1 {
            subpaths.push((points, straight));
        }
        for (pts, straight) in subpaths {
            if !straight {
                continue;
            }
            let left = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
            let right = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
            let bottom = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
            let top = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
            let (w, h) = (right - left, top - bottom);
            let axis_aligned = pts.windows(2).all(|p| (p[0].0 - p[1].0).abs() < AXIS_TOLERANCE || (p[0].1 - p[1].1).abs() < AXIS_TOLERANCE);
            if visible_fill && axis_aligned && pts.len() >= 4 {
                // A thin filled box is a rule; a larger one is a background.
                if h <= THIN && w > THIN {
                    hs.push(HRule { y: (top + bottom) / 2.0, x1: left, x2: right });
                    continue;
                }
                if w <= THIN && h > THIN {
                    vs.push(VRule { x: (left + right) / 2.0, y1: bottom, y2: top });
                    continue;
                }
                if w > THIN && h > THIN {
                    boxes.push(FilledBox { left, right, bottom, top, color: [fill.red(), fill.green(), fill.blue()] });
                }
            }
            if visible_stroke {
                for p in pts.windows(2) {
                    let ((x1, y1), (x2, y2)) = (p[0], p[1]);
                    if (y1 - y2).abs() < AXIS_TOLERANCE && (x1 - x2).abs() > THIN {
                        hs.push(HRule { y: (y1 + y2) / 2.0, x1: x1.min(x2), x2: x1.max(x2) });
                    } else if (x1 - x2).abs() < AXIS_TOLERANCE && (y1 - y2).abs() > THIN {
                        vs.push(VRule { x: (x1 + x2) / 2.0, y1: y1.min(y2), y2: y1.max(y2) });
                    }
                }
            }
        }
        Ok(())
    })?;
    Ok((merge_h(hs), merge_v(vs), boxes))
}

fn merge_h(mut rules: Vec<HRule>) -> Vec<HRule> {
    rules.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x1.total_cmp(&b.x1)));
    let mut out: Vec<HRule> = Vec::new();
    for rule in rules {
        if let Some(last) = out.iter_mut().rev().find(|r| (r.y - rule.y).abs() <= 1.5 && rule.x1 <= r.x2 + 2.0 && rule.x2 >= r.x1 - 2.0) {
            last.x1 = last.x1.min(rule.x1);
            last.x2 = last.x2.max(rule.x2);
        } else {
            out.push(rule);
        }
    }
    out
}

fn merge_v(mut rules: Vec<VRule>) -> Vec<VRule> {
    rules.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y1.total_cmp(&b.y1)));
    let mut out: Vec<VRule> = Vec::new();
    for rule in rules {
        if let Some(last) = out.iter_mut().rev().find(|r| (r.x - rule.x).abs() <= 1.5 && rule.y1 <= r.y2 + 2.0 && rule.y2 >= r.y1 - 2.0) {
            last.y1 = last.y1.min(rule.y1);
            last.y2 = last.y2.max(rule.y2);
        } else {
            out.push(rule);
        }
    }
    out
}

fn cluster(mut values: Vec<f32>, tolerance: f32) -> Vec<f32> {
    values.sort_by(f32::total_cmp);
    let mut groups: Vec<Vec<f32>> = Vec::new();
    for v in values {
        match groups.last_mut() {
            Some(g) if v - *g.last().unwrap() <= tolerance => g.push(v),
            _ => groups.push(vec![v]),
        }
    }
    groups.into_iter().map(|g| g.iter().sum::<f32>() / g.len() as f32).collect()
}

/// A grid found on the page. Rows go top to bottom (ys descending).
#[derive(Clone, Debug)]
pub(crate) struct Grid {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    /// cells[r] = (start column, span, vmerge) in grid order.
    pub cells: Vec<Vec<(usize, usize, VMerge)>>,
}

impl Grid {
    fn contains(&self, x: f32, y: f32) -> bool {
        x > self.xs[0] && x < *self.xs.last().unwrap() && y < self.ys[0] && y > *self.ys.last().unwrap()
    }

    /// The (row, cell index) owning a point, following vertical merges up to their first row.
    fn owner(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        if !self.contains(x, y) {
            return None;
        }
        let col = self.xs.windows(2).position(|w| x >= w[0] && x < w[1])?;
        let mut row = self.ys.windows(2).position(|w| y <= w[0] && y > w[1])?;
        loop {
            let index = self.cells[row].iter().position(|(start, span, _)| col >= *start && col < start + span)?;
            if self.cells[row][index].2 == VMerge::Continue && row > 0 {
                row -= 1;
                continue;
            }
            return Some((row, index));
        }
    }
}

pub(crate) fn find_grids(hs: &[HRule], vs: &[VRule]) -> Vec<Grid> {
    // Union-find over rules: a horizontal and a vertical rule that touch are connected.
    let n = hs.len() + vs.len();
    if hs.len() < 2 || vs.is_empty() || n > 6000 {
        return Vec::new();
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (i, h) in hs.iter().enumerate() {
        for (j, v) in vs.iter().enumerate() {
            if v.x >= h.x1 - 2.0 && v.x <= h.x2 + 2.0 && h.y >= v.y1 - 2.0 && h.y <= v.y2 + 2.0 {
                let (a, b) = (root(&mut parent, i), root(&mut parent, hs.len() + j));
                parent[a] = b;
            }
        }
    }
    let mut components: std::collections::BTreeMap<usize, (Vec<HRule>, Vec<VRule>)> = Default::default();
    for (i, h) in hs.iter().enumerate() {
        components.entry(root(&mut parent, i)).or_default().0.push(*h);
    }
    for (j, v) in vs.iter().enumerate() {
        components.entry(root(&mut parent, hs.len() + j)).or_default().1.push(*v);
    }

    let mut grids = Vec::new();
    for (_, (ch, cv)) in components {
        if ch.len() < 2 || cv.is_empty() {
            continue;
        }
        let mut xs = cluster(cv.iter().map(|v| v.x).collect(), 2.0);
        let mut ys = cluster(ch.iter().map(|h| h.y).collect(), 2.0);
        // Open-sided tables: the horizontal rules' ends are the outer edges.
        let min_x = ch.iter().map(|h| h.x1).fold(f32::MAX, f32::min);
        let max_x = ch.iter().map(|h| h.x2).fold(f32::MIN, f32::max);
        if min_x < xs[0] - 3.0 {
            xs.insert(0, min_x);
        }
        if max_x > *xs.last().unwrap() + 3.0 {
            xs.push(max_x);
        }
        let min_y = cv.iter().map(|v| v.y1).fold(f32::MAX, f32::min);
        let max_y = cv.iter().map(|v| v.y2).fold(f32::MIN, f32::max);
        if min_y < ys[0] - 3.0 {
            ys.insert(0, min_y);
        }
        if max_y > *ys.last().unwrap() + 3.0 {
            ys.push(max_y);
        }
        // Drop slivers left by double rules (borders drawn twice, slightly apart).
        xs = cluster(xs, 4.0);
        ys = cluster(ys, 4.0);
        ys.reverse();
        let (cols, rows) = (xs.len().saturating_sub(1), ys.len().saturating_sub(1));
        if cols == 0 || rows == 0 || cols * rows < 2 || cols > 40 || rows > 400 {
            continue;
        }
        let v_at = |x: f32, mid_y: f32| cv.iter().any(|v| (v.x - x).abs() <= 2.5 && mid_y >= v.y1 - 1.0 && mid_y <= v.y2 + 1.0);
        let h_at = |y: f32, mid_x: f32| ch.iter().any(|h| (h.y - y).abs() <= 2.5 && mid_x >= h.x1 - 1.0 && mid_x <= h.x2 + 1.0);
        let mut cells: Vec<Vec<(usize, usize, VMerge)>> = Vec::with_capacity(rows);
        for r in 0..rows {
            let mid_y = (ys[r] + ys[r + 1]) / 2.0;
            let mut row = Vec::new();
            let mut c = 0;
            while c < cols {
                let mut span = 1;
                while c + span < cols && !v_at(xs[c + span], mid_y) {
                    span += 1;
                }
                row.push((c, span, VMerge::None));
                c += span;
            }
            cells.push(row);
        }
        for r in 1..rows {
            for i in 0..cells[r].len() {
                let (start, span, _) = cells[r][i];
                let mid_x = (xs[start] + xs[start + span]) / 2.0;
                if h_at(ys[r], mid_x) {
                    continue;
                }
                if let Some(above) = cells[r - 1].iter().position(|(s, sp, _)| *s == start && *sp == span) {
                    if cells[r - 1][above].2 == VMerge::None {
                        cells[r - 1][above].2 = VMerge::Restart;
                    }
                    cells[r][i].2 = VMerge::Continue;
                }
            }
        }
        grids.push(Grid { xs, ys, cells });
    }
    grids
}

/// Builds a table from a grid and the characters inside it; `None` when the grid
/// holds too little text to be a table (charts, frames, decorations).
pub(crate) fn build_table(grid: &Grid, chars: &[Ch], boxes: &[FilledBox]) -> Option<WordTable> {
    let mut buckets: Vec<Vec<Vec<Ch>>> = grid.cells.iter().map(|row| vec![Vec::new(); row.len()]).collect();
    let mut last: Option<(usize, usize)> = None;
    for ch in chars {
        let owner = if ch.is_newline() || ch.right <= ch.left { last } else { grid.owner(ch.cx(), ch.cy()) };
        if let Some((r, i)) = owner {
            buckets[r][i].push(ch.clone());
            last = Some((r, i));
        }
    }
    let filled = buckets.iter().flatten().filter(|b| b.iter().any(|c| !c.c.is_whitespace())).count();
    if filled < 2 {
        return None;
    }
    let mut rows = Vec::with_capacity(grid.cells.len());
    for (r, row) in grid.cells.iter().enumerate() {
        let mut cells = Vec::with_capacity(row.len());
        for (i, (start, span, vmerge)) in row.iter().enumerate() {
            let left = grid.xs[*start];
            let right = grid.xs[start + span];
            let top = grid.ys[r];
            // A merged cell's region reaches down through its continued rows.
            let mut bottom_row = r;
            if *vmerge == VMerge::Restart {
                while bottom_row + 1 < grid.cells.len()
                    && grid.cells[bottom_row + 1].iter().any(|(s, sp, v)| s == start && sp == span && *v == VMerge::Continue)
                {
                    bottom_row += 1;
                }
            }
            let bottom = grid.ys[bottom_row + 1];
            let lines = build_lines(&buckets[r][i]);
            let paragraphs: Vec<WordParagraph> =
                build_paragraphs(&lines, (left + 2.0, right - 2.0)).into_iter().map(|raw| raw.paragraph).collect();
            let area = (right - left) * (top - bottom);
            let shading = boxes
                .iter()
                .filter(|b| {
                    let w = (b.right.min(right) - b.left.max(left)).max(0.0);
                    let h = (b.top.min(top) - b.bottom.max(bottom)).max(0.0);
                    area > 0.0 && w * h >= area * 0.7 && (b.right - b.left) * (b.top - b.bottom) <= area * 4.0
                })
                .map(|b| b.color)
                .last();
            cells.push(TableCell { paragraphs, span: *span as u16, vmerge: *vmerge, shading });
        }
        rows.push(cells);
    }
    let col_widths_pt = grid.xs.windows(2).map(|w| w[1] - w[0]).collect();
    Some(WordTable { rows, col_widths_pt, page_break_before: false, borderless: false })
}

// ---------------------------------------------------------------- images

const MAX_IMAGE_SIDE: u32 = 2400;

fn encode_image(image: image::DynamicImage) -> Option<(Vec<u8>, ImageFormat)> {
    let image = if image.width().max(image.height()) > MAX_IMAGE_SIDE {
        image.resize(MAX_IMAGE_SIDE, MAX_IMAGE_SIDE, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let has_alpha = image.color().has_alpha() && image.pixels().any(|(_, _, pixel)| pixel[3] < 255);
    let large = image.width() as u64 * image.height() as u64 > 250_000;
    let mut out = std::io::Cursor::new(Vec::new());
    if large && !has_alpha {
        let rgb = image.to_rgb8();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
        rgb.write_with_encoder(encoder).ok()?;
        Some((out.into_inner(), ImageFormat::Jpeg))
    } else {
        image.write_to(&mut out, image::ImageFormat::Png).ok()?;
        Some((out.into_inner(), ImageFormat::Png))
    }
}

/// Placed sizes include every enclosing Form, without decoding image pixels.
pub(crate) fn image_areas(page: &PdfPage) -> Result<Vec<f32>> {
    let mut areas = Vec::new();
    visit_page_objects(page, |object, ancestor, _| {
        if object.as_image_object().is_some() {
            let bounds = image_bounds(object, ancestor)?;
            areas.push(bounds.width().value * bounds.height().value);
        }
        Ok(())
    })?;
    Ok(areas)
}



/// Isolate one placement on a disposable page. Hiding whole unrelated Forms
/// preserves independently reused resources; metadata decoding stays disposable.
fn isolate_image(page: &PdfPage, selected: &[usize]) -> Result<(i32, i32)> {
    fn descend(object: PdfPageObject<'_>, remaining: &[usize]) -> Result<(i32, i32)> {
        let Some((&selected, tail)) = remaining.split_first() else {
            return image_source_size(object.as_image_object().context("Selected placement is not an image")?);
        };
        let form = object.as_x_object_form_object().context("Image placement no longer belongs to a Form")?;
        let mut pixels = None;
        for index in 0..form.len() {
            let mut child = form.get(index)?;
            if index as usize == selected { pixels = Some(descend(child, tail)?); }
            else { child.set_inactive()?; }
        }
        pixels.context("Selected image placement is missing from its Form")
    }
    let (&selected, tail) = selected.split_first().context("Image placement path is empty")?;
    let objects = page.objects();
    let mut pixels = None;
    for index in 0..objects.len() {
        let mut object = objects.get(index)?;
        if index as usize == selected { pixels = Some(descend(object, tail)?); }
        else { object.set_inactive()?; }
    }
    pixels.context("Selected image placement is missing from its page")
}

/// Render through PDFium's original Form tree, retaining arbitrary clips, Form
/// bounding boxes and transparency. No clipping fill-rule reconstruction and no
/// full-page bitmap allocation; unrelated text/images are temporarily inactive.
fn render_form_image(
    pdfium: &Pdfium,
    document: &PdfDocument,
    page_index: PdfPageIndex,
    selected: &[usize],
    bounds: PdfRect,
    matrix: PdfMatrix,
) -> Result<Option<(image::DynamicImage, PdfRect)>> {
    // Both page raster caches and metadata's decoded stream cache belong to this
    // temporary document. Never query image metadata on the long-lived source.
    let mut isolated = pdfium.create_new_pdf()?;
    isolated.pages_mut().copy_page_from_document(document, page_index, 0)?;
    let page = isolated.pages().get(0)?;
    let pixels = isolate_image(&page, selected)?;
    let determinant = matrix.determinant();
    if !determinant.is_finite() || determinant == 0.0 { bail!("Embedded image transform cannot be inverted"); }
    let inverse = checked_matrix(matrix.invert())?;
    // Source-pixel derivatives bound each device axis independently. A stretched
    // image retains its rows without needlessly upsampling its sparse columns.
    let density_x = pixels.0 as f32 * inverse.a().abs() + pixels.1 as f32 * inverse.b().abs();
    let density_y = pixels.0 as f32 * inverse.c().abs() + pixels.1 as f32 * inverse.d().abs();
    if !density_x.is_finite() || !density_y.is_finite() || density_x <= 0.0 || density_y <= 0.0 {
        bail!("Embedded image density is invalid");
    }
    // Word's physical bounds restore the original aspect after rasterization.
    let width = (bounds.width().value * density_x).ceil().clamp(1.0, MAX_IMAGE_SIDE as f32) as i32;
    let height = (bounds.height().value * density_y).ceil().clamp(1.0, MAX_IMAGE_SIDE as f32) as i32;
    let sx = width as f32 / bounds.width().value;
    let sy = height as f32 / bounds.height().value;
    let sample = PdfMatrix::new(sx, 0.0, 0.0, -sy, -bounds.left().value * sx, bounds.top().value * sy);
    let screen_to_page = PdfMatrix::new(1.0, 0.0, 0.0, -1.0, 0.0, page.height().value)
        .multiply(display_to_page_matrix(&page)?);
    let render_matrix = checked_matrix(screen_to_page.multiply(sample))?;
    let config = PdfRenderConfig::new().set_fixed_size(width, height)
        .limit_render_image_cache_size(true)
        .render_form_data(false).render_annotations(false)
        .set_clear_color(PdfColor::new(0, 0, 0, 0)).reset_matrix(render_matrix)?;
    let mut image = page.render_with_config(&config)?.as_image()?.into_rgba8();
    drop(page);
    drop(isolated);
    let (mut left, mut top, mut right, mut bottom) = (image.width(), image.height(), 0, 0);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        if pixel[3] == 0 {
            *pixel = image::Rgba([0, 0, 0, 0]);
        } else {
            left = left.min(x); top = top.min(y);
            right = right.max(x + 1); bottom = bottom.max(y + 1);
        }
    }
    if right == 0 || bottom == 0 { return Ok(None); }
    let clipped = PdfRect::new_from_values(
        bounds.top().value - bottom as f32 / sy,
        bounds.left().value + left as f32 / sx,
        bounds.top().value - top as f32 / sy,
        bounds.left().value + right as f32 / sx,
    );
    let image = if left == 0 && top == 0 && right == image.width() && bottom == image.height() {
        image
    } else {
        image::imageops::crop_imm(&image, left, top, right - left, bottom - top).to_image()
    };
    Ok(Some((image::DynamicImage::ImageRgba8(image), clipped)))
}


/// Extract leaf images once; text remains in the single page.text() pass.
pub(crate) fn collect_images(pdfium: &Pdfium, document: &PdfDocument, page: &PdfPage, page_index: PdfPageIndex, page_has_text: bool) -> Result<Vec<(f32, f32, WordImage)>> {
    let page_area = page.width().value * page.height().value;
    let mut out = Vec::new();
    visit_page_objects(page, |object, ancestor, path| {
        if object.as_image_object().is_none() { return Ok(()); }
        let bounds = image_bounds(object, ancestor)?;
        let (width, height) = (bounds.width().value, bounds.height().value);
        if width < 8.0 || height < 8.0 {
            return Ok(());
        }
        if page_has_text && width * height > page_area * 0.85 {
            return Ok(());
        }
        let original = checked_matrix(object.matrix()?)?;
        let matrix = checked_matrix(original.multiply(ancestor))?;
        let Some((decoded, bounds)) = render_form_image(pdfium, document, page_index, path, bounds, matrix)? else { return Ok(()); };
        let (width, height) = (bounds.width().value, bounds.height().value);
        if page_has_text && width * height > page_area * 0.85 { return Ok(()); }
        let (data, format) = encode_image(decoded).context("Cannot encode embedded image")?;
        out.push((bounds.top().value, bounds.left().value,
            WordImage { data, format, width_pt: width, height_pt: height, align: Align::Left, page_break_before: false }));
        Ok(())
    })?;
    Ok(out)
}

// ---------------------------------------------------------------- column-aligned text

/// A run of characters on one line, separated from the next by a column-sized gap.
struct Segment {
    chars: Vec<Ch>,
    left: f32,
    right: f32,
}

fn segments(line: &TextLine) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut prev_right: Option<f32> = None;
    for ch in &line.chars {
        let visible = !ch.c.is_whitespace() && ch.right > ch.left;
        if visible {
            let gap = prev_right.map(|r| ch.left - r).unwrap_or(0.0);
            if out.is_empty() || gap > ch.size.max(line.size).max(1.0) * 1.4 {
                out.push(Segment { chars: Vec::new(), left: ch.left, right: ch.right });
            }
            prev_right = Some(ch.right);
        }
        if let Some(seg) = out.last_mut() {
            seg.chars.push(ch.clone());
            if visible {
                seg.right = seg.right.max(ch.right);
            }
        }
    }
    for seg in &mut out {
        while seg.chars.last().is_some_and(|c| c.c.is_whitespace()) {
            seg.chars.pop();
        }
    }
    out
}

pub(crate) enum FlowItem {
    Lines(Vec<TextLine>),
    Table(f32, WordTable),
}

/// Finds runs of lines whose words line up in columns (tables drawn without vertical
/// rules) and turns them into borderless tables; everything else stays lines.
pub(crate) fn split_tabular(lines: Vec<TextLine>) -> Vec<FlowItem> {
    let segs: Vec<Vec<Segment>> = lines.iter().map(segments).collect();
    let mut items: Vec<FlowItem> = Vec::new();
    let mut plain: Vec<TextLine> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let mut j = i;
        while j < lines.len()
            && segs[j].len() >= 2
            && (j == i || {
                let (a, b) = (&lines[j - 1], &lines[j]);
                let gap = a.bottom - b.bottom;
                gap > 0.0 && gap <= a.size.max(b.size).max(1.0) * 2.6
            })
        {
            j += 1;
        }
        let table = if j - i >= 2 { tabular_table(&lines[i..j], &segs[i..j]) } else { None };
        match table {
            Some(table) => {
                if !plain.is_empty() {
                    items.push(FlowItem::Lines(std::mem::take(&mut plain)));
                }
                items.push(FlowItem::Table(lines[i].top, table));
                i = j;
            }
            None => {
                plain.push(lines[i].clone());
                i += 1;
            }
        }
    }
    if !plain.is_empty() {
        items.push(FlowItem::Lines(plain));
    }
    items
}

fn tabular_table(lines: &[TextLine], segs: &[Vec<Segment>]) -> Option<WordTable> {
    let starts = cluster(segs.iter().flatten().map(|s| s.left).collect(), 6.0);
    if starts.len() < 2 || starts.len() > 12 {
        return None;
    }
    // Numbered or bulleted lists also line up in two columns; they are not tables.
    let marker = |seg: &Segment| {
        let text: String = seg.chars.iter().map(|c| c.c).collect();
        let t = text.trim();
        t.chars().count() <= 3 || starts_list_item(&format!("{t} x"))
    };
    if segs.iter().all(|row| marker(&row[0])) {
        return None;
    }
    let column_of = |x: f32| starts.iter().rposition(|s| *s <= x + 6.0).unwrap_or(0);
    // Every column must be used by at least two rows.
    let mut usage = vec![0usize; starts.len()];
    for row in segs {
        let mut seen = vec![false; starts.len()];
        for seg in row {
            seen[column_of(seg.left)] = true;
        }
        for (u, s) in usage.iter_mut().zip(seen) {
            *u += usize::from(s);
        }
    }
    if usage.iter().any(|u| *u < 2) {
        return None;
    }
    let right = lines.iter().map(|l| l.right).fold(f32::MIN, f32::max);
    let mut col_widths_pt: Vec<f32> = starts.windows(2).map(|w| w[1] - w[0]).collect();
    // The last column has no next column to bound it; leave room for a wider font.
    let average = col_widths_pt.iter().sum::<f32>() / col_widths_pt.len() as f32;
    col_widths_pt.push(((right - starts[starts.len() - 1]) * 1.25 + 8.0).max(average).max(20.0));
    let mut rows = Vec::with_capacity(segs.len());
    for row in segs {
        let mut buckets: Vec<Vec<Ch>> = vec![Vec::new(); starts.len()];
        for seg in row {
            let bucket = &mut buckets[column_of(seg.left)];
            if !bucket.is_empty() {
                let mut space = seg.chars[0].clone();
                space.c = ' ';
                bucket.push(space);
            }
            bucket.extend(seg.chars.iter().cloned());
        }
        let cells = buckets
            .into_iter()
            .map(|chars| {
                let lines = build_lines(&chars);
                let region = lines.first().map(|l| (l.left, l.right)).unwrap_or((0.0, 1.0));
                let paragraphs = build_paragraphs(&lines, region).into_iter().map(|raw| raw.paragraph).collect();
                TableCell { paragraphs, span: 1, vmerge: VMerge::None, shading: None }
            })
            .collect();
        rows.push(cells);
    }
    Some(WordTable { rows, col_widths_pt, page_break_before: false, borderless: true })
}

// ---------------------------------------------------------------- underline / strike

/// PDFs draw underline and strike-through as short rules; marks the characters a rule
/// runs under (or through). Rules that are table borders, or much longer than the text
/// they touch, are left alone.
pub(crate) fn mark_text_rules(chars: &mut [Ch], hs: &[HRule], grids: &[Grid]) {
    for rule in hs {
        let on_grid = grids.iter().any(|g| {
            rule.x1 >= g.xs[0] - 3.0 && rule.x2 <= *g.xs.last().unwrap() + 3.0 && g.ys.iter().any(|y| (y - rule.y).abs() <= 2.5)
        });
        if on_grid {
            continue;
        }
        for strike in [false, true] {
            let hits: Vec<usize> = (0..chars.len())
                .filter(|&i| {
                    let c = &chars[i];
                    if c.c.is_whitespace() || c.right <= c.left || c.cx() < rule.x1 || c.cx() > rule.x2 {
                        return false;
                    }
                    let rel = (rule.y - c.bottom) / c.size.max(1.0);
                    if strike {
                        (0.42..=0.75).contains(&rel)
                    } else {
                        rel >= -0.15 && rel <= 0.35
                    }
                })
                .collect();
            if hits.is_empty() {
                continue;
            }
            let left = hits.iter().map(|&i| chars[i].left).fold(f32::MAX, f32::min);
            let right = hits.iter().map(|&i| chars[i].right).fold(f32::MIN, f32::max);
            let span = right - left;
            let length = rule.x2 - rule.x1;
            if span >= length * 0.6 && length <= span * 1.2 + 4.0 {
                for &i in &hits {
                    if strike {
                        chars[i].style.strike = true;
                    } else {
                        chars[i].style.underline = true;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- page

/// Rebuilds one page into blocks in reading order.
pub(crate) fn extract_page(pdfium: &Pdfium, document: &PdfDocument, page: &PdfPage, page_index: PdfPageIndex) -> Result<Vec<Block>> {
    let mut chars = page.text().map(|text| collect_chars(&text)).unwrap_or_default();
    let page_has_text = chars.iter().any(|c| !c.c.is_whitespace());
    let (hs, vs, boxes) = collect_rules(page)?;
    let grids = find_grids(&hs, &vs);
    mark_text_rules(&mut chars, &hs, &grids);

    // Tables first; characters they do not claim stay in the text flow.
    let mut tables: Vec<(f32, WordTable)> = Vec::new();
    let mut claimed = vec![false; chars.len()];
    for grid in &grids {
        let inside: Vec<usize> = (0..chars.len())
            .filter(|&i| !claimed[i] && !chars[i].is_newline() && chars[i].right > chars[i].left && grid.contains(chars[i].cx(), chars[i].cy()))
            .collect();
        if inside.is_empty() {
            continue;
        }
        // Keep the newlines between claimed characters so cell lines still split.
        let first = inside[0];
        let last = *inside.last().unwrap();
        let members: Vec<usize> = (first..=last)
            .filter(|&i| !claimed[i] && (inside.binary_search(&i).is_ok() || chars[i].is_newline()))
            .collect();
        let subset: Vec<Ch> = members.iter().map(|&i| chars[i].clone()).collect();
        if let Some(table) = build_table(grid, &subset, &boxes) {
            for &i in &inside {
                claimed[i] = true;
            }
            tables.push((grid.ys[0], table));
        }
    }
    let flow: Vec<Ch> = chars.iter().zip(&claimed).filter(|(_, c)| !**c).map(|(ch, _)| ch.clone()).collect();
    let lines = build_lines(&flow);
    let region = if lines.is_empty() {
        (0.0, page.width().value)
    } else {
        (
            lines.iter().map(|l| l.left).fold(f32::MAX, f32::min),
            lines.iter().map(|l| l.right).fold(f32::MIN, f32::max),
        )
    };
    let mut paragraphs = Vec::new();
    for item in split_tabular(lines) {
        match item {
            FlowItem::Lines(run) => paragraphs.extend(build_paragraphs(&run, region)),
            FlowItem::Table(top, table) => tables.push((top, table)),
        }
    }
    let images = collect_images(pdfium, document, page, page_index, page_has_text)?;

    // Paragraphs keep PDFium's reading order (multi-column text stays intact); each
    // table and image goes in front of the first paragraph that starts below it.
    let mut inserts: Vec<(f32, Block)> = tables.into_iter().map(|(top, t)| (top, Block::Table(t))).collect();
    inserts.extend(images.into_iter().map(|(top, _, img)| (top, Block::Image(img))));
    inserts.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut blocks = Vec::with_capacity(paragraphs.len() + inserts.len());
    let mut pending = inserts.into_iter().peekable();
    for raw in paragraphs {
        while pending.peek().is_some_and(|(top, _)| *top > raw.top + 1.0) {
            blocks.push(pending.next().unwrap().1);
        }
        blocks.push(Block::Paragraph(raw.paragraph));
    }
    blocks.extend(pending.map(|(_, b)| b));
    Ok(blocks)
}

/// Document-wide pass: finds the body text size, turns larger lines into headings
/// and drops sizes that only repeat the body size.
pub(crate) fn finish_styles(blocks: &mut [Block]) -> f32 {
    let mut histogram: std::collections::HashMap<i32, usize> = Default::default();
    let mut add = |p: &WordParagraph| {
        for run in &p.runs {
            if let Some(size) = run.style.size {
                *histogram.entry((size * 2.0).round() as i32).or_default() += run.text.chars().filter(|c| !c.is_whitespace()).count();
            }
        }
    };
    for block in blocks.iter() {
        match block {
            Block::Paragraph(p) => add(p),
            Block::Table(t) => t.rows.iter().flatten().flat_map(|c| &c.paragraphs).for_each(&mut add),
            Block::Image(_) => {}
        }
    }
    let body = histogram.into_iter().max_by_key(|(size, count)| (*count, -*size)).map(|(s, _)| s as f32 / 2.0).unwrap_or(11.0).clamp(6.0, 36.0);

    let dominant = |p: &WordParagraph| -> f32 {
        let mut best = (0usize, body);
        let mut sizes: std::collections::HashMap<i32, usize> = Default::default();
        for run in &p.runs {
            let n = run.text.chars().filter(|c| !c.is_whitespace()).count();
            *sizes.entry((run.style.size.unwrap_or(body) * 2.0).round() as i32).or_default() += n;
        }
        for (s, n) in sizes {
            if n > best.0 {
                best = (n, s as f32 / 2.0);
            }
        }
        best.1
    };
    let normalize = |p: &mut WordParagraph, allow_heading: bool| {
        let size = dominant(p);
        let all_bold = p.runs.iter().all(|r| r.text.trim().is_empty() || r.style.bold);
        let short = p.text().chars().count() <= 120;
        p.kind = if !allow_heading || !short {
            ParagraphKind::Body
        } else if size >= body * 1.6 {
            ParagraphKind::Heading1
        } else if size >= body * 1.3 {
            ParagraphKind::Heading2
        } else if size >= body * 1.12 && all_bold {
            ParagraphKind::Heading3
        } else {
            ParagraphKind::Body
        };
        for run in &mut p.runs {
            if p.kind != ParagraphKind::Body {
                // The heading style carries size and weight.
                run.style.size = None;
                run.style.bold = false;
            } else if run.style.size.is_some_and(|s| (s - body).abs() <= 0.75) {
                run.style.size = None;
            }
        }
        // Merge runs that became identical.
        let runs = std::mem::take(&mut p.runs);
        for run in runs {
            p.push(&run.text, &run.style);
        }
    };
    for block in blocks.iter_mut() {
        match block {
            Block::Paragraph(p) => normalize(p, true),
            Block::Table(t) => {
                for p in t.rows.iter_mut().flatten().flat_map(|c| c.paragraphs.iter_mut()) {
                    normalize(p, false);
                }
            }
            Block::Image(_) => {}
        }
    }
    body
}
