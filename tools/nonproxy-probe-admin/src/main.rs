mod args;
mod key_file;
mod verify;

use std::{env, process::ExitCode};

use args::Command;
use thiserror::Error;

#[derive(Debug, Error)]
enum AdminError {
    #[error("命令参数无效")]
    Usage,
    #[error("随机数生成失败")]
    Random,
    #[error("密钥文件不可用")]
    File,
    #[error("签名密钥无效")]
    SigningKey,
    #[error("出口探针验证失败")]
    Verification,
    #[error("系统时间无效")]
    Clock,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(AdminError::Usage) => {
            eprintln!("{}", args::usage());
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("NonProxy 出口探针管理失败：{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), AdminError> {
    match args::parse(env::args_os().skip(1))? {
        Command::Keygen { output } => {
            let metadata = key_file::generate(&output)?;
            println!("key_id={}", metadata.key_id);
            println!("public_key={}", metadata.public_key);
            println!("secret_file={}", output.display());
        }
        Command::Inspect { input } => {
            let metadata = key_file::inspect(&input)?;
            println!("key_id={}", metadata.key_id);
            println!("public_key={}", metadata.public_key);
        }
        Command::Verify {
            endpoint,
            public_keys,
        } => {
            let verified = verify::run(&endpoint, &public_keys).await?;
            println!("probe_id={}", verified.probe_id());
            println!("key_id={}", verified.key_id());
            println!("observed_ip={}", verified.observed_ip());
            println!("observed_at_unix_ms={}", verified.observed_at_unix_ms());
        }
    }
    Ok(())
}
