use super::docx_read::{parse_document, Package};
use super::docx_write::{docx_document_xml, fitted_widths, xml_escape};
use super::model::*;
use super::pdf_extract::{build_paragraphs, build_table, find_grids, mark_text_rules, split_tabular, Ch, FlowItem, HRule, VRule};

const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"";

fn read(xml: &str) -> WordDocument {
    parse_document(xml, &Package::default()).unwrap().0
}

fn para(text: &str, brk: bool) -> Block {
    let mut p = WordParagraph::plain(text);
    p.page_break_before = brk;
    Block::Paragraph(p)
}

fn texts(doc: &WordDocument) -> Vec<(String, bool)> {
    doc.blocks
        .iter()
        .map(|b| match b {
            Block::Paragraph(p) => (p.text(), p.page_break_before),
            Block::Table(t) => ("<table>".into(), t.page_break_before),
            Block::Image(i) => ("<image>".into(), i.page_break_before),
        })
        .collect()
}

fn test_png() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(3, 1, image::Rgb([190, 40, 30]));
    let mut data = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image).write_to(&mut data, image::ImageFormat::Png).unwrap();
    data.into_inner()
}

#[test]
fn xml_entities_are_decoded_once_and_cdata_stays_literal() {
    let document = read(&format!("<w:document {W}><w:body><w:p><w:r><w:t>\
        &amp;lt; &lt; &#x130; &#287; <![CDATA[&amp;]]>\
        </w:t></w:r></w:p></w:body></w:document>"));
    assert_eq!(texts(&document), vec![("&lt; < İ ğ &amp;".into(), false)]);
}

#[test]
fn malformed_or_excessively_nested_word_xml_is_rejected() {
    use super::docx_read::parse_xml;
    let nested = format!("{}{}", "<node>".repeat(256), "</node>".repeat(256));
    for xml in [
        nested,
        "<root value=\"one\" value=\"two\"/>".into(),
        "<root><child>".into(),
        "<root>&not_defined;</root>".into(),
        format!("<root value=\"{}\"/>", "x".repeat(64 * 1024 + 1)),
    ] {
        assert!(parse_xml(&xml).is_err());
    }
}

#[test]
fn non_ascii_color_values_cannot_be_sliced_as_hex_bytes() {
    for value in ["aéabc", "abİcd", "auto", "#000000", "12xx34"] {
        assert_eq!(parse_hex_color(value), None);
    }
    assert_eq!(parse_hex_color("#a1B2c3"), Some([161, 178, 195]));
}

#[test]
fn explicit_page_break_before_inline_image_is_preserved() {
    let image = WordImage { data: test_png(), format: ImageFormat::Png,
        width_pt: 90.0, height_pt: 30.0, align: Align::Left, page_break_before: false };
    let document = WordDocument { blocks: vec![para("BEFORE", false), Block::Image(image)], body_size: 11.0 };
    let (xml, images) = docx_document_xml(&document);
    let xml = xml.replacen("<w:drawing>", "<w:br w:type=\"page\"/><w:drawing>", 1);
    let mut package = Package::default();
    package.images.insert("rIdImg1".into(), images[0].data.clone());
    let (document, skipped) = parse_document(&xml, &package).unwrap();
    assert_eq!(skipped, 0);
    assert_eq!(texts(&document), vec![("BEFORE".into(), false), ("<image>".into(), true)]);
}


#[test]
fn xml_escape_drops_illegal_characters() {
    assert_eq!(xml_escape("a<b>&\u{1}\u{FFFE}\u{FFFF}x"), concat!("a&", "lt;b&", "gt;&", "amp;x"));
    assert_eq!(xml_escape("x\ty"), "x    y");
}

#[test]
fn page_breaks_survive_write_then_read() {
    let doc = WordDocument {
        blocks: vec![para("A1", false), para("A2", false), para("B1", true), para("C1", true)],
        body_size: 11.0,
    };
    let back = read(&docx_document_xml(&doc).0);
    assert_eq!(
        texts(&back),
        vec![("A1".into(), false), ("A2".into(), false), ("B1".into(), true), ("C1".into(), true)]
    );
}

#[test]
fn word_style_page_breaks_are_read() {
    let xml = format!(
        "<w:document {W}><w:body>\
<w:p><w:r><w:t>one</w:t></w:r><w:r><w:br w:type=\"page\"/></w:r></w:p>\
<w:p><w:r><w:t>two</w:t></w:r></w:p>\
<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>\
<w:p><w:r><w:t>three</w:t></w:r></w:p>\
<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>four</w:t></w:r></w:p>\
<w:p><w:r><w:t>five</w:t><w:br/><w:t>line</w:t></w:r></w:p>\
</w:body></w:document>"
    );
    assert_eq!(
        texts(&read(&xml)),
        vec![
            ("one".into(), false),
            ("two".into(), true),
            ("three".into(), true),
            ("four".into(), true),
            ("five\nline".into(), false),
        ]
    );
}

#[test]
fn run_styles_and_alignment_round_trip() {
    let mut p = WordParagraph { align: Align::Center, ..Default::default() };
    p.push("Kalın ", &TextStyle { bold: true, ..Default::default() });
    p.push("eğik ", &TextStyle { italic: true, ..Default::default() });
    p.push("altı çizili ", &TextStyle { underline: true, strike: true, ..Default::default() });
    p.push("kırmızı 14", &TextStyle { color: Some([200, 0, 0]), size: Some(14.0), ..Default::default() });
    let mut heading = WordParagraph::plain("Başlık");
    heading.kind = ParagraphKind::Heading2;
    let doc = WordDocument { blocks: vec![Block::Paragraph(heading), Block::Paragraph(p.clone())], body_size: 11.0 };
    let back = read(&docx_document_xml(&doc).0);
    assert_eq!(back.blocks.len(), 2);
    match (&back.blocks[0], &back.blocks[1]) {
        (Block::Paragraph(h), Block::Paragraph(q)) => {
            assert_eq!(h.kind, ParagraphKind::Heading2);
            assert_eq!(q, &p);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn tables_with_merges_and_shading_round_trip() {
    let cell = |t: &str| TableCell::new(vec![WordParagraph::plain(t)]);
    let mut header = TableCell::new(vec![WordParagraph::plain("Başlık")]);
    header.span = 2;
    header.shading = Some([220, 230, 240]);
    let mut tall = cell("Uzun");
    tall.vmerge = VMerge::Restart;
    let mut covered = TableCell::new(Vec::new());
    covered.vmerge = VMerge::Continue;
    let table = WordTable {
        rows: vec![vec![header], vec![tall, cell("b1")], vec![covered, cell("b2")]],
        col_widths_pt: vec![120.0, 200.0],
        page_break_before: true,
        borderless: false,
    };
    let doc = WordDocument { blocks: vec![para("Önce", false), Block::Table(table.clone()), para("Sonra", false)], body_size: 11.0 };
    let back = read(&docx_document_xml(&doc).0);
    assert_eq!(texts(&back), vec![("Önce".into(), false), ("<table>".into(), true), ("Sonra".into(), false)]);
    let Block::Table(t) = &back.blocks[1] else { panic!() };
    assert_eq!(t.rows, table.rows);
    assert_eq!(t.col_widths_pt, vec![120.0, 200.0]);
}

#[test]
fn images_are_written_with_relationships() {
    let png = test_png();
    let image = WordImage { data: png, format: ImageFormat::Png, width_pt: 900.0, height_pt: 300.0, align: Align::Center, page_break_before: false };
    let doc = WordDocument { blocks: vec![Block::Image(image)], body_size: 11.0 };
    let (xml, images) = docx_document_xml(&doc);
    let mut package = Package::default();
    package.images.insert("rIdImg1".into(), images[0].data.clone());
    let (back, skipped) = parse_document(&xml, &package).unwrap();
    assert_eq!(skipped, 0);
    let Block::Image(i) = &back.blocks[0] else { panic!("{back:?}") };
    assert_eq!(i.format, ImageFormat::Png);
    assert!((i.width_pt / i.height_pt - 3.0).abs() < 0.01);
    let decoded = image::load_from_memory(&i.data).unwrap().into_rgb8();
    assert_eq!(decoded.dimensions(), (3, 1));
    assert_eq!(decoded.get_pixel(1, 0).0, [190, 40, 30]);
}

#[test]
fn lists_hyperlinks_deletions_and_localized_headings() {
    let xml = format!(
        "<w:document {W}><w:body>\
<w:p><w:pPr><w:pStyle w:val=\"Balk1\"/><w:outlineLvl w:val=\"0\"/></w:pPr><w:r><w:t>Giriş</w:t></w:r></w:p>\
<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"5\"/></w:numPr></w:pPr><w:r><w:t>madde</w:t></w:r></w:p>\
<w:p><w:hyperlink r:id=\"x\"><w:r><w:t>bağlantı</w:t></w:r></w:hyperlink><w:del><w:r><w:delText>silindi</w:delText></w:r></w:del><w:ins><w:r><w:t> eklendi</w:t></w:r></w:ins></w:p>\
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl>\
</w:body></w:document>"
    );
    let doc = read(&xml);
    let Block::Paragraph(h) = &doc.blocks[0] else { panic!() };
    assert_eq!(h.kind, ParagraphKind::Heading1);
    let Block::Paragraph(item) = &doc.blocks[1] else { panic!() };
    assert_eq!(item.text(), "•\tmadde");
    assert_eq!(item.list_level, Some(0));
    let Block::Paragraph(link) = &doc.blocks[2] else { panic!() };
    assert_eq!(link.text(), "bağlantı eklendi");
    assert!(matches!(doc.blocks[3], Block::Table(_)));
}

#[test]
fn fitted_widths_scale_and_default() {
    assert_eq!(fitted_widths(&[], 4, 400.0), vec![100.0; 4]);
    assert_eq!(fitted_widths(&[100.0, 300.0], 2, 200.0), vec![50.0, 150.0]);
    assert_eq!(fitted_widths(&[10.0], 3, 300.0), vec![100.0; 3]);
}

fn ch(c: char, left: f32, bottom: f32, size: f32) -> Ch {
    Ch { c, left, right: left + size * 0.5, bottom, top: bottom + size, size, style: TextStyle { size: Some(size), ..Default::default() } }
}

fn word(text: &str, left: f32, bottom: f32) -> Vec<Ch> {
    text.chars().enumerate().map(|(i, c)| ch(c, left + i as f32 * 5.0, bottom, 10.0)).collect()
}

fn newline() -> Ch {
    Ch { c: '\n', left: 0.0, right: 0.0, bottom: 0.0, top: 0.0, size: 0.0, style: TextStyle::default() }
}

#[test]
fn grid_detection_finds_cells_and_merges() {
    // 3 columns x 3 rows; the first row's two right cells are merged (no rule at x=200
    // in the top row) and the left column's lower two cells are merged (no rule at y=40).
    let hs = vec![
        HRule { y: 100.0, x1: 0.0, x2: 300.0 },
        HRule { y: 70.0, x1: 0.0, x2: 300.0 },
        HRule { y: 40.0, x1: 100.0, x2: 300.0 },
        HRule { y: 10.0, x1: 0.0, x2: 300.0 },
    ];
    let vs = vec![
        VRule { x: 0.0, y1: 10.0, y2: 100.0 },
        VRule { x: 100.0, y1: 10.0, y2: 100.0 },
        VRule { x: 200.0, y1: 10.0, y2: 70.0 },
        VRule { x: 300.0, y1: 10.0, y2: 100.0 },
    ];
    let grids = find_grids(&hs, &vs);
    assert_eq!(grids.len(), 1);
    let g = &grids[0];
    assert_eq!(g.xs.len(), 4);
    assert_eq!(g.ys.len(), 4);
    assert_eq!(g.cells[0], vec![(0, 1, VMerge::None), (1, 2, VMerge::None)]);
    assert_eq!(g.cells[1][0], (0, 1, VMerge::Restart));
    assert_eq!(g.cells[2][0], (0, 1, VMerge::Continue));

    let mut chars = Vec::new();
    for (t, x, y) in [("Ad", 10.0, 80.0), ("Toplam", 110.0, 80.0), ("Uzun", 10.0, 50.0), ("b", 110.0, 50.0), ("c", 210.0, 50.0), ("d", 110.0, 20.0)] {
        chars.extend(word(t, x, y));
        chars.push(newline());
    }
    let table = build_table(g, &chars, &[]).unwrap();
    let cell_text = |r: usize, i: usize| table.rows[r][i].paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>().join("|");
    assert_eq!(cell_text(0, 0), "Ad");
    assert_eq!(cell_text(0, 1), "Toplam");
    assert_eq!(table.rows[0][1].span, 2);
    assert_eq!(cell_text(1, 0), "Uzun");
    assert_eq!(cell_text(1, 2), "c");
    assert_eq!(table.rows[2][0].vmerge, VMerge::Continue);
    assert_eq!(cell_text(2, 1), "d");
}

#[test]
fn lone_frames_are_not_tables() {
    // A box with one line of text inside is a frame, not a table.
    let hs = vec![HRule { y: 100.0, x1: 0.0, x2: 300.0 }, HRule { y: 10.0, x1: 0.0, x2: 300.0 }];
    let vs = vec![VRule { x: 0.0, y1: 10.0, y2: 100.0 }, VRule { x: 300.0, y1: 10.0, y2: 100.0 }];
    assert!(find_grids(&hs, &vs).is_empty());
}

#[test]
fn wrapped_lines_join_and_hyphens_rejoin() {
    let mut chars = word("Bu bir uzun satır ve devamı ge-", 0.0, 100.0);
    chars.push(newline());
    chars.extend(word("liyor burada.", 0.0, 88.0));
    chars.push(newline());
    chars.extend(word("Yeni paragraf", 0.0, 60.0));
    let lines = super::pdf_extract::build_lines(&chars);
    assert_eq!(lines.len(), 3);
    let paragraphs = build_paragraphs(&lines, (0.0, 155.0));
    let texts: Vec<String> = paragraphs.iter().map(|p| p.paragraph.text()).collect();
    assert_eq!(texts, vec!["Bu bir uzun satır ve devamı geliyor burada.", "Yeni paragraf"]);
}

#[test]
fn style_sheet_colours_sizes_and_character_styles_apply() {
    let styles = format!(
        "<w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"24\"/></w:rPr></w:rPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:styleId=\"Balk1\"><w:name w:val=\"heading 1\"/><w:rPr><w:color w:val=\"2F5496\"/><w:sz w:val=\"32\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Ozel\"><w:name w:val=\"Benim\"/><w:basedOn w:val=\"Balk1\"/><w:rPr><w:i/></w:rPr></w:style>\
<w:style w:type=\"character\" w:styleId=\"Guclu\"><w:name w:val=\"Strong\"/><w:rPr><w:b/></w:rPr></w:style>\
</w:styles>"
    );
    let package = Package::from_styles(&styles);
    let xml = format!(
        "<w:document {W}><w:body>\
<w:p><w:pPr><w:pStyle w:val=\"Ozel\"/></w:pPr><w:r><w:t>Başlık</w:t></w:r></w:p>\
<w:p><w:r><w:rPr><w:rStyle w:val=\"Guclu\"/></w:rPr><w:t>güçlü</w:t></w:r></w:p>\
</w:body></w:document>"
    );
    let (doc, _) = parse_document(&xml, &package).unwrap();
    assert_eq!(doc.body_size, 12.0);
    let Block::Paragraph(h) = &doc.blocks[0] else { panic!() };
    assert_eq!(h.kind, ParagraphKind::Heading1);
    assert_eq!(h.runs[0].style, TextStyle { italic: true, color: Some([0x2F, 0x54, 0x96]), size: Some(16.0), ..Default::default() });
    let Block::Paragraph(b) = &doc.blocks[1] else { panic!() };
    assert!(b.runs[0].style.bold);
}

#[test]
fn underline_and_strike_rules_mark_their_text_only() {
    // "altı" (x 0..20) has a rule just above its bottom; "çizik" (x 30..55) has one
    // through its middle; a long rule far below touches nothing.
    let mut chars = word("altı", 0.0, 100.0);
    chars.extend(word("çizik", 30.0, 100.0));
    let rules = vec![
        HRule { y: 101.5, x1: 0.0, x2: 20.0 },
        HRule { y: 105.5, x1: 30.0, x2: 55.0 },
        HRule { y: 60.0, x1: 0.0, x2: 300.0 },
    ];
    mark_text_rules(&mut chars, &rules, &[]);
    assert!(chars[..4].iter().all(|c| c.style.underline && !c.style.strike));
    assert!(chars[4..].iter().all(|c| c.style.strike && !c.style.underline));
}

fn line_of(parts: &[(&str, f32)], bottom: f32) -> Vec<Ch> {
    let mut chars = Vec::new();
    for (text, x) in parts {
        if !chars.is_empty() {
            chars.push(ch(' ', x - 5.0, bottom, 10.0));
        }
        chars.extend(word(text, *x, bottom));
    }
    chars.push(newline());
    chars
}

#[test]
fn column_aligned_rows_become_a_borderless_table_but_lists_do_not() {
    let mut chars = line_of(&[("Kalem", 0.0), ("Tutar", 150.0), ("Tarih", 280.0)], 200.0);
    chars.extend(line_of(&[("Elektrik", 0.0), ("850 TL", 150.0), ("01.10", 280.0)], 184.0));
    chars.extend(line_of(&[("Su", 0.0), ("210 TL", 150.0), ("03.10", 280.0)], 168.0));
    chars.extend(line_of(&[("Normal bir paragraf satırı", 0.0)], 140.0));
    let items = split_tabular(super::pdf_extract::build_lines(&chars));
    assert_eq!(items.len(), 2);
    let FlowItem::Table(_, table) = &items[0] else { panic!("expected a table first") };
    assert!(table.borderless);
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[1][1].paragraphs[0].text(), "850 TL");
    assert!(matches!(items[1], FlowItem::Lines(ref l) if l.len() == 1));

    let mut list = line_of(&[("1.", 0.0), ("Birinci madde", 40.0)], 200.0);
    list.extend(line_of(&[("2.", 0.0), ("İkinci madde", 40.0)], 184.0));
    let items = split_tabular(super::pdf_extract::build_lines(&list));
    assert!(items.iter().all(|i| matches!(i, FlowItem::Lines(_))));
}

#[cfg(windows)]
#[test]
fn native_page_and_form_clips_do_not_export_hidden_photo_pixels() {
    use lopdf::{dictionary, Document, Object, Stream};
    use std::sync::atomic::AtomicBool;
    let scratch = crate::scratch::ScratchDir::new().unwrap();
    let mut source = Document::with_version("1.7");
    let pages = source.new_object_id();
    let mut pixels = Vec::with_capacity(200 * 100 * 3);
    for _y in 0..100 {
        for x in 0..200 {
            pixels.extend_from_slice(if x < 100 { &[0, 60, 230] } else { &[240, 15, 15] });
        }
    }
    let image = source.add_object(Stream::new(dictionary! {
        "Type"=>"XObject", "Subtype"=>"Image", "Width"=>200, "Height"=>100,
        "ColorSpace"=>"DeviceRGB", "BitsPerComponent"=>8
    }, pixels));
    let form = source.add_object(Stream::new(dictionary! {
        "Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),200.into(),100.into()],
        "Resources"=>dictionary!{"XObject"=>dictionary!{"Im1"=>image}}
    }, b"q 200 0 0 100 0 0 cm /Im1 Do Q".to_vec()));
    let font = source.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let content = source.add_object(Stream::new(dictionary!{}, b"q 50 200 100 100 re W n 1 0 0 1 50 200 cm /Fm1 Do Q BT /F1 12 Tf 60 250 Td (VISIBLE FORM CONTROL) Tj ET".to_vec()));
    let page = source.add_object(dictionary! {
        "Type"=>"Page", "Parent"=>pages, "MediaBox"=>vec![0.into(),0.into(),400.into(),400.into()],
        "Contents"=>content, "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font},"XObject"=>dictionary!{"Fm1"=>form,"Im1"=>image}}
    });
    source.objects.insert(pages, Object::Dictionary(dictionary!{"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}));
    let catalog = source.add_object(dictionary!{"Type"=>"Catalog","Pages"=>pages});
    source.trailer.set("Root", catalog);
    // The standard Windows build fetches the pinned native runtime before tests.
    let dll = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/pdfium.dll");
    let engine = crate::Engine::new(&dll).unwrap();
    for (label, commands) in [
        ("form", "q 50 200 100 100 re W n 1 0 0 1 50 200 cm /Fm1 Do Q"),
        ("page", "q 50 200 100 100 re W n 200 0 0 100 50 200 cm /Im1 Do Q"),
        ("reflected", "q 150 200 100 100 re W n -200 0 0 100 250 200 cm /Im1 Do Q"),
        ("rotated", "q 150 100 100 100 re W n 0 200 -100 0 250 100 cm /Im1 Do Q"),
    ] {
        let input = scratch.path().join(format!("{label}-clipped-photo.pdf"));
        let output = scratch.path().join(format!("{label}-clipped-photo.docx"));
        source.get_object_mut(content).unwrap().as_stream_mut().unwrap().set_content(
            format!("{commands} BT /F1 12 Tf 60 250 Td (VISIBLE FORM CONTROL) Tj ET").into_bytes());
        source.save(&input).unwrap();
        let request = serde_json::from_value(serde_json::json!({
            "id":"clip-regression", "kind":"pdf-to-word",
            "inputs":[input], "output":output, "ocr":false
        })).unwrap();
        engine.perform(&request, &AtomicBool::new(false), |_| {}).unwrap();
        let document = super::docx_read::read_docx(&output).unwrap().document;
        let images: Vec<_> = document.blocks.iter().filter_map(|block|
            if let Block::Image(image) = block { Some(image) } else { None }).collect();
        assert_eq!(images.len(), 1);
        assert!((images[0].width_pt / images[0].height_pt - 1.0).abs() < 0.02);
        let image = image::load_from_memory(&images[0].data).unwrap().into_rgba8();
        let blue = image.pixels().filter(|pixel| pixel[0] < 80 && pixel[1] < 140 && pixel[2] > 160).count();
        let red = image.pixels().filter(|pixel| pixel[0] > 180 && pixel[1] < 80 && pixel[2] < 80).count();
        assert!(blue * 100 >= image.width() as usize * image.height() as usize * 99);
        assert_eq!(red, 0, "DOCX must not contain the photo half hidden by the enclosing clip");
        let text = document.blocks.iter().filter_map(|block|
            if let Block::Paragraph(paragraph) = block { Some(paragraph.text()) } else { None }).collect::<Vec<_>>().join("\n");
        assert_eq!(text.matches("VISIBLE FORM CONTROL").count(), 1);
    }
}
