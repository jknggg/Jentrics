use jentrics_core::process::{run_probe, ProbeError, ProbeSpec};
use std::{
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};

fn spec(args: &[&str], cwd: &std::path::Path) -> ProbeSpec {
    ProbeSpec {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_probe-helper")),
        args: args.iter().map(OsString::from).collect(),
        cwd: cwd.into(),
        timeout: Duration::from_millis(600),
        max_stream_bytes: 1024,
    }
}

#[tokio::test]
async fn hung_process_returns_timeout_within_deadline() {
    let cwd = tempfile::tempdir().unwrap();
    let started = Instant::now();
    assert_eq!(
        run_probe(spec(&["sleep"], cwd.path())).await.unwrap_err(),
        ProbeError::Timeout
    );
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[tokio::test]
async fn both_output_streams_have_limits() {
    let cwd = tempfile::tempdir().unwrap();
    for stream in ["stdout", "stderr"] {
        assert_eq!(
            run_probe(spec(&[stream], cwd.path())).await.unwrap_err(),
            ProbeError::OutputLimit
        );
    }
}

#[tokio::test]
async fn shell_shims_are_rejected_before_spawn() {
    let cwd = tempfile::tempdir().unwrap();
    for file in ["codex.cmd", "claude.bat"] {
        let mut input = spec(&[], cwd.path());
        input.executable = cwd.path().join(file);
        std::fs::write(&input.executable, "echo unexpected").unwrap();
        assert_eq!(
            run_probe(input).await.unwrap_err(),
            ProbeError::InvalidExecutable
        );
    }
}

#[tokio::test]
async fn shell_metacharacters_remain_literal_arguments() {
    let cwd = tempfile::tempdir().unwrap();
    let result = run_probe(spec(&["echo", "hello & echo injected"], cwd.path()))
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().trim(),
        "hello & echo injected"
    );
    assert_eq!(result.exit_code, Some(0));
}

#[tokio::test]
async fn inherited_pipes_cannot_outlive_probe_deadline() {
    let cwd = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let pid_file = cwd.path().join("pid");
    assert_eq!(
        run_probe(spec(
            &["descendant", pid_file.to_str().unwrap()],
            cwd.path()
        ))
        .await
        .unwrap_err(),
        ProbeError::Timeout
    );
    assert!(started.elapsed() < Duration::from_secs(4));
    #[cfg(windows)]
    assert_process_exited(std::fs::read_to_string(pid_file).unwrap().parse().unwrap());
}

#[cfg(windows)]
fn assert_process_exited(pid: u32) {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject},
    };
    unsafe {
        let handle = OpenProcess(0x0010_0000, 0, pid); // SYNCHRONIZE
        if handle.is_null() {
            assert_eq!(GetLastError(), ERROR_INVALID_PARAMETER);
            return;
        }
        let result = WaitForSingleObject(handle, 2000);
        CloseHandle(handle);
        assert_eq!(result, WAIT_OBJECT_0, "descendant is still alive");
    }
}

#[tokio::test]
async fn successful_parent_exit_also_cleans_up_descendants() {
    let cwd = tempfile::tempdir().unwrap();
    let output = run_probe(spec(&["detached-descendant"], cwd.path()))
        .await
        .unwrap();
    assert_eq!(output.exit_code, Some(0));
    #[cfg(windows)]
    assert_process_exited(
        String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap(),
    );
}
