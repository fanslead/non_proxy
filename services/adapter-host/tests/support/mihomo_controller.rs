use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const TEST_SECRET: &str = "nonproxy-test-secret";
const MAXIMUM_REQUEST_BYTES: usize = 64 * 1024;

pub(crate) struct MockMihomoController {
    address: SocketAddr,
    reload_fails: Arc<AtomicBool>,
    wrong_rules: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockMihomoController {
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .unwrap_or_else(|error| panic!("Mihomo 模拟控制器绑定失败: {error}"));
        listener
            .set_nonblocking(true)
            .unwrap_or_else(|error| panic!("Mihomo 模拟控制器非阻塞设置失败: {error}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("Mihomo 模拟控制器地址读取失败: {error}"));
        let wrong_rules = Arc::new(AtomicBool::new(false));
        let reload_fails = Arc::new(AtomicBool::new(false));
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_wrong_rules = wrong_rules.clone();
        let worker_reload_fails = reload_fails.clone();
        let worker_stopping = stopping.clone();
        let thread = thread::spawn(move || {
            while !worker_stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _peer)) => handle(
                        stream,
                        worker_wrong_rules.load(Ordering::Acquire),
                        worker_reload_fails.load(Ordering::Acquire),
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            reload_fails,
            wrong_rules,
            stopping,
            thread: Some(thread),
        }
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(crate) fn set_wrong_rules(&self, enabled: bool) {
        self.wrong_rules.store(enabled, Ordering::Release);
    }

    pub(crate) fn set_reload_failure(&self, enabled: bool) {
        self.reload_fails.store(enabled, Ordering::Release);
    }
}

impl Drop for MockMihomoController {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _wake = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _join = thread.join();
        }
    }
}

fn handle(mut stream: TcpStream, wrong_rules: bool, reload_fails: bool) {
    let _read_timeout = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _write_timeout = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    let authorized = request
        .lines()
        .any(|line| line.eq_ignore_ascii_case(&format!("Authorization: Bearer {TEST_SECRET}")));
    if !authorized {
        write_response(&mut stream, "401 Unauthorized", b"{}");
        return;
    }
    if request.starts_with("GET /version HTTP/1.1\r\n") {
        write_response(&mut stream, "200 OK", br#"{"version":"1.19.16"}"#);
    } else if request.starts_with("PUT /configs?force=true HTTP/1.1\r\n")
        && valid_reload_body(&request)
    {
        if reload_fails {
            write_response(&mut stream, "500 Internal Server Error", b"{}");
        } else {
            write_response(&mut stream, "204 No Content", &[]);
        }
    } else if request.starts_with("GET /rules HTTP/1.1\r\n") {
        let body = if wrong_rules {
            br#"{"rules":[{"payload":"other","proxy":"PROXY"}]}"#.as_slice()
        } else {
            br#"{"rules":[{"payload":"nonproxy-mihomo-primary","proxy":"DIRECT"}]}"#.as_slice()
        };
        write_response(&mut stream, "200 OK", body);
    } else {
        write_response(&mut stream, "404 Not Found", b"{}");
    }
}

fn valid_reload_body(request: &str) -> bool {
    let Some((_headers, body)) = request.split_once("\r\n\r\n") else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    value["payload"].as_str() == Some("")
        && value["path"].as_str().is_some_and(|path| {
            std::path::Path::new(path).is_absolute() && path.ends_with("config.yaml")
        })
}

fn read_request(stream: &mut TcpStream) -> Option<String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4 * 1024];
    loop {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAXIMUM_REQUEST_BYTES {
            return None;
        }
        let Some(header_end) = find_header_end(&bytes) else {
            continue;
        };
        let headers = std::str::from_utf8(&bytes[..header_end]).ok()?;
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if bytes.len() >= header_end.checked_add(content_length)? {
            break;
        }
    }
    String::from_utf8(bytes).ok()
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn write_response(stream: &mut TcpStream, status: &str, body: &[u8]) {
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _write = stream
        .write_all(headers.as_bytes())
        .and_then(|()| stream.write_all(body))
        .and_then(|()| stream.flush());
}
