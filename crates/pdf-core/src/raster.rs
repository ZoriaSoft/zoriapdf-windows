//! Native rasterization with bounded decoded-image working sets. PDFium's page
//! cache flag does not bound document-level decoded streams, so dense image pages
//! use independent clipped viewports and release their documents between tiles.
use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;
use std::{fs::{File, OpenOptions}, path::Path};
use crate::geometry::{bounded_render_size, display_to_page_matrix, MAX_RENDER_PIXELS, MAX_RENDER_SIDE};

const SOURCE_PIXEL_BUDGET: u64 = 64_000_000;
const BLEED: i32 = 2;
const MAX_TILES: usize = 2048;

pub(crate) fn checked_matrix(matrix: PdfMatrix) -> Result<PdfMatrix> {
    if ![matrix.a(), matrix.b(), matrix.c(), matrix.d(), matrix.e(), matrix.f()].iter().all(|v| v.is_finite()) {
        bail!("PDF object transform contains non-finite values");
    }
    Ok(matrix)
}

/// Visit placements, not resources: a reused Form legitimately occurs twice.
pub(crate) fn visit_page_objects(page: &PdfPage, mut visit: impl FnMut(&mut PdfPageObject<'_>, PdfMatrix, &[usize]) -> Result<()>) -> Result<()> {
    fn descend(mut object: PdfPageObject<'_>, ancestor: PdfMatrix, depth: usize, visited: &mut usize,
        path: &mut [usize; 33], visit: &mut impl FnMut(&mut PdfPageObject<'_>, PdfMatrix, &[usize]) -> Result<()>) -> Result<()> {
        *visited += 1;
        if depth > 32 || *visited > 100_000 { bail!("PDF Form nesting or object count limit exceeded"); }
        if let Some(form) = object.as_x_object_form_object() {
            let matrix = checked_matrix(form.matrix()?.multiply(ancestor))?;
            if depth == 32 && form.len() != 0 { bail!("PDF Form nesting limit exceeded"); }
            for index in 0..form.len() {
                path[depth + 1] = index as usize;
                descend(form.get(index)?, matrix, depth + 1, visited, path, visit)?;
            }
            Ok(())
        } else { visit(&mut object, ancestor, &path[..=depth]) }
    }
    let mut visited = 0;
    let mut path = [0; 33];
    let objects = page.objects();
    for index in 0..objects.len() {
        path[0] = index as usize;
        descend(objects.get(index)?, PdfMatrix::IDENTITY, 0, &mut visited, &mut path, &mut visit)?;
    }
    Ok(())
}

pub(crate) fn image_bounds(object: &PdfPageObject, ancestor: PdfMatrix) -> Result<PdfRect> {
    let bounds = object.bounds()?.transform(ancestor);
    if ![bounds.x1().value, bounds.y1().value, bounds.x2().value, bounds.y2().value,
        bounds.x3().value, bounds.y3().value, bounds.x4().value, bounds.y4().value].iter().all(|v| v.is_finite()) {
        bail!("PDF image bounds contain non-finite values");
    }
    Ok(bounds.to_rect())
}

pub(crate) fn image_source_size(image: &PdfPageImageObject<'_>) -> Result<(i32, i32)> {
    let width = image.width()?;
    let height = image.height()?;
    if width <= 0 || height <= 0 || width > MAX_RENDER_SIDE || height > MAX_RENDER_SIDE
        || width as u64 * height as u64 > MAX_RENDER_PIXELS { bail!("Embedded image exceeds the supported source dimensions"); }
    Ok((width, height))
}

#[derive(Clone, Copy, Debug)]
struct Rect { left: i32, top: i32, right: i32, bottom: i32 }
impl Rect {
    fn width(self) -> i32 { self.right - self.left }
    fn height(self) -> i32 { self.bottom - self.top }
    fn padded(self, width: i32, height: i32) -> Self {
        Self { left: (self.left - BLEED).max(0), top: (self.top - BLEED).max(0),
            right: (self.right + BLEED).min(width), bottom: (self.bottom + BLEED).min(height) }
    }
    fn intersects(self, other: Self) -> bool {
        self.left < other.right && self.right > other.left && self.top < other.bottom && self.bottom > other.top
    }
}
struct Region { rect: Rect, path: [usize; 33], depth: usize, pixels: u64 }

fn regions(page: &PdfPage, width: i32, height: i32) -> Result<Vec<Region>> {
    let to_display = checked_matrix(display_to_page_matrix(page)?.invert())?;
    let sx = width as f32 / page.width().value;
    let sy = height as f32 / page.height().value;
    let mut regions = Vec::new();
    visit_page_objects(page, |object, ancestor, path| {
        if object.as_image_object().is_none() { return Ok(()); }
        let bounds = image_bounds(object, ancestor)?;
        let (mut left, mut top, mut right, mut bottom) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (x, y) in [(bounds.left(), bounds.bottom()), (bounds.left(), bounds.top()), (bounds.right(), bounds.bottom()), (bounds.right(), bounds.top())] {
            let (x, y) = to_display.apply_to_points(x, y);
            let x = x.value * sx;
            let y = (page.height().value - y.value) * sy;
            left = left.min(x); right = right.max(x); top = top.min(y); bottom = bottom.max(y);
        }
        if ![left, top, right, bottom].iter().all(|v| v.is_finite()) { bail!("PDF image viewport is invalid"); }
        let rect = Rect { left: left.floor().clamp(0.0, width as f32) as i32, top: top.floor().clamp(0.0, height as f32) as i32,
            right: right.ceil().clamp(0.0, width as f32) as i32, bottom: bottom.ceil().clamp(0.0, height as f32) as i32 };
        if rect.width() == 0 || rect.height() == 0 { return Ok(()); }
        if regions.len() >= 4096 { bail!("PDF image placement limit exceeded"); }
        let mut stored_path = [0; 33]; stored_path[..path.len()].copy_from_slice(path);
        regions.push(Region { rect, path: stored_path, depth: path.len(), pixels: 0 });
        Ok(())
    })?;
    Ok(regions)
}

fn source_pixels(page: &PdfPage, path: &[usize]) -> Result<u64> {
    fn descend(object: PdfPageObject<'_>, path: &[usize]) -> Result<u64> {
        if let Some((&index, tail)) = path.split_first() {
            let form = object.as_x_object_form_object().context("Image placement no longer belongs to a Form")?;
            return descend(form.get(index.try_into().context("Image index is invalid")?)?, tail);
        }
        let image = object.as_image_object().context("Image placement is missing")?;
        let (width, height) = (image.width()?, image.height()?);
        if width <= 0 || height <= 0 || width as u64 * height as u64 > SOURCE_PIXEL_BUDGET {
            bail!("Embedded image exceeds the bounded rendering budget");
        }
        Ok(width as u64 * height as u64)
    }
    let (&index, tail) = path.split_first().context("Image placement path is empty")?;
    descend(page.objects().get(index.try_into().context("Image index is invalid")?)?, tail)
}

fn plan(width: i32, height: i32, images: &[Region]) -> Result<Vec<Rect>> {
    let mut pending = vec![Rect { left: 0, top: 0, right: width, bottom: height }];
    let mut tiles = Vec::new();
    while let Some(rect) = pending.pop() {
        let padded = rect.padded(width, height);
        let pixels: u64 = images.iter().filter(|image| padded.intersects(image.rect)).map(|image| image.pixels).sum();
        if pixels <= SOURCE_PIXEL_BUDGET { tiles.push(rect); continue; }
        if tiles.len() + pending.len() + 2 > MAX_TILES || (rect.width() == 1 && rect.height() == 1) {
            bail!("Overlapping PDF image layers exceed the bounded rendering budget");
        }
        if rect.width() >= rect.height() && rect.width() > 1 {
            let middle = rect.left + rect.width() / 2;
            pending.push(Rect { right: middle, ..rect }); pending.push(Rect { left: middle, ..rect });
        } else {
            let middle = rect.top + rect.height() / 2;
            pending.push(Rect { bottom: middle, ..rect }); pending.push(Rect { top: middle, ..rect });
        }
    }
    Ok(tiles)
}

fn open_source(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new(); options.read(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); // FILE_SHARE_READ: a render cannot mix replaced source revisions.
    }
    options.open(path).with_context(|| format!("Cannot open PDF for rendering: '{}'", path.display()))
}

pub(crate) fn render_requested(pdfium: &Pdfium, path: &Path, password: Option<&str>, page: u32,
    width: u32, max_width: u32, annotations: bool) -> Result<image::DynamicImage> {
    let source = open_source(path)?;
    let doc = pdfium.load_pdf_from_reader(source.try_clone()?, password)?;
    let index = page.checked_sub(1).context("Page number must be 1-based")?.try_into().context("Page index is invalid")?;
    let native_page = doc.pages().get(index)?;
    let size = bounded_render_size(native_page.width().value, native_page.height().value, width, max_width)?;
    drop(native_page);
    render_loaded(pdfium, source, doc, password, index, size, annotations)
}

pub(crate) fn render_fixed(pdfium: &Pdfium, path: &Path, password: Option<&str>, page: PdfPageIndex,
    size: (i32, i32), annotations: bool) -> Result<image::DynamicImage> {
    if size.0 < 1 || size.1 < 1 || size.0 > MAX_RENDER_SIDE || size.1 > MAX_RENDER_SIDE
        || size.0 as u64 * size.1 as u64 > MAX_RENDER_PIXELS { bail!("Raster dimensions exceed rendering limits"); }
    let source = open_source(path)?;
    let doc = pdfium.load_pdf_from_reader(source.try_clone()?, password)?;
    render_loaded(pdfium, source, doc, password, page, size, annotations)
}

fn config(width: i32, height: i32, annotations: bool) -> PdfRenderConfig {
    PdfRenderConfig::new().set_fixed_size(width, height).limit_render_image_cache_size(true)
        .render_annotations(annotations).render_form_data(true)
}

fn render_loaded<'a>(pdfium: &'a Pdfium, source: File, doc: PdfDocument<'a>, password: Option<&str>,
    index: PdfPageIndex, (width, height): (i32, i32), annotations: bool) -> Result<image::DynamicImage> {
    let page = doc.pages().get(index)?;
    let mut images = regions(&page, width, height)?;
    // Metadata can retain decoded streams too. Release each image's document
    // before querying the next, including sources larger than Word's image cap.
    if let Some(region) = images.first_mut() {
        region.pixels = source_pixels(&page, &region.path[..region.depth])?;
    }
    if images.len() <= 1 {
        return Ok(page.render_with_config(&config(width, height, annotations))?.as_image()?);
    }
    drop(page); drop(doc);
    for region in &mut images[1..] {
        let doc = pdfium.load_pdf_from_reader(source.try_clone()?, password)?;
        let page = doc.pages().get(index)?;
        region.pixels = source_pixels(&page, &region.path[..region.depth])?;
    }
    let tiles = plan(width, height, &images)?;
    let mut output = image::RgbaImage::new(width as u32, height as u32);
    for tile in tiles {
        let viewport = tile.padded(width, height);
        let doc = pdfium.load_pdf_from_reader(source.try_clone()?, password)?;
        let page = doc.pages().get(index)?;
        let mut bitmap = PdfBitmap::empty(viewport.width(), viewport.height(), PdfBitmapFormat::BGRA)?;
        let settings = config(width, height, annotations).set_origin(-viewport.left, -viewport.top);
        // Native clipping is the bitmap's viewport; full-page scale and origin
        // stay identical in every tile, including form widgets and annotations.
        page.render_into_bitmap_with_config(&mut bitmap, &settings)?;
        let pixels = bitmap.as_image()?.into_rgba8();
        let row_bytes = tile.width() as usize * 4;
        for row in 0..tile.height() as usize {
            let from = (((tile.top - viewport.top) as usize + row) * pixels.width() as usize
                + (tile.left - viewport.left) as usize) * 4;
            let to = ((tile.top as usize + row) * width as usize + tile.left as usize) * 4;
            output.as_mut()[to..to + row_bytes].copy_from_slice(&pixels.as_raw()[from..from + row_bytes]);
        }
    }
    Ok(image::DynamicImage::ImageRgba8(output))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tiles_partition_the_raster_and_bound_each_decode_working_set() {
        let mut regions = Vec::new();
        for y in 0..10 { for x in 0..10 {
            regions.push(Region { rect: Rect { left: x * 100 + 10, top: y * 100 + 10, right: x * 100 + 90, bottom: y * 100 + 90 },
                path: [0; 33], depth: 1, pixels: 4_194_304 });
        }}
        let tiles = plan(1000, 1000, &regions).unwrap();
        assert_eq!(tiles.iter().map(|r| r.width() as u64 * r.height() as u64).sum::<u64>(), 1_000_000);
        for (index, tile) in tiles.iter().enumerate() {
            assert!(regions.iter().filter(|r| tile.padded(1000, 1000).intersects(r.rect)).map(|r| r.pixels).sum::<u64>() <= SOURCE_PIXEL_BUDGET);
            for other in &tiles[index + 1..] { assert!(!tile.intersects(*other)); }
        }
    }
    #[test]
    fn overlapping_layers_fail_before_allocating_an_unbounded_raster() {
        let regions: Vec<_> = (0..3).map(|_| Region {
            rect: Rect { left: 0, top: 0, right: 16, bottom: 16 },
            path: [0; 33], depth: 1, pixels: MAX_RENDER_PIXELS,
        }).collect();
        assert!(plan(16, 16, &regions).is_err());
    }
}
