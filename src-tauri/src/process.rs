use std::{ffi::OsString, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
};

#[cfg(windows)]
mod windows_job;

pub struct ProbeSpec {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub max_stream_bytes: usize,
}

#[derive(Debug)]
pub struct ProbeOutput {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProbeError {
    Timeout,
    OutputLimit,
    SpawnFailed,
    CleanupFailed,
    InvalidExecutable,
    ProtocolFailed,
}

pub(crate) struct OwnedProcess {
    pub child: Child,
    #[cfg(windows)]
    _job: windows_job::Job,
}

impl OwnedProcess {
    pub async fn wait(&mut self) -> Result<std::process::ExitStatus, ProbeError> {
        let status = self
            .child
            .wait()
            .await
            .map_err(|_| ProbeError::SpawnFailed)?;
        #[cfg(windows)]
        self._job.terminate()?;
        Ok(status)
    }
    pub async fn terminate(&mut self) -> Result<(), ProbeError> {
        if self
            .child
            .try_wait()
            .map_err(|_| ProbeError::CleanupFailed)?
            .is_none()
        {
            tokio::time::timeout(Duration::from_secs(2), self.child.kill())
                .await
                .map_err(|_| ProbeError::CleanupFailed)?
                .map_err(|_| ProbeError::CleanupFailed)?;
        }
        Ok(())
    }
}

pub(crate) fn spawn_owned(spec: &ProbeSpec, piped_stdin: bool) -> Result<OwnedProcess, ProbeError> {
    // Only backend callers resolve executables; shell shims never enter a shell.
    if !spec.executable.is_absolute()
        || !spec.executable.is_file()
        || !spec.cwd.is_absolute()
        || !spec.cwd.is_dir()
        || spec.timeout.is_zero()
        || spec.max_stream_bytes == 0
    {
        return Err(ProbeError::InvalidExecutable);
    }
    let extension = spec.executable.extension().and_then(|s| s.to_str());
    #[cfg(windows)]
    if !extension.is_some_and(|value| value.eq_ignore_ascii_case("exe")) {
        return Err(ProbeError::InvalidExecutable);
    }
    if extension
        .is_some_and(|value| value.eq_ignore_ascii_case("cmd") || value.eq_ignore_ascii_case("bat"))
    {
        return Err(ProbeError::InvalidExecutable);
    }
    let mut command = Command::new(&spec.executable);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .kill_on_drop(true)
        .stdin(if piped_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000 | 0x0000_0004); // NO_WINDOW | SUSPENDED
    let child = command.spawn().map_err(|_| ProbeError::SpawnFailed)?;
    #[cfg(windows)]
    let job = windows_job::Job::attach_and_resume(&child)?;
    Ok(OwnedProcess {
        child,
        #[cfg(windows)]
        _job: job,
    })
}

pub(crate) async fn read_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, ProbeError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = reader
            .read(&mut buffer)
            .await
            .map_err(|_| ProbeError::SpawnFailed)?;
        if count == 0 {
            return Ok(output);
        }
        if count > limit.saturating_sub(output.len()) {
            return Err(ProbeError::OutputLimit);
        }
        output.extend_from_slice(&buffer[..count]);
    }
}

pub async fn run_probe(spec: ProbeSpec) -> Result<ProbeOutput, ProbeError> {
    let mut process = spawn_owned(&spec, false)?;
    let stdout = process.child.stdout.take().ok_or(ProbeError::SpawnFailed)?;
    let stderr = process.child.stderr.take().ok_or(ProbeError::SpawnFailed)?;
    let result = tokio::time::timeout(spec.timeout, async {
        let (stdout, stderr, status) = tokio::try_join!(
            read_bounded(stdout, spec.max_stream_bytes),
            read_bounded(stderr, spec.max_stream_bytes),
            process.wait()
        )?;
        Ok(ProbeOutput {
            exit_code: status.code(),
            stdout,
            stderr,
        })
    })
    .await
    .unwrap_or(Err(ProbeError::Timeout));
    // Job closure kills descendants even when the root has already exited.
    // Reap the root before returning on error; kill_on_drop also covers cancellation.
    if result.is_err() {
        process.terminate().await?;
    }
    drop(process);
    result
}
