//! Windows Job Object: kill child processes when the desktop process dies.
//!
//! On non-Windows, [`KillOnCloseJob::new`] returns `None`.

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    pub struct Inner {
        handle: HANDLE,
    }

    unsafe impl Send for Inner {}
    unsafe impl Sync for Inner {}

    fn handle_is_invalid(h: HANDLE) -> bool {
        h == INVALID_HANDLE_VALUE || (h as usize) == 0
    }

    impl Inner {
        pub fn create() -> Option<Self> {
            unsafe {
                let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle_is_invalid(h) {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    h,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    let _ = CloseHandle(h);
                    return None;
                }
                Some(Self { handle: h })
            }
        }

        pub fn assign_pid(&self, pid: u32) -> Result<(), String> {
            unsafe {
                let proc = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
                if handle_is_invalid(proc) {
                    return Err(format!("OpenProcess 失败：pid={pid}"));
                }
                let ok = AssignProcessToJobObject(self.handle, proc);
                let _ = CloseHandle(proc);
                if ok == 0 {
                    Err("AssignProcessToJobObject 失败".into())
                } else {
                    Ok(())
                }
            }
        }

        pub fn assign_handle(&self, process: std::os::windows::io::RawHandle) -> Result<(), String> {
            unsafe {
                let ok = AssignProcessToJobObject(self.handle, process as HANDLE);
                if ok == 0 {
                    Err("AssignProcessToJobObject 失败".into())
                } else {
                    Ok(())
                }
            }
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }
}

/// Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
///
/// Closing the handle (on drop) asks Windows to terminate remaining processes
/// in the job — so a killed desktop process takes its `grok agent` children with it.
pub struct KillOnCloseJob {
    #[cfg(windows)]
    inner: win::Inner,
}

impl KillOnCloseJob {
    /// Create a kill-on-close job. Returns `None` on non-Windows or if creation fails.
    pub fn new() -> Option<Self> {
        #[cfg(windows)]
        {
            win::Inner::create().map(|inner| Self { inner })
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    pub fn assign_pid(&self, pid: u32) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.inner.assign_pid(pid)
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            Err("Job Object 仅支持 Windows".into())
        }
    }

    #[cfg(windows)]
    pub fn assign_handle(&self, process: std::os::windows::io::RawHandle) -> Result<(), String> {
        self.inner.assign_handle(process)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_is_none_off_windows() {
        #[cfg(not(windows))]
        {
            assert!(super::KillOnCloseJob::new().is_none());
        }
    }
}
