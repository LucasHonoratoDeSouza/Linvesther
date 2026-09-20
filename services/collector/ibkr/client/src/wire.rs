//! Parses Interactive Brokers' Flex Query report XML — documented at
//! <https://www.ibkrguides.com/reportingapi/> (IBKR's Flex Web Service).
//! Only the three sections this collector needs are modeled: daily NAV
//! (`EquitySummaryInBase`), deposits/withdrawals (`CashTransactions`),
//! and trades (`Trades`, for win rate only — no position-level detail
//! is parsed here, see the module-level limitation note in `client.rs`).
//!
//! **Not verified against a real account** (no IBKR account was
//! available while writing this) — built from IBKR's published Flex
//! Query XML reference and forum examples of a real report. The shapes
//! below match that documentation; a live account's exact field set
//! should be checked against it before trusting this in production.
use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum WireError {
    #[error("could not parse the Flex report XML: {0}")]
    Xml(String),
    #[error("the report has no FlexStatement (nothing was generated for this query)")]
    NoStatement,
}

#[derive(Debug, Deserialize)]
struct SendRequestResponse {
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "ReferenceCode")]
    reference_code: Option<String>,
    #[serde(rename = "ErrorCode")]
    error_code: Option<String>,
    #[serde(rename = "ErrorMessage")]
    error_message: Option<String>,
}

/// Either the reference code to poll `GetStatement` with, or the exact
/// failure IBKR reported (invalid token/query, disabled Flex Web
/// Service, etc — surfaced to the person verbatim, never guessed at).
#[derive(Debug)]
pub enum SendRequestOutcome {
    Accepted { reference_code: String },
    Rejected { code: String, message: String },
}

pub fn parse_send_request(xml: &str) -> Result<SendRequestOutcome, WireError> {
    if !xml.trim_start().starts_with('<') {
        return Err(WireError::Xml(format!("Interactive Brokers returned: {}", xml.trim())));
    }
    let response: SendRequestResponse = quick_xml::de::from_str(xml).map_err(|e| WireError::Xml(e.to_string()))?;
    if response.status == "Success" {
        let reference_code = response.reference_code.ok_or_else(|| WireError::Xml("Success response missing ReferenceCode".to_string()))?;
        Ok(SendRequestOutcome::Accepted { reference_code })
    } else {
        Ok(SendRequestOutcome::Rejected {
            code: response.error_code.unwrap_or_default(),
            message: response.error_message.unwrap_or_else(|| "no error message given".to_string()),
        })
    }
}

/// IBKR's own "statement generation in progress" error code — the
/// caller should wait and retry `GetStatement`, not treat it as a
/// real failure.
pub const STILL_GENERATING_ERROR_CODE: &str = "1019";

#[derive(Debug, Deserialize)]
struct GetStatementError {
    #[serde(rename = "ErrorCode")]
    error_code: Option<String>,
    #[serde(rename = "ErrorMessage")]
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FlexQueryResponse {
    #[serde(rename = "FlexStatements")]
    flex_statements: FlexStatementsWrapper,
}

#[derive(Debug, Deserialize)]
struct FlexStatementsWrapper {
    #[serde(rename = "FlexStatement")]
    statement: Option<RawFlexStatement>,
}

#[derive(Debug, Deserialize)]
struct RawFlexStatement {
    #[serde(rename = "@accountId")]
    account_id: String,
    #[serde(rename = "EquitySummaryInBase")]
    equity_summary: Option<EquitySummaryWrapper>,
    #[serde(rename = "CashTransactions")]
    cash_transactions: Option<CashTransactionsWrapper>,
    #[serde(rename = "Trades")]
    trades: Option<TradesWrapper>,
}

#[derive(Debug, Deserialize)]
struct EquitySummaryWrapper {
    #[serde(rename = "EquitySummaryByReportDateInBase", default)]
    rows: Vec<RawEquityRow>,
}

#[derive(Debug, Deserialize)]
struct RawEquityRow {
    #[serde(rename = "@reportDate")]
    report_date: String,
    #[serde(rename = "@total")]
    total: Decimal,
}

#[derive(Debug, Deserialize)]
struct CashTransactionsWrapper {
    #[serde(rename = "CashTransaction", default)]
    rows: Vec<RawCashTransaction>,
}

#[derive(Debug, Deserialize)]
struct RawCashTransaction {
    #[serde(rename = "@reportDate")]
    report_date: String,
    #[serde(rename = "@amount")]
    amount: Decimal,
    #[serde(rename = "@currency")]
    currency: String,
    #[serde(rename = "@type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct TradesWrapper {
    #[serde(rename = "Trade", default)]
    rows: Vec<RawTrade>,
}

#[derive(Debug, Deserialize)]
struct RawTrade {
    #[serde(rename = "@tradeDate")]
    trade_date: String,
    #[serde(rename = "@symbol")]
    symbol: String,
    #[serde(rename = "@buySell")]
    buy_sell: String,
    #[serde(rename = "@quantity")]
    quantity: Decimal,
    #[serde(rename = "@tradePrice")]
    trade_price: Decimal,
}

/// One day's total account value in the report's base currency.
#[derive(Debug, Clone, PartialEq)]
pub struct EquityPoint {
    /// `YYYYMMDD`, as IBKR reports it.
    pub report_date: String,
    pub total: Decimal,
}

/// Money that entered or left the account without a trade explaining
/// it — the "type" filter here is exactly IBKR's own
/// `"Deposits/Withdrawals"` cash transaction type; dividends, interest
/// and fees are separate types and are not treated as flows.
#[derive(Debug, Clone, PartialEq)]
pub struct CashFlow {
    pub report_date: String,
    pub amount: Decimal,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeRecord {
    pub trade_date: String,
    pub symbol: String,
    pub is_buy: bool,
    pub quantity: Decimal,
    pub price: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlexStatement {
    pub account_id: String,
    pub equity: Vec<EquityPoint>,
    pub flows: Vec<CashFlow>,
    pub trades: Vec<TradeRecord>,
}

pub fn parse_get_statement(xml: &str) -> Result<Result<FlexStatement, (String, String)>, WireError> {
    if !xml.trim_start().starts_with('<') {
        return Err(WireError::Xml(format!("Interactive Brokers returned: {}", xml.trim())));
    }
    if xml.contains("<FlexStatementResponse") {
        let error: GetStatementError = quick_xml::de::from_str(xml).map_err(|e| WireError::Xml(e.to_string()))?;
        return Ok(Err((error.error_code.unwrap_or_default(), error.error_message.unwrap_or_else(|| "unknown error".to_string()))));
    }

    let response: FlexQueryResponse = quick_xml::de::from_str(xml).map_err(|e| WireError::Xml(e.to_string()))?;
    let raw = response.flex_statements.statement.ok_or(WireError::NoStatement)?;

    const DEPOSITS_WITHDRAWALS: &str = "Deposits/Withdrawals";
    Ok(Ok(FlexStatement {
        account_id: raw.account_id,
        equity: raw
            .equity_summary
            .map(|w| w.rows)
            .unwrap_or_default()
            .into_iter()
            .map(|row| EquityPoint { report_date: row.report_date, total: row.total })
            .collect(),
        flows: raw
            .cash_transactions
            .map(|w| w.rows)
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.kind == DEPOSITS_WITHDRAWALS)
            .map(|row| CashFlow { report_date: row.report_date, amount: row.amount, currency: row.currency })
            .collect(),
        trades: raw
            .trades
            .map(|w| w.rows)
            .unwrap_or_default()
            .into_iter()
            .map(|row| TradeRecord { trade_date: row.trade_date, symbol: row.symbol, is_buy: row.buy_sell == "BUY", quantity: row.quantity, price: row.trade_price })
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn a_successful_send_request_gives_the_reference_code_to_poll_with() {
        let xml = r#"<FlexStatementResponse timestamp='01 January, 2026 12:00 PM EST'>
<Status>Success</Status>
<ReferenceCode>1234567890</ReferenceCode>
<Url>https://ndcdyn.interactivebrokers.com/AccountManagement/FlexWebService/GetStatement</Url>
</FlexStatementResponse>"#;
        match parse_send_request(xml).unwrap() {
            SendRequestOutcome::Accepted { reference_code } => assert_eq!(reference_code, "1234567890"),
            SendRequestOutcome::Rejected { .. } => panic!("expected Accepted"),
        }
    }

    #[test]
    fn a_rejected_send_request_surfaces_ibkrs_own_error() {
        let xml = r#"<FlexStatementResponse timestamp='01 January, 2026 12:00 PM EST'>
<Status>Fail</Status>
<ErrorCode>1003</ErrorCode>
<ErrorMessage>Invalid request or unable to validate request</ErrorMessage>
</FlexStatementResponse>"#;
        match parse_send_request(xml).unwrap() {
            SendRequestOutcome::Rejected { code, message } => {
                assert_eq!(code, "1003");
                assert_eq!(message, "Invalid request or unable to validate request");
            }
            SendRequestOutcome::Accepted { .. } => panic!("expected Rejected"),
        }
    }

    #[test]
    fn still_generating_is_told_apart_from_a_real_failure() {
        let xml = r#"<FlexStatementResponse timestamp='01 January, 2026 12:00 PM EST'>
<Status>Warn</Status>
<ErrorCode>1019</ErrorCode>
<ErrorMessage>Statement generation in progress. Please try again shortly.</ErrorMessage>
</FlexStatementResponse>"#;
        match parse_get_statement(xml).unwrap() {
            Err((code, _)) => assert_eq!(code, STILL_GENERATING_ERROR_CODE),
            Ok(_) => panic!("expected an error"),
        }
    }

    #[test]
    fn a_real_statement_gives_equity_flows_and_trades() {
        let xml = r#"<FlexQueryResponse queryName="Linvesther" type="AF">
<FlexStatements count="1">
<FlexStatement accountId="U1234567" fromDate="20260101" toDate="20260110" period="Custom" whenGenerated="20260110;120000">
<EquitySummaryInBase>
<EquitySummaryByReportDateInBase accountId="U1234567" reportDate="20260101" cash="1000" stock="9000" total="10000" />
<EquitySummaryByReportDateInBase accountId="U1234567" reportDate="20260102" cash="1000" stock="9100" total="10100" />
</EquitySummaryInBase>
<CashTransactions>
<CashTransaction accountId="U1234567" currency="USD" amount="500" type="Deposits/Withdrawals" reportDate="20260103" dateTime="20260103;093000" />
<CashTransaction accountId="U1234567" currency="USD" amount="-2.50" type="Broker Interest Paid" reportDate="20260103" dateTime="20260103;093000" />
</CashTransactions>
<Trades>
<Trade accountId="U1234567" symbol="AAPL" buySell="BUY" quantity="10" tradePrice="150.25" tradeDate="20260102" currency="USD" />
<Trade accountId="U1234567" symbol="AAPL" buySell="SELL" quantity="5" tradePrice="155.00" tradeDate="20260105" currency="USD" />
</Trades>
</FlexStatement>
</FlexStatements>
</FlexQueryResponse>"#;
        let statement = parse_get_statement(xml).unwrap().unwrap();
        assert_eq!(statement.account_id, "U1234567");
        assert_eq!(statement.equity, vec![
            EquityPoint { report_date: "20260101".to_string(), total: dec!(10000) },
            EquityPoint { report_date: "20260102".to_string(), total: dec!(10100) },
        ]);
        // Only the Deposits/Withdrawals cash transaction counts as a flow —
        // interest is excluded.
        assert_eq!(statement.flows, vec![CashFlow { report_date: "20260103".to_string(), amount: dec!(500), currency: "USD".to_string() }]);
        assert_eq!(statement.trades.len(), 2);
        assert!(statement.trades[0].is_buy);
        assert!(!statement.trades[1].is_buy);
    }

    #[test]
    fn a_plain_text_error_page_is_a_clear_error_not_a_confusing_deserialize_failure() {
        let error = parse_send_request("Error 403 - Access Denied").unwrap_err();
        assert!(matches!(error, WireError::Xml(msg) if msg.contains("Error 403 - Access Denied")));
    }

    #[test]
    fn a_report_with_no_statement_at_all_is_a_typed_error_not_a_panic() {
        let xml = r#"<FlexQueryResponse queryName="Linvesther" type="AF"><FlexStatements count="0"></FlexStatements></FlexQueryResponse>"#;
        assert_eq!(parse_get_statement(xml).unwrap_err(), WireError::NoStatement);
    }
}
