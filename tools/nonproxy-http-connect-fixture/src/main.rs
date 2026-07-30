use std::{
    env,
    error::Error,
    fs,
    io::{Error as IoError, ErrorKind},
    path::PathBuf,
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};

const OPERATION_TIMEOUT: Duration = Duration::from_secs(15);
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(30);
const MAXIMUM_HEADER_BYTES: usize = 16 * 1024;
const EXPECTED_PAYLOAD: &[u8] = b"hello";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let port_file = port_file_argument()?;
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    fs::write(port_file, port.to_string())?;

    let accepted = timeout(ACCEPT_TIMEOUT, listener.accept())
        .await
        .map_err(|_| timeout_error("等待代理连接超时"))??;
    let mut stream = accepted.0;
    let request = read_header(&mut stream).await?;
    let request_text = String::from_utf8_lossy(&request);
    if !request_text.starts_with("CONNECT example.test:443 HTTP/1.1\r\n") {
        return Err(IoError::new(ErrorKind::InvalidData, "CONNECT 目标不符合预期").into());
    }

    stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;
    let mut payload = [0_u8; EXPECTED_PAYLOAD.len()];
    timeout(OPERATION_TIMEOUT, stream.read_exact(&mut payload))
        .await
        .map_err(|_| timeout_error("等待中继数据超时"))??;
    if payload != EXPECTED_PAYLOAD {
        return Err(IoError::new(ErrorKind::InvalidData, "中继数据不符合预期").into());
    }
    stream.write_all(&payload).await?;
    timeout(OPERATION_TIMEOUT, wait_for_close(&mut stream))
        .await
        .map_err(|_| timeout_error("等待中继关闭超时"))??;
    Ok(())
}

fn port_file_argument() -> Result<PathBuf, IoError> {
    env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| IoError::new(ErrorKind::InvalidInput, "缺少端口文件参数"))
}

async fn read_header(stream: &mut TcpStream) -> Result<Vec<u8>, IoError> {
    let mut header = Vec::new();
    let mut chunk = [0_u8; 1024];
    while header.len() < MAXIMUM_HEADER_BYTES {
        let read = timeout(OPERATION_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| timeout_error("读取 CONNECT 请求超时"))??;
        if read == 0 {
            return Err(IoError::new(
                ErrorKind::UnexpectedEof,
                "CONNECT 请求提前结束",
            ));
        }
        header.extend_from_slice(&chunk[..read]);
        if header.windows(4).any(|value| value == b"\r\n\r\n") {
            return Ok(header);
        }
    }
    Err(IoError::new(
        ErrorKind::InvalidData,
        "CONNECT 请求头超过上限",
    ))
}

async fn wait_for_close(stream: &mut TcpStream) -> Result<(), IoError> {
    let mut byte = [0_u8; 1];
    if stream.read(&mut byte).await? == 0 {
        Ok(())
    } else {
        Err(IoError::new(
            ErrorKind::InvalidData,
            "关闭前收到额外中继数据",
        ))
    }
}

fn timeout_error(message: &'static str) -> IoError {
    IoError::new(ErrorKind::TimedOut, message)
}
