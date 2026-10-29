//! Owned temporary files, with an OS lease so an interrupted job can be recovered.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Read;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const LEASE_FILE: &str = "owner.lock";
const OWNERSHIP_FILE: &str = "owner.json";
const STAGE_PREFIX: &str = ".zoriapdf-output-";

#[derive(Serialize, Deserialize)]
struct Ownership {
    version: u8,
    stage: Option<PathBuf>,
}

pub struct ScratchDir {
    path: PathBuf,
    id: Uuid,
    lease: Option<File>,
    stage: Option<PathBuf>,
    cleanup_on_drop: bool,
}

fn jobs_root() -> PathBuf {
    std::env::temp_dir().join("zoriapdf-jobs-v1")
}

fn plain_directory(path: &Path) -> std::io::Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(false);
    }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Ok(false); // FILE_ATTRIBUTE_REPARSE_POINT
        }
    }
    Ok(true)
}

fn registry_lease(root: &Path) -> Result<File> {
    fs::create_dir_all(root).context("Cannot create temporary jobs registry")?;
    if !plain_directory(root)? {
        bail!("Temporary jobs registry must not be a link");
    }
    let file = OpenOptions::new().read(true).write(true).create(true)
        .truncate(false).open(root.join("registry.lock"))?;
    file.lock().context("Cannot lock temporary jobs registry")?;
    Ok(file)
}

fn remove_stage(stage: &Path, id: Uuid) -> Result<()> {
    let expected = format!("{STAGE_PREFIX}{id}");
    if !stage.is_absolute() || stage.file_name().and_then(|name| name.to_str()) != Some(expected.as_str()) {
        bail!("Temporary output ownership record is invalid");
    }
    match plain_directory(stage) {
        Ok(true) => fs::remove_dir_all(stage)
            .with_context(|| format!("Cannot clean temporary output '{}'", stage.display())),
        Ok(false) => bail!("Temporary output is not an owned directory: '{}'", stage.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

impl ScratchDir {
    pub fn new() -> Result<Self> {
        Self::new_in(&jobs_root())
    }

    fn new_in(root: &Path) -> Result<Self> {
        // Registration and recovery are serialized: another process must not
        // mistake a just-created, not-yet-leased directory for an abandoned job.
        let _registry = registry_lease(root)?;
        Self::recover_locked(root)?;
        let id = Uuid::new_v4();
        let path = root.join(id.to_string());
        fs::create_dir_all(path.parent().expect("jobs directory has a parent"))?;
        fs::create_dir(&path).context("Cannot create temporary job directory")?;
        let lease = match OpenOptions::new().read(true).write(true).create_new(true).open(path.join(LEASE_FILE)) {
            Ok(file) => file,
            Err(error) => {
                let _ = fs::remove_dir(&path);
                return Err(error).context("Cannot create temporary job ownership record");
            }
        };
        let mut owned = Self { path, id, lease: Some(lease), stage: None, cleanup_on_drop: false };
        let initialized: Result<()> = (|| {
            owned.lease.as_ref().expect("new lease").try_lock()?;
            owned.write_ownership()?;
            crate::fs_security::make_private(&owned.path)
        })();
        if let Err(error) = initialized {
            return match owned.cleanup_locked() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(error.context(format!("Temporary registration cleanup failed: {cleanup:#}"))),
            };
        }
        owned.cleanup_on_drop = true;
        Ok(owned)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn output_stage(&mut self, parent: &Path) -> Result<PathBuf> {
        let stage = parent.join(format!("{STAGE_PREFIX}{}", self.id));
        fs::create_dir(&stage).context("Cannot create output staging directory")?;
        // No output bytes are written until this atomic ownership update is durable.
        self.stage = Some(stage.clone());
        self.write_ownership()?;
        Ok(stage)
    }

    fn write_ownership(&mut self) -> Result<()> {
        let pending = self.path.join("owner.next.json");
        let mut file = File::create(&pending)?;
        serde_json::to_writer(&mut file, &Ownership { version: 1, stage: self.stage.clone() })?;
        file.sync_all()?;
        drop(file);
        fs::rename(pending, self.path.join(OWNERSHIP_FILE))?;
        Ok(())
    }

    pub fn cleanup(&mut self) -> Result<()> {
        if matches!(fs::symlink_metadata(&self.path), Err(error) if error.kind() == std::io::ErrorKind::NotFound) {
            return self.cleanup_locked();
        }
        let root = self.path.parent().context("Temporary job has no registry")?;
        // Keep recovery/registration excluded until the unlocked lease and
        // ownership files have all been removed, not just until payload cleanup.
        let _registry = registry_lease(root)?;
        self.cleanup_locked()
    }

    fn cleanup_locked(&mut self) -> Result<()> {
        match plain_directory(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(stage) = self.stage.as_ref() {
                    remove_stage(stage, self.id)?;
                    self.stage = None;
                }
                self.lease.take();
                return Ok(());
            }
            Err(error) => return Err(error.into()),
            Ok(false) => bail!("Temporary job directory must not be a link"),
            Ok(true) => {}
        }
        let nested = self.path.join("zoriapdf-jobs-v1");
        match fs::symlink_metadata(&nested) {
            Ok(_) => {
                let registry = registry_lease(&nested)?;
                Self::recover_locked(&nested)?;
                // Never recursively delete a nested live lease (or unknown job).
                // Its process may still own an external plaintext/staging tree.
                for entry in fs::read_dir(&nested)? {
                    if entry?.file_name() != "registry.lock" {
                        bail!("Nested temporary jobs are still active or unrecognized");
                    }
                }
                drop(registry);
                fs::remove_file(nested.join("registry.lock"))?;
                fs::remove_dir(&nested).context("Nested temporary jobs are still active")?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("Cannot inspect nested temporary jobs"),
        }
        if let Some(stage) = self.stage.as_ref() {
            remove_stage(stage, self.id)?;
            self.stage = None;
        }
        // Keep ownership records until every payload is gone. If an antivirus or
        // another handle blocks deletion, the next recovery can still identify it.
        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            if entry.file_name() == LEASE_FILE || entry.file_name() == OWNERSHIP_FILE {
                continue;
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                let mut linked = metadata.file_type().is_symlink();
                #[cfg(windows)] {
                    use std::os::windows::fs::MetadataExt;
                    linked |= metadata.file_attributes() & 0x400 != 0;
                }
                if linked { fs::remove_dir(&path)?; } else { fs::remove_dir_all(&path)?; }
            } else {
                fs::remove_file(&path)?;
            }
        }
        self.lease.take();
        for name in [OWNERSHIP_FILE, LEASE_FILE] {
            match fs::remove_file(self.path.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("Cannot remove temporary job ownership record"),
            }
        }
        fs::remove_dir(&self.path).context("Cannot remove empty temporary job directory")
    }

    /// Only versioned, UUID-named directories with an unlocked ownership record
    /// belong to an interrupted job. Active jobs and unrecognized files are left alone.
    pub fn recover_abandoned() -> Result<()> {
        Self::recover_at(&jobs_root())
    }

    fn recover_at(root: &Path) -> Result<()> {
        let _registry = registry_lease(root)?;
        Self::recover_locked(root)
    }

    fn recover_locked(root: &Path) -> Result<()> {
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).context("Cannot inspect interrupted job files"),
        };
        for entry in entries {
            let entry = entry?;
            let Ok(id) = Uuid::parse_str(&entry.file_name().to_string_lossy()) else { continue };
            let path = entry.path();
            match plain_directory(&path) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            }
            let lease = match OpenOptions::new().read(true).write(true).open(path.join(LEASE_FILE)) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error).context("Cannot inspect temporary job ownership"),
            };
            match lease.try_lock() {
                Ok(()) => {}
                Err(TryLockError::WouldBlock) => continue,
                Err(TryLockError::Error(error)) => return Err(error).context("Cannot lock interrupted job files"),
            }
            let ownership_file = match File::open(path.join(OWNERSHIP_FILE)) {
                Ok(file) => Some(file),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error).context("Cannot read interrupted job ownership"),
            };
            let ownership = if let Some(file) = ownership_file {
                let mut bytes = Vec::new();
                file.take(128 * 1024).read_to_end(&mut bytes)?;
                serde_json::from_slice::<Ownership>(&bytes)
                    .context("Interrupted job ownership record is unreadable")?
            } else {
                Ownership { version: 1, stage: None }
            };
            if ownership.version != 1 {
                continue;
            }
            // The caller already owns this registry lock. Explicit recovery
            // must not re-enter it through cleanup or Drop after an error.
            let mut abandoned = Self { path, id, lease: Some(lease),
                stage: ownership.stage, cleanup_on_drop: false };
            abandoned.cleanup_locked()?;
        }
        Ok(())
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if self.cleanup_on_drop { let _ = self.cleanup(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("zoriapdf-scratch-test-{}", Uuid::new_v4())))
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn recovery_preserves_a_live_jobs_plaintext_and_staged_result() {
        let root = TestRoot::new();
        let jobs = root.0.join("jobs");
        let output_parent = root.0.join("exports");
        fs::create_dir_all(&output_parent).unwrap();
        let mut job = ScratchDir::new_in(&jobs).unwrap();
        let plaintext = job.path().join("protected-page.png");
        fs::write(&plaintext, b"live private page").unwrap();
        let stage = job.output_stage(&output_parent).unwrap();
        fs::write(stage.join("result.pdf"), b"live staged result").unwrap();
        ScratchDir::recover_at(&jobs).unwrap();
        assert_eq!(fs::read(&plaintext).unwrap(), b"live private page");
        assert_eq!(fs::read(stage.join("result.pdf")).unwrap(), b"live staged result");
    }

    #[test]
    fn recovery_removes_both_abandoned_plaintext_and_partial_outputs() {
        let root = TestRoot::new();
        let jobs = root.0.join("jobs");
        let output_parent = root.0.join("exports");
        fs::create_dir_all(&output_parent).unwrap();
        let mut abandoned = ScratchDir::new_in(&jobs).unwrap();
        let private_dir = abandoned.path().to_path_buf();
        fs::write(private_dir.join("protected-page.png"), b"decrypted page").unwrap();
        let stage = abandoned.output_stage(&output_parent).unwrap();
        fs::write(stage.join("page-1.pdf"), b"partial result").unwrap();
        // Process death releases the OS lease without running Rust destructors.
        abandoned.lease.take();
        ScratchDir::recover_at(&jobs).unwrap();
        assert!(!private_dir.exists());
        assert!(!stage.exists());
        assert!(fs::read_dir(&output_parent).unwrap().next().is_none());
    }

    #[test]
    fn parent_cleanup_preserves_live_nested_ownership_and_external_outputs() {
        let root = TestRoot::new();
        let outputs = root.0.join("exports");
        fs::create_dir_all(&outputs).unwrap();
        let mut parent = ScratchDir::new_in(&root.0.join("jobs")).unwrap();
        let parent_stage = parent.output_stage(&outputs).unwrap();
        fs::write(parent_stage.join("partial.pdf"), b"parent result").unwrap();
        let mut child = ScratchDir::new_in(&parent.path().join("zoriapdf-jobs-v1")).unwrap();
        let child_stage = child.output_stage(&outputs).unwrap();
        fs::write(child_stage.join("scan.png"), b"child plaintext").unwrap();
        assert!(parent.cleanup().is_err());
        assert!(parent.path().join(OWNERSHIP_FILE).is_file());
        assert!(child.path().join(OWNERSHIP_FILE).is_file());
        assert_eq!(fs::read(child_stage.join("scan.png")).unwrap(), b"child plaintext");
        assert_eq!(fs::read(parent_stage.join("partial.pdf")).unwrap(), b"parent result");
        // Death releases the nested lease; recovery must then discover and
        // remove its external tree before removing the parent's metadata.
        child.lease.take();
        parent.cleanup().unwrap();
        assert!(!child_stage.exists());
        assert!(!parent_stage.exists());
        assert!(!parent.path().exists());
    }

    #[test]
    fn recovery_does_not_delete_unrecognized_user_files() {
        let root = TestRoot::new();
        let jobs = root.0.join("jobs");
        let unrelated = jobs.join("other-program");
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(unrelated.join("keep.pdf"), b"user document").unwrap();
        ScratchDir::recover_at(&jobs).unwrap();
        assert_eq!(fs::read(unrelated.join("keep.pdf")).unwrap(), b"user document");
    }
}
