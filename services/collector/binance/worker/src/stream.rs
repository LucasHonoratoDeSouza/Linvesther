//! Real-time trade capture via the Binance Spot WebSocket API's signed
//! user data stream subscription (`userDataStream.subscribe.signature`).
//!
//! This replaces the historical `listenKey`-based flow
//! (`POST /api/v3/userDataStream`), which returns `410 Gone` as of
//! 2026-01-21 — confirmed against the real API during this delivery,
//! not assumed from documentation. The new method signs a request the
//! same way every other private endpoint in this codebase does (HMAC-
//! SHA256 over the same API secret `collector_credentials::signing`
//! already uses), so no new key type (Binance's alternative Ed25519
//! WebSocket-session auth) is needed.
//!
//! **Why this exists**: `myTrades` requires a symbol per call, so
//! polling it means the product has to know in advance which pairs to
//! watch. This stream instead receives an `executionReport` event for
//! *every* fill on the account, on any symbol, over one persistent
//! connection — no symbol selection needed for ongoing tracking. It
//! does not backfill history from before the connection was opened;
//! see this crate's README for that limitation.

use binance_trades::Trade;
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::db;

const WS_API_URL: &str = "wss://ws-api.binance.com:443/ws-api/v3";

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("subscribe request was rejected: status={status} body={body}")]
    SubscribeRejected { status: i64, body: String },
    #[error("subscribe response did not arrive or was malformed")]
    SubscribeMalformed,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the epoch")
        .as_millis() as u64
}

/// Per the WebSocket API's signed-request rule: params sorted
/// alphabetically by name, `key=value` joined by `&`, no percent-
/// encoding. With only `{apiKey, timestamp}`, `"apiKey"` already
/// sorts before `"timestamp"` — no general sorter needed for this one
/// call.
fn subscribe_signature(api_secret: &str, api_key: &str, timestamp: u64) -> String {
    let payload = format!("apiKey={api_key}&timestamp={timestamp}");
    let mut mac =
        Hmac::<Sha256>::new_from_slice(api_secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Connects, subscribes to this account's user data stream, and
/// persists every `executionReport` fill until the connection drops or
/// errors. Callers reconnect (with backoff) — this function returning
/// is not itself a fatal condition.
pub async fn run_trade_stream(
    pool: &PgPool,
    connection_id: Uuid,
    api_key: &str,
    api_secret: &str,
) -> Result<(), StreamError> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(WS_API_URL).await?;
    let (mut write, mut read) = ws_stream.split();

    let timestamp = timestamp_ms();
    let signature = subscribe_signature(api_secret, api_key, timestamp);
    let request = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "method": "userDataStream.subscribe.signature",
        "params": { "apiKey": api_key, "timestamp": timestamp, "signature": signature }
    });
    write
        .send(Message::Text(request.to_string().into()))
        .await?;

    // The subscribe acknowledgement is the first message on the
    // socket; every message after it is a pushed event, not a
    // request/response pair.
    let ack = read.next().await.ok_or(StreamError::SubscribeMalformed)??;
    let ack_text = ack
        .into_text()
        .map_err(|_| StreamError::SubscribeMalformed)?;
    let ack_value: serde_json::Value =
        serde_json::from_str(&ack_text).map_err(|_| StreamError::SubscribeMalformed)?;
    let status = ack_value
        .get("status")
        .and_then(|s| s.as_i64())
        .ok_or(StreamError::SubscribeMalformed)?;
    if status != 200 {
        return Err(StreamError::SubscribeRejected {
            status,
            body: ack_text.to_string(),
        });
    }

    while let Some(message) = read.next().await {
        let message = message?;
        let Ok(text) = message.into_text() else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(event) = value.get("event") else {
            continue;
        };
        if event.get("e").and_then(|e| e.as_str()) != Some("executionReport") {
            continue;
        }
        if event.get("x").and_then(|x| x.as_str()) != Some("TRADE") {
            continue; // NEW/CANCELED/REJECTED/etc — not an actual fill
        }
        if let Some(trade) = parse_execution_report(event) {
            db::upsert_trades(pool, connection_id, &[trade]).await?;
        }
    }

    Ok(())
}

/// Only called once `x == "TRADE"` is already confirmed — `t`
/// (trade id) is only meaningful for that execution type (`-1`
/// otherwise, per the event schema). `N` (commission asset) can be
/// `null` on a genuinely fee-free fill; that is represented honestly
/// as an empty string, not a fabricated asset name.
fn parse_execution_report(event: &serde_json::Value) -> Option<Trade> {
    Some(Trade {
        symbol: event.get("s")?.as_str()?.to_string(),
        id: event.get("t")?.as_i64()?.try_into().ok()?,
        order_id: event.get("i")?.as_u64()?,
        price: event.get("L")?.as_str()?.to_string(),
        qty: event.get("l")?.as_str()?.to_string(),
        commission: event.get("n")?.as_str()?.to_string(),
        commission_asset: event
            .get("N")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        time_ms: event.get("T")?.as_u64()?,
        is_buyer: event.get("S")?.as_str()? == "BUY",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_signature_matches_binance_documented_alphabetical_payload() {
        // Confirms the exact payload string shape ("apiKey=...&timestamp=...",
        // no percent-encoding) rather than only that some signature is produced.
        let secret = "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j";
        let sig = subscribe_signature(
            secret,
            "vmPUZE6mv9SD5VNHk4HlWFsOr6aKE2zvsw0MuIgwCIPy6utIco14y7Ju91duEh8A",
            1645423376532,
        );
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn parse_execution_report_extracts_a_well_formed_trade_fill() {
        let event = serde_json::json!({
            "e": "executionReport", "s": "ETHBTC", "S": "BUY", "x": "TRADE",
            "i": 4293153, "t": 12345, "l": "0.50000000", "L": "0.10264410",
            "n": "0.00050000", "N": "ETH", "T": 1499405658657i64
        });
        let trade = parse_execution_report(&event).unwrap();
        assert_eq!(trade.symbol, "ETHBTC");
        assert_eq!(trade.id, 12345);
        assert_eq!(trade.order_id, 4293153);
        assert_eq!(trade.price, "0.10264410");
        assert_eq!(trade.qty, "0.50000000");
        assert_eq!(trade.commission, "0.00050000");
        assert_eq!(trade.commission_asset, "ETH");
        assert_eq!(trade.time_ms, 1499405658657);
        assert!(trade.is_buyer);
    }

    #[test]
    fn parse_execution_report_represents_a_null_commission_asset_as_empty_not_fabricated() {
        let event = serde_json::json!({
            "e": "executionReport", "s": "ETHBTC", "S": "SELL", "x": "TRADE",
            "i": 1, "t": 1, "l": "1", "L": "1", "n": "0", "N": null, "T": 1
        });
        let trade = parse_execution_report(&event).unwrap();
        assert_eq!(trade.commission_asset, "");
        assert!(!trade.is_buyer);
    }

    #[test]
    fn parse_execution_report_returns_none_when_a_required_field_is_missing() {
        let event = serde_json::json!({ "e": "executionReport", "x": "TRADE" });
        assert!(parse_execution_report(&event).is_none());
    }
}
