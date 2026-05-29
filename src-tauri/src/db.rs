use rusqlite::{Connection, Result, params};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Shared handle to the single SQLite connection. Behind a Mutex because
/// rusqlite::Connection isn't Sync; the contents can be swapped out when the
/// user relocates their storage folder.
pub type Db = Arc<Mutex<Connection>>;

pub fn open(storage_dir: &Path) -> Result<Connection> {
    let db_path = crate::storage::index_db_path(storage_dir);
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_versions (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;

    let current: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_versions",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if current < 1 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS items (
                id         TEXT PRIMARY KEY,
                type       TEXT NOT NULL,
                url        TEXT,
                title      TEXT,
                text       TEXT,
                html       TEXT,
                file_path  TEXT,
                notes      TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tags (
                id   TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS item_tags (
                item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
                tag_id  TEXT REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (item_id, tag_id)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
                title, text, notes,
                content=items,
                content_rowid=rowid
            );

            CREATE TRIGGER IF NOT EXISTS items_fts_insert AFTER INSERT ON items BEGIN
                INSERT INTO items_fts(rowid, title, text, notes)
                VALUES (new.rowid, new.title, new.text, new.notes);
            END;

            CREATE TRIGGER IF NOT EXISTS items_fts_delete AFTER DELETE ON items BEGIN
                INSERT INTO items_fts(items_fts, rowid, title, text, notes)
                VALUES ('delete', old.rowid, old.title, old.text, old.notes);
            END;

            CREATE TRIGGER IF NOT EXISTS items_fts_update AFTER UPDATE ON items BEGIN
                INSERT INTO items_fts(items_fts, rowid, title, text, notes)
                VALUES ('delete', old.rowid, old.title, old.text, old.notes);
                INSERT INTO items_fts(rowid, title, text, notes)
                VALUES (new.rowid, new.title, new.text, new.notes);
            END;",
        )?;

        conn.execute(
            "INSERT INTO schema_versions (version, applied_at) VALUES (1, datetime('now'))",
            [],
        )?;
    }

    if current < 2 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            INSERT OR IGNORE INTO settings (key, value) VALUES ('hotkey', 'Meta+KeyE');",
        )?;
        conn.execute(
            "INSERT INTO schema_versions (version, applied_at) VALUES (2, datetime('now'))",
            [],
        )?;
    }

    if current < 3 {
        // Optional screenshot attached to any item.
        conn.execute_batch("ALTER TABLE items ADD COLUMN image_path TEXT;")?;
        conn.execute(
            "INSERT INTO schema_versions (version, applied_at) VALUES (3, datetime('now'))",
            [],
        )?;
    }

    if current < 4 {
        // Optional one-shot reminder (RFC3339 UTC); cleared once fired.
        conn.execute_batch("ALTER TABLE items ADD COLUMN remind_at TEXT;")?;
        conn.execute(
            "INSERT INTO schema_versions (version, applied_at) VALUES (4, datetime('now'))",
            [],
        )?;
    }

    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

/// Insert one item row into the index. The id, on-disk artifact path
/// (`file_path`) and timestamp are decided by the caller (which also wrote the
/// .md / file to the typed folder). Used both for live saves and for rebuilding
/// the index from the folders. `INSERT OR IGNORE` so a rebuild won't duplicate.
pub fn insert_item(conn: &Connection, id: &str, item: &SaveRequest, created_at: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO items (id, type, url, title, text, html, file_path, notes, image_path, remind_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
        params![
            id,
            item.item_type,
            item.url,
            item.title,
            item.text,
            item.html,
            item.file_path,
            item.notes,
            item.image_path,
            item.remind_at,
            created_at,
        ],
    )?;
    Ok(())
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct SaveRequest {
    #[serde(rename = "type")]
    pub item_type: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub html: Option<String>,
    pub file_path: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub image_path: Option<String>,
    #[serde(default)]
    pub remind_at: Option<String>,
}
