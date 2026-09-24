//! Reading back the executed trades already collected for a connection:
//! narrowed to one market or one period, in a stable order, a page at a
//! time.
//!
//! Pure over an already-loaded list, so the same code answers for every
//! exchange (`binance_synced_trades`, Coinbase's fills, Kraken's
//! trades all arrive here as `Trade`) and can be tested without a
//! database.
//!
//! Order is `(time_ms, symbol, id)`, never `time_ms` alone: per
//! `binance-trades`, `(symbol, id)` is the namespace of an execution, so
//! two different trades can share both a millisecond and a numeric id.
//! A page boundary drawn on the incomplete key would drop or repeat
//! trades at that boundary.

use binance_trades::Trade;

/// Where a previous page stopped — the full order key of its last trade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub time_ms: u64,
    pub id: u64,
    pub symbol: String,
}

impl Cursor {
    /// `<time_ms>:<id>:<symbol>`. The symbol comes last and is not split
    /// again, so a symbol containing a colon still round-trips.
    pub fn encode(&self) -> String {
        format!("{}:{}:{}", self.time_ms, self.id, self.symbol)
    }

    /// `None` for anything this service did not issue, so a caller gets a
    /// refusal instead of a silently different page.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.splitn(3, ':');
        let time_ms = parts.next()?.parse().ok()?;
        let id = parts.next()?.parse().ok()?;
        let symbol = parts.next()?;
        if symbol.is_empty() {
            return None;
        }
        Some(Cursor { time_ms, id, symbol: symbol.to_string() })
    }

    fn of(trade: &Trade) -> Self {
        Cursor { time_ms: trade.time_ms, id: trade.id, symbol: trade.symbol.clone() }
    }
}

/// The most trades one page may carry, and how many it carries when the
/// caller does not ask for a size. A page is bounded so one request
/// cannot ask for an account's whole history in a single answer.
pub const MAX_PAGE: usize = 500;
pub const DEFAULT_PAGE: usize = 100;

/// Which trades a caller asked for. Every field is optional except the
/// page size: no filter means "everything collected, from the start".
#[derive(Debug, Default, Clone)]
pub struct TradeQuery {
    /// One market, compared without case (Binance writes `BTCUSDT`,
    /// Coinbase `BTC-USD`).
    pub symbol: Option<String>,
    /// Inclusive bounds on the execution time, in epoch milliseconds.
    pub since_ms: Option<u64>,
    pub until_ms: Option<u64>,
    pub after: Option<Cursor>,
    pub limit: usize,
}

impl TradeQuery {
    /// Refuses a page size outside `1..=MAX_PAGE` rather than quietly
    /// answering a different one, and refuses a period that cannot
    /// contain anything.
    pub fn check(&self) -> Result<(), String> {
        if self.limit == 0 || self.limit > MAX_PAGE {
            return Err(format!("limit must be between 1 and {MAX_PAGE}"));
        }
        match (self.since_ms, self.until_ms) {
            (Some(since), Some(until)) if since > until => {
                Err("sinceMs must not be after untilMs".to_string())
            }
            _ => Ok(()),
        }
    }
}

pub struct TradePage {
    pub trades: Vec<Trade>,
    /// Where to resume; `None` once the last page has been handed out.
    pub next: Option<Cursor>,
}

fn matches(trade: &Trade, query: &TradeQuery) -> bool {
    if let Some(symbol) = &query.symbol {
        if !trade.symbol.eq_ignore_ascii_case(symbol) {
            return false;
        }
    }
    if let Some(since) = query.since_ms {
        if trade.time_ms < since {
            return false;
        }
    }
    if let Some(until) = query.until_ms {
        if trade.time_ms > until {
            return false;
        }
    }
    match &query.after {
        // Strictly after the previous page's last trade, on the full
        // order key — never `>=`, which would repeat that trade.
        Some(after) => (trade.time_ms, trade.symbol.as_str(), trade.id) > (after.time_ms, after.symbol.as_str(), after.id),
        None => true,
    }
}

/// One page of `trades` matching `query`. `trades` may arrive in any
/// order; the order key is applied here, so a caller cannot get a
/// different page out of the same data by loading it differently.
pub fn page(trades: Vec<Trade>, query: &TradeQuery) -> TradePage {
    let mut matching: Vec<Trade> = trades.into_iter().filter(|trade| matches(trade, query)).collect();
    matching.sort_by(|a, b| (a.time_ms, &a.symbol, a.id).cmp(&(b.time_ms, &b.symbol, b.id)));
    // One more than the page is read only to learn whether a further
    // page exists — it is never returned, so a `next` cursor is only
    // ever handed out when there is something behind it.
    let has_more = matching.len() > query.limit;
    matching.truncate(query.limit);
    let next = if has_more { matching.last().map(Cursor::of) } else { None };
    TradePage { trades: matching, next }
}
