use std::{ffi::OsString, io, path::PathBuf, process::Stdio, time::Duration};

use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    task::JoinError,
    time::{Instant, timeout_at},
};

const MAXIMUM_OUTPUT_BYTES: u64 = 64 * 1024;
const MAXIMUM_ALLOWED_OUTPUT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Error)]
pub(crate) enum ProcessExecutionError {
    #[error("子进程文件操作失败")]
    Io(#[source] io::Error),
    #[error("子进程输出任务失败")]
    Task(#[source] JoinError),
    #[error("子进程执行超时")]
    Timeout,
    #[error("子进程返回失败状态")]
    Failed,
    #[error("子进程输出超过上限")]
    OutputTooLarge,
}

pub(crate) struct ProcessRequest {
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub working_directory: Option<PathBuf>,
    pub home_directory: Option<PathBuf>,
    pub timeout: Duration,
}

pub(crate) async fn run(request: ProcessRequest) -> Result<Vec<u8>, ProcessExecutionError> {
    run_bounded(request, MAXIMUM_OUTPUT_BYTES).await
}

pub(crate) async fn run_bounded(
    request: ProcessRequest,
    maximum_output_bytes: u64,
) -> Result<Vec<u8>, ProcessExecutionError> {
    if maximum_output_bytes == 0 || maximum_output_bytes > MAXIMUM_ALLOWED_OUTPUT_BYTES {
        return Err(ProcessExecutionError::OutputTooLarge);
    }
    let deadline = Instant::now() + request.timeout;
    let mut command = Command::new(&request.executable);
    command
        .args(&request.arguments)
        .env_clear()
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(directory) = &request.working_directory {
        command.current_dir(directory);
    }
    if let Some(directory) = &request.home_directory {
        command.env("HOME", directory);
    }
    let mut child = command.spawn().map_err(ProcessExecutionError::Io)?;
    let stdout = child.stdout.take().ok_or(ProcessExecutionError::Failed)?;
    let stderr = child.stderr.take().ok_or(ProcessExecutionError::Failed)?;
    let mut stdout_task = tokio::spawn(read_bounded(stdout, maximum_output_bytes));
    let mut stderr_task = tokio::spawn(read_bounded(stderr, maximum_output_bytes));
    let status = match timeout_at(deadline, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(source)) => {
            let _kill_result = child.kill().await;
            let _wait_result = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            let _stdout_result = stdout_task.await;
            let _stderr_result = stderr_task.await;
            return Err(ProcessExecutionError::Io(source));
        }
        Err(_) => {
            let _kill_result = child.kill().await;
            let _wait_result = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            let _stdout_result = stdout_task.await;
            let _stderr_result = stderr_task.await;
            return Err(ProcessExecutionError::Timeout);
        }
    };
    let captures = timeout_at(deadline, async {
        tokio::join!(&mut stdout_task, &mut stderr_task)
    })
    .await;
    let (stdout_result, stderr_result) = match captures {
        Ok(results) => results,
        Err(_) => {
            stdout_task.abort();
            stderr_task.abort();
            let _stdout_result = stdout_task.await;
            let _stderr_result = stderr_task.await;
            return Err(ProcessExecutionError::Timeout);
        }
    };
    let (stdout, stdout_overflow) = stdout_result.map_err(ProcessExecutionError::Task)??;
    let (stderr, stderr_overflow) = stderr_result.map_err(ProcessExecutionError::Task)??;
    if stdout_overflow || stderr_overflow {
        return Err(ProcessExecutionError::OutputTooLarge);
    }
    if !status.success() {
        return Err(ProcessExecutionError::Failed);
    }
    let combined_length = stdout
        .len()
        .checked_add(stderr.len())
        .ok_or(ProcessExecutionError::OutputTooLarge)?;
    if u64::try_from(combined_length).map_or(true, |length| length > maximum_output_bytes) {
        return Err(ProcessExecutionError::OutputTooLarge);
    }
    let mut output = stdout;
    if !output.is_empty() && !stderr.is_empty() {
        output.push(b'\n');
    }
    output.extend_from_slice(&stderr);
    Ok(output)
}

async fn read_bounded(
    reader: impl AsyncRead + Unpin,
    maximum_output_bytes: u64,
) -> Result<(Vec<u8>, bool), ProcessExecutionError> {
    let mut output = Vec::new();
    reader
        .take(maximum_output_bytes + 1)
        .read_to_end(&mut output)
        .await
        .map_err(ProcessExecutionError::Io)?;
    let overflow = u64::try_from(output.len()).map_or(true, |length| length > maximum_output_bytes);
    Ok((output, overflow))
}

#[cfg(all(test, unix))]
mod tests {
    use std::{fs, time::Duration};

    use super::{ProcessRequest, run};

    #[tokio::test]
    async fn runner_has_timeout_and_output_bound() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let slow = directory.path().join("slow");
        fs::write(&slow, b"#!/bin/sh\nsleep 2\n")
            .unwrap_or_else(|error| panic!("慢命令写入失败: {error}"));
        fs::set_permissions(&slow, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("慢命令权限设置失败: {error}"));

        assert!(
            run(ProcessRequest {
                executable: slow,
                arguments: Vec::new(),
                working_directory: None,
                home_directory: None,
                timeout: Duration::from_millis(20),
            })
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn runner_does_not_wait_for_descendant_holding_output_pipe() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let executable = directory.path().join("leaky-descendant");
        fs::write(&executable, b"#!/bin/sh\nsleep 2 &\nexit 0\n")
            .unwrap_or_else(|error| panic!("后代进程命令写入失败: {error}"));
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("后代进程命令权限设置失败: {error}"));

        let result = run(ProcessRequest {
            executable,
            arguments: Vec::new(),
            working_directory: None,
            home_directory: None,
            timeout: Duration::from_millis(50),
        })
        .await;

        assert!(result.is_err());
    }
}
