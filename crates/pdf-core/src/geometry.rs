use anyhow::{bail, Context, Result};
use pdfium_render::prelude::*;

pub const MAX_RENDER_PIXELS: u64 = 32_000_000;
pub const MAX_RENDER_SIDE: i32 = 16_384;

/// Check decoder metadata before allocating the raster, for files and DOCX parts.
pub fn decode_bounded_image<R: std::io::BufRead + std::io::Seek>(
    mut reader: image::ImageReader<R>,
) -> Result<image::DynamicImage> {
    use image::ImageDecoder;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_RENDER_SIDE as u32);
    limits.max_image_height = Some(MAX_RENDER_SIDE as u32);
    limits.max_alloc = Some(MAX_RENDER_PIXELS * 4);
    reader.limits(limits);
    let decoder = reader.into_decoder().context("Cannot read image metadata")?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || width > MAX_RENDER_SIDE as u32 || height > MAX_RENDER_SIDE as u32
        || width as u64 * height as u64 > MAX_RENDER_PIXELS || decoder.total_bytes() > MAX_RENDER_PIXELS * 4 {
        bail!("Image dimensions exceed the supported decoding limits");
    }
    Ok(image::DynamicImage::from_decoder(decoder)?)
}

pub fn load_bounded_image(path: &std::path::Path) -> Result<image::DynamicImage> {
    let reader = image::ImageReader::open(path)
        .with_context(|| format!("Cannot open image '{}'", path.display()))?
        .with_guessed_format().context("Cannot identify image format")?;
    decode_bounded_image(reader)
}

/// Fix both bitmap dimensions before PDFium allocates it. One scale preserves
/// aspect ratio; pathological geometry is rejected, never independently clamped.
pub fn bounded_render_size(width_points: f32, height_points: f32, requested_width: u32, max_width: u32) -> Result<(i32, i32)> {
    if !width_points.is_finite() || !height_points.is_finite()
        || width_points <= 0.0 || height_points <= 0.0 || requested_width == 0 || max_width == 0
    {
        bail!("Page dimensions or render size are invalid");
    }
    let width = f64::from(width_points);
    let height = f64::from(height_points);
    let side = f64::from(MAX_RENDER_SIDE);
    let scale = (f64::from(requested_width.min(max_width)) / width)
        .min(side / width)
        .min(side / height)
        .min((MAX_RENDER_PIXELS as f64 / (width * height)).sqrt());
    let scaled_width = width * scale;
    let scaled_height = height * scale;
    if scaled_width < 1.0 || scaled_height < 1.0 {
        bail!("Page aspect ratio exceeds the supported rendering limits");
    }
    let out_width = scaled_width.floor() as i32;
    let out_height = scaled_height.floor() as i32;
    if out_width < 1 || out_height < 1 || out_width > MAX_RENDER_SIDE || out_height > MAX_RENDER_SIDE
        || out_width as u64 * out_height as u64 > MAX_RENDER_PIXELS
        || !(f64::from(out_width) / width).is_finite()
        || !(f64::from(out_height) / height).is_finite()
        || !((f64::from(out_width) / width) as f32).is_finite()
        || !((f64::from(out_height) / height) as f32).is_finite()
    {
        bail!("Page dimensions exceed the supported rendering limits");
    }
    Ok((out_width, out_height))
}

/// Convert displayed, bottom-left point coordinates into the document's native
/// page coordinates. PDFium accounts for intrinsic rotation and effective boxes.
pub fn display_to_page_matrix(page: &PdfPage<'_>) -> Result<PdfMatrix> {
    let width = page.width().value;
    let height = page.height().value;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        bail!("Page dimensions are invalid");
    }
    let config = PdfRenderConfig::new().set_fixed_size(1, 1);
    let origin = page.pixels_to_points(0, 1, &config).context("Cannot map displayed page origin")?;
    let right = page.pixels_to_points(1, 1, &config).context("Cannot map displayed page width")?;
    let top = page.pixels_to_points(0, 0, &config).context("Cannot map displayed page height")?;
    let coefficients = [
        (right.0.value - origin.0.value) / width,
        (right.1.value - origin.1.value) / width,
        (top.0.value - origin.0.value) / height,
        (top.1.value - origin.1.value) / height,
        origin.0.value,
        origin.1.value,
    ];
    if coefficients.iter().any(|value| !value.is_finite()) {
        bail!("Page coordinate transform is invalid");
    }
    Ok(PdfMatrix::new(coefficients[0], coefficients[1], coefficients[2], coefficients[3], coefficients[4], coefficients[5]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_pages_keep_their_requested_aspect_and_export_resolution() {
        assert_eq!(bounded_render_size(600.0, 800.0, 2400, 2400).unwrap(), (2400, 3200));
        assert_eq!(bounded_render_size(600.0, 800.0, 6000, 4800).unwrap(), (4800, 6400));
        assert_eq!(bounded_render_size(4000.0, 8000.0, 4000, 16384).unwrap(), (4000, 8000));
    }

    #[test]
    fn pixel_and_side_limits_apply_together_without_axis_distortion() {
        let square = bounded_render_size(1000.0, 1000.0, 10000, 16384).unwrap();
        assert_eq!(square, (5656, 5656));
        let tall = bounded_render_size(100.0, 1000.0, u32::MAX, u32::MAX).unwrap();
        assert!(tall.0 as u64 * tall.1 as u64 <= MAX_RENDER_PIXELS);
        assert!(tall.1 <= MAX_RENDER_SIDE);
        assert!((f64::from(tall.1) / f64::from(tall.0) - 10.0).abs() < 0.01);
    }

    #[test]
    fn impossible_or_nonfinite_page_geometry_fails_before_allocation() {
        for (width, height) in [(0.0, 800.0), (-1.0, 800.0), (f32::NAN, 800.0), (600.0, f32::INFINITY), (1.0, 20000.0), (f32::from_bits(1), 1.0), (f32::from_bits(1), f32::from_bits(1))] {
            assert!(bounded_render_size(width, height, 2400, 2400).is_err());
        }
        assert!(bounded_render_size(600.0, 800.0, 0, 2400).is_err());
    }
}
