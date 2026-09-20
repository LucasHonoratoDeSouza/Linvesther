//! A single `klines` candle, as needed to mark valuation.
//!
//! Per the protocol specification: "close da última vela de 1 minuto
//! já encerrada no instante avaliado, idade máxima 120 s. Não usar vela
//! futura."

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candle {
    pub symbol: String,
    pub open_time_ms: u64,
    pub close_time_ms: u64,
    /// Decimal string (never `f64`).
    pub close_price: String,
    /// Binance's kline stream can emit an in-progress (not yet closed)
    /// candle; this field is how the caller tells this crate which is
    /// which — this crate never infers closedness from timing alone,
    /// since a candle believed closed by clock math could still be the
    /// venue's still-updating current bar.
    pub is_closed: bool,
}
