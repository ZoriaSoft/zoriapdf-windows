use anyhow::{bail, Context, Result};

/// Parses 1-based page specifications into an ordered list of page numbers.
///
/// Supports:
/// - Single numbers: `"3"` -> `[3]`
/// - Inclusive forward ranges: `"3-5"` -> `[3, 4, 5]`
/// - Inclusive reverse ranges: `"5-1"` -> `[5, 4, 3, 2, 1]`
/// - Comma-delimited combinations: `"1, 3-5, 2"` -> `[1, 3, 4, 5, 2]`
/// - Empty or whitespace string -> returns all pages `1..=total_pages`
pub fn parse_page_ranges(range_str: &str, total_pages: u32) -> Result<Vec<u32>> {
    let trimmed = range_str.trim();
    if trimmed.is_empty() {
        if total_pages == 0 {
            return Ok(Vec::new());
        }
        return Ok((1..=total_pages).collect());
    }

    if total_pages == 0 {
        bail!("Cannot select pages from an empty document (0 pages)");
    }

    let mut result = Vec::new();
    for token in trimmed.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        if let Some((start_raw, end_raw)) = token.split_once('-') {
            let start: u32 = start_raw
                .trim()
                .parse()
                .with_context(|| format!("Invalid page number '{}' in range '{}'", start_raw, token))?;
            let end: u32 = end_raw
                .trim()
                .parse()
                .with_context(|| format!("Invalid page number '{}' in range '{}'", end_raw, token))?;

            if start == 0 || end == 0 {
                bail!("Page numbers must be 1-based (found 0 in '{}')", token);
            }
            if start > total_pages {
                bail!(
                    "Start page {} exceeds total document pages ({}) in '{}'",
                    start,
                    total_pages,
                    token
                );
            }
            if end > total_pages {
                bail!(
                    "End page {} exceeds total document pages ({}) in '{}'",
                    end,
                    total_pages,
                    token
                );
            }

            if start <= end {
                for p in start..=end {
                    result.push(p);
                }
            } else {
                for p in (end..=start).rev() {
                    result.push(p);
                }
            }
        } else {
            let p: u32 = token
                .parse()
                .with_context(|| format!("Invalid page number '{}'", token))?;
            if p == 0 {
                bail!("Page numbers must be 1-based (found 0)");
            }
            if p > total_pages {
                bail!(
                    "Page {} exceeds total document pages ({})",
                    p,
                    total_pages
                );
            }
            result.push(p);
        }
    }

    if result.is_empty() {
        bail!("No valid pages specified in range string: '{}'", range_str);
    }

    Ok(result)
}
