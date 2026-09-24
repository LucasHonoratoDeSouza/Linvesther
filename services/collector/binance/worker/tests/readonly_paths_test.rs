//! Structural guarantee: `series` and `performance` — the two worker
//! subcommands the read-only account API's `/mcp/account/series` and
//! `/mcp/account/performance` routes call — must never reach a
//! market-opening path that syncs. See `open_market_readonly`'s own doc
//! comment in `src/main.rs` for why: a token scoped to `read:account`
//! must never be able to write a trade, flow, snapshot, ledger entry or
//! `last_synced_at`, and `open_market`'s Coinbase/Kraken/wallet branches
//! do exactly that.
//!
//! Reading the source is the only way to test this without live
//! Coinbase/Kraken credentials and a Postgres connection kept open long
//! enough to prove nothing was written to it — and unlike a live test,
//! it also catches the fix being silently reverted by a future edit
//! that swaps `open_market_readonly` back for `open_market` in either
//! function, or that adds a third caller of the syncing path without
//! reading this file's own doc comment.

use std::fs;

fn source() -> String {
    fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs")).expect("read src/main.rs")
}

/// The text of one function's body: from its declaration to the closing
/// brace at the same nesting depth as its opening one. Panics if the
/// declaration is not found — a test that silently matched nothing would
/// be worse than one that fails loudly when the function is renamed.
fn function_body(source: &str, declaration: &str) -> String {
    let start = source.find(declaration).unwrap_or_else(|| panic!("`{declaration}` not found in src/main.rs"));
    let open = source[start..].find('{').expect("function has a body") + start;
    let mut depth = 0i32;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return source[open..=open + offset].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("`{declaration}`'s body never closes its opening brace");
}

#[test]
fn series_and_performance_open_their_market_client_read_only() {
    let source = source();
    for declaration in ["async fn run_series()", "async fn run_performance()"] {
        let body = function_body(&source, declaration);
        assert!(
            body.contains("open_market_readonly("),
            "{declaration} must open its market client via open_market_readonly, not open_market"
        );
        // "open_market(" is not a substring of "open_market_readonly(" —
        // the character right after "open_market" differs ('(' vs
        // '_') — so this only matches the syncing function.
        assert!(
            !body.contains("open_market("),
            "{declaration} must never call the syncing open_market — a read-only token must not write"
        );
    }
}

#[test]
fn the_readonly_market_path_never_syncs() {
    let body = function_body(&source(), "async fn open_market_readonly(");
    for write_call in ["coinbase::sync(", "kraken::sync(", "sync_wallet(", "kraken::verify_if_due("] {
        assert!(!body.contains(write_call), "open_market_readonly must never call {write_call} — that is exactly the write a read-only token must not cause");
    }
}
