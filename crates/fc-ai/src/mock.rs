//! A local mock HTTP server for provider tests (no network): answers each request with a
//! handler's response and records what it received (method, path, headers, body), so tests
//! can check both the wire format and where secrets went.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// A recorded request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockRequest {
    /// `GET`, `POST`, …
    pub method: String,
    /// Path with query.
    pub path: String,
    /// Headers, names lowercased, in order.
    pub headers: Vec<(String, String)>,
    /// Body as text.
    pub body: String,
}

impl MockRequest {
    /// First header value named `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The body parsed as JSON (`Null` if it is not JSON).
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

/// A response to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockResponse {
    /// HTTP status.
    pub status: u16,
    /// Body (sent as `application/json`).
    pub body: String,
}

impl MockResponse {
    /// `200` with a JSON body.
    pub fn json(v: &serde_json::Value) -> Self {
        Self {
            status: 200,
            body: v.to_string(),
        }
    }

    /// Any status with a JSON body.
    pub fn status(status: u16, v: &serde_json::Value) -> Self {
        Self {
            status,
            body: v.to_string(),
        }
    }
}

type Handler = dyn Fn(&MockRequest) -> MockResponse + Send + Sync;

/// The server; stops when dropped.
pub struct MockServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<MockRequest>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockServer {
    /// Starts on a free localhost port; `handler` answers every request.
    pub fn start(handler: impl Fn(&MockRequest) -> MockResponse + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("mock server address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let handler: Arc<Handler> = Arc::new(handler);
        let (reqs, stopping) = (requests.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let _ = serve(stream, &*handler, &reqs);
            }
        });
        Self {
            addr,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    /// Base URL (`http://127.0.0.1:<port>`).
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Requests received so far.
    pub fn requests(&self) -> Vec<MockRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn serve(
    stream: TcpStream,
    handler: &Handler,
    requests: &Mutex<Vec<MockRequest>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    if method.is_empty() {
        return Ok(());
    }
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 {
            break;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
        }
    }
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; len];
    reader.read_exact(&mut body)?;
    let request = MockRequest {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    let response = handler(&request);
    requests.lock().unwrap().push(request);
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {} Mock\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        response.status,
        response.body.len(),
        response.body
    )?;
    stream.flush()
}
