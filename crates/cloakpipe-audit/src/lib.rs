//! CloakPipe Audit — structured audit logging.
//!
//! Audit entries contain metadata only: counts, categories, request IDs, surface,
//! and optional user/session IDs. They never contain raw sensitive values.

pub mod sqlite;

use anyhow::{Context, Result};
use chrono::Utc;
use cloakpipe_core::config::AuditConfig;
use serde::Serialize;
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
#[derive(Debug, Serialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: String,
    pub event: AuditEvent,
    pub surface: String,
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities_detected: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities_replaced: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_rehydrated: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
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
