//! Loopback HTTP fixture server. Adapter tests send real reqwest requests here.
//! The handler is scripted; the bytes on the wire are not.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::error::SyncError;

#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    #[allow(dead_code)]
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RecordedRequest {
    #[must_use]
    pub fn body_str(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value.as_str()))
    }
}

pub enum ScriptedReply {
    Response {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    /// Accept the request, then wait. Used for timeout-after-commit: the
    /// handler records the mutation before returning this variant's inner work
    /// via [`ScriptedReply::after`].
    Delay {
        delay: Duration,
        response: Box<ScriptedReply>,
    },
    #[allow(dead_code)]
    Close,
}

pub struct ScriptedServer {
    pub addr: SocketAddr,
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ScriptedServer {
    pub fn spawn(
        handler: impl Fn(&RecordedRequest) -> ScriptedReply + Send + Sync + 'static,
    ) -> Result<Self, SyncError> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(SyncError::io)?;
        listener.set_nonblocking(true).map_err(SyncError::io)?;
        let addr = listener.local_addr().map_err(SyncError::io)?;
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let recorded_thread = Arc::clone(&recorded);
        let shutdown_thread = Arc::clone(&shutdown);
        let handler: Arc<dyn Fn(&RecordedRequest) -> ScriptedReply + Send + Sync> =
            Arc::new(handler);
        let thread = thread::spawn(move || {
            while !shutdown_thread.load(std::sync::atomic::Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = serve(stream, &handler, &recorded_thread);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            addr,
            recorded,
            shutdown,
            thread: Some(thread),
        })
    }

    #[must_use]
    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    #[must_use]
    pub fn recorded(&self) -> Vec<RecordedRequest> {
        self.recorded.lock().expect("recorded").clone()
    }
}

impl Drop for ScriptedServer {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    mut stream: TcpStream,
    handler: &Arc<dyn Fn(&RecordedRequest) -> ScriptedReply + Send + Sync>,
    recorded: &Mutex<Vec<RecordedRequest>>,
) -> Result<(), ()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut temp = [0u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut temp).map_err(|_| ())?;
        if read == 0 {
            return Err(());
        }
        buf.extend_from_slice(&temp[..read]);
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
        if buf.len() > 1024 * 1024 {
            return Err(());
        }
    };
    let header_text = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or(())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let path = parts.next().unwrap_or("").to_owned();
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }
    let length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < length {
        let read = stream.read(&mut temp).map_err(|_| ())?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&temp[..read]);
    }
    body.truncate(length);
    let request = RecordedRequest {
        method,
        path,
        headers,
        body,
    };
    recorded.lock().map_err(|_| ())?.push(request.clone());
    let reply = handler(&request);
    write_reply(&mut stream, &reply)
}

fn write_reply(stream: &mut TcpStream, reply: &ScriptedReply) -> Result<(), ()> {
    match reply {
        ScriptedReply::Close => Ok(()),
        ScriptedReply::Delay { delay, response } => {
            std::thread::sleep(*delay);
            write_reply(stream, response)
        }
        ScriptedReply::Response {
            status,
            headers,
            body,
        } => {
            let reason = match status {
                200 => "OK",
                201 => "Created",
                308 => "Resume Incomplete",
                400 => "Bad Request",
                401 => "Unauthorized",
                403 => "Forbidden",
                404 => "Not Found",
                409 => "Conflict",
                422 => "Unprocessable Entity",
                429 => "Too Many Requests",
                500 => "Server Error",
                _ => "Result",
            };
            let mut head = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            let mut has_type = false;
            for (name, value) in headers {
                if name.eq_ignore_ascii_case("content-type") {
                    has_type = true;
                }
                head.push_str(name);
                head.push_str(": ");
                head.push_str(value);
                head.push_str("\r\n");
            }
            if !has_type {
                head.push_str("Content-Type: application/json\r\n");
            }
            head.push_str("\r\n");
            stream.write_all(head.as_bytes()).map_err(|_| ())?;
            stream.write_all(body).map_err(|_| ())?;
            let _ = stream.flush();
            Ok(())
        }
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

pub fn json_reply(status: u16, value: serde_json::Value) -> ScriptedReply {
    ScriptedReply::Response {
        status,
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        body: value.to_string().into_bytes(),
    }
}
