use super::ProbeError;
use std::{
    mem::{size_of, zeroed},
    ptr::null,
};
use tokio::process::Child;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
    },
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(super) struct Job(Handle);
// A job handle is an owned kernel handle; it has no thread affinity.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    pub fn terminate(&self) -> Result<(), ProbeError> {
        if unsafe { TerminateJobObject(self.0 .0, 1) } == 0 {
            return Err(ProbeError::CleanupFailed);
        }
        Ok(())
    }
    pub fn attach_and_resume(child: &Child) -> Result<Self, ProbeError> {
        // Spawn suspended so no child code can run before membership is enforced.
        unsafe {
            let handle = CreateJobObjectW(null(), null());
            if handle.is_null() {
                return Err(ProbeError::CleanupFailed);
            }
            let job = Self(Handle(handle));
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.0 .0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return Err(ProbeError::CleanupFailed);
            }
            let process = child.raw_handle().ok_or(ProbeError::CleanupFailed)?;
            if AssignProcessToJobObject(job.0 .0, process as HANDLE) == 0 {
                return Err(ProbeError::CleanupFailed);
            }
            let snapshot_handle = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot_handle == INVALID_HANDLE_VALUE {
                return Err(ProbeError::CleanupFailed);
            }
            let snapshot = Handle(snapshot_handle);
            let mut entry: THREADENTRY32 = zeroed();
            entry.dwSize = size_of::<THREADENTRY32>() as u32;
            let pid = child.id().ok_or(ProbeError::CleanupFailed)?;
            let mut found = Thread32First(snapshot.0, &mut entry);
            while found != 0 {
                if entry.th32OwnerProcessID == pid {
                    let thread_handle = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if thread_handle.is_null() {
                        return Err(ProbeError::CleanupFailed);
                    }
                    let thread = Handle(thread_handle);
                    if ResumeThread(thread.0) == u32::MAX {
                        return Err(ProbeError::CleanupFailed);
                    }
                    return Ok(job);
                }
                found = Thread32Next(snapshot.0, &mut entry);
            }
            Err(ProbeError::CleanupFailed)
        }
    }
}
