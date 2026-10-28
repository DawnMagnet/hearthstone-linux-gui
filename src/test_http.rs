//! Minimal HTTP/1.1 server for download tests. No extra crates: one
//! `TcpListener`, one thread per connection, and a request log.

use std::{
    io::{BufRead, BufReader, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Behavior {
    pub accept_ranges: bool,
    pub head_status: u16,
    pub get_status: u16,
    pub ignore_range: bool,
    pub reject_ranges: bool,
    pub fail_range_start_once: Option<u64>,
    pub fail_range_delay: Duration,
    pub cut_first_get_after: Option<u64>,
    pub pace: Option<Duration>,
    pub stall_after_headers: bool,
    pub content_range_total: Option<u64>,
    pub range_body_limit: Option<usize>,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            accept_ranges: true,
            head_status: 200,
            get_status: 200,
            ignore_range: false,
            reject_ranges: false,
            fail_range_start_once: None,
            fail_range_delay: Duration::ZERO,
            cut_first_get_after: None,
            pace: None,
            stall_after_headers: false,
            content_range_total: None,
            range_body_limit: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedRequest {
    pub method: String,
    pub path: String,
    pub range: Option<String>,
}

struct Shared {
    body: Vec<u8>,
    log: Mutex<Vec<LoggedRequest>>,
    inflight: AtomicUsize,
    max_inflight: AtomicUsize,
    behavior: Mutex<Behavior>,
    shutdown: AtomicBool,
    cut_used: AtomicBool,
}

pub struct FakeServer {
    addr: SocketAddr,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl FakeServer {
    pub fn start(body: Vec<u8>, behavior: Behavior) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake http");
        let addr = listener.local_addr().expect("local addr");
        let shared = Arc::new(Shared {
            body,
            log: Mutex::new(Vec::new()),
            inflight: AtomicUsize::new(0),
            max_inflight: AtomicUsize::new(0),
            behavior: Mutex::new(behavior),
            shutdown: AtomicBool::new(false),
            cut_used: AtomicBool::new(false),
        });
        let accept_shared = Arc::clone(&shared);
        let thread = thread::spawn(move || accept_loop(listener, accept_shared));
        Self {
            addr,
            shared,
            thread: Some(thread),
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> url::Url {
        url::Url::parse(&format!("http://{}/blob", self.addr)).expect("server url")
    }

    pub fn log(&self) -> Vec<LoggedRequest> {
        self.shared.log.lock().expect("log").clone()
    }

    pub fn clear_log(&self) {
        self.shared.log.lock().expect("log").clear();
    }

    pub fn max_inflight(&self) -> usize {
        self.shared.max_inflight.load(Ordering::SeqCst)
    }

    pub fn set_behavior(&self, behavior: Behavior) {
        *self.shared.behavior.lock().expect("behavior") = behavior;
    }

    pub fn set_status(&self, status: u16) {
        let mut behavior = self.shared.behavior.lock().expect("behavior");
        behavior.head_status = status;
        behavior.get_status = status;
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    loop {
        if shared.shutdown.load(Ordering::SeqCst) {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) if shared.shutdown.load(Ordering::SeqCst) => {
                drop(stream);
                break;
            }
            Ok((stream, _)) => {
                let shared = Arc::clone(&shared);
                thread::spawn(move || handle(stream, shared));
            }
            Err(_) => break,
        }
    }
}

fn handle(stream: TcpStream, shared: Arc<Shared>) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader
        .read_line(&mut request_line)
        .ok()
        .filter(|read| *read > 0)
        .is_none()
    {
        return;
    }
    let mut header_lines = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        header_lines.push(line);
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();
    let range = header_lines.iter().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("range") {
            Some(value.trim().trim_end_matches('\r').to_string())
        } else {
            None
        }
    });
    shared.log.lock().expect("log").push(LoggedRequest {
        method: method.clone(),
        path,
        range: range.clone(),
    });

    let current = shared.inflight.fetch_add(1, Ordering::SeqCst) + 1;
    shared.max_inflight.fetch_max(current, Ordering::SeqCst);
    let _guard = Inflight(Arc::clone(&shared));

    let behavior = shared.behavior.lock().expect("behavior").clone();
    let stream = reader.get_mut();
    if method == "HEAD" {
        write_head(stream, &shared.body, &behavior);
        return;
    }
    if method != "GET" || behavior.get_status != 200 {
        let status = if method != "GET" {
            405
        } else {
            behavior.get_status
        };
        write_status(stream, status, &[]);
        return;
    }

    if let Some((start, end)) = range.as_deref().and_then(parse_range) {
        if behavior.reject_ranges {
            write_status(
                stream,
                416,
                &[("Content-Range", format!("bytes */{}", shared.body.len()))],
            );
            return;
        }
        let fail = {
            let mut behavior = shared.behavior.lock().expect("behavior");
            if behavior.fail_range_start_once == Some(start) {
                behavior.fail_range_start_once = None;
                true
            } else {
                false
            }
        };
        if fail {
            if !behavior.fail_range_delay.is_zero() {
                thread::sleep(behavior.fail_range_delay);
            }
            write_status(stream, 500, &[]);
            return;
        }
        if !behavior.ignore_range {
            if start as usize >= shared.body.len() {
                write_status(
                    stream,
                    416,
                    &[("Content-Range", format!("bytes */{}", shared.body.len()))],
                );
                return;
            }
            let last = (shared.body.len() - 1) as u64;
            let end = end.unwrap_or(last).min(last);
            if start > end {
                write_status(stream, 416, &[]);
                return;
            }
            let mut payload = shared.body[start as usize..=end as usize].to_vec();
            if let Some(limit) = behavior.range_body_limit {
                payload.truncate(limit);
            }
            let total = behavior
                .content_range_total
                .unwrap_or(shared.body.len() as u64);
            let headers = [
                ("Content-Range", format!("bytes {start}-{end}/{total}")),
                ("Accept-Ranges", "bytes".to_string()),
            ];
            write_body(stream, &shared, 206, &headers, &payload, false);
            return;
        }
    }

    let headers: [(&str, String); 1] = [(
        "Accept-Ranges",
        if behavior.accept_ranges {
            "bytes".to_string()
        } else {
            "none".to_string()
        },
    )];
    write_body(stream, &shared, 200, &headers, &shared.body.clone(), true);
}

fn write_head(stream: &mut TcpStream, body: &[u8], behavior: &Behavior) {
    if behavior.head_status != 200 {
        write_status(stream, behavior.head_status, &[]);
        return;
    }
    let mut headers = vec![("Content-Length", body.len().to_string())];
    if behavior.accept_ranges {
        headers.push(("Accept-Ranges", "bytes".to_string()));
    }
    write_status(stream, 200, &headers);
}

fn write_body(
    stream: &mut TcpStream,
    shared: &Shared,
    status: u16,
    extra: &[(&str, String)],
    payload: &[u8],
    allow_cut: bool,
) {
    let behavior = shared.behavior.lock().expect("behavior").clone();
    let mut headers = vec![("Content-Length", payload.len().to_string())];
    headers.extend(extra.iter().map(|(name, value)| (*name, value.clone())));
    if behavior.stall_after_headers {
        write_status(stream, status, &headers);
        while !shared.shutdown.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(50));
        }
        return;
    }
    if allow_cut {
        if let Some(limit) = behavior.cut_first_get_after {
            if !shared.cut_used.swap(true, Ordering::SeqCst) {
                let kept = payload
                    .len()
                    .min(usize::try_from(limit).unwrap_or(usize::MAX));
                // Advertise the full length, then drop the connection early.
                write_status(stream, status, &headers);
                let _ = stream.write_all(&payload[..kept]);
                let _ = stream.flush();
                thread::sleep(Duration::from_millis(150));
                let _ = stream.shutdown(Shutdown::Both);
                return;
            }
        }
    }
    write_status(stream, status, &headers);
    if let Some(pace) = behavior.pace {
        for chunk in payload.chunks(64) {
            if shared.shutdown.load(Ordering::SeqCst) {
                return;
            }
            if stream.write_all(chunk).is_err() {
                return;
            }
            if stream.flush().is_err() {
                return;
            }
            thread::sleep(pace);
        }
        return;
    }
    let _ = stream.write_all(payload);
    let _ = stream.flush();
}

fn write_status(stream: &mut TcpStream, status: u16, headers: &[(&str, String)]) {
    let reason = match status {
        200 => "OK",
        206 => "Partial Content",
        404 => "Not Found",
        405 => "Method Not Allowed",
        416 => "Range Not Satisfiable",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\n");
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    if !headers.iter().any(|(name, _)| *name == "Content-Length") {
        head.push_str("Content-Length: 0\r\n");
    }
    head.push_str("Connection: close\r\n\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.flush();
}

fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes=")?;
    let (start, end) = rest.split_once('-')?;
    let start = start.parse().ok()?;
    let end = if end.is_empty() {
        None
    } else {
        Some(end.parse().ok()?)
    };
    Some((start, end))
}

struct Inflight(Arc<Shared>);

impl Drop for Inflight {
    fn drop(&mut self) {
        self.0.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}
