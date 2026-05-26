//! SQLite-backed audit logger — replaces JSONL files.
//!
//! Benefits over JSONL:
//! - Queryable: filter by date, user, event type
//! - Automatic retention: DELETE WHERE timestamp < cutoff
//! - Concurrent-safe via WAL mode
//! - Single file instead of daily log rotation

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::{AuditEntry, AuditEvent};

/// SQLite-backed audit logger.
pub struct SqliteAuditLogger {
    conn: Connection,
    log_entities: bool,
    retention_days: u32,
}

impl SqliteAuditLogger {
    /// Open or create a SQLite audit database.
    pub fn open(path: &str, log_entities: bool, retention_days: u32) -> Result<Self> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open audit database: {}", path))?;

        conn.pragma_update(None, "journal_mode", "WAL")?;

        Self::init_schema(&conn)?;

        let logger = Self {
            conn,
            log_entities,
            retention_days,
        };

        // Clean up old entries on open
        logger.enforce_retention()?;

        Ok(logger)
    }

    /// Create an in-memory audit logger for testing.
    pub fn in_memory(log_entities: bool) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn,
            log_entities,
            retention_days: 90,
        })
    }

    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_log (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                event TEXT NOT NULL,
                surface TEXT NOT NULL DEFAULT 'unknown',
                request_id TEXT,
                user_id TEXT,
                session_id TEXT,
                entities_detected INTEGER,
                entities_replaced INTEGER,
                tokens_rehydrated INTEGER,
                categories TEXT,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
            CREATE INDEX IF NOT EXISTS idx_audit_event ON audit_log(event);
            CREATE INDEX IF NOT EXISTS idx_audit_surface ON audit_log(surface);
            CREATE INDEX IF NOT EXISTS idx_audit_user ON audit_log(user_id);
            CREATE INDEX IF NOT EXISTS idx_audit_session ON audit_log(session_id);",
        )
        .context("Failed to initialize audit schema")?;

        if !column_exists(conn, "audit_log", "surface")? {
            conn.execute(
                "ALTER TABLE audit_log ADD COLUMN surface TEXT NOT NULL DEFAULT 'unknown'",
                [],
            )?;
        }
        Ok(())
    }

    pub fn logs_entities(&self) -> bool {
        self.log_entities
    }

    pub fn log_entry(&self, entry: &AuditEntry) -> Result<()> {
        let cats = entry
            .categories
            .as_ref()
            .filter(|_| self.log_entities)
            .map(serde_json::to_string)
            .transpose()?;

        self.conn.execute(
            "INSERT INTO audit_log (id, timestamp, event, surface, request_id, user_id, session_id, entities_detected, entities_replaced, tokens_rehydrated, categories, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                entry.id.as_str(),
                entry.timestamp.as_str(),
                event_name(entry.event),
                entry.surface.as_str(),
                entry.request_id.as_deref(),
                entry.user_id.as_deref(),
                entry.session_id.as_deref(),
                entry.entities_detected.map(|n| n as i64),
                entry.entities_replaced.map(|n| n as i64),
                entry.tokens_rehydrated.map(|n| n as i64),
                cats.as_deref(),
                entry.error.as_deref(),
            ],
        )?;
        Ok(())
    }

    /// Log a pseudonymization event.
    pub fn log_pseudonymize(
        &self,
        request_id: &str,
        entities_detected: usize,
        entities_replaced: usize,
        categories: Vec<String>,
    ) -> Result<()> {
        self.log_pseudonymize_for_user(
            request_id,
            entities_detected,
            entities_replaced,
            categories,
            None,
            None,
        )
    }

    /// Log a pseudonymization event with user/session context.
    pub fn log_pseudonymize_for_user(
        &self,
        request_id: &str,
        entities_detected: usize,
        entities_replaced: usize,
        categories: Vec<String>,
        user_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<()> {
        let cats = if self.log_entities {
            Some(serde_json::to_string(&categories)?)
        } else {
            None
        };

        self.conn.execute(
            "INSERT INTO audit_log (id, timestamp, event, surface, request_id, user_id, session_id, entities_detected, entities_replaced, categories)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                Utc::now().to_rfc3339(),
                "pseudonymize",
                "unknown",
                request_id,
                user_id,
                session_id,
                entities_detected as i64,
                entities_replaced as i64,
                cats,
            ],
        )?;
        Ok(())
    }

    /// Log a rehydration event.
    pub fn log_rehydrate(&self, request_id: &str, tokens_rehydrated: usize) -> Result<()> {
        self.log_rehydrate_for_user(request_id, tokens_rehydrated, None, None)
    }

    /// Log a rehydration event with user/session context.
    pub fn log_rehydrate_for_user(
        &self,
        request_id: &str,
        tokens_rehydrated: usize,
        user_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit_log (id, timestamp, event, surface, request_id, user_id, session_id, tokens_rehydrated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                Uuid::new_v4().to_string(),
                Utc::now().to_rfc3339(),
                "rehydrate",
                "unknown",
                request_id,
                user_id,
                session_id,
                tokens_rehydrated as i64,
            ],
        )?;
        Ok(())
    }

    /// Log an error event.
    pub fn log_error(&self, request_id: &str, error: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit_log (id, timestamp, event, surface, request_id, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                Utc::now().to_rfc3339(),
                "error",
                "unknown",
                request_id,
                error,
            ],
        )?;
        Ok(())
    }

    /// Delete entries older than retention_days.
    fn enforce_retention(&self) -> Result<()> {
        if self.retention_days == 0 {
            return Ok(());
        }
        self.conn.execute(
            "DELETE FROM audit_log WHERE timestamp < datetime('now', ?1)",
            params![format!("-{} days", self.retention_days)],
        )?;
        Ok(())
    }

    /// Query recent audit entries.
    pub fn recent(&self, limit: usize) -> Result<Vec<AuditEntry>> {
        let mut stmt = self.conn.prepare(
              "SELECT id, timestamp, event, surface, request_id, user_id, session_id, entities_detected, entities_replaced, tokens_rehydrated, categories, error
             FROM audit_log ORDER BY timestamp DESC LIMIT ?1"
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            let event_str: String = row.get(2)?;
            let cats_json: Option<String> = row.get(10)?;

            Ok(AuditEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                event: match event_str.as_str() {
                    "pseudonymize" => AuditEvent::Pseudonymize,
                    "rehydrate" => AuditEvent::Rehydrate,
                    "detect" => AuditEvent::Detect,
                    "vault_stats" => AuditEvent::VaultStats,
                    "configure" => AuditEvent::Configure,
                    "session_context" => AuditEvent::SessionContext,
                    "vault_save" => AuditEvent::VaultSave,
                    "vault_load" => AuditEvent::VaultLoad,
                    "proxy_request" => AuditEvent::ProxyRequest,
                    "error" => AuditEvent::Error,
                    _ => AuditEvent::ProxyRequest,
                },
                surface: row.get(3)?,
                request_id: row.get(4)?,
                user_id: row.get(5)?,
                session_id: row.get(6)?,
                entities_detected: row.get::<_, Option<i64>>(7)?.map(|n| n as usize),
                entities_replaced: row.get::<_, Option<i64>>(8)?.map(|n| n as usize),
                tokens_rehydrated: row.get::<_, Option<i64>>(9)?.map(|n| n as usize),
                categories: cats_json.and_then(|j| serde_json::from_str(&j).ok()),
                error: row.get(11)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Count events by type.
    pub fn event_counts(&self) -> Result<Vec<(String, usize)>> {
        let mut stmt = self.conn.prepare(
            "SELECT event, COUNT(*) FROM audit_log GROUP BY event ORDER BY COUNT(*) DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let event: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((event, count as usize))
        })?;

        let mut counts = Vec::new();
        for row in rows {
            counts.push(row?);
        }
        Ok(counts)
    }

    /// Total entry count.
    pub fn total_entries(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn event_name(event: AuditEvent) -> &'static str {
    match event {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_audit_pseudonymize() {
        let logger = SqliteAuditLogger::in_memory(true).unwrap();
        logger
            .log_pseudonymize("req-1", 5, 3, vec!["PERSON".into(), "ORG".into()])
            .unwrap();

        let entries = logger.recent(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entities_detected, Some(5));
        assert_eq!(entries[0].entities_replaced, Some(3));
        assert!(entries[0].categories.is_some());
    }

    #[test]
    fn test_sqlite_audit_rehydrate() {
        let logger = SqliteAuditLogger::in_memory(true).unwrap();
        logger.log_rehydrate("req-1", 7).unwrap();

        let entries = logger.recent(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tokens_rehydrated, Some(7));
    }

    #[test]
    fn test_sqlite_audit_error() {
        let logger = SqliteAuditLogger::in_memory(true).unwrap();
        logger.log_error("req-1", "something went wrong").unwrap();

        let entries = logger.recent(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].error.as_deref(), Some("something went wrong"));
    }

    #[test]
    fn test_sqlite_audit_event_counts() {
        let logger = SqliteAuditLogger::in_memory(true).unwrap();
        logger.log_pseudonymize("r1", 1, 1, vec![]).unwrap();
        logger.log_pseudonymize("r2", 2, 2, vec![]).unwrap();
        logger.log_rehydrate("r3", 3).unwrap();

        let counts = logger.event_counts().unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(logger.total_entries().unwrap(), 3);
    }

    #[test]
    fn test_sqlite_audit_multi_user() {
        let logger = SqliteAuditLogger::in_memory(true).unwrap();
        logger
            .log_pseudonymize_for_user("r1", 3, 2, vec![], Some("user-1"), Some("sess-a"))
            .unwrap();
        logger
            .log_pseudonymize_for_user("r2", 1, 1, vec![], Some("user-2"), Some("sess-b"))
            .unwrap();

        assert_eq!(logger.total_entries().unwrap(), 2);
    }

    #[test]
    fn test_sqlite_audit_log_entry_preserves_surface_and_session() {
        let logger = SqliteAuditLogger::in_memory(true).unwrap();
        logger
            .log_entry(&AuditEntry {
                id: "entry-1".into(),
                timestamp: Utc::now().to_rfc3339(),
                event: AuditEvent::Detect,
                surface: "mcp".into(),
                request_id: Some("req-1".into()),
                user_id: Some("user-1".into()),
                session_id: Some("global".into()),
                entities_detected: Some(2),
                entities_replaced: None,
                tokens_rehydrated: None,
                categories: Some(vec!["Email".into()]),
                error: None,
            })
            .unwrap();

        let entries = logger.recent(10).unwrap();
        assert_eq!(entries[0].event, AuditEvent::Detect);
        assert_eq!(entries[0].surface, "mcp");
        assert_eq!(entries[0].session_id.as_deref(), Some("global"));
        assert_eq!(entries[0].user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn test_sqlite_audit_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("audit.db");
        let path_str = db_path.to_str().unwrap();

        {
            let logger = SqliteAuditLogger::open(path_str, true, 90).unwrap();
            logger
                .log_pseudonymize("r1", 5, 3, vec!["PERSON".into()])
                .unwrap();
            logger.log_rehydrate("r2", 4).unwrap();
        }

        {
            let logger = SqliteAuditLogger::open(path_str, true, 90).unwrap();
            assert_eq!(logger.total_entries().unwrap(), 2);
        }
    }
}
