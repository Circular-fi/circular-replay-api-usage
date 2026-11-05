//! HTTP client for the Circular Pro replay API (`/v2/replay/*`).
//!
//! Auth: `X-API-KEY` header. Responses use `{ success, data, errors?, meta }`.

use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_ENCODING};
use serde_json::Value;

/// Query options for `GET /v2/replay/tx/{signature}`.
#[derive(Clone, Debug)]
pub struct CircularReplayQuery {
    pub alts: bool,
    pub spl: bool,
    /// Full program bytecode in `programs_pre` / `programs_post` (LiteSVM needs this).
    pub program_details: bool,
    /// Up to 100 extra pubkeys (comma-separated in the query string).
    pub bonus_accounts: Vec<String>,
}

impl Default for CircularReplayQuery {
    fn default() -> Self {
        Self {
            alts: true,
            spl: true,
            program_details: true,
            bonus_accounts: Vec::new(),
        }
    }
}

/// Query options for `GET /v2/replay/slot/{slot}`.
#[derive(Clone, Debug)]
pub struct ReplaySlotQuery {
    pub alts: bool,
    pub spl: bool,
    pub program_details: bool,
    /// One to 100 base58 pubkeys.
    pub accounts: Vec<String>,
}

impl Default for ReplaySlotQuery {
    fn default() -> Self {
        Self {
            alts: true,
            spl: true,
            program_details: true,
            accounts: Vec::new(),
        }
    }
}

/// Client for Circular replay on `pro.circular.fi` (or another Pro gateway base URL).
#[derive(Clone, Debug)]
pub struct CircularReplayClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl CircularReplayClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("zstd, gzip"));

        let http = reqwest::Client::builder()
            .user_agent(concat!("circular-replay/", env!("CARGO_PKG_VERSION")))
            .default_headers(default_headers)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            http,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `GET /v2/replay/status` — replayable slot window.
    pub async fn status(&self) -> Result<Value> {
        self.get_v2("/v2/replay/status").await
    }

    /// `GET /v2/replay/tx/{signature}`.
    pub async fn replay(&self, signature: &str, query: &CircularReplayQuery) -> Result<Value> {
        let sig = signature.trim();
        let mut path = format!(
            "/v2/replay/tx/{sig}?alts={}&spl={}&program_details={}",
            query.alts, query.spl, query.program_details
        );
        append_csv_query(&mut path, "bonus_accounts", &query.bonus_accounts);
        self.get_v2(&path).await
    }

    /// `GET /v2/replay/slot/{slot}` — account states at the start of the slot.
    pub async fn replay_slot(&self, slot: u64, query: &ReplaySlotQuery) -> Result<Value> {
        let mut path = format!(
            "/v2/replay/slot/{slot}?alts={}&spl={}&program_details={}",
            query.alts, query.spl, query.program_details
        );
        append_csv_query(&mut path, "accounts", &query.accounts);
        self.get_v2(&path).await
    }

    /// `GET /v2/replay/slot/{slot}/entries` — signatures grouped by entry.
    pub async fn slot_entries(&self, slot: u64) -> Result<Value> {
        self.get_v2(&format!("/v2/replay/slot/{slot}/entries"))
            .await
    }

    async fn get_v2(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .header("X-API-KEY", &self.api_key)
            .send()
            .await
            .with_context(|| format!("GET {url} failed"))?;
        Self::parse_v2_envelope(resp, &url).await
    }

    async fn parse_v2_envelope(resp: reqwest::Response, url: &str) -> Result<Value> {
        let status = resp.status();
        let text = resp
            .text()
            .await
            .with_context(|| format!("failed to read response body from {url}"))?;

        let envelope: Value = serde_json::from_str(&text)
            .with_context(|| format!("response from {url} is not valid JSON"))?;

        if envelope.get("success").and_then(Value::as_bool) == Some(true) {
            return envelope
                .get("data")
                .cloned()
                .filter(|v| !v.is_null())
                .ok_or_else(|| anyhow!("{url}: success response missing `data`"));
        }

        let detail = envelope
            .get("errors")
            .and_then(|v| v.as_array())
            .map(|errs| {
                errs.iter()
                    .filter_map(|e| e.get("message").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .filter(|s| !s.is_empty())
            .or_else(|| {
                envelope
                    .pointer("/detail/message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| text.clone());
        Err(anyhow!("HTTP {} from {url}: {detail}", status.as_u16()))
    }
}

fn append_csv_query(path: &mut String, key: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    path.push('&');
    path.push_str(key);
    path.push('=');
    path.push_str(&values.join(","));
}
