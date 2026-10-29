//! A private worker job owns its descendants and bounds their aggregate memory.
pub struct Containment {
    #[cfg(windows)]
    job: windows::Job,
}

impl Containment {
    pub fn terminate(&self) -> Result<(), String> {
        #[cfg(windows)] { self.job.terminate() }
        #[cfg(not(windows))] { Ok(()) }
    }
}

#[cfg(windows)]
mod windows {
    use std::{os::windows::io::AsRawHandle, process::Child, time::{Duration, Instant}};
    use windows_sys::Win32::{Foundation::{CloseHandle, HANDLE}, System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, QueryInformationJobObject,
        SetInformationJobObject, TerminateJobObject, JobObjectBasicAccountingInformation,
        JobObjectExtendedLimitInformation, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_JOB_MEMORY,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    }};

    pub(super) struct Job(HANDLE);
    // The owned kernel handle never references thread-local or Rust memory.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}
    impl Drop for Job {
        fn drop(&mut self) { unsafe { CloseHandle(self.0); } }
    }

    impl Job {
        pub(super) fn create(child: &mut Child) -> Result<Self, String> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() { return Err(std::io::Error::last_os_error().to_string()); }
                let job = Job(handle);
                let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                limits.BasicLimitInformation.LimitFlags =
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_JOB_MEMORY;
                limits.JobMemoryLimit = 1024 * 1024 * 1024;
                if SetInformationJobObject(job.0, JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&limits) as u32) == 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                if AssignProcessToJobObject(job.0, child.as_raw_handle()) == 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                Ok(job)
            }
        }

        pub(super) fn terminate(&self) -> Result<(), String> {
            if unsafe { TerminateJobObject(self.0, 1) } == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            // Termination is asynchronous. Do not delete plaintext scratch files
            // until every owned OCR descendant has released its handles.
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                if unsafe { QueryInformationJobObject(self.0, JobObjectBasicAccountingInformation,
                    (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32, std::ptr::null_mut()) } == 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                if info.ActiveProcesses == 0 { return Ok(()); }
                if Instant::now() >= deadline {
                    return Err("PDF işlem süreçleri kapatılamadı; geçici dosyalar henüz temizlenemedi.".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

pub fn contain(child: &mut std::process::Child) -> Result<Containment, String> {
    #[cfg(windows)] {
        match windows::Job::create(child) {
            Ok(job) => Ok(Containment { job }),
            Err(message) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(format!("PDF işlem süreci güvenli biçimde başlatılamadı: {message}"))
            }
        }
    }
    #[cfg(not(windows))] { let _ = child; Ok(Containment {}) }
}
