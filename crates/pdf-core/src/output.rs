//! Produce complete results privately, then publish them without replacing a file.
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::collision::check_collision;
use crate::fs_security::{make_private, new_file_access, AccessControls};
use crate::scratch::ScratchDir;
use crate::types::{OperationRequest, OperationResult};

pub struct OutputTransaction {
    target: PathBuf,
    staged: PathBuf,
    directory: bool,
    access: AccessControls,
    scratch: ScratchDir,
}

fn directory_output(kind: &str) -> bool {
    matches!(kind, "split" | "pdf-to-images" | "ocr")
}

fn ensure_destination(path: &Path, directory: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("Cannot inspect output destination"),
        Ok(metadata) if directory && metadata.is_dir() && !metadata.file_type().is_symlink() => {
            #[cfg(windows)] {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    bail!("Output directory '{}' must not be a link", path.display());
                }
            }
            let mut entries = fs::read_dir(path)?;
            if entries.next().transpose()?.is_none() {
                return Ok(());
            }
        }
        Ok(_) => {}
    }
    if directory {
        bail!("Output directory '{}' is not empty. Choose an empty folder; existing files are never overwritten.", path.display());
    }
    bail!("Output file '{}' already exists. Choose a different name; existing files are never overwritten.", path.display());
}

#[cfg(windows)]
fn move_no_replace(source: &Path, target: &Path, _directory: bool) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // No COPY_ALLOWED or REPLACE_EXISTING: staging is on the destination volume.
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn rename_handle_no_replace(file: &fs::File, target: &Path) -> std::io::Result<()> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{SetFileInformationByHandle, FileRenameInfo, FILE_RENAME_INFO};
    let header = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    // WTF-8 byte length bounds the UTF-16 length. One aligned allocation and
    // one encoding pass; the API receives only the populated bytes.
    let capacity = header + (target.as_os_str().len() + 1) * 2;
    let mut buffer = vec![0u64; capacity.div_ceil(std::mem::size_of::<u64>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    let mut length = 0;
    unsafe {
        let name = (*info).FileName.as_mut_ptr();
        for unit in target.as_os_str().encode_wide() {
            name.add(length).write(unit);
            length += 1;
        }
        (*info).FileNameLength = (length * 2) as u32;
        // Zeroed ReplaceIfExists=false: never replace another writer's file.
        if SetFileInformationByHandle(file.as_raw_handle(), FileRenameInfo,
            info.cast(), (header + length * 2) as u32) == 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn move_no_replace(source: &Path, target: &Path, directory: bool) -> std::io::Result<()> {
    if directory {
        // POSIX rename only replaces an empty directory, never its contents.
        fs::rename(source, target)
    } else {
        // An exclusive hard link is the portable same-volume no-clobber primitive.
        fs::hard_link(source, target)?;
        fs::remove_file(source)
    }
}

impl OutputTransaction {
    pub fn new(request: &OperationRequest) -> Result<Self> {
        let kind = request.kind.to_ascii_lowercase();
        if kind == "install-ocr" {
            bail!("The component installer manages its own destination");
        }
        let directory = directory_output(&kind);
        let output = Path::new(&request.output);
        let name = output.file_name().filter(|name| !name.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Choose an output file or folder"))?;
        check_collision(&request.inputs, &request.output, directory)?;
        if let Some(images) = request.image_paths.as_ref() {
            check_collision(images, &request.output, directory)?;
        }
        ensure_destination(output, directory)?;
        let parent = output.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).context("Cannot create output parent directory")?;
        let parent = parent.canonicalize().context("Cannot resolve output parent directory")?;
        let target = parent.join(name);
        let mut scratch = ScratchDir::new()?;
        let root = scratch.output_stage(&parent)?;
        let staged = root.join(name);
        let access = if directory {
            AccessControls::capture(if target.exists() { &target } else { &root })?
        } else {
            new_file_access(&parent)?
        };
        make_private(&root)?;
        if directory {
            fs::create_dir(&staged).context("Cannot prepare staged output folder")?;
            access.apply(&staged)?;
        }
        Ok(Self { target, staged, directory, access, scratch })
    }

    pub fn staged_request(&self, request: &OperationRequest) -> OperationRequest {
        let mut staged = request.clone();
        staged.output = self.staged.to_string_lossy().into_owned();
        staged
    }

    pub fn scratch_path(&self) -> &Path {
        self.scratch.path()
    }

    pub fn commit(&mut self, mut result: OperationResult, cancel: &AtomicBool) -> Result<OperationResult> {
        if cancel.load(Ordering::Acquire) {
            bail!("Operation cancelled");
        }
        let metadata = fs::symlink_metadata(&self.staged).context("Completed output is missing")?;
        if metadata.file_type().is_symlink() || metadata.is_dir() != self.directory {
            bail!("Completed output has an unexpected type");
        }
        let mut outputs = Vec::with_capacity(result.outputs.len());
        for output in &result.outputs {
            let path = Path::new(output);
            let suffix = path.strip_prefix(&self.staged).context("Worker returned an output outside its transaction")?;
            if suffix.components().any(|part| !matches!(part, Component::Normal(_))) {
                bail!("Worker returned an invalid output path");
            }
            if !path.is_file() {
                bail!("Completed result file is missing: '{}'", path.display());
            }
            let published = if suffix.as_os_str().is_empty() { self.target.clone() } else { self.target.join(suffix) };
            outputs.push(published.to_string_lossy().into_owned());
        }
        #[cfg(windows)]
        let publication = self.access.publication_handle(&self.staged)?;
        if !self.directory { self.access.apply(&self.staged)?; }
        ensure_destination(&self.target, self.directory)?;
        if self.directory && self.target.exists() {
            // remove_dir itself verifies emptiness. A concurrent new file prevents
            // removal; a concurrent replacement after it makes the move fail safely.
            fs::remove_dir(&self.target).context("Output directory is no longer empty")?;
        }
        #[cfg(windows)]
        let moved = match publication.as_ref() {
            Some(file) => rename_handle_no_replace(file, &self.target),
            None => move_no_replace(&self.staged, &self.target, self.directory),
        };
        #[cfg(not(windows))]
        let moved = move_no_replace(&self.staged, &self.target, self.directory);
        moved.with_context(|| format!("Output destination changed while processing; existing files were not replaced: '{}'", self.target.display()))?;
        #[cfg(windows)]
        if let Some(file) = publication.as_ref() {
            if let Err(error) = self.access.published(file) {
                return match rename_handle_no_replace(file, &self.staged) {
                    Ok(()) => Err(error.context("Output permission restoration failed; publication was rolled back")),
                    Err(rollback) => Err(error.context(format!(
                        "Completed output remains at '{}'; permission restoration and rollback failed: {rollback}",
                        self.target.display()))),
                };
            }
        }
        result.outputs = outputs;
        result.message = result.message.replace(self.staged.to_string_lossy().as_ref(), self.target.to_string_lossy().as_ref());
        Ok(result)
    }

    pub fn cleanup(&mut self) -> Result<()> {
        self.scratch.cleanup()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn request(kind: &str, output: &Path) -> OperationRequest {
        serde_json::from_value(serde_json::json!({
            "id": "transaction-test", "kind": kind, "inputs": [], "output": output
        })).unwrap()
    }

    fn result(path: &Path) -> OperationResult {
        OperationResult { outputs: vec![path.to_string_lossy().into_owned()], message: "Complete".into() }
    }

    #[test]
    fn a_destination_created_during_processing_is_not_replaced() {
        let temp = ScratchDir::new().unwrap();
        let target = temp.path().join("result.pdf");
        let request = request("rotate", &target);
        let mut transaction = OutputTransaction::new(&request).unwrap();
        let staged = PathBuf::from(transaction.staged_request(&request).output);
        fs::write(&staged, b"new completed result").unwrap();
        fs::write(&target, b"another writer's document").unwrap();
        assert!(transaction.commit(result(&staged), &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"another writer's document");
    }

    #[test]
    fn directory_results_appear_together_and_map_to_the_final_folder() {
        let temp = ScratchDir::new().unwrap();
        let target = temp.path().join("pages");
        fs::create_dir(&target).unwrap();
        let request = request("split", &target);
        let mut transaction = OutputTransaction::new(&request).unwrap();
        let staged = PathBuf::from(transaction.staged_request(&request).output);
        fs::create_dir_all(&staged).unwrap();
        let first = staged.join("one.pdf");
        let second = staged.join("two.pdf");
        fs::write(&first, b"first page").unwrap();
        fs::write(&second, b"second page").unwrap();
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        let outputs = OperationResult { outputs: vec![first.to_string_lossy().into(), second.to_string_lossy().into()], message: "Complete".into() };
        let published = transaction.commit(outputs, &AtomicBool::new(false)).unwrap();
        assert_eq!(published.outputs.len(), 2);
        let final_directory = target.canonicalize().unwrap();
        for output in &published.outputs {
            assert_eq!(Path::new(output).parent().unwrap().canonicalize().unwrap(), final_directory);
        }
        assert_eq!(fs::read(&published.outputs[0]).unwrap(), b"first page");
        assert_eq!(fs::read(&published.outputs[1]).unwrap(), b"second page");
    }

    #[test]
    fn cancellation_does_not_publish_a_partial_directory_or_block_retry() {
        let temp = ScratchDir::new().unwrap();
        let target = temp.path().join("pages");
        let request = request("split", &target);
        let mut transaction = OutputTransaction::new(&request).unwrap();
        let staged = PathBuf::from(transaction.staged_request(&request).output);
        fs::create_dir_all(&staged).unwrap();
        let first = staged.join("one.pdf");
        fs::write(&first, b"only first page completed").unwrap();
        assert!(transaction.commit(result(&first), &AtomicBool::new(true)).is_err());
        transaction.cleanup().unwrap();
        assert!(!target.exists());
        let retry = OutputTransaction::new(&request).unwrap();
        assert!(fs::read_dir(&retry.staged).unwrap().next().is_none());
    }

    #[test]
    fn a_file_added_to_an_initially_empty_output_directory_is_preserved() {
        let temp = ScratchDir::new().unwrap();
        let target = temp.path().join("pages");
        fs::create_dir(&target).unwrap();
        let request = request("split", &target);
        let mut transaction = OutputTransaction::new(&request).unwrap();
        let staged = PathBuf::from(transaction.staged_request(&request).output);
        fs::create_dir_all(&staged).unwrap();
        let output = staged.join("one.pdf");
        fs::write(&output, b"exported page").unwrap();
        fs::write(target.join("keep.txt"), b"user data").unwrap();
        assert!(transaction.commit(result(&output), &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"user data");
        assert!(!target.join("one.pdf").exists());
    }
}
