//! CloakPipe Audit — structured audit logging.
//!
//! Audit entries contain metadata only: counts, categories, request IDs, surface,
//! and optional user/session IDs. They never contain raw sensitive values.

pub mod sqlite;

use anyhow::{Context, Result};
use chrono::Utc;
use cloakpipe_core::config::AuditConfig;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Audit logger that appends JSONL entries to a log directory.
#[derive(Clone)]
pub struct AuditLogger {
    log_dir: String,
    log_entities: bool,
}

/// Shared audit sink used by proxy, direct API, and MCP surfaces.
#[derive(Clone)]
pub struct AuditSink {
    inner: Arc<AuditSinkInner>,
}

enum AuditSinkInner {
    Disabled,
    Jsonl(AuditLogger),
    Sqlite(Mutex<sqlite::SqliteAuditLogger>),
}

/// Context attached to every audit entry.
#[derive(Debug, Clone, Copy)]
pub struct AuditContext<'a> {
    pub surface: &'a str,
    pub request_id: &'a str,
    pub user_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
}

impl<'a> AuditContext<'a> {
    pub fn new(surface: &'a str, request_id: &'a str) -> Self {
        Self {
            surface,
            request_id,
            user_id: None,
            session_id: None,
        }
    }

    pub fn with_session(mut self, session_id: Option<&'a str>) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_user(mut self, user_id: Option<&'a str>) -> Self {
        self.user_id = user_id;
        self
    }
}

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: String,
    pub event: AuditEvent,
    #[serde(default = "default_surface")]
    pub surface: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entities_detected: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entities_replaced: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_rehydrated: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn default_surface() -> String {
    "unknown".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvent {
    Pseudonymize,
    Rehydrate,
    Detect,
    VaultStats,
    Configure,
    SessionContext,
    VaultSave,
    VaultLoad,
    ProxyRequest,
    Error,
}

impl AuditEvent {
    /// Stable snake_case name used in storage and APIs.
    pub fn name(&self) -> &'static str {
        match self {
            AuditEvent::Pseudonymize => "pseudonymize",
            AuditEvent::Rehydrate => "rehydrate",
            AuditEvent::Detect => "detect",
            AuditEvent::VaultStats => "vault_stats",
            AuditEvent::Configure => "configure",
            AuditEvent::SessionContext => "session_context",
            AuditEvent::VaultSave => "vault_save",
            AuditEvent::VaultLoad => "vault_load",
            AuditEvent::ProxyRequest => "proxy_request",
            AuditEvent::Error => "error",
        }
    }
}

impl AuditLogger {
    pub fn new(log_dir: &str, log_entities: bool) -> Result<Self> {
        fs::create_dir_all(log_dir).context("Failed to create audit log directory")?;
        Ok(Self {
            log_dir: log_dir.to_string(),
            log_entities,
        })
    }

    /// Log a pseudonymization event.
    pub fn log_pseudonymize(
        &self,
        request_id: &str,
        entities_detected: usize,
        entities_replaced: usize,
        categories: Vec<String>,
    ) -> Result<()> {
        let ctx = AuditContext::new("unknown", request_id);
        self.log_pseudonymize_with_context(ctx, entities_detected, entities_replaced, categories)
    }

    pub fn log_pseudonymize_with_context(
        &self,
        ctx: AuditContext<'_>,
        entities_detected: usize,
        entities_replaced: usize,
        categories: Vec<String>,
    ) -> Result<()> {
        let entry = build_entry(
            ctx,
            AuditEvent::Pseudonymize,
            self.log_entities,
            Some(entities_detected),
            Some(entities_replaced),
            None,
            categories,
            None,
        );
        self.log_entry(&entry)
    }

    /// Log a rehydration event.
    pub fn log_rehydrate(&self, request_id: &str, tokens_rehydrated: usize) -> Result<()> {
        let ctx = AuditContext::new("unknown", request_id);
        self.log_rehydrate_with_context(ctx, tokens_rehydrated)
    }

    pub fn log_rehydrate_with_context(
        &self,
        ctx: AuditContext<'_>,
        tokens_rehydrated: usize,
    ) -> Result<()> {
        let entry = build_entry(
            ctx,
            AuditEvent::Rehydrate,
            self.log_entities,
            None,
            None,
            Some(tokens_rehydrated),
            Vec::new(),
            None,
        );
        self.log_entry(&entry)
    }

    /// Log an error event.
    pub fn log_error(&self, request_id: &str, error: &str) -> Result<()> {
        let ctx = AuditContext::new("unknown", request_id);
        self.log_error_with_context(ctx, error)
    }

    pub fn log_error_with_context(&self, ctx: AuditContext<'_>, error: &str) -> Result<()> {
        let entry = build_entry(
            ctx,
            AuditEvent::Error,
            self.log_entities,
            None,
            None,
            None,
            Vec::new(),
            Some(error.to_string()),
        );
        self.log_entry(&entry)
    }

    pub fn log_event_with_context(
        &self,
        ctx: AuditContext<'_>,
        event: AuditEvent,
        entities_detected: Option<usize>,
        categories: Vec<String>,
    ) -> Result<()> {
        let entry = build_entry(
            ctx,
            event,
            self.log_entities,
            entities_detected,
            None,
            None,
            categories,
            None,
        );
        self.log_entry(&entry)
    }

    pub fn log_entry(&self, entry: &AuditEntry) -> Result<()> {
        let date = Utc::now().format("%Y-%m-%d");
        let path = format!("{}/audit-{}.jsonl", self.log_dir, date);
        let mut line = serde_json::to_string(entry)?;
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open audit log: {}", path))?;
        file.write_all(line.as_bytes())?;
        Ok(())
    }

    /// Get the log directory path.
    pub fn log_dir(&self) -> &str {
        &self.log_dir
    }

    pub fn logs_entities(&self) -> bool {
        self.log_entities
    }
}

impl AuditSink {
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(AuditSinkInner::Disabled),
        }
    }

    pub fn from_config(config: &AuditConfig) -> Result<Self> {
        if !config.enabled {
            return Ok(Self::disabled());
        }

        match config.backend.to_ascii_lowercase().as_str() {
            "sqlite" => {
                let path = sqlite_path(&config.log_path);
                let logger = sqlite::SqliteAuditLogger::open(
                    &path.to_string_lossy(),
                    config.log_entities,
                    config.retention_days,
                )?;
                Ok(Self {
                    inner: Arc::new(AuditSinkInner::Sqlite(Mutex::new(logger))),
                })
            }
            _ => Ok(Self {
                inner: Arc::new(AuditSinkInner::Jsonl(AuditLogger::new(
                    &config.log_path,
                    config.log_entities,
                )?)),
            }),
        }
    }

    pub fn log_pseudonymize(
        &self,
        ctx: AuditContext<'_>,
        entities_detected: usize,
        entities_replaced: usize,
        categories: Vec<String>,
    ) -> Result<()> {
        self.log_event(
            ctx,
            AuditEvent::Pseudonymize,
            Some(entities_detected),
            Some(entities_replaced),
            None,
            categories,
            None,
        )
    }

    pub fn log_rehydrate(&self, ctx: AuditContext<'_>, tokens_rehydrated: usize) -> Result<()> {
        self.log_event(
            ctx,
            AuditEvent::Rehydrate,
            None,
            None,
            Some(tokens_rehydrated),
            Vec::new(),
            None,
        )
    }

    pub fn log_metadata(
        &self,
        ctx: AuditContext<'_>,
        event: AuditEvent,
        entities_detected: Option<usize>,
        categories: Vec<String>,
    ) -> Result<()> {
        self.log_event(ctx, event, entities_detected, None, None, categories, None)
    }

    pub fn log_error(&self, ctx: AuditContext<'_>, error: &str) -> Result<()> {
        self.log_event(
            ctx,
            AuditEvent::Error,
            None,
            None,
            None,
            Vec::new(),
            Some(error.to_string()),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "audit logging forwards optional counters and metadata to a single entry builder"
    )]
    fn log_event(
        &self,
        ctx: AuditContext<'_>,
        event: AuditEvent,
        entities_detected: Option<usize>,
        entities_replaced: Option<usize>,
        tokens_rehydrated: Option<usize>,
        categories: Vec<String>,
        error: Option<String>,
    ) -> Result<()> {
        match self.inner.as_ref() {
            AuditSinkInner::Disabled => Ok(()),
            AuditSinkInner::Jsonl(logger) => {
                let entry = build_entry(
                    ctx,
                    event,
                    logger.logs_entities(),
                    entities_detected,
                    entities_replaced,
                    tokens_rehydrated,
                    categories,
                    error,
                );
                logger.log_entry(&entry)
            }
            AuditSinkInner::Sqlite(logger) => {
                let logger = logger.lock().expect("audit sqlite logger lock poisoned");
                let entry = build_entry(
                    ctx,
                    event,
                    logger.logs_entities(),
                    entities_detected,
                    entities_replaced,
                    tokens_rehydrated,
                    categories,
                    error,
                );
                logger.log_entry(&entry)
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "audit entries are constructed from several optional counters plus shared request metadata"
)]
fn build_entry(
    ctx: AuditContext<'_>,
    event: AuditEvent,
    log_entities: bool,
    entities_detected: Option<usize>,
    entities_replaced: Option<usize>,
    tokens_rehydrated: Option<usize>,
    categories: Vec<String>,
    error: Option<String>,
) -> AuditEntry {
    AuditEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        event,
        surface: ctx.surface.to_string(),
        request_id: Some(ctx.request_id.to_string()),
        user_id: ctx.user_id.map(str::to_string),
        session_id: ctx.session_id.map(str::to_string),
        entities_detected,
        entities_replaced,
        tokens_rehydrated,
        categories: if log_entities && !categories.is_empty() {
            Some(categories)
        } else {
            None
        },
        error,
    }
}

fn sqlite_path(path: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(path);
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("db") | Some("sqlite") | Some("sqlite3") => path,
        _ => path.join("audit.db"),
    }
}

/// Which storage backend an [`AuditSink`] is using.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditBackend {
    Disabled,
    Jsonl,
    Sqlite,
}

/// Filter parameters for querying persisted audit entries.
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    pub event: Option<String>,
    pub surface: Option<String>,
    pub session_id: Option<String>,
    /// Inclusive lower bound (RFC3339 timestamp string comparison).
    pub since: Option<String>,
    /// Inclusive upper bound (RFC3339 timestamp string comparison).
    pub until: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

impl AuditQuery {
    fn matches(&self, entry: &AuditEntry) -> bool {
        if let Some(event) = &self.event {
            if !event.is_empty() && entry.event.name() != event.to_ascii_lowercase() {
                return false;
            }
        }
        if let Some(surface) = &self.surface {
            if !surface.is_empty() && &entry.surface != surface {
                return false;
            }
        }
        if let Some(session) = &self.session_id {
            if !session.is_empty() && entry.session_id.as_deref() != Some(session.as_str()) {
                return false;
            }
        }
        if let Some(since) = &self.since {
            if !since.is_empty() && entry.timestamp.as_str() < since.as_str() {
                return false;
            }
        }
        if let Some(until) = &self.until {
            if !until.is_empty() && entry.timestamp.as_str() > until.as_str() {
                return false;
            }
        }
        true
    }
}

/// Result of an audit query, including total matched count for pagination.
#[derive(Debug, Clone, Serialize)]
pub struct AuditQueryResult {
    pub backend: AuditBackend,
    pub entries: Vec<AuditEntry>,
    pub total_matched: usize,
}

impl AuditSink {
    /// Return which backend this sink writes to.
    pub fn backend(&self) -> AuditBackend {
        match self.inner.as_ref() {
            AuditSinkInner::Disabled => AuditBackend::Disabled,
            AuditSinkInner::Jsonl(_) => AuditBackend::Jsonl,
            AuditSinkInner::Sqlite(_) => AuditBackend::Sqlite,
        }
    }

    /// Human-readable description of where audit entries are stored.
    pub fn location(&self) -> Option<String> {
        match self.inner.as_ref() {
            AuditSinkInner::Disabled => None,
            AuditSinkInner::Jsonl(logger) => Some(logger.log_dir().to_string()),
            AuditSinkInner::Sqlite(logger) => {
                Some(logger.lock().ok()?.db_path().unwrap_or_default())
            }
        }
    }

    /// Query persisted audit entries using the provided filter.
    ///
    /// Both SQLite and JSONL backends are supported. A disabled sink returns an
    /// empty result set with `backend = disabled` so callers can render a clear
    /// unsupported/empty state.
    pub fn query(&self, query: &AuditQuery) -> Result<AuditQueryResult> {
        match self.inner.as_ref() {
            AuditSinkInner::Disabled => Ok(AuditQueryResult {
                backend: AuditBackend::Disabled,
                entries: Vec::new(),
                total_matched: 0,
            }),
            AuditSinkInner::Sqlite(logger) => {
                let logger = logger.lock().expect("audit sqlite logger lock poisoned");
                let (entries, total) = logger.query(query)?;
                Ok(AuditQueryResult {
                    backend: AuditBackend::Sqlite,
                    entries,
                    total_matched: total,
                })
            }
            AuditSinkInner::Jsonl(logger) => {
                let (entries, total) = logger.query(query)?;
                Ok(AuditQueryResult {
                    backend: AuditBackend::Jsonl,
                    entries,
                    total_matched: total,
                })
            }
        }
    }

    /// Aggregate event counts across all persisted entries.
    pub fn summary(&self) -> Result<Vec<(String, usize)>> {
        match self.inner.as_ref() {
            AuditSinkInner::Disabled => Ok(Vec::new()),
            AuditSinkInner::Sqlite(logger) => logger
                .lock()
                .expect("audit sqlite logger lock poisoned")
                .event_counts(),
            AuditSinkInner::Jsonl(logger) => logger.event_counts(),
        }
    }
}

impl AuditLogger {
    /// Read and filter entries from the JSONL log directory.
    ///
    /// Returns `(page, total_matched)`. Entries are returned newest-first.
    pub fn query(&self, query: &AuditQuery) -> Result<(Vec<AuditEntry>, usize)> {
        let mut matched: Vec<AuditEntry> = Vec::new();
        let read_dir = match fs::read_dir(&self.log_dir) {
            Ok(rd) => rd,
            Err(_) => return Ok((Vec::new(), 0)),
        };
        let mut files: Vec<std::path::PathBuf> = read_dir
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().and_then(|e| e.to_str()) == Some("jsonl")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with("audit-"))
                        .unwrap_or(false)
            })
            .collect();
        files.sort();
        for path in files {
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(entry) = serde_json::from_str::<AuditEntry>(line) {
                    if query.matches(&entry) {
                        matched.push(entry);
                    }
                }
            }
        }
        matched.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        let total = matched.len();
        let page = matched
            .into_iter()
            .skip(query.offset)
            .take(if query.limit == 0 { total } else { query.limit })
            .collect();
        Ok((page, total))
    }

    /// Count events by type across all JSONL files.
    pub fn event_counts(&self) -> Result<Vec<(String, usize)>> {
        let (all, _) = self.query(&AuditQuery {
            limit: 0,
            ..Default::default()
        })?;
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for entry in all {
            *counts.entry(entry.event.name().to_string()).or_default() += 1;
        }
        let mut counts: Vec<(String, usize)> = counts.into_iter().collect();
        counts.sort_by_key(|b| std::cmp::Reverse(b.1));
        Ok(counts)
    }
}

#[cfg(test)]
mod jsonl_tests {
    use super::*;

    #[test]
    fn jsonl_sink_query_and_summary() {
        let dir = tempfile::tempdir().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_path: dir.path().to_string_lossy().to_string(),
            format: "jsonl".into(),
            retention_days: 90,
            log_entities: true,
            log_mappings: false,
            backend: "jsonl".into(),
        };
        let sink = AuditSink::from_config(&config).unwrap();
        assert_eq!(sink.backend(), AuditBackend::Jsonl);

        sink.log_pseudonymize(AuditContext::new("api", "r1"), 3, 3, vec!["PERSON".into()])
            .unwrap();
        sink.log_rehydrate(AuditContext::new("api", "r2"), 2)
            .unwrap();

        let result = sink
            .query(&AuditQuery {
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(result.total_matched, 2);
        assert_eq!(result.entries.len(), 2);

        let filtered = sink
            .query(&AuditQuery {
                event: Some("rehydrate".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(filtered.total_matched, 1);
        assert_eq!(filtered.entries[0].event, AuditEvent::Rehydrate);

        let summary = sink.summary().unwrap();
        assert_eq!(summary.iter().map(|(_, c)| c).sum::<usize>(), 2);
    }

    #[test]
    fn disabled_sink_query_is_empty() {
        let sink = AuditSink::disabled();
        assert_eq!(sink.backend(), AuditBackend::Disabled);
        let result = sink.query(&AuditQuery::default()).unwrap();
        assert!(result.entries.is_empty());
        assert_eq!(result.total_matched, 0);
    }
}
