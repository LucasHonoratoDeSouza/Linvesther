//! Interactive Brokers' Flex Web Service: a read-only, no-login way to
//! pull a periodic report (trades, cash transactions, daily account
//! value) for an account, authorized by a token and a pre-built "Flex
//! Query" the account owner configures once in Account Management —
//! never a username/password/2FA session (a
//! read-only credential is preferred over anything that imitates a login).
//!
//! Fetching a report is two HTTP calls: `SendRequest` starts generating
//! it and returns a reference code, then `GetStatement` is polled with
//! that code until the report is ready (IBKR generates it
//! asynchronously, typically in a few seconds).
//!
//! **Not verified against a real account.** Built from IBKR's published
//! Flex Web Service reference; nothing here has been run against a live
//! statement. Treat the exact request/response shapes as a strong best
//! effort, not a confirmed fact, until checked against a real account.
use crate::wire::{parse_get_statement, parse_send_request, FlexStatement, SendRequestOutcome, STILL_GENERATING_ERROR_CODE};
use std::time::Duration;

const HOST: &str = "https://ndcdyn.interactivebrokers.com/AccountManagement/FlexWebService";
/// IBKR's Flex Web Service versions its response format; 3 is the
/// current documented one.
const VERSION: &str = "3";

#[derive(Debug, thiserror::Error)]
pub enum IbkrError {
    #[error("network error talking to Interactive Brokers: {0}")]
    Network(String),
    #[error("could not read the report: {0}")]
    Wire(#[from] crate::wire::WireError),
    #[error("Interactive Brokers rejected the Flex Query (code {code}): {message}")]
    Rejected { code: String, message: String },
    #[error("the report did not finish generating in time — try syncing again shortly")]
    TimedOut,
}

/// A Flex Query token and query ID, exactly as IBKR's Account Management
/// → Reporting → Flex Queries → "Flex Web Service Configuration" page
/// shows them. Both are read-only: they can only ever produce the
/// report the query was configured to include, never place an order or
/// move money.
#[derive(Clone)]
pub struct Credentials {
    token: String,
    query_id: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials").field("query_id", &self.query_id).finish_non_exhaustive()
    }
}

impl Credentials {
    pub fn new(token: &str, query_id: &str) -> Result<Self, IbkrError> {
        let token = token.trim().to_string();
        let query_id = query_id.trim().to_string();
        if token.is_empty() || query_id.is_empty() {
            return Err(IbkrError::Rejected { code: "0".to_string(), message: "both the token and the query ID are required".to_string() });
        }
        Ok(Credentials { token, query_id })
    }
}

pub struct IbkrClient {
    credentials: Credentials,
    http: reqwest::blocking::Client,
    /// How many times to poll `GetStatement` while IBKR is still
    /// generating the report, and how long to wait between polls —
    /// overridable only by tests, so the real client always waits the
    /// same real few seconds a report actually takes.
    poll_attempts: u32,
    poll_delay: Duration,
}

impl IbkrClient {
    pub fn new(credentials: Credentials) -> Self {
        IbkrClient {
            credentials,
            http: reqwest::blocking::Client::builder().timeout(Duration::from_secs(30)).build().expect("TLS backend available"),
            poll_attempts: 10,
            poll_delay: Duration::from_secs(3),
        }
    }

    /// Fetches the account's report right now. Blocking HTTP — callers
    /// run this under `spawn_blocking`.
    pub fn fetch_report(&self) -> Result<FlexStatement, IbkrError> {
        let reference_code = self.send_request()?;
        self.poll_statement(&reference_code)
    }

    fn send_request(&self) -> Result<String, IbkrError> {
        let url = format!("{HOST}/SendRequest?t={}&q={}&v={VERSION}", self.credentials.token, self.credentials.query_id);
        let body = self.http.get(&url).send().map_err(network_error)?.text().map_err(network_error)?;
        match parse_send_request(&body)? {
            SendRequestOutcome::Accepted { reference_code } => Ok(reference_code),
            SendRequestOutcome::Rejected { code, message } => Err(IbkrError::Rejected { code, message }),
        }
    }

    fn poll_statement(&self, reference_code: &str) -> Result<FlexStatement, IbkrError> {
        let url = format!("{HOST}/GetStatement?q={reference_code}&t={}&v={VERSION}", self.credentials.token);
        for attempt in 0..self.poll_attempts {
            if attempt > 0 {
                std::thread::sleep(self.poll_delay);
            }
            let body = self.http.get(&url).send().map_err(network_error)?.text().map_err(network_error)?;
            match parse_get_statement(&body)? {
                Ok(statement) => return Ok(statement),
                Err((code, _)) if code == STILL_GENERATING_ERROR_CODE => continue,
                Err((code, message)) => return Err(IbkrError::Rejected { code, message }),
            }
        }
        Err(IbkrError::TimedOut)
    }
}

/// A failed request must never carry its own URL into an error message: the
/// URL holds the Flex Web Service token, and the message is shown to the
/// person and written to logs.
fn network_error(error: reqwest::Error) -> IbkrError {
    IbkrError::Network(error.without_url().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_request_does_not_repeat_the_token_in_its_error() {
        let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(2)).build().unwrap();
        // Nothing listens on port 1, so this fails to connect.
        let error = client.get("http://127.0.0.1:1/SendRequest?t=SECRET-TOKEN&q=123").send().unwrap_err();
        assert!(error.to_string().contains("SECRET-TOKEN"), "premise: reqwest puts the URL in its own message");
        let message = network_error(error).to_string();
        assert!(!message.contains("SECRET-TOKEN"), "the token must not appear in: {message}");
    }
}
