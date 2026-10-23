//! Child-process cleanup that survives an abnormal exit.
//!
//! Stream Deck plugins are separate executables. We kill them on a normal exit,
//! but a Task Manager kill or a crash leaves them behind — some plugins (the
//! ATEM one, for example) never notice their socket died. Windows job objects
//! solve this: every child is assigned to a job with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so the OS terminates the children when
//! the app's process dies, however it dies.

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    struct Job(HANDLE);

    // The handle is only used for the lifetime of the process.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    static JOB: OnceLock<Option<Job>> = OnceLock::new();

    fn job() -> Option<&'static Job> {
        JOB.get_or_init(|| unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                tracing::warn!("could not create the plugin job object");
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                tracing::warn!("could not configure the plugin job object");
                let _ = CloseHandle(handle);
                return None;
            }
            Some(Job(handle))
        })
        .as_ref()
    }

    /// Put a child process into the kill-on-close job.
    pub fn assign(pid: u32) {
        let Some(job) = job() else { return };
        unsafe {
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if process.is_null() {
                tracing::debug!(pid, "could not open the child process for job assignment");
                return;
            }
            let ok = AssignProcessToJobObject(job.0, process);
            if ok == 0 {
                tracing::debug!(pid, "could not assign the child process to the job");
            }
            let _ = CloseHandle(process);
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    /// No-op on platforms without job objects; the normal exit path still stops
    /// the children.
    pub fn assign(_pid: u32) {}
}

pub use imp::assign;
