//! What a block explorer's account API returns, parsed into plain types. Etherscan and
//! Blockscout answer the same questions in the same shape, so one parser serves both.
//! Kept apart from the HTTP so every shape can be tested from a fixture.

use serde_json::Value;

/// An address as the explorer reports it, in lower case.
pub type Address = String;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WireError {
    /// The explorer refused because too many calls were made.
    #[error("the explorer's rate limit was reached: {0}")]
    RateLimited(String),
    /// The explorer said no, or said something that is not an answer.
    #[error("the explorer refused the request: {0}")]
    Refused(String),
    #[error("the explorer sent something unexpected: {0}")]
    Unexpected(String),
}

/// A list the explorer returned, and whether it says the list is whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing<T> {
    pub rows: Vec<T>,
    /// The explorer says some of what lies in this range has not been indexed yet, so
    /// rows may be missing. Its answer is still usable, but it is not proof of completeness.
    pub incomplete: bool,
}

/// A transaction the address sent or received (ether moved directly, and the gas paid).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalTx {
    pub hash: String,
    pub block: u64,
    pub time_ms: u64,
    pub from: Address,
    pub to: Address,
    /// Wei, as an integer string. Not transferred when the transaction failed.
    pub value: String,
    pub gas_used: String,
    pub gas_price: String,
    pub failed: bool,
}

/// Ether moved by a contract during a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalTx {
    pub hash: String,
    pub block: u64,
    pub time_ms: u64,
    pub from: Address,
    pub to: Address,
    pub value: String,
    /// Tells apart the transfers of one transaction.
    pub index: String,
    pub failed: bool,
}

/// A token transfer event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenTransfer {
    pub hash: String,
    pub block: u64,
    pub time_ms: u64,
    pub contract: Address,
    pub symbol: String,
    pub decimals: u32,
    pub from: Address,
    pub to: Address,
    /// In the token's smallest unit, as an integer string.
    pub value: String,
    /// Where in its block the event sits, when the explorer says.
    pub log_index: Option<String>,
}

/// A token the address holds now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldToken {
    pub contract: Address,
    pub symbol: String,
    pub decimals: u32,
    pub balance: String,
}

/// What an explorer said if its answer is a request to slow down, whatever the shape of the rest.
pub fn rate_limit_message(body: &Value) -> Option<String> {
    let message = text(body, "message").unwrap_or_default();
    let reason = body.get("result").and_then(Value::as_str).unwrap_or_default();
    if is_rate_limit(&message) || is_rate_limit(reason) {
        Some(if message.is_empty() { reason.to_string() } else { message })
    } else {
        None
    }
}

fn is_rate_limit(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("rate limit") || lower.contains("too many requests") || lower.contains("max calls per sec")
}

fn text(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn number(value: &Value, key: &str) -> Result<u64, WireError> {
    text(value, key).and_then(|s| s.parse().ok()).ok_or_else(|| WireError::Unexpected(format!("a row had no readable {key}")))
}

fn required(value: &Value, key: &str) -> Result<String, WireError> {
    text(value, key).ok_or_else(|| WireError::Unexpected(format!("a row had no {key}")))
}

fn integer_string(value: &Value, key: &str) -> Result<String, WireError> {
    let raw = required(value, key)?;
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(WireError::Unexpected(format!("a row had an amount that is not a whole number: {key}")));
    }
    Ok(raw)
}

fn lower(value: &Value, key: &str) -> Result<Address, WireError> {
    required(value, key).map(|s| s.to_lowercase())
}

/// The list inside an explorer's answer. `status` 1 is a list, 0 with a "no ..." message is an
/// empty one, and 2 (Blockscout) is a list the explorer warns is not complete.
fn rows(body: &Value) -> Result<(Vec<Value>, bool), WireError> {
    let status = text(body, "status").unwrap_or_default();
    let message = text(body, "message").unwrap_or_default();
    let result = body.get("result").cloned().unwrap_or(Value::Null);
    if is_rate_limit(&message) || result.as_str().is_some_and(is_rate_limit) {
        return Err(WireError::RateLimited(message));
    }
    match (status.as_str(), result) {
        ("1", Value::Array(rows)) => Ok((rows, false)),
        ("2", Value::Array(rows)) => Ok((rows, true)),
        ("0", Value::Array(rows)) if rows.is_empty() && message.to_lowercase().starts_with("no ") => Ok((rows, false)),
        (_, Value::String(reason)) => Err(WireError::Refused(format!("{message}: {reason}"))),
        _ => Err(WireError::Refused(message)),
    }
}

pub fn parse_normal_txs(body: &Value) -> Result<Listing<NormalTx>, WireError> {
    let (raw, incomplete) = rows(body)?;
    let mut out = Vec::with_capacity(raw.len());
    for row in &raw {
        out.push(NormalTx {
            hash: lower(row, "hash")?,
            block: number(row, "blockNumber")?,
            time_ms: number(row, "timeStamp")? * 1000,
            from: lower(row, "from")?,
            // A contract creation has no recipient.
            to: text(row, "to").unwrap_or_default().to_lowercase(),
            value: integer_string(row, "value")?,
            gas_used: integer_string(row, "gasUsed")?,
            gas_price: integer_string(row, "gasPrice")?,
            failed: text(row, "isError").as_deref() == Some("1") || text(row, "txreceipt_status").as_deref() == Some("0"),
        });
    }
    Ok(Listing { rows: out, incomplete })
}

pub fn parse_internal_txs(body: &Value) -> Result<Listing<InternalTx>, WireError> {
    let (raw, incomplete) = rows(body)?;
    let mut out = Vec::with_capacity(raw.len());
    for row in &raw {
        out.push(InternalTx {
            hash: lower(row, "transactionHash").or_else(|_| lower(row, "hash"))?,
            block: number(row, "blockNumber")?,
            time_ms: number(row, "timeStamp")? * 1000,
            from: lower(row, "from")?,
            to: text(row, "to").unwrap_or_default().to_lowercase(),
            value: integer_string(row, "value")?,
            index: text(row, "traceId").or_else(|| text(row, "index")).unwrap_or_default(),
            failed: text(row, "isError").as_deref() == Some("1"),
        });
    }
    Ok(Listing { rows: out, incomplete })
}

pub fn parse_token_transfers(body: &Value) -> Result<Listing<TokenTransfer>, WireError> {
    let (raw, incomplete) = rows(body)?;
    let mut out = Vec::with_capacity(raw.len());
    for row in &raw {
        out.push(TokenTransfer {
            hash: lower(row, "hash")?,
            block: number(row, "blockNumber")?,
            time_ms: number(row, "timeStamp")? * 1000,
            contract: lower(row, "contractAddress")?,
            symbol: text(row, "tokenSymbol").unwrap_or_default(),
            decimals: text(row, "tokenDecimal").and_then(|d| d.parse().ok()).unwrap_or(0),
            from: lower(row, "from")?,
            to: lower(row, "to")?,
            value: integer_string(row, "value")?,
            log_index: text(row, "logIndex"),
        });
    }
    Ok(Listing { rows: out, incomplete })
}

/// Blockscout's list of what an address holds: fungible tokens only.
pub fn parse_held_tokens(body: &Value) -> Result<Vec<HeldToken>, WireError> {
    let (raw, _) = rows(body)?;
    let mut out = Vec::new();
    for row in &raw {
        // NFTs and other kinds are not something this can value.
        if text(row, "type").is_some_and(|t| t != "ERC-20") {
            continue;
        }
        out.push(HeldToken {
            contract: lower(row, "contractAddress")?,
            symbol: text(row, "symbol").unwrap_or_default(),
            decimals: text(row, "decimals").and_then(|d| d.parse().ok()).unwrap_or(0),
            balance: integer_string(row, "balance")?,
        });
    }
    Ok(out)
}

/// A single integer the explorer answers in `result` (a balance).
pub fn parse_integer_result(body: &Value) -> Result<String, WireError> {
    let message = text(body, "message").unwrap_or_default();
    match body.get("result") {
        Some(Value::String(s)) if is_rate_limit(s) || is_rate_limit(&message) => Err(WireError::RateLimited(s.clone())),
        Some(Value::String(s)) if text(body, "status").as_deref() == Some("1") && !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => Ok(s.clone()),
        Some(Value::String(s)) => Err(WireError::Refused(format!("{message}: {s}"))),
        _ if is_rate_limit(&message) => Err(WireError::RateLimited(message)),
        _ => Err(WireError::Refused(message)),
    }
}

/// A block number, from a JSON-RPC style answer (`0x1a2b`) or a plain integer.
pub fn parse_block_number(body: &Value) -> Result<u64, WireError> {
    let message = text(body, "message").unwrap_or_default();
    if is_rate_limit(&message) {
        return Err(WireError::RateLimited(message));
    }
    match body.get("result") {
        Some(Value::String(s)) => {
            let parsed = match s.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16),
                None => s.parse(),
            };
            parsed.map_err(|_| WireError::Refused(format!("{message}: {s}")))
        }
        _ => Err(WireError::Refused(message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_normal_transaction_keeps_its_value_gas_and_failure() {
        let body = json!({ "status": "1", "message": "OK", "result": [{
            "blockNumber": "2370740", "timeStamp": "1691530827", "hash": "0xABC",
            "from": "0x3C95f06542dcf6490263b2754a7f3dd9bcea0f9f", "to": "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045",
            "value": "1000000000000", "gasUsed": "21000", "gasPrice": "1500000050", "isError": "0", "txreceipt_status": "1"
        }, {
            "blockNumber": "2370741", "timeStamp": "1691530829", "hash": "0xdef", "from": "0xa", "to": "",
            "value": "5", "gasUsed": "50000", "gasPrice": "7", "isError": "1", "txreceipt_status": "0"
        }]});
        let listing = parse_normal_txs(&body).unwrap();
        assert!(!listing.incomplete);
        let first = &listing.rows[0];
        assert_eq!((first.hash.as_str(), first.block, first.time_ms), ("0xabc", 2_370_740, 1_691_530_827_000));
        assert_eq!(first.from, "0x3c95f06542dcf6490263b2754a7f3dd9bcea0f9f");
        assert_eq!((first.value.as_str(), first.gas_used.as_str(), first.gas_price.as_str()), ("1000000000000", "21000", "1500000050"));
        assert!(!first.failed);
        assert!(listing.rows[1].failed && listing.rows[1].to.is_empty(), "a failed contract creation");
    }

    #[test]
    fn an_internal_transaction_reads_either_hash_name_and_a_warned_answer_is_marked_incomplete() {
        let body = json!({ "status": "2", "message": "Some internal transactions within this block range have not yet been processed", "result": [{
            "blockNumber": "2843040", "timeStamp": "1692475427", "transactionHash": "0xAB", "from": "0xF3", "to": "0xd8",
            "value": "3125000000000", "index": "2", "isError": "0"
        }]});
        let listing = parse_internal_txs(&body).unwrap();
        assert!(listing.incomplete, "the explorer says rows may be missing");
        assert_eq!((listing.rows[0].hash.as_str(), listing.rows[0].index.as_str(), listing.rows[0].value.as_str()), ("0xab", "2", "3125000000000"));
        let etherscan = json!({ "status": "1", "message": "OK", "result": [{ "blockNumber": "1", "timeStamp": "2", "hash": "0xh", "from": "0xa", "to": "0xb", "value": "9", "traceId": "0_1", "isError": "0" }] });
        assert_eq!(parse_internal_txs(&etherscan).unwrap().rows[0].index, "0_1");
    }

    #[test]
    fn a_token_transfer_carries_the_contract_symbol_decimals_and_raw_amount() {
        let body = json!({ "status": "1", "message": "OK", "result": [{
            "value": "63245553203367586638977", "blockNumber": "1965746", "timeStamp": "1690720839", "hash": "0x5d86",
            "contractAddress": "0x5FF177aEbFf7f944102406E1340B2550256fF92", "from": "0x4bea", "to": "0xd8da",
            "tokenDecimal": "18", "tokenSymbol": "vLS2", "logIndex": "12"
        }]});
        let t = &parse_token_transfers(&body).unwrap().rows[0];
        assert_eq!((t.contract.as_str(), t.symbol.as_str(), t.decimals, t.value.as_str()), ("0x5ff177aebff7f944102406e1340b2550256ff92", "vLS2", 18, "63245553203367586638977"));
        assert_eq!(t.log_index.as_deref(), Some("12"));
    }

    #[test]
    fn an_empty_answer_is_a_whole_empty_list_but_any_other_refusal_is_an_error() {
        assert_eq!(parse_normal_txs(&json!({ "status": "0", "message": "No transactions found", "result": [] })).unwrap().rows.len(), 0);
        assert!(matches!(parse_normal_txs(&json!({ "status": "0", "message": "NOTOK", "result": "Invalid API Key" })), Err(WireError::Refused(_))));
        assert!(matches!(parse_token_transfers(&json!({ "status": "0", "message": "NOTOK", "result": [] })), Err(WireError::Refused(_))), "an empty list without the 'no ...' message is not an answer");
    }

    #[test]
    fn rate_limit_answers_are_recognised_whichever_way_each_explorer_words_them() {
        let etherscan = json!({ "status": "0", "message": "NOTOK", "result": "Max calls per sec rate limit reached (5/sec)" });
        let blockscout = json!({ "message": "Too many requests. Increase limits now", "result": null, "status": "0" });
        assert!(matches!(parse_normal_txs(&etherscan), Err(WireError::RateLimited(_))));
        assert!(matches!(parse_normal_txs(&blockscout), Err(WireError::RateLimited(_))));
        assert!(matches!(parse_integer_result(&blockscout), Err(WireError::RateLimited(_))));
    }

    #[test]
    fn an_amount_that_is_not_a_whole_number_is_refused_not_guessed() {
        let body = json!({ "status": "1", "message": "OK", "result": [{
            "blockNumber": "1", "timeStamp": "2", "hash": "0xa", "from": "0xb", "to": "0xc", "value": "1.5", "gasUsed": "1", "gasPrice": "1"
        }]});
        assert!(matches!(parse_normal_txs(&body), Err(WireError::Unexpected(_))));
    }

    #[test]
    fn held_tokens_skip_what_is_not_a_fungible_token() {
        let body = json!({ "status": "1", "message": "OK", "result": [
            { "balance": "366748084131360708767516", "contractAddress": "0x023A", "decimals": "18", "name": "Cellana", "symbol": "CELL", "type": "ERC-20" },
            { "balance": "1", "contractAddress": "0x0999", "decimals": "", "name": "Art", "symbol": "ART", "type": "ERC-721" }
        ]});
        let tokens = parse_held_tokens(&body).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!((tokens[0].contract.as_str(), tokens[0].decimals), ("0x023a", 18));
    }

    #[test]
    fn a_balance_and_a_block_number_are_read_from_their_answers() {
        assert_eq!(parse_integer_result(&json!({ "message": "OK", "result": "6717890894598020373", "status": "1" })).unwrap(), "6717890894598020373");
        assert!(parse_integer_result(&json!({ "message": "NOTOK", "result": "Error!", "status": "0" })).is_err());
        assert_eq!(parse_block_number(&json!({ "jsonrpc": "2.0", "id": 1, "result": "0x1a2b" })).unwrap(), 6699);
        assert_eq!(parse_block_number(&json!({ "status": "1", "message": "OK", "result": "6699" })).unwrap(), 6699);
        assert!(parse_block_number(&json!({ "message": "NOTOK", "result": "nope", "status": "0" })).is_err());
    }
}
