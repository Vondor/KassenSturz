// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A fake OpenAI-compatible provider for tests: a tiny HTTP server on this
//! computer that gives scripted answers and records what it received. CI
//! never talks to a real provider and never holds a real key (AGENTS.md
//! §3.1 rules 2 and 3); the keys in tests are made up.
//!
//! It listens on `127.0.0.1` only, on a port the system picks, and serves
//! one connection per scripted answer, then stops.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// What the fake answers to one connection.
#[derive(Clone, Debug)]
pub enum Answer {
    /// A JSON body with this HTTP status.
    Json(u16, String),
    /// A redirect to `location`.
    Redirect(String),
    /// A body of `len` bytes of JSON-looking text, longer than a test's
    /// limit.
    Huge(usize),
    /// Headers, then a body that keeps coming for a long time.
    Endless,
    /// These bytes and nothing else, not even HTTP.
    Raw(Vec<u8>),
}

impl Answer {
    /// A chat reply with `text` that ends normally.
    pub fn text(text: &str) -> Self {
        let body = serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": text},
            }],
        });
        Self::Json(200, body.to_string())
    }
}

/// One request the fake received.
#[derive(Clone, Debug, Default)]
pub struct Received {
    /// Everything that arrived, as bytes.
    pub raw: Vec<u8>,
    /// The request line, e.g. `POST /v1/chat/completions HTTP/1.1`.
    pub line: String,
    /// The headers, names in lowercase.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
}

impl Received {
    /// The value of the header `name` (lowercase).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(found, _)| found == name)
            .map(|(_, value)| value.as_str())
    }
}

/// The running fake.
pub struct FakeProvider {
    port: u16,
    received: Arc<Mutex<Vec<Received>>>,
}

impl FakeProvider {
    /// Starts a fake that gives `answers`, one per connection, in order.
    pub fn start(answers: Vec<Answer>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the fake can listen");
        let port = listener.local_addr().expect("the fake has a port").port();
        let received = Arc::new(Mutex::new(Vec::new()));
        let log = received.clone();
        thread::spawn(move || {
            for answer in answers {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let request = read_request(&mut stream);
                log.lock().unwrap().push(request);
                answer_with(&mut stream, &answer);
            }
        });
        Self { port, received }
    }

    /// The fake's base address, as a user would enter it.
    pub fn address(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    /// The same server with `https://`, which it doesn't speak.
    pub fn https_address(&self) -> String {
        format!("https://127.0.0.1:{}/v1", self.port)
    }

    /// What it received so far.
    pub fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }
}

fn read_request(stream: &mut TcpStream) -> Received {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut raw = Vec::new();
    let mut buffer = [0u8; 4096];
    let head_end = loop {
        if let Some(end) = find(&raw, b"\r\n\r\n") {
            break Some(end);
        }
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break None,
            Ok(n) => raw.extend_from_slice(&buffer[..n]),
        }
    };
    let Some(head_end) = head_end else {
        // Not HTTP, e.g. a TLS handshake.
        return Received {
            raw,
            ..Received::default()
        };
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let line = lines.next().unwrap_or_default().to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    while raw.len() < head_end + 4 + length {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => raw.extend_from_slice(&buffer[..n]),
        }
    }
    let body = raw[head_end + 4..].to_vec();
    Received {
        raw,
        line,
        headers,
        body,
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn answer_with(stream: &mut TcpStream, answer: &Answer) {
    let _ = match answer {
        Answer::Json(status, body) => stream.write_all(
            format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        ),
        Answer::Redirect(location) => stream.write_all(
            format!(
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\n\
                 Content-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        ),
        Answer::Huge(len) => {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                 Content-Length: {len}\r\nConnection: close\r\n\r\n"
            );
            let mut bytes = head.into_bytes();
            bytes.extend(std::iter::repeat_n(b' ', *len));
            stream.write_all(&bytes)
        }
        Answer::Endless => {
            let head = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                        Transfer-Encoding: chunked\r\n\r\n";
            let mut result = stream.write_all(head.as_bytes());
            for _ in 0..200 {
                if result.is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
                result = stream.write_all(b"1\r\n \r\n");
            }
            result
        }
        Answer::Raw(bytes) => stream.write_all(bytes),
    };
    let _ = stream.flush();
}
