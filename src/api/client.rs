//! HTTP client for the Circular Replay API.
//!
//! Endpoints:
//! - `GET  /status`                      — queryable slot window (no auth)
//! - `GET  /v1/replay/{signature}`       — pre/post state of a transaction
//! - `POST /v1/replay`                   — same, plus up to 100 bonus accounts
//! - `POST /v1/replay-slot`              — state of accounts at the start of a slot
//! - `POST /v1/slot-entries`             — signatures of a slot, grouped by entry
//!
//! Program bytecode is referenced by default (`programs=ref`); pass
//! [`ProgramsMode::Inline`] to embed the full programdata in the response (what
//! this crate's LiteSVM loader expects for a self-contained snapshot).

use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_ENCODING};
use serde_json::{json, Value};
use std::str::FromStr;

/// How program bytecode is returned by the `/replay*` endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramsMode {
    /// Bytecode by reference (`data_ref`); fetch misses via `GET /v1/programs/{id}`.
    Ref,
    /// Full programdata embedded in the response (~1–12 MB per call).
    Inline,
    /// No program data at all.
    None,
}

impl ProgramsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ProgramsMode::Ref => "ref",
            ProgramsMode::Inline => "inline",
            ProgramsMode::None => "none",
        }
    }
}

impl FromStr for ProgramsMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ref" => Ok(ProgramsMode::Ref),
            "inline" => Ok(ProgramsMode::Inline),
            "none" => Ok(ProgramsMode::None),
            other => Err(format!("invalid programs mode '{other}' (expected ref|inline|none)")),
        }
    }
}

/// Query options for `GET /v1/replay/{signature}`.
#[derive(Clone, Debug)]
pub struct CircularReplayQuery {
    pub alts: bool,
    pub spl: bool,
}

impl Default for CircularReplayQuery {
    fn default() -> Self {
        Self { alts: true, spl: true }
    }
}

/// Body for `POST /v1/replay` (transaction replay with optional bonus accounts).
#[derive(Clone, Debug)]
pub struct CircularReplayRequest {
    pub signature: String,
    /// Optional hint: the slot containing the transaction.
    pub slot: Option<u64>,
    /// Up to 100 extra pubkeys captured just before the transaction.
    pub bonus_accounts: Vec<String>,
    pub alt_details: bool,
    pub spl_details: bool,
    pub programs: ProgramsMode,
}

impl CircularReplayRequest {
    /// Inline programs by default so the snapshot is self-contained (reusable by
    /// `compare_rpc -f …` and the LiteSVM loader without `/v1/programs` hydration).
    pub fn new(signature: impl Into<String>) -> Self {
        Self {
            signature: signature.into(),
            slot: None,
            bonus_accounts: Vec::new(),
            alt_details: true,
            spl_details: true,
            programs: ProgramsMode::Inline,
        }
    }

    pub fn with_slot(mut self, slot: u64) -> Self {
        self.slot = Some(slot);
        self
    }

    pub fn with_bonus_accounts(mut self, accounts: Vec<String>) -> Self {
        self.bonus_accounts = accounts;
        self
    }

    pub fn with_programs(mut self, programs: ProgramsMode) -> Self {
        self.programs = programs;
        self
    }

    fn to_body(&self) -> Value {
        let mut body = json!({
            "signature": self.signature.trim(),
            "bonus_accounts": self.bonus_accounts,
            "alt_details": self.alt_details,
            "spl_details": self.spl_details,
            "programs": self.programs.as_str(),
        });
        if let Some(slot) = self.slot {
            body["slot"] = json!(slot);
        }
        body
    }
}

/// Body for `POST /v1/replay-slot` (account states at the start of a slot).
#[derive(Clone, Debug)]
pub struct ReplaySlotRequest {
    pub slot: u64,
    pub accounts: Vec<String>,
    pub alt_details: bool,
    pub spl_details: bool,
    pub programs: ProgramsMode,
}

impl ReplaySlotRequest {
    pub fn new(slot: u64, accounts: Vec<String>) -> Self {
        Self {
            slot,
            accounts,
            alt_details: true,
            spl_details: true,
            programs: ProgramsMode::Inline,
        }
    }

    pub fn with_programs(mut self, programs: ProgramsMode) -> Self {
        self.programs = programs;
        self
    }

    fn to_body(&self) -> Value {
        json!({
            "slot": self.slot,
            "accounts": self.accounts,
            "alt_details": self.alt_details,
            "spl_details": self.spl_details,
            "programs": self.programs.as_str(),
        })
    }
}

/// Body for `POST /v1/slot-entries` (signatures of a slot, grouped by entry).
#[derive(Clone, Debug)]
pub struct SlotEntriesRequest {
    pub slot: u64,
}

impl SlotEntriesRequest {
    fn to_body(&self) -> Value {
        json!({ "slot": self.slot })
    }
}

/// Client for Circular's replay service.
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

    /// `GET /status` — queryable slot window. Public route, no API key required.
    pub async fn status(&self) -> Result<Value> {
        let url = format!("{}/status", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url} failed"))?;
        Self::parse_json_response(resp, &url).await
    }

    /// `GET /v1/replay/{signature}` (programs embedded inline).
    pub async fn replay(&self, signature: &str, query: &CircularReplayQuery) -> Result<Value> {
        let sig = signature.trim();
        let url = format!(
            "{}/v1/replay/{}?alts={}&spl={}&programs=inline",
            self.base_url, sig, query.alts, query.spl
        );
        let resp = self
            .http
            .get(&url)
            .header("X-Api-Key", &self.api_key)
            .send()
            .await
            .with_context(|| format!("GET {url} failed"))?;
        Self::parse_json_response(resp, &url).await
    }

    /// `POST /v1/replay` — replay with up to 100 bonus accounts.
    pub async fn replay_with_accounts(&self, req: &CircularReplayRequest) -> Result<Value> {
        self.post_json("/v1/replay", &req.to_body()).await
    }

    /// `POST /v1/replay-slot` — account states at the start of `req.slot`.
    pub async fn replay_slot(&self, req: &ReplaySlotRequest) -> Result<Value> {
        self.post_json("/v1/replay-slot", &req.to_body()).await
    }

    /// `POST /v1/slot-entries` — transaction signatures of a slot, grouped by entry.
    pub async fn slot_entries(&self, req: &SlotEntriesRequest) -> Result<Value> {
        self.post_json("/v1/slot-entries", &req.to_body()).await
    }

    async fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {url} failed"))?;
        Self::parse_json_response(resp, &url).await
    }

    async fn parse_json_response(resp: reqwest::Response, url: &str) -> Result<Value> {
        let status = resp.status();
        let text = resp
            .text()
            .await
            .with_context(|| format!("failed to read response body from {url}"))?;

        if status.is_success() {
            return serde_json::from_str(&text)
                .with_context(|| format!("response from {url} is not valid JSON"));
        }

        let detail = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("error").or_else(|| v.get("message")).cloned())
            .map(|v| v.to_string())
            .unwrap_or(text);
        Err(anyhow!("HTTP {} from {url}: {detail}", status.as_u16()))
    }
}
