//! The node client against a stand-in for a JSON-RPC endpoint on a local port.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wallet_client::{Rpc, RpcError};

/// The calls a node has been asked, in order.
type Calls = Arc<Mutex<Vec<(String, Value)>>>;

/// Answers every JSON-RPC request through `handler(method, params) -> Ok(result) | Err(message)`,
/// batches included, and remembers each call.
fn node(handler: impl Fn(&str, &Value) -> Result<Value, String> + Send + Sync + 'static) -> (String, Calls) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(handler);
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (handler, log) = (handler.clone(), log.clone());
            std::thread::spawn(move || {
                let mut raw = Vec::new();
                let mut chunk = [0u8; 8192];
                let (head, mut body) = loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    raw.extend_from_slice(&chunk[..n]);
                    if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break (String::from_utf8_lossy(&raw[..at]).to_lowercase(), raw[at + 4..].to_vec());
                    }
                };
                let length: usize = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
                while body.len() < length {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&chunk[..n]);
                }
                let requests: Vec<Value> = serde_json::from_slice(&body).unwrap();
                let answers: Vec<Value> = requests
                    .iter()
                    .map(|r| {
                        let (method, params) = (r["method"].as_str().unwrap(), &r["params"]);
                        log.lock().unwrap().push((method.to_string(), params.clone()));
                        match handler(method, params) {
                            Ok(result) => json!({ "jsonrpc": "2.0", "id": r["id"], "result": result }),
                            Err(message) => json!({ "jsonrpc": "2.0", "id": r["id"], "error": { "code": -32000, "message": message } }),
                        }
                    })
                    .collect();
                let text = Value::Array(answers).to_string();
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", text.len(), text);
            });
        }
    });
    (url, seen)
}

fn rpc(url: &str) -> Rpc {
    Rpc::new(url).with_pacing(Duration::ZERO, Duration::from_millis(5))
}

#[test]
fn the_head_and_a_balance_as_of_a_block_are_read_exactly() {
    let (url, seen) = node(|method, params| match method {
        "eth_blockNumber" => Ok(json!("0x64")),
        "eth_getBalance" => {
            assert_eq!(params[1], "0x60", "the block is asked for by number, in hex");
            Ok(json!("0x5d3abe2cfc589d15"))
        }
        other => Err(format!("unexpected {other}")),
    });
    let node = rpc(&url);
    assert_eq!(node.head_block().unwrap(), 100);
    assert_eq!(node.native_balance_at("0xAbC", 96).unwrap(), "6717890894598020373");
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn token_balances_are_asked_for_in_batches_and_a_contract_that_does_not_answer_is_none() {
    let (url, seen) = node(|method, params| {
        assert_eq!(method, "eth_call");
        let data = params[0]["data"].as_str().unwrap();
        assert!(data.starts_with("0x70a08231") && data.ends_with("abc") && data.len() == 2 + 8 + 64, "balanceOf(address): {data}");
        match params[0]["to"].as_str().unwrap() {
            "0xnotatoken" => Ok(json!("0x")),
            "0xreverts" => Err("execution reverted".into()),
            _ => Ok(json!("0x0000000000000000000000000000000000000000000000000000000005f5e100")),
        }
    });
    let contracts: Vec<String> = ["0xa", "0xnotatoken", "0xb", "0xreverts"].map(String::from).to_vec();
    let balances = rpc(&url).token_balances_at("0xABC", &contracts, 200).unwrap();
    assert_eq!(balances, vec![Some("100000000".to_string()), None, Some("100000000".to_string()), None]);
    assert_eq!(seen.lock().unwrap().len(), 4);
    let many: Vec<String> = (0..45).map(|i| format!("0x{i:x}")).collect();
    let (url, _) = node(|_, _| Ok(json!("0x1")));
    assert_eq!(rpc(&url).token_balances_at("0xabc", &many, 1).unwrap().len(), 45, "more than one batch, in order");
}

#[test]
fn a_node_that_refuses_the_whole_call_is_an_error_naming_why() {
    let (url, _) = node(|_, _| Err("header not found".into()));
    match rpc(&url).native_balance_at("0xabc", 1) {
        Err(RpcError::Refused(why)) => assert!(why.contains("header not found")),
        other => panic!("expected a refusal, got {other:?}"),
    }
}
