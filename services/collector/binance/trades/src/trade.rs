//! A single `myTrades` execution, namespaced by `(symbol, id)`.
//!
//! Per the Binance adapter specification: "cada `(symbol,id)` é namespace
//! distinto" — the same numeric id on two different symbols is two
//! unrelated trades, never a collision.

#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    pub symbol: String,
    pub id: u64,
    pub order_id: u64,
    /// Decimal quantities as strings (never `f64`), per the protocol's
    /// general precision rules.
    pub price: String,
    pub qty: String,
    pub commission: String,
    pub commission_asset: String,
    pub time_ms: u64,
    pub is_buyer: bool,
}

impl Trade {
    /// The full namespace key: `(symbol, id)`.
    pub fn key(&self) -> (String, u64) {
        (self.symbol.clone(), self.id)
    }
}
