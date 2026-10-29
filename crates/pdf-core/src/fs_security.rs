//! Keep a selected output directory's access controls while staging elsewhere.
use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::path::Path;

#[cfg(unix)]
pub(crate) struct AccessControls(fs::Permissions);

#[cfg(unix)]
impl AccessControls {
    pub(crate) fn capture(path: &Path) -> Result<Self> {
        Ok(Self(fs::metadata(path)?.permissions()))
    }

    pub(crate) fn apply(&self, path: &Path) -> Result<()> {
        fs::set_permissions(path, self.0.clone()).context("Cannot preserve output permissions")
    }
}

#[cfg(unix)]
pub(crate) fn make_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .context("Cannot restrict temporary directory access")
}

#[cfg(windows)]
mod windows {
    use super::*;
    use anyhow::bail;
    use std::os::windows::{ffi::OsStrExt, fs::{MetadataExt, OpenOptionsExt}, io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW,
                GetSecurityInfo, SetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT},
            GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
            PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
        },
        Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_READ_ATTRIBUTES, GetVolumeInformationW, GetVolumePathNameW, READ_CONTROL, WRITE_DAC},
    };

    struct Descriptor(PSECURITY_DESCRIPTOR);
    impl Drop for Descriptor {
        fn drop(&mut self) { unsafe { LocalFree(self.0); } }
    }
    // LocalAlloc memory is immutable here, owns no thread-affine resources, and
    // is freed only after all shared references to its access controls are gone.
    unsafe impl Send for Descriptor {}
    unsafe impl Sync for Descriptor {}

    pub(crate) struct AccessControls {
        descriptor: Option<Descriptor>,
        protected: bool,
    }

    fn persistent_acls(path: &Path) -> Result<bool> {
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut root = vec![0u16; path.len().max(260)];
        if unsafe { GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
            return Err(std::io::Error::last_os_error()).context("Cannot inspect output filesystem");
        }
        let mut flags = 0;
        if unsafe { GetVolumeInformationW(root.as_ptr(), std::ptr::null_mut(), 0,
            std::ptr::null_mut(), std::ptr::null_mut(), &mut flags, std::ptr::null_mut(), 0) } == 0 {
            return Err(std::io::Error::last_os_error()).context("Cannot inspect output filesystem permissions");
        }
        Ok(flags & 0x0000_0008 != 0) // FILE_PERSISTENT_ACLS; FAT/exFAT have no ACL to copy.
    }

    fn open_security(path: &Path, access: u32) -> Result<fs::File> {
        let file = OpenOptions::new().access_mode(access | FILE_READ_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path).context("Cannot open output access controls")?;
        if file.metadata()?.file_attributes() & 0x400 != 0 {
            bail!("Output access controls must not be read from a link");
        }
        Ok(file)
    }

    impl AccessControls {
        fn from_descriptor(descriptor: Descriptor) -> Result<Self> {
            let mut control = 0;
            let mut revision = 0;
            if unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) } == 0 {
                return Err(std::io::Error::last_os_error()).context("Cannot inspect output access controls");
            }
            Ok(Self { descriptor: Some(descriptor), protected: control & SE_DACL_PROTECTED != 0 })
        }

        pub(crate) fn capture(path: &Path) -> Result<Self> {
            Self::capture_file(&open_security(path, READ_CONTROL)?, path)
        }

        pub(super) fn capture_file(file: &fs::File, path: &Path) -> Result<Self> {
            if !persistent_acls(path)? { return Ok(Self { descriptor: None, protected: false }); }
            let mut descriptor = std::ptr::null_mut();
            let status = unsafe { GetSecurityInfo(file.as_raw_handle(), SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION, std::ptr::null_mut(), std::ptr::null_mut(),
                std::ptr::null_mut(), std::ptr::null_mut(), &mut descriptor) };
            if status != 0 {
                return Err(std::io::Error::from_raw_os_error(status as i32))
                    .context("Cannot read output access controls");
            }
            Self::from_descriptor(Descriptor(descriptor))
        }

        pub(crate) fn apply(&self, path: &Path) -> Result<()> {
            self.apply_at(path, true)
        }

        pub(crate) fn publication_handle(&self, path: &Path) -> Result<Option<fs::File>> {
            if self.protected || self.descriptor.is_none() { return Ok(None); }
            // Acquire before the captured ACL can remove the owner's WRITE_DAC.
            // DELETE also permits handle-anchored publication/rollback.
            Ok(Some(open_security(path, READ_CONTROL | WRITE_DAC |
                windows_sys::Win32::Storage::FileSystem::DELETE)?))
        }

        pub(crate) fn published(&self, file: &fs::File) -> Result<()> {
            self.apply_to_file(file, false)
        }

        fn apply_at(&self, path: &Path, staging: bool) -> Result<()> {
            if self.descriptor.is_none() { return Ok(()); }
            if !persistent_acls(path)? { return Ok(()); }
            let file = open_security(path, READ_CONTROL | WRITE_DAC)?;
            self.apply_to_file(&file, staging)
        }

        fn apply_to_file(&self, file: &fs::File, staging: bool) -> Result<()> {
            let Some(descriptor) = self.descriptor.as_ref() else { return Ok(()); };
            let mut present = 0;
            let mut defaulted = 0;
            let mut dacl = std::ptr::null_mut();
            if unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted) } == 0 {
                return Err(std::io::Error::last_os_error()).context("Cannot inspect output permissions");
            }
            if present == 0 { bail!("Output access controls have no discretionary ACL"); }
            // Freeze the captured ACL while below the private staging parent.
            // Resume inheritance only after moving to the actual output parent.
            let flags = DACL_SECURITY_INFORMATION | if staging || self.protected {
                PROTECTED_DACL_SECURITY_INFORMATION
            } else {
                windows_sys::Win32::Security::UNPROTECTED_DACL_SECURITY_INFORMATION
            };
            let status = unsafe { SetSecurityInfo(file.as_raw_handle(), SE_FILE_OBJECT, flags,
                std::ptr::null_mut(), std::ptr::null_mut(), dacl, std::ptr::null()) };
            if status != 0 {
                return Err(std::io::Error::from_raw_os_error(status as i32))
                    .context("Cannot preserve output access controls");
            }
            Ok(())
        }
    }

    pub(crate) fn make_private(path: &Path) -> Result<()> {
        let mut descriptor = std::ptr::null_mut();
        if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(
            windows_sys::core::w!("D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"), SDDL_REVISION_1,
            &mut descriptor, std::ptr::null_mut()) } == 0 {
            return Err(std::io::Error::last_os_error()).context("Cannot create private temporary access controls");
        }
        AccessControls::from_descriptor(Descriptor(descriptor))?.apply(path)
    }
}

#[cfg(windows)]
pub(crate) use windows::{make_private, AccessControls};

pub(crate) fn new_file_access(parent: &Path) -> Result<AccessControls> {
    let path = parent.join(format!(".zoriapdf-access-{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::{Foundation::{GENERIC_READ, GENERIC_WRITE},
            Storage::FileSystem::{DELETE, FILE_FLAG_DELETE_ON_CLOSE}};
        options.access_mode(GENERIC_READ | GENERIC_WRITE | DELETE).custom_flags(FILE_FLAG_DELETE_ON_CLOSE);
    }
    let file = options.open(&path).context("Cannot determine new output file access controls")?;
    // The probe contains no document bytes. Windows removes it even if the
    // process dies; it also gives the real parent's file (not directory) ACL.
    #[cfg(windows)] { AccessControls::capture_file(&file, &path) }
    #[cfg(unix)] {
        let access = file.metadata().map(|metadata| AccessControls(metadata.permissions()));
        fs::remove_file(&path).context("Cannot remove empty output access probe")?;
        Ok(access?)
    }
}
