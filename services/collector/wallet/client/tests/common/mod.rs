//! A stand-in for an HTTP service on a local port, shared by the tests that talk to one.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub type Handler = dyn Fn(&str) -> (u16, String) + Send + Sync;

pub struct StandIn {
    pub url: String,
    pub seen: Arc<Mutex<Vec<String>>>,
}

/// Serves `handler(query)` for every request and remembers the queries.
pub fn stand_in(handler: impl Fn(&str) -> (u16, String) + Send + Sync + 'static) -> StandIn {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let handler: Arc<Handler> = Arc::new(handler);
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (handler, log) = (handler.clone(), log.clone());
            std::thread::spawn(move || {
                let mut buffer = [0u8; 8192];
                let n = stream.read(&mut buffer).unwrap_or(0);
                let head = String::from_utf8_lossy(&buffer[..n]).to_string();
                let target = head.lines().next().unwrap_or_default().split_whitespace().nth(1).unwrap_or("/").to_string();
                let query = target.split_once('?').map(|(_, q)| q).unwrap_or("").to_string();
                log.lock().unwrap().push(query.clone());
                let (status, body) = handler(&query);
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            });
        }
    });
    StandIn { url, seen }
}

pub fn param(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| pair.strip_prefix(&format!("{name}=")).map(str::to_string))
}
