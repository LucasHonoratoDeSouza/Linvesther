//! A node's JSON-RPC, as far as this needs: the newest block, and an address's balances as of a
//! given block. Reading a balance at a block that is a few dozen behind the head is something
//! any full node serves, and it is exact: no arithmetic on recent transfers is involved.

use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Calls sent in one batch, well inside what public nodes accept.
const BATCH: usize = 20;
const RATE_LIMIT_RETRIES: u32 = 3;
/// `balanceOf(address)`.
const BALANCE_OF: &str = "0x70a08231";

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("could not reach the node: {0}")]
    Network(String),
    #[error("the node's rate limit was reached")]
    RateLimited,
    #[error("the node sent something unexpected: {0}")]
    Unexpected(String),
    #[error("the node refused: {0}")]
    Refused(String),
}

pub struct Rpc {
    url: String,
    http: reqwest::blocking::Client,
    spacing: Duration,
    retry_wait: Duration,
    next_call: Mutex<Instant>,
}

/// A hexadecimal quantity (`0x1a2b`, up to 256 bits) as a decimal string.
pub fn hex_to_decimal(hex: &str) -> Option<String> {
    let digits = hex.strip_prefix("0x")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // Repeated division of the hex digits by ten; small inputs, so clarity beats speed.
    let mut number: Vec<u32> = digits.bytes().map(|b| (b as char).to_digit(16).expect("checked hex digit")).collect();
    let mut out = Vec::new();
    while number.iter().any(|d| *d != 0) {
        let mut remainder = 0u32;
        for digit in number.iter_mut() {
            let value = remainder * 16 + *digit;
            *digit = value / 10;
            remainder = value % 10;
        }
        out.push(char::from_digit(remainder, 10).expect("a digit"));
    }
    if out.is_empty() {
        return Some("0".into());
    }
    out.reverse();
    Some(out.into_iter().collect())
}

impl Rpc {
    pub fn new(url: impl Into<String>) -> Self {
        Rpc {
            url: url.into(),
            http: reqwest::blocking::Client::builder().user_agent("linvesther-collector").timeout(Duration::from_secs(30)).build().expect("the HTTP client can be built"),
            spacing: Duration::from_millis(100),
            retry_wait: Duration::from_secs(2),
            next_call: Mutex::new(Instant::now()),
        }
    }

    pub fn with_pacing(mut self, spacing: Duration, retry_wait: Duration) -> Self {
        self.spacing = spacing;
        self.retry_wait = retry_wait;
        self
    }

    /// One batch of calls, answered in the order asked. A call the node refused is `Err`.
    fn batch(&self, calls: Vec<(&str, Value)>) -> Result<Vec<Result<Value, String>>, RpcError> {
        let body: Vec<Value> = calls.iter().enumerate().map(|(i, (method, params))| json!({ "jsonrpc": "2.0", "id": i, "method": method, "params": params })).collect();
        for attempt in 0..=RATE_LIMIT_RETRIES {
            let wait = {
                let mut next = self.next_call.lock().expect("the spacing lock is not poisoned");
                let slot = (*next).max(Instant::now());
                *next = slot + self.spacing;
                slot.saturating_duration_since(Instant::now())
            };
            std::thread::sleep(wait);
            let response = self.http.post(&self.url).json(&body).send().map_err(|e| RpcError::Network(e.without_url().to_string()))?;
            let status = response.status();
            let text = response.text().map_err(|e| RpcError::Network(e.without_url().to_string()))?;
            let limited = status.as_u16() == 429 || text.to_lowercase().contains("rate limit") || text.to_lowercase().contains("too many requests");
            if limited {
                if attempt < RATE_LIMIT_RETRIES {
                    std::thread::sleep(self.retry_wait * (attempt + 1));
                    continue;
                }
                return Err(RpcError::RateLimited);
            }
            if !status.is_success() {
                return Err(RpcError::Unexpected(format!("answered {status}")));
            }
            let answers: Value = serde_json::from_str(&text).map_err(|_| RpcError::Unexpected("the answer was not JSON".into()))?;
            let answers = answers.as_array().ok_or_else(|| RpcError::Unexpected("a batch was not answered with a list".into()))?;
            let mut ordered: Vec<Option<Result<Value, String>>> = vec![None; calls.len()];
            for answer in answers {
                let id = answer.get("id").and_then(Value::as_u64).ok_or_else(|| RpcError::Unexpected("an answer had no id".into()))? as usize;
                let slot = ordered.get_mut(id).ok_or_else(|| RpcError::Unexpected("an answer named a call that was not made".into()))?;
                *slot = Some(match answer.get("error") {
                    Some(error) if !error.is_null() => Err(error.get("message").and_then(Value::as_str).unwrap_or("refused").to_string()),
                    _ => Ok(answer.get("result").cloned().unwrap_or(Value::Null)),
                });
            }
            return ordered.into_iter().map(|a| a.ok_or_else(|| RpcError::Unexpected("a call was not answered".into()))).collect();
        }
        unreachable!("the loop returns on its last attempt")
    }

    fn one(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.batch(vec![(method, params)])?.remove(0).map_err(RpcError::Refused)
    }

    pub fn head_block(&self) -> Result<u64, RpcError> {
        let result = self.one("eth_blockNumber", json!([]))?;
        let hex = result.as_str().ok_or_else(|| RpcError::Unexpected("the block number was not a string".into()))?;
        u64::from_str_radix(hex.trim_start_matches("0x"), 16).map_err(|_| RpcError::Unexpected(format!("unreadable block number {hex}")))
    }

    /// The native balance in wei as of `block`.
    pub fn native_balance_at(&self, address: &str, block: u64) -> Result<String, RpcError> {
        let result = self.one("eth_getBalance", json!([address, format!("0x{block:x}")]))?;
        result.as_str().and_then(hex_to_decimal).ok_or_else(|| RpcError::Unexpected("the balance was not a hexadecimal number".into()))
    }

    /// Each token's balance of `address` as of `block`, in the same order as `contracts`. A
    /// contract that does not answer `balanceOf` (not a token, or not deployed yet) is `None`.
    pub fn token_balances_at(&self, address: &str, contracts: &[String], block: u64) -> Result<Vec<Option<String>>, RpcError> {
        let holder = address.trim_start_matches("0x").to_lowercase();
        let data = format!("{BALANCE_OF}{holder:0>64}");
        let tag = format!("0x{block:x}");
        let mut out = Vec::with_capacity(contracts.len());
        for group in contracts.chunks(BATCH) {
            let calls: Vec<(&str, Value)> = group.iter().map(|contract| ("eth_call", json!([{ "to": contract, "data": data }, tag]))).collect();
            for answer in self.batch(calls)? {
                out.push(answer.ok().and_then(|value| value.as_str().map(str::to_string)).and_then(|hex| hex_to_decimal(&hex)));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_quantities_become_exact_decimal_strings_beyond_64_bits() {
        assert_eq!(hex_to_decimal("0x0").unwrap(), "0");
        assert_eq!(hex_to_decimal("0x1a2b").unwrap(), "6699");
        assert_eq!(hex_to_decimal("0x5d3abe2cfc589d15").unwrap(), "6717890894598020373");
        // 2^256 - 1
        assert_eq!(hex_to_decimal(&format!("0x{}", "f".repeat(64))).unwrap(), "115792089237316195423570985008687907853269984665640564039457584007913129639935");
        assert!(hex_to_decimal("1a2b").is_none() && hex_to_decimal("0x").is_none() && hex_to_decimal("0xzz").is_none());
    }
}
