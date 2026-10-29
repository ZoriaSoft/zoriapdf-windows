//! Reads a .docx package into the document model: styled runs, headings (also
//! localized style ids such as Turkish "Balk1"), alignment, page breaks, numbered and
//! bulleted lists, tables (grid widths, gridSpan, vMerge, shading) and inline or
//! floating PNG/JPEG images. Deleted tracked changes are skipped.
use anyhow::{bail, Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use super::model::{
    parse_hex_color, Align, Block, ImageFormat, ParagraphKind, TableCell, TextStyle, VMerge, WordDocument,
    WordImage, WordParagraph, WordTable,
};

// Caps against crafted packages (zip bombs): the body and each image part.
const MAX_DOCUMENT_XML: u64 = 256 * 1024 * 1024;
const MAX_SIDE_PART: u64 = 32 * 1024 * 1024;
const MAX_IMAGE_PART: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_IMAGES: u64 = 512 * 1024 * 1024;

const MAX_XML_DEPTH: usize = 128;
const MAX_XML_NODES: usize = 1_000_000;
const MAX_XML_TREE_BYTES: usize = 64 * 1024 * 1024;
const MAX_START_TAG: usize = 256 * 1024;
const MAX_ATTR_COUNT: usize = 256;
const MAX_ATTR_VALUE_LEN: usize = 64 * 1024;

/// A minimal XML element tree; names are local (namespace prefix dropped).
#[derive(Debug, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

#[derive(Debug)]
pub enum Node {
    Element(Element),
    Text(String),
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    /// Depth-first search for the first descendant with this name.
    pub fn find(&self, name: &str) -> Option<&Element> {
        for child in self.elements() {
            if child.name == name {
                return Some(child);
            }
            if let Some(found) = child.find(name) {
                return Some(found);
            }
        }
        None
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for node in &self.children {
            match node {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => out.push_str(&e.text()),
            }
        }
        out
    }
}

fn resolve_general_ref(reference: &quick_xml::events::BytesRef) -> Result<char> {
    if reference.len() > 64 {
        bail!("XML entity reference is too long");
    }
    if let Some(character) = reference.resolve_char_ref()? {
        return Ok(character);
    }
    match reference.decode()?.as_ref() {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "quot" => Ok('"'),
        "apos" => Ok('\''),
        _ => bail!("Unsupported XML entity reference"),
    }
}

fn reserve_xml_bytes(used: &mut usize, bytes: usize) -> Result<()> {
    let next = used.checked_add(bytes).context("XML tree size overflow")?;
    if next > MAX_XML_TREE_BYTES {
        bail!("XML tree size limit exceeded");
    }
    *used = next;
    Ok(())
}

fn count_xml_node(used: &mut usize) -> Result<()> {
    *used += 1;
    if *used > MAX_XML_NODES {
        bail!("XML node count limit exceeded");
    }
    Ok(())
}

fn start_element(event: &BytesStart, tree_bytes: &mut usize) -> Result<Element> {
    if event.as_ref().len() > MAX_START_TAG {
        bail!("XML start tag size limit exceeded");
    }
    reserve_xml_bytes(tree_bytes, event.local_name().as_ref().len())?;
    let name = std::str::from_utf8(event.local_name().as_ref())?.to_owned();
    let mut attrs = Vec::new();
    for (index, attribute) in event.attributes().enumerate() {
        if index >= MAX_ATTR_COUNT {
            bail!("XML attribute count limit exceeded");
        }
        let attribute = attribute.context("Invalid XML attribute")?;
        if attribute.value.len() > MAX_ATTR_VALUE_LEN {
            bail!("XML attribute value length limit exceeded");
        }
        reserve_xml_bytes(tree_bytes, attribute.key.local_name().as_ref().len() + attribute.value.len())?;
        let key = std::str::from_utf8(attribute.key.local_name().as_ref())?.to_owned();
        let value = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0)?.into_owned();
        attrs.push((key, value));
    }
    Ok(Element { name, attrs, children: Vec::new() })
}

fn append_text(stack: &mut [Element], text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(last_elem) = stack.last_mut() {
        if let Some(Node::Text(existing)) = last_elem.children.last_mut() {
            existing.push_str(text);
        } else {
            last_elem.children.push(Node::Text(text.to_string()));
        }
    }
}

pub fn parse_xml(xml: &str) -> Result<Element> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Element> = vec![Element { name: "#root".into(), ..Default::default() }];
    let mut node_count = 0usize;
    let mut tree_bytes = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                if stack.len() > MAX_XML_DEPTH {
                    bail!("XML nesting depth limit exceeded");
                }
                count_xml_node(&mut node_count)?;
                stack.push(start_element(&event, &mut tree_bytes)?);
            }
            Ok(Event::Empty(event)) => {
                if stack.len() > MAX_XML_DEPTH {
                    bail!("XML nesting depth limit exceeded");
                }
                count_xml_node(&mut node_count)?;
                let element = start_element(&event, &mut tree_bytes)?;
                stack.last_mut().unwrap().children.push(Node::Element(element));
            }
            Ok(Event::End(_)) => {
                if stack.len() > 1 {
                    let done = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(Node::Element(done));
                }
            }
            Ok(Event::Text(event)) => {
                let text = event.xml10_content()?;
                if !text.is_empty() {
                    count_xml_node(&mut node_count)?;
                    reserve_xml_bytes(&mut tree_bytes, text.len())?;
                    append_text(&mut stack, &text);
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                let character = resolve_general_ref(&reference)?;
                let mut buffer = [0; 4];
                let text = character.encode_utf8(&mut buffer);
                count_xml_node(&mut node_count)?;
                reserve_xml_bytes(&mut tree_bytes, text.len())?;
                append_text(&mut stack, text);
            }
            Ok(Event::CData(event)) => {
                let text = event.xml10_content()?;
                if !text.is_empty() {
                    count_xml_node(&mut node_count)?;
                    reserve_xml_bytes(&mut tree_bytes, text.len())?;
                    append_text(&mut stack, &text);
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => bail!("Failed parsing Word document XML: {error}"),
            _ => {}
        }
    }
    if stack.len() != 1 {
        bail!("Word document XML is incomplete");
    }
    Ok(stack.pop().unwrap())
}

/// `<w:b/>`, `<w:i/>`... are on unless their value is explicitly off.
fn switch_on(element: &Element) -> bool {
    !matches!(
        element.attr("val").map(|v| v.to_ascii_lowercase()).as_deref(),
        Some("0") | Some("false") | Some("off") | Some("none")
    )
}

#[derive(Clone, Copy)]
enum NumFormat {
    Bullet,
    Decimal,
    LowerLetter,
    UpperLetter,
    LowerRoman,
    UpperRoman,
}

/// Everything outside document.xml the body refers to.
#[derive(Default)]
pub struct Package {
    /// Relationship id -> image bytes.
    pub images: HashMap<String, Vec<u8>>,
    /// Style id -> heading level (from the style's name, so localized ids work).
    heading_styles: HashMap<String, ParagraphKind>,
    /// Style id -> run properties the style sets (its basedOn chain resolved, base first).
    style_props: HashMap<String, Vec<RprProp>>,
    /// (numId, level) -> format.
    numbering: HashMap<(String, u8), NumFormat>,
    /// Default body text size from docDefaults / the Normal style.
    body_size: Option<f32>,
}

#[cfg(test)]
impl Package {
    pub fn from_styles(xml: &str) -> Package {
        let sheet = parse_styles(xml);
        Package { heading_styles: sheet.kinds, style_props: sheet.props, body_size: sheet.body_size, ..Default::default() }
    }
}

struct ReadCtx<'a> {
    package: &'a Package,
    /// Counters for numbered lists, per (numId, level).
    counters: HashMap<(String, u8), u32>,
    pending_break: bool,
    skipped_images: usize,
}

pub struct ReadResult {
    pub document: WordDocument,
    pub skipped_images: usize,
}

fn read_part(archive: &mut zip::ZipArchive<std::fs::File>, name: &str, cap: u64) -> Option<Vec<u8>> {
    let part = archive.by_name(name).ok()?;
    if part.size() > cap {
        return None;
    }
    let mut data = Vec::with_capacity(part.size() as usize);
    part.take(cap).read_to_end(&mut data).ok()?;
    Some(data)
}

pub fn read_docx(path: &Path) -> Result<ReadResult> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed opening Word document '{}'", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("'{}' is not a valid .docx package", path.display()))?;
    let body = archive
        .by_name("word/document.xml")
        .context("Word document has no word/document.xml part")?;
    if body.size() > MAX_DOCUMENT_XML {
        bail!("Word document body is too large to convert");
    }
    let mut xml = String::new();
    body.take(MAX_DOCUMENT_XML)
        .read_to_string(&mut xml)
        .context("Failed reading Word document body")?;

    let mut package = Package::default();
    if let Some(styles) = read_part(&mut archive, "word/styles.xml", MAX_SIDE_PART) {
        let sheet = parse_styles(&String::from_utf8_lossy(&styles));
        package.heading_styles = sheet.kinds;
        package.style_props = sheet.props;
        package.body_size = sheet.body_size;
    }
    if let Some(numbering) = read_part(&mut archive, "word/numbering.xml", MAX_SIDE_PART) {
        package.numbering = parse_numbering(&String::from_utf8_lossy(&numbering));
    }
    if let Some(rels) = read_part(&mut archive, "word/_rels/document.xml.rels", MAX_SIDE_PART) {
        let mut total = 0u64;
        for (id, target) in parse_image_rels(&String::from_utf8_lossy(&rels)) {
            let part = normalize_target(&target);
            if let Some(data) = read_part(&mut archive, &part, MAX_IMAGE_PART) {
                total += data.len() as u64;
                if total > MAX_TOTAL_IMAGES {
                    break;
                }
                package.images.insert(id, data);
            }
        }
    }
    let (document, skipped_images) = parse_document(&xml, &package)?;
    Ok(ReadResult { document, skipped_images })
}

/// "media/image1.png" (relative to word/) or "/word/media/image1.png" -> zip part name.
fn normalize_target(target: &str) -> String {
    let target = target.replace('\\', "/");
    if let Some(stripped) = target.strip_prefix('/') {
        return stripped.to_string();
    }
    let mut parts: Vec<&str> = vec!["word"];
    for piece in target.split('/') {
        match piece {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            p => parts.push(p),
        }
    }
    parts.join("/")
}

fn parse_image_rels(xml: &str) -> Vec<(String, String)> {
    let Ok(root) = parse_xml(xml) else { return Vec::new() };
    let Some(rels) = root.child("Relationships") else { return Vec::new() };
    rels.elements()
        .filter(|r| r.attr("Type").is_some_and(|t| t.ends_with("/image")) && r.attr("TargetMode") != Some("External"))
        .filter_map(|r| Some((r.attr("Id")?.to_string(), r.attr("Target")?.to_string())))
        .collect()
}

fn half_points(rpr: Option<&Element>) -> Option<f32> {
    rpr?.child("sz")?.attr("val")?.parse::<f32>().ok().map(|half| half / 2.0).filter(|s| *s >= 4.0 && *s <= 72.0)
}

/// A run property as written: element name and its `w:val` (if any).
type RprProp = (String, Option<String>);

fn rpr_props(rpr: &Element) -> Vec<RprProp> {
    rpr.elements().map(|e| (e.name.clone(), e.attr("val").map(str::to_string))).collect()
}

fn prop_on(value: &Option<String>) -> bool {
    !matches!(value.as_deref().map(str::to_ascii_lowercase).as_deref(), Some("0") | Some("false") | Some("off") | Some("none"))
}

fn apply_props(style: &mut TextStyle, props: &[RprProp]) {
    for (name, value) in props {
        match name.as_str() {
            "b" => style.bold = prop_on(value),
            "i" => style.italic = prop_on(value),
            "strike" | "dstrike" => style.strike = prop_on(value),
            "u" => style.underline = prop_on(value),
            "color" => style.color = value.as_deref().and_then(parse_hex_color),
            "sz" => {
                if let Some(size) = value.as_deref().and_then(|v| v.parse::<f32>().ok()).map(|half| half / 2.0) {
                    style.size = Some(size);
                }
            }
            _ => {}
        }
    }
}

struct StyleSheet {
    kinds: HashMap<String, ParagraphKind>,
    props: HashMap<String, Vec<RprProp>>,
    body_size: Option<f32>,
}

fn parse_styles(xml: &str) -> StyleSheet {
    let mut sheet = StyleSheet { kinds: HashMap::new(), props: HashMap::new(), body_size: None };
    let Ok(root) = parse_xml(xml) else { return sheet };
    let Some(styles) = root.child("styles") else { return sheet };
    sheet.body_size = half_points(styles.child("docDefaults").and_then(|d| d.find("rPr")));
    let mut raw: HashMap<String, (Option<String>, Vec<RprProp>)> = HashMap::new();
    for style in styles.elements().filter(|e| e.name == "style") {
        let Some(id) = style.attr("styleId") else { continue };
        let name = style.child("name").and_then(|n| n.attr("val")).unwrap_or("").to_ascii_lowercase();
        if style.attr("default") == Some("1") && style.attr("type") == Some("paragraph") {
            if let Some(size) = half_points(style.child("rPr")) {
                sheet.body_size = Some(size);
            }
        }
        let based_on = style.child("basedOn").and_then(|b| b.attr("val")).map(str::to_string);
        raw.insert(id.to_string(), (based_on, style.child("rPr").map(rpr_props).unwrap_or_default()));
        let outline = style
            .child("pPr")
            .and_then(|p| p.child("outlineLvl"))
            .and_then(|o| o.attr("val"))
            .and_then(|v| v.parse::<u8>().ok());
        let kind = match (name.as_str(), outline) {
            ("title", _) | ("heading 1", _) | (_, Some(0)) => Some(ParagraphKind::Heading1),
            ("subtitle", _) | ("heading 2", _) | (_, Some(1)) => Some(ParagraphKind::Heading2),
            (n, _) if n.starts_with("heading ") => Some(ParagraphKind::Heading3),
            (_, Some(_)) => Some(ParagraphKind::Heading3),
            _ => None,
        };
        if let Some(kind) = kind {
            sheet.kinds.insert(id.to_string(), kind);
        }
    }
    // Resolve basedOn chains (base first), with a depth cap against cycles; a style
    // based on a heading is a heading too.
    for id in raw.keys() {
        let mut chain = Vec::new();
        let mut current = Some(id.clone());
        let mut kind = None;
        while let Some(cid) = current.take() {
            if chain.len() >= 12 {
                break;
            }
            let Some((based_on, props)) = raw.get(&cid) else { break };
            if kind.is_none() {
                kind = sheet.kinds.get(&cid).copied();
            }
            chain.push(props.clone());
            current = based_on.clone();
        }
        let props: Vec<RprProp> = chain.into_iter().rev().flatten().collect();
        sheet.props.insert(id.clone(), props);
        if let Some(kind) = kind {
            sheet.kinds.entry(id.clone()).or_insert(kind);
        }
    }
    sheet
}

fn parse_numbering(xml: &str) -> HashMap<(String, u8), NumFormat> {
    let mut out = HashMap::new();
    let Ok(root) = parse_xml(xml) else { return out };
    let Some(numbering) = root.child("numbering") else { return out };
    let mut abstract_formats: HashMap<String, HashMap<u8, NumFormat>> = HashMap::new();
    for abs in numbering.elements().filter(|e| e.name == "abstractNum") {
        let Some(id) = abs.attr("abstractNumId") else { continue };
        let mut levels = HashMap::new();
        for lvl in abs.elements().filter(|e| e.name == "lvl") {
            let level = lvl.attr("ilvl").and_then(|v| v.parse::<u8>().ok()).unwrap_or(0);
            let format = match lvl.child("numFmt").and_then(|f| f.attr("val")).unwrap_or("bullet") {
                "decimal" | "decimalZero" => NumFormat::Decimal,
                "lowerLetter" => NumFormat::LowerLetter,
                "upperLetter" => NumFormat::UpperLetter,
                "lowerRoman" => NumFormat::LowerRoman,
                "upperRoman" => NumFormat::UpperRoman,
                _ => NumFormat::Bullet,
            };
            levels.insert(level, format);
        }
        abstract_formats.insert(id.to_string(), levels);
    }
    for num in numbering.elements().filter(|e| e.name == "num") {
        let Some(num_id) = num.attr("numId") else { continue };
        let Some(abs_id) = num.child("abstractNumId").and_then(|a| a.attr("val")) else { continue };
        if let Some(levels) = abstract_formats.get(abs_id) {
            for (level, format) in levels {
                out.insert((num_id.to_string(), *level), *format);
            }
        }
    }
    out
}

fn roman(mut n: u32) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"),
        (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i"),
    ];
    let mut out = String::new();
    for (value, digits) in TABLE {
        while n >= value {
            out.push_str(digits);
            n -= value;
        }
    }
    out
}

fn letter(n: u32) -> String {
    let mut n = n.max(1);
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push((b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    out.iter().rev().collect()
}

pub fn parse_document(xml: &str, package: &Package) -> Result<(WordDocument, usize)> {
    let root = parse_xml(xml)?;
    let body = root
        .find("body")
        .context("Word document contains no readable text")?;
    let mut ctx = ReadCtx { package, counters: HashMap::new(), pending_break: false, skipped_images: 0 };
    let mut blocks = Vec::new();
    read_container(body, &mut ctx, &mut blocks);
    let document = WordDocument { blocks, body_size: package.body_size.unwrap_or(11.0) };
    if !document.has_content() {
        bail!("Word document contains no readable text");
    }
    Ok((document, ctx.skipped_images))
}

/// Reads body-level content (also the inside of content controls).
fn read_container(container: &Element, ctx: &mut ReadCtx, blocks: &mut Vec<Block>) {
    for element in container.elements() {
        match element.name.as_str() {
            "p" => read_paragraph(element, ctx, blocks),
            "tbl" => {
                let mut table = read_table(element, ctx);
                table.page_break_before = std::mem::take(&mut ctx.pending_break);
                if !table.rows.is_empty() {
                    blocks.push(Block::Table(table));
                }
            }
            "sdt" => {
                if let Some(content) = element.child("sdtContent") {
                    read_container(content, ctx, blocks);
                }
            }
            "customXml" | "ins" | "smartTag" => read_container(element, ctx, blocks),
            _ => {}
        }
    }
}

struct ParagraphState {
    paragraph: WordParagraph,
    break_before: bool,
    /// List marker ("•", "3.") put in front of the first text of the paragraph.
    marker: Option<String>,
}

fn read_paragraph(p: &Element, ctx: &mut ReadCtx, blocks: &mut Vec<Block>) {
    let ppr = p.child("pPr");
    let mut base = WordParagraph::default();
    let mut break_before = std::mem::take(&mut ctx.pending_break);
    let mut marker = None;
    if let Some(ppr) = ppr {
        if let Some(style) = ppr.child("pStyle").and_then(|s| s.attr("val")) {
            base.kind = ctx.package.heading_styles.get(style).copied().unwrap_or_else(|| {
                let lower = style.to_ascii_lowercase();
                match lower.as_str() {
                    v if v.contains("heading1") || v == "title" => ParagraphKind::Heading1,
                    v if v.contains("heading2") || v == "subtitle" => ParagraphKind::Heading2,
                    v if v.contains("heading3") => ParagraphKind::Heading3,
                    _ => ParagraphKind::Body,
                }
            });
        }
        if let Some(level) = ppr.child("outlineLvl").and_then(|o| o.attr("val")).and_then(|v| v.parse::<u8>().ok()) {
            base.kind = match level {
                0 => ParagraphKind::Heading1,
                1 => ParagraphKind::Heading2,
                2..=8 => ParagraphKind::Heading3,
                _ => base.kind,
            };
        }
        base.align = match ppr.child("jc").and_then(|j| j.attr("val")) {
            Some("center") => Align::Center,
            Some("right") | Some("end") => Align::Right,
            Some("both") | Some("distribute") => Align::Justify,
            _ => Align::Left,
        };
        if ppr.child("pageBreakBefore").is_some_and(switch_on) {
            break_before = true;
        }
        if let Some(num) = ppr.child("numPr") {
            let num_id = num.child("numId").and_then(|n| n.attr("val")).unwrap_or("0").to_string();
            let level = num.child("ilvl").and_then(|n| n.attr("val")).and_then(|v| v.parse::<u8>().ok()).unwrap_or(0);
            if num_id != "0" {
                base.list_level = Some(level);
                let format = ctx.package.numbering.get(&(num_id.clone(), level)).copied().unwrap_or(NumFormat::Bullet);
                // Deeper levels restart when a shallower item appears.
                ctx.counters.retain(|(id, l), _| !(id == &num_id && *l > level));
                let counter = ctx.counters.entry((num_id, level)).or_insert(0);
                *counter += 1;
                let marker_text = match format {
                    NumFormat::Bullet => "•".to_string(),
                    NumFormat::Decimal => format!("{counter}."),
                    NumFormat::LowerLetter => format!("{})", letter(*counter)),
                    NumFormat::UpperLetter => format!("{}.", letter(*counter).to_uppercase()),
                    NumFormat::LowerRoman => format!("{}.", roman(*counter)),
                    NumFormat::UpperRoman => format!("{}.", roman(*counter).to_uppercase()),
                };
                marker = Some(format!("{text}\t", text = marker_text));
            }
        }
    }
    // Runs start from the paragraph style's run properties (pPr/rPr only styles the
    // paragraph mark, so it is not used).
    let mut paragraph_style = TextStyle::default();
    if let Some(props) = ppr.and_then(|p| p.child("pStyle")).and_then(|s| s.attr("val")).and_then(|id| ctx.package.style_props.get(id)) {
        apply_props(&mut paragraph_style, props);
    }
    let mut state = ParagraphState { paragraph: base.clone(), break_before, marker };
    read_inline(p, ctx, &mut state, blocks, &base, &paragraph_style);
    flush_paragraph(&mut state, blocks, &base);
    ctx.pending_break |= std::mem::take(&mut state.break_before);
}

/// Emits the paragraph collected so far (when it has text) and starts a fresh one.
fn flush_paragraph(state: &mut ParagraphState, blocks: &mut Vec<Block>, base: &WordParagraph) {
    let mut paragraph = std::mem::replace(&mut state.paragraph, WordParagraph { runs: Vec::new(), list_level: None, ..base.clone() });
    paragraph.trim_end();
    let has_text = paragraph.runs.iter().any(|r| !r.text.trim().is_empty());
    if has_text {
        if let Some(marker) = state.marker.take() {
            paragraph.runs.insert(0, super::model::Run { text: marker, style: TextStyle::default() });
        } else {
            paragraph.list_level = None;
        }
        paragraph.page_break_before = std::mem::take(&mut state.break_before);
        blocks.push(Block::Paragraph(paragraph));
    }
}

/// Run style: inherited (paragraph style), then the run's character style, then its own properties.
fn run_style(rpr: &Element, inherited: &TextStyle, package: &Package) -> TextStyle {
    let mut style = inherited.clone();
    if let Some(props) = rpr.child("rStyle").and_then(|r| r.attr("val")).and_then(|id| package.style_props.get(id)) {
        apply_props(&mut style, props);
    }
    apply_props(&mut style, &rpr_props(rpr));
    style
}

fn read_inline(
    container: &Element,
    ctx: &mut ReadCtx,
    state: &mut ParagraphState,
    blocks: &mut Vec<Block>,
    base: &WordParagraph,
    paragraph_style: &TextStyle,
) {
    for element in container.elements() {
        match element.name.as_str() {
            "r" => {
                let style = element.child("rPr").map(|r| run_style(r, paragraph_style, ctx.package)).unwrap_or_else(|| paragraph_style.clone());
                for part in element.elements() {
                    match part.name.as_str() {
                        "t" => state.paragraph.push(&part.text(), &style),
                        "tab" | "ptab" => state.paragraph.push("\t", &style),
                        "cr" => state.paragraph.push("\n", &style),
                        "noBreakHyphen" => state.paragraph.push("-", &style),
                        "softHyphen" => {}
                        "br" => {
                            if part.attr("type") == Some("page") {
                                if state.paragraph.is_blank() {
                                    state.break_before = true;
                                } else {
                                    // A break after text splits the paragraph.
                                    flush_paragraph(state, blocks, base);
                                    state.break_before = true;
                                }
                            } else {
                                state.paragraph.push("\n", &style);
                            }
                        }
                        "drawing" | "pict" | "object" => {
                            if let Some(mut image) = read_image(part, ctx, base.align) {
                                flush_paragraph(state, blocks, base);
                                image.page_break_before = std::mem::take(&mut state.break_before);
                                blocks.push(Block::Image(image));
                            }
                        }
                        "AlternateContent" => {
                            // Prefer the modern choice; the fallback repeats it.
                            if let Some(choice) = part.child("Choice") {
                                if let Some(mut image) = read_image(choice, ctx, base.align) {
                                    flush_paragraph(state, blocks, base);
                                    image.page_break_before = std::mem::take(&mut state.break_before);
                                    blocks.push(Block::Image(image));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Containers that hold runs; "del" (deleted tracked changes) is skipped on purpose.
            "hyperlink" | "ins" | "smartTag" | "customXml" | "fldSimple" | "bdo" | "dir" => {
                read_inline(element, ctx, state, blocks, base, paragraph_style)
            }
            "sdt" => {
                if let Some(content) = element.child("sdtContent") {
                    read_inline(content, ctx, state, blocks, base, paragraph_style);
                }
            }
            _ => {}
        }
    }
}

fn read_image(container: &Element, ctx: &mut ReadCtx, align: Align) -> Option<WordImage> {
    // DrawingML (<a:blip r:embed>) or legacy VML (<v:imagedata r:id>).
    let rel = container
        .find("blip")
        .and_then(|b| b.attr("embed"))
        .or_else(|| container.find("imagedata").and_then(|i| i.attr("id")))?;
    let Some(data) = ctx.package.images.get(rel) else {
        ctx.skipped_images += 1;
        return None;
    };
    let Some(format) = ImageFormat::sniff(data) else {
        // EMF/WMF/GIF/... cannot be placed; counted so the user is told.
        ctx.skipped_images += 1;
        return None;
    };
    let (mut width_pt, mut height_pt) = container
        .find("extent")
        .and_then(|e| Some((e.attr("cx")?.parse::<f64>().ok()?, e.attr("cy")?.parse::<f64>().ok()?)))
        .map(|(cx, cy)| ((cx / 12700.0) as f32, (cy / 12700.0) as f32))
        .unwrap_or((0.0, 0.0));
    if width_pt < 1.0 || height_pt < 1.0 {
        // No declared size: use the pixel size at 96 dpi.
        let (w, h) = image::load_from_memory(data).ok().map(|i| (i.width(), i.height()))?;
        width_pt = w as f32 * 0.75;
        height_pt = h as f32 * 0.75;
    }
    Some(WordImage { data: data.clone(), format, width_pt, height_pt, align, page_break_before: false })
}

fn read_table(tbl: &Element, ctx: &mut ReadCtx) -> WordTable {
    let col_widths_pt: Vec<f32> = tbl
        .child("tblGrid")
        .map(|grid| {
            grid.elements()
                .filter(|g| g.name == "gridCol")
                .map(|g| g.attr("w").and_then(|w| w.parse::<f32>().ok()).unwrap_or(0.0) / 20.0)
                .collect()
        })
        .unwrap_or_default();
    let mut rows = Vec::new();
    collect_rows(tbl, ctx, &mut rows);
    // Word's "Table Normal" has no borders: a table is borderless when it names no
    // table style and declares no visible borders of its own.
    let tblpr = tbl.child("tblPr");
    let declared = tblpr.and_then(|p| p.child("tblBorders"));
    let visible = |e: &Element| !matches!(e.attr("val"), Some("none") | Some("nil") | None);
    let borderless = match declared {
        Some(borders) => !borders.elements().any(visible),
        None => tblpr.and_then(|p| p.child("tblStyle")).is_none(),
    };
    WordTable { rows, col_widths_pt, page_break_before: false, borderless }
}

fn collect_rows(container: &Element, ctx: &mut ReadCtx, rows: &mut Vec<Vec<TableCell>>) {
    for element in container.elements() {
        match element.name.as_str() {
            "tr" => {
                let mut cells = Vec::new();
                collect_cells(element, ctx, &mut cells);
                if !cells.is_empty() {
                    rows.push(cells);
                }
            }
            "sdt" => {
                if let Some(content) = element.child("sdtContent") {
                    collect_rows(content, ctx, rows);
                }
            }
            "customXml" => collect_rows(element, ctx, rows),
            _ => {}
        }
    }
}

fn collect_cells(container: &Element, ctx: &mut ReadCtx, cells: &mut Vec<TableCell>) {
    for element in container.elements() {
        match element.name.as_str() {
            "tc" => cells.push(read_cell(element, ctx)),
            "sdt" => {
                if let Some(content) = element.child("sdtContent") {
                    collect_cells(content, ctx, cells);
                }
            }
            "customXml" => collect_cells(element, ctx, cells),
            _ => {}
        }
    }
}

fn read_cell(tc: &Element, ctx: &mut ReadCtx) -> TableCell {
    let tcpr = tc.child("tcPr");
    let span = tcpr
        .and_then(|t| t.child("gridSpan"))
        .and_then(|g| g.attr("val"))
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(1)
        .max(1);
    let vmerge = match tcpr.and_then(|t| t.child("vMerge")) {
        Some(v) if v.attr("val") == Some("restart") => VMerge::Restart,
        Some(_) => VMerge::Continue,
        None => VMerge::None,
    };
    let shading = tcpr
        .and_then(|t| t.child("shd"))
        .and_then(|s| s.attr("fill"))
        .and_then(parse_hex_color)
        .filter(|rgb| *rgb != [255, 255, 255]);
    // Cell content: paragraphs, and nested tables flattened row by row.
    let saved_break = std::mem::take(&mut ctx.pending_break);
    let mut inner = Vec::new();
    read_container(tc, ctx, &mut inner);
    ctx.pending_break = saved_break;
    // Pictures inside cells are not placed (counted so the user is told).
    ctx.skipped_images += inner.iter().filter(|b| matches!(b, Block::Image(_))).count();
    let mut paragraphs = Vec::new();
    flatten_into(&inner, &mut paragraphs);
    for p in &mut paragraphs {
        p.page_break_before = false;
    }
    TableCell { paragraphs, span, vmerge, shading }
}

fn flatten_into(blocks: &[Block], out: &mut Vec<WordParagraph>) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => out.push(p.clone()),
            Block::Table(t) => {
                for row in &t.rows {
                    let mut line = WordParagraph::default();
                    for (i, cell) in row.iter().enumerate() {
                        if i > 0 {
                            line.push(" | ", &TextStyle::default());
                        }
                        for (j, p) in cell.paragraphs.iter().enumerate() {
                            if j > 0 {
                                line.push(" ", &TextStyle::default());
                            }
                            for run in &p.runs {
                                line.push(&run.text, &run.style);
                            }
                        }
                    }
                    if !line.is_blank() {
                        out.push(line);
                    }
                }
            }
            Block::Image(_) => {}
        }
    }
}
