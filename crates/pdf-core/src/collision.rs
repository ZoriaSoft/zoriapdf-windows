use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// Resolves a path to an absolute path for comparison.
/// If `canonicalize` succeeds, uses that (resolving symlinks).
/// Otherwise falls back to resolving against the current working directory.
pub fn normalize_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        canonical
    } else if let Ok(cwd) = std::env::current_dir() {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        }
    } else {
        path.to_path_buf()
    }
}

/// Checks that the proposed output destination does not collide with any input file.
/// If `is_directory` is true, verifies that the output directory is not identical to
/// or parent of any input file.
/// If `is_directory` is false, verifies that the output file path does not point to
/// the same location as any input file.
pub fn check_collision(inputs: &[String], output: &str, is_directory: bool) -> Result<()> {
    let out_path = Path::new(output);
    let norm_out = normalize_path(out_path);

    for inp in inputs {
        let inp_path = Path::new(inp);
        let norm_inp = normalize_path(inp_path);

        if is_directory {
            if norm_out == norm_inp {
                bail!(
                    "Output directory '{}' collides directly with input file '{}'",
                    output,
                    inp
                );
            }
            if norm_inp.starts_with(&norm_out) {
                bail!(
                    "Output directory '{}' contains input file '{}'; refusing to modify input directory in-place",
                    output,
                    inp
                );
            }
        } else {
            if norm_out == norm_inp {
                bail!(
                    "Output file '{}' collides with input file '{}'. Overwriting input files is strictly prohibited.",
                    output,
                    inp
                );
            }
        }
    }

    Ok(())
}

/// Ensures the parent directory of an output file exists.
pub fn ensure_parent_dir(file_path: &Path) -> Result<()> {
    if let Some(parent) = file_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}
