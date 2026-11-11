//! A deliberately tiny HTTP/1.1 server.
//!
//! The dashboard needs exactly four things: serve one HTML page, answer a JSON
//! state endpoint, accept a handful of commands, and never block the engine.
//! That does not justify a framework, and the workspace's rule is "no network
//! access at build time", so this is std-only.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

/// A parsed request.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub body: String,
}

impl Request {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.query.get(key).map(|s| s.as_str())
    }

    pub fn u64_param(&self, key: &str) -> Option<u64> {
        self.param(key).and_then(|v| v.parse().ok())
    }
}

/// What to send back.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(body: &str) -> Response {
        Response {
            status: 200,
            content_type: "application/json; charset=utf-8".to_string(),
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn html(body: String) -> Response {
        Response {
            status: 200,
            content_type: "text/html; charset=utf-8".to_string(),
            body: body.into_bytes(),
        }
    }

    pub fn text(status: u16, body: &str) -> Response {
        Response {
            status,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: body.as_bytes().to_vec(),
        }
    }
}

/// Read, dispatch and answer one connection.
pub fn serve_one(
    mut stream: TcpStream,
    handler: &mut dyn FnMut(&Request) -> Response,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut filled = 0;
    let header_end = loop {
        if filled == buffer.len() {
            break None;
        }
        let read = stream.read(&mut buffer[filled..])?;
        if read == 0 {
            break None;
        }
        filled += read;
        if let Some(pos) = find_subslice(&buffer[..filled], b"\r\n\r\n") {
            break Some(pos + 4);
        }
    };

    let Some(header_end) = header_end else {
        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
        return Ok(());
    };

    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = head.lines();
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or("GET").to_string();
    let target = request_line.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = buffer[header_end..filled].to_vec();
    while body.len() < content_length {
        let mut more = vec![0u8; content_length - body.len()];
        let read = stream.read(&mut more)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&more[..read]);
    }

    let (path, query) = split_target(&target);
    let request = Request {
        method,
        path,
        query,
        body: String::from_utf8_lossy(&body).to_string(),
    };
    let response = handler(&request);

    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        status_text(response.status),
        response.content_type,
        response.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()?;
    Ok(())
}

/// Bind a listener on all interfaces (required for sandbox previews).
pub fn bind(port: u16) -> std::io::Result<TcpListener> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    Ok(listener)
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    }
}

fn split_target(target: &str) -> (String, BTreeMap<String, String>) {
    let mut parts = target.splitn(2, '?');
    let path = parts.next().unwrap_or("/").to_string();
    let mut query = BTreeMap::new();
    if let Some(text) = parts.next() {
        for pair in text.split('&') {
            let mut kv = pair.splitn(2, '=');
            let key = decode(kv.next().unwrap_or_default());
            let value = decode(kv.next().unwrap_or_default());
            if !key.is_empty() {
                query.insert(key, value);
            }
        }
    }
    (path, query)
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_splits_path_and_query() {
        let (path, query) = split_target("/api/tick?n=3&x=a%20b");
        assert_eq!(path, "/api/tick");
        assert_eq!(query["n"], "3");
        assert_eq!(query["x"], "a b");
    }

    #[test]
    fn paths_without_query_work() {
        let (path, query) = split_target("/");
        assert_eq!(path, "/");
        assert!(query.is_empty());
    }

    #[test]
    fn finds_header_terminator() {
        let data = b"GET / HTTP/1.1\r\nHost: x\r\n\r\nbody";
        // "GET / HTTP/1.1\r\nHost: x" is 23 bytes; the header terminator
        // starts there and ends at byte 27.
        assert_eq!(find_subslice(data, b"\r\n\r\n"), Some(23));
        assert_eq!(find_subslice(data, b"nope"), None);
    }
}
