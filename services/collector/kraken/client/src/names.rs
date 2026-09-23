//! Kraken's asset and market names, turned into the ones everything else in
//! Linvesther uses (`XXBT` → `BTC`, `DOT.S` → `DOT`, `XXBTZUSD` → `BTC-USD`).

use std::collections::{BTreeMap, BTreeSet};

/// The asset a Kraken code stands for: its short name (from Kraken's own asset
/// list), with the wallet variants folded into the asset itself — a staked or
/// reward-earning balance (`DOT.S`, `USD.M`, `ATOM21.S`) and funds on hold
/// (`EUR.HOLD`) are still that asset — and Kraken's few unusual tickers mapped
/// to the usual ones.
pub fn normalize_asset(code: &str, altnames: &BTreeMap<String, String>, known: &BTreeSet<String>) -> String {
    let alt = altnames.get(code).map(String::as_str).unwrap_or(code);
    let mut name = alt.to_string();
    if let Some((base, suffix)) = alt.rsplit_once('.') {
        if is_wallet_suffix(suffix) {
            // A bonded staking variant carries its lock length (`ATOM21.S`).
            let trimmed = base.trim_end_matches(|c: char| c.is_ascii_digit());
            name = if trimmed != base && !trimmed.is_empty() && known.contains(trimmed) { trimmed.to_string() } else { base.to_string() };
        }
    }
    match name.as_str() {
        "XBT" => "BTC".to_string(),
        "XDG" => "DOGE".to_string(),
        // Staked ETH, on Kraken's old name for it.
        "ETH2" => "ETH".to_string(),
        _ => name,
    }
}

fn is_wallet_suffix(suffix: &str) -> bool {
    matches!(suffix, "S" | "M" | "F" | "B" | "P" | "HOLD")
}

/// True for a balance that is not freely spendable: staked, bonded, earning or on hold.
pub fn is_locked_wallet(code: &str, altnames: &BTreeMap<String, String>) -> bool {
    let alt = altnames.get(code).map(String::as_str).unwrap_or(code);
    alt.rsplit_once('.').is_some_and(|(_, suffix)| is_wallet_suffix(suffix))
}

/// The name Linvesther gives a market: `BTC-USD`.
pub fn market_symbol(base: &str, quote: &str) -> String {
    format!("{base}-{quote}")
}

/// Splits a market name Kraken no longer lists (a delisted pair still shows up in
/// the trade history) into its two asset codes, by recognising the quote at its end.
pub fn split_unlisted_pair(name: &str) -> Option<(String, String)> {
    const QUOTES: [&str; 22] = [
        "ZUSD", "ZEUR", "ZGBP", "ZCAD", "ZJPY", "ZAUD", "USDT", "USDC", "XXBT", "XETH", "XBT", "ETH", "USD", "EUR", "GBP", "CAD", "JPY", "AUD", "CHF", "DAI", "BTC", "DOT",
    ];
    QUOTES
        .iter()
        .filter(|quote| name.len() > quote.len() && name.ends_with(**quote))
        .max_by_key(|quote| quote.len())
        .map(|quote| (name[..name.len() - quote.len()].to_string(), quote.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alt(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }
    fn known(altnames: &BTreeMap<String, String>) -> BTreeSet<String> {
        altnames.values().cloned().collect()
    }

    #[test]
    fn the_old_prefixed_codes_and_kraken_own_tickers_become_the_usual_names() {
        let a = alt(&[("XXBT", "XBT"), ("XETH", "ETH"), ("ZUSD", "USD"), ("XXDG", "XDG"), ("SOL", "SOL"), ("ZEUR", "EUR")]);
        let k = known(&a);
        let n = |c: &str| normalize_asset(c, &a, &k);
        assert_eq!((n("XXBT"), n("XETH"), n("ZUSD"), n("XXDG"), n("SOL"), n("ZEUR")), ("BTC".into(), "ETH".into(), "USD".into(), "DOGE".into(), "SOL".into(), "EUR".into()));
    }

    #[test]
    fn modern_four_letter_assets_starting_with_x_or_z_keep_their_names() {
        // These are real assets, not old X/Z-prefixed codes: the asset list is what decides.
        let a = alt(&[("XAUT", "XAUT"), ("ZETA", "ZETA"), ("XION", "XION")]);
        let k = known(&a);
        assert_eq!(normalize_asset("XAUT", &a, &k), "XAUT");
        assert_eq!(normalize_asset("ZETA", &a, &k), "ZETA");
        assert_eq!(normalize_asset("XION", &a, &k), "XION");
    }

    #[test]
    fn staked_earning_bonded_and_held_balances_fold_into_their_asset() {
        let a = alt(&[("DOT", "DOT"), ("DOT.S", "DOT.S"), ("ATOM", "ATOM"), ("ATOM21.S", "ATOM21.S"), ("ETH2.S", "ETH2.S"), ("ETH", "ETH"), ("ZUSD.M", "USD.M"), ("ZUSD", "USD"), ("EUR.HOLD", "EUR.HOLD"), ("EUR", "EUR"), ("XXBT.M", "XBT.M"), ("XXBT", "XBT")]);
        let k = known(&a);
        let n = |c: &str| normalize_asset(c, &a, &k);
        assert_eq!((n("DOT.S"), n("ATOM21.S"), n("ETH2.S"), n("ZUSD.M"), n("EUR.HOLD"), n("XXBT.M")), ("DOT".into(), "ATOM".into(), "ETH".into(), "USD".into(), "EUR".into(), "BTC".into()));
        assert!(is_locked_wallet("DOT.S", &a) && is_locked_wallet("EUR.HOLD", &a));
        assert!(!is_locked_wallet("DOT", &a) && !is_locked_wallet("XXBT", &a));
    }

    #[test]
    fn a_wallet_with_an_unfamiliar_suffix_is_left_as_its_own_asset() {
        let a = alt(&[("ETH.INK", "ETH.INK")]);
        assert_eq!(normalize_asset("ETH.INK", &a, &known(&a)), "ETH.INK");
    }

    #[test]
    fn a_delisted_market_is_split_at_its_quote() {
        assert_eq!(split_unlisted_pair("XXBTZUSD"), Some(("XXBT".into(), "ZUSD".into())));
        assert_eq!(split_unlisted_pair("DOTUSDT"), Some(("DOT".into(), "USDT".into())));
        assert_eq!(split_unlisted_pair("XETHXXBT"), Some(("XETH".into(), "XXBT".into())));
        assert_eq!(split_unlisted_pair("USD"), None);
    }

    #[test]
    fn markets_are_named_base_dash_quote() {
        assert_eq!(market_symbol("BTC", "USD"), "BTC-USD");
    }
}
