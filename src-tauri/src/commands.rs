use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::db::{self, Db, SaveRequest};
use crate::storage::{self, StorageDir};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Item {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub html: Option<String>,
    pub file_path: Option<String>,
    pub notes: Option<String>,
    pub image_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub remind_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PopupData {
    pub url: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    #[serde(default)]
    pub html: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
    /// Pre-attached screenshot: a `data:image/png;base64,…` preview plus the
    /// temp file path that gets moved into Images/ on save.
    #[serde(default)]
    pub screenshot: Option<String>,
    #[serde(default)]
    pub screenshot_path: Option<String>,
}

/// Save a highlight or link: write a human-readable `.md` (the source of truth)
/// into Highlights/ or Links/, then index it in the DB. `file_path` records the
/// relative path to that .md so it survives a move and can be opened/rebuilt.
#[tauri::command]
pub fn cmd_save_item(
    app: AppHandle,
    db: tauri::State<Db>,
    storage: tauri::State<StorageDir>,
    mut req: SaveRequest,
    screenshot_path: Option<String>,
) -> Result<String, String> {
    let dir = storage.lock().map_err(|e| e.to_string())?.clone();
    storage::ensure_layout(&dir);

    let id = uuid::Uuid::new_v4().to_string();
    let created = chrono::Utc::now().to_rfc3339();

    // Move an attached screenshot from its temp file into Images/.
    if let Some(tmp) = screenshot_path.filter(|p| !p.trim().is_empty()) {
        let src = Path::new(&tmp);
        if src.is_file() {
            let imgs = storage::images_dir(&dir);
            std::fs::create_dir_all(&imgs).map_err(|e| e.to_string())?;
            let stem_src = req
                .title
                .clone()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| req.text.as_ref().map(|t| t.chars().take(40).collect()))
                .unwrap_or_else(|| "Screenshot".to_string());
            let dest = storage::unique_path(&imgs, &storage::sanitize_filename(&stem_src), "png");
            if std::fs::rename(src, &dest).is_err() {
                std::fs::copy(src, &dest).map_err(|e| e.to_string())?;
                let _ = std::fs::remove_file(src);
            }
            let name = dest
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("shot.png");
            req.image_path = Some(format!("Images/{name}"));
        }
    }

    let has_text = req.text.as_deref().is_some_and(|s| !s.trim().is_empty());
    let has_url = req.url.as_deref().is_some_and(|s| !s.trim().is_empty());

    if !has_text && !has_url && req.image_path.is_some() {
        // Screenshot-only item: the PNG is the artifact, no .md.
        req.item_type = "image".to_string();
        req.file_path = req.image_path.clone();
    } else {
        // Highlight / link → write the .md (which references the image, if any).
        let rel = write_markdown(&dir, &id, &req, &created)?;
        req.file_path = Some(rel);
    }

    {
        let conn = db.lock().map_err(|e| e.to_string())?;
        db::insert_item(&conn, &id, &req, &created).map_err(|e| e.to_string())?;
    }
    // Tell the main window to refresh its list.
    let _ = app.emit("item-saved", &id);
    Ok(id)
}

/// Copy one or more files into Files/ (originals, real names) and index each.
/// `url` holds the original source path; `file_path` holds the stored copy
/// relative to the storage folder, so it survives a move.
#[tauri::command]
pub fn cmd_save_files(
    app: AppHandle,
    db: tauri::State<Db>,
    storage: tauri::State<StorageDir>,
    paths: Vec<String>,
    notes: Option<String>,
) -> Result<Vec<String>, String> {
    let dir = storage.lock().map_err(|e| e.to_string())?.clone();
    let files_dir = storage::files_dir(&dir);
    std::fs::create_dir_all(&files_dir).map_err(|e| e.to_string())?;

    let notes = notes.filter(|s| !s.trim().is_empty());
    let mut ids = Vec::new();
    {
        let conn = db.lock().map_err(|e| e.to_string())?;
        for path in &paths {
            let src = Path::new(path);
            if !src.is_file() {
                continue;
            }
            let fname = src
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
                .to_string();
            // Keep the real name; only disambiguate on collision.
            let (stem, ext) = split_name(&fname);
            let dest = storage::unique_path(&files_dir, &stem, &ext);
            std::fs::copy(src, &dest).map_err(|e| e.to_string())?;
            let stored_name = dest
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&fname)
                .to_string();

            let id = uuid::Uuid::new_v4().to_string();
            let created = chrono::Utc::now().to_rfc3339();
            let req = SaveRequest {
                item_type: "file".to_string(),
                url: Some(src.to_string_lossy().to_string()),
                title: Some(fname),
                text: None,
                html: None,
                file_path: Some(format!("Files/{stored_name}")),
                notes: notes.clone(),
                image_path: None,
                remind_at: None,
            };
            db::insert_item(&conn, &id, &req, &created).map_err(|e| e.to_string())?;
            ids.push(id);
        }
    }
    if !ids.is_empty() {
        let _ = app.emit("item-saved", &ids);
    }
    Ok(ids)
}

/// Write a highlight/link as a Markdown file with YAML-ish front matter and
/// return its path relative to the storage folder.
fn write_markdown(
    dir: &Path,
    id: &str,
    req: &SaveRequest,
    created: &str,
) -> Result<String, String> {
    let folder = if req.item_type == "link" {
        storage::links_dir(dir)
    } else {
        storage::highlights_dir(dir)
    };
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;

    let stem_src = req
        .title
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| req.text.clone())
        .or_else(|| req.url.clone())
        .unwrap_or_else(|| "untitled".to_string());
    let stem = storage::sanitize_filename(&stem_src);
    let path = storage::unique_path(&folder, &stem, "md");

    let mut out = String::from("---\n");
    out.push_str(&format!("id: {id}\n"));
    out.push_str(&format!("type: {}\n", req.item_type));
    if let Some(t) = req.title.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("title: {}\n", t.replace('\n', " ")));
    }
    if let Some(u) = req.url.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("source: {u}\n"));
    }
    if let Some(img) = req.image_path.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("image: {img}\n"));
    }
    if let Some(r) = req.remind_at.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("remind: {r}\n"));
    }
    out.push_str(&format!("created: {created}\n"));
    out.push_str("---\n\n");
    if let Some(text) = req.text.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(text);
        out.push('\n');
    }
    if let Some(img) = req.image_path.as_deref().filter(|s| !s.trim().is_empty()) {
        // Relative from Highlights|Links/ up to Images/.
        out.push_str(&format!("\n![screenshot](../{img})\n"));
    }
    if let Some(note) = req.notes.as_deref().filter(|s| !s.trim().is_empty()) {
        out.push_str(&format!("\n> Note: {note}\n"));
    }

    std::fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(path
        .strip_prefix(dir)
        .unwrap_or(&path)
        .to_string_lossy()
        .to_string())
}

/// Split "name.ext" into ("name", "ext"); ("name", "") if no extension.
fn split_name(fname: &str) -> (String, String) {
    match fname.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            (storage::sanitize_filename(stem), ext.to_string())
        }
        _ => (storage::sanitize_filename(fname), String::new()),
    }
}

#[tauri::command]
pub fn cmd_get_storage_dir(storage: tauri::State<StorageDir>) -> Result<String, String> {
    Ok(storage
        .lock()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string())
}

/// Copy one stored artifact (a relative path like "Highlights/foo.md") from one
/// library into another, collision-safe (never overwrites — unique-names on
/// clash). Returns its new relative path. Used by the merge path.
fn copy_artifact(from_dir: &Path, to_dir: &Path, rel: Option<&str>) -> Option<String> {
    let rel = rel.filter(|s| !s.trim().is_empty())?;
    let src = from_dir.join(rel);
    if !src.is_file() {
        return Some(rel.to_string()); // nothing to copy; keep the reference as-is
    }
    let relp = Path::new(rel);
    let dest_dir = to_dir.join(relp.parent().unwrap_or(Path::new("")));
    let _ = std::fs::create_dir_all(&dest_dir);
    let stem = relp.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = relp.extension().and_then(|s| s.to_str()).unwrap_or("");
    let dest = storage::unique_path(&dest_dir, stem, ext);
    if std::fs::copy(&src, &dest).is_err() {
        return Some(rel.to_string());
    }
    dest.strip_prefix(to_dir)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

/// Relocate the storage folder:
/// - Target is EMPTY → MOVE the current library into it (copy artifacts + index,
///   then remove the old copies).
/// - Target already has a Shiro library → ADOPT it, and MERGE the current library's
///   items into it (union: each artifact copied collision-safe, rows inserted by id
///   so re-pointing never duplicates). Both folders are left intact.
/// Either way the DB is reopened at `new_dir` and the choice is remembered.
#[tauri::command]
pub fn cmd_set_storage_dir(
    app: AppHandle,
    db: tauri::State<Db>,
    storage: tauri::State<StorageDir>,
    new_dir: String,
) -> Result<(), String> {
    let new = PathBuf::from(&new_dir);
    if new.as_os_str().is_empty() {
        return Err("empty path".into());
    }

    let old = storage.lock().map_err(|e| e.to_string())?.clone();
    if old == new {
        return Ok(());
    }

    // Does the target already hold a Shiro library (e.g. an old data folder after a
    // reinstall)? If so we ADOPT + MERGE rather than overwrite.
    let adopting = storage::has_existing_data(&new);

    std::fs::create_dir_all(&new).map_err(|e| e.to_string())?;
    storage::ensure_layout(&new);

    // ADOPTING → snapshot the current library's items now (from the live connection,
    // so it includes screenshots and anything only in the DB) to merge in below.
    // MOVING → just copy the folders over.
    let to_merge: Vec<(String, String, SaveRequest)> = if adopting {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, created_at, type, url, title, text, html, file_path, notes, image_path, remind_at FROM items")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    SaveRequest {
                        item_type: r.get(2)?,
                        url: r.get(3)?,
                        title: r.get(4)?,
                        text: r.get(5)?,
                        html: r.get(6)?,
                        file_path: r.get(7)?,
                        notes: r.get(8)?,
                        image_path: r.get(9)?,
                        remind_at: r.get(10)?,
                    },
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.flatten().collect()
    } else {
        // Empty target → move the current library into it (fold the WAL first so a
        // plain copy is consistent).
        {
            let conn = db.lock().map_err(|e| e.to_string())?;
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
        for sub in [".shiro", "Highlights", "Links", "Files", "Images"] {
            storage::copy_dir(&old.join(sub), &new.join(sub)).map_err(|e| e.to_string())?;
        }
        Vec::new()
    };

    // Open the target's own database (relative file_paths resolve against `new`).
    let new_conn = db::open(&new).map_err(|e| e.to_string())?;

    // Merge the snapshot in: copy each artifact collision-safe, then insert the row
    // (INSERT OR IGNORE by id — re-pointing at the same folder never duplicates).
    for (id, created, mut req) in to_merge {
        let new_fp = copy_artifact(&old, &new, req.file_path.as_deref());
        // Image-only items store the same path in file_path AND image_path — copy the
        // artifact once and point both at the single copy.
        req.image_path = if req.image_path == req.file_path {
            new_fp.clone()
        } else {
            copy_artifact(&old, &new, req.image_path.as_deref())
        };
        req.file_path = new_fp;
        let _ = db::insert_item(&new_conn, &id, &req, &created);
    }

    // Top up the index from the folders (adopted content / a moved copy).
    rebuild_index(&new_conn, &new);

    // Swap the connection into the shared Mutex.
    {
        let mut guard = db.lock().map_err(|e| e.to_string())?;
        *guard = new_conn;
    }
    *storage.lock().map_err(|e| e.to_string())? = new.clone();
    storage::write_config(&app, &new)?;

    // Only remove the old copies when we MOVED — when adopting/merging the old folder
    // is left intact (nothing is deleted).
    if !adopting {
        for sub in [".shiro", "Highlights", "Links", "Files", "Images"] {
            let _ = std::fs::remove_dir_all(old.join(sub));
        }
    }

    let _ = app.emit("storage-changed", new.to_string_lossy().to_string());
    Ok(())
}

/// Reveal a stored file selected in Finder (open -R).
#[tauri::command]
pub fn cmd_reveal_in_finder(storage: tauri::State<StorageDir>, path: String) -> Result<(), String> {
    let p = Path::new(&path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        storage.lock().map_err(|e| e.to_string())?.join(p)
    };
    std::process::Command::new("open")
        .arg("-R")
        .arg(&full)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    /// Temp file path (moved into Images/ on save).
    pub path: String,
    /// `data:image/png;base64,…` preview for the popup.
    pub data_url: String,
}

fn b64(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD.encode(bytes)
}

/// Run the native macOS region selector. Returns the temp PNG + a data-URL
/// preview, or `None` if the user pressed Escape (no file produced).
fn take_screenshot_to_temp() -> Option<Screenshot> {
    let tmp = std::env::temp_dir().join(format!("shiro-shot-{}.png", uuid::Uuid::new_v4()));
    // -i interactive region/window selection, -x no camera sound.
    let _ = std::process::Command::new("screencapture")
        .arg("-i")
        .arg("-x")
        .arg(&tmp)
        .status();
    if !tmp.is_file() {
        return None; // cancelled
    }
    match std::fs::read(&tmp) {
        Ok(bytes) => Some(Screenshot {
            path: tmp.to_string_lossy().to_string(),
            data_url: format!("data:image/png;base64,{}", b64(&bytes)),
        }),
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            None
        }
    }
}

/// Invoked from the popup's "Attach screenshot" button.
#[tauri::command]
pub fn cmd_take_screenshot() -> Option<Screenshot> {
    take_screenshot_to_temp()
}

/// Resolve a stored image path (relative to the storage folder, or absolute) to
/// an absolute filesystem path. The webview then loads it via the asset protocol
/// (`convertFileSrc`) — streamed natively by WebKit instead of being shipped as a
/// multi-MB base64 string over IPC and parsed on the main thread. This is what
/// keeps opening large images/files smooth.
#[tauri::command]
pub fn cmd_image_src(storage: tauri::State<StorageDir>, path: String) -> Result<String, String> {
    let p = Path::new(&path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        storage.lock().map_err(|e| e.to_string())?.join(p)
    };
    Ok(full.to_string_lossy().to_string())
}

/// Check whether macOS has granted screen-recording access.
/// Uses CGPreflightScreenCaptureAccess() (available macOS 11+).
#[tauri::command]
pub fn cmd_screen_capture_status() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" { fn CGPreflightScreenCaptureAccess() -> bool; }
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    { true }
}

/// Prompt the user for screen-recording access.
/// CGRequestScreenCaptureAccess() shows the system dialog and returns the
/// (new) trust state. macOS also opens System Settings if already denied.
#[tauri::command]
pub fn cmd_request_screen_capture() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" { fn CGRequestScreenCaptureAccess() -> bool; }
        unsafe { CGRequestScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    { true }
}

/// Is Shiro currently trusted for Accessibility? (Non-macOS always true.)
#[tauri::command]
pub fn cmd_accessibility_status() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::accessibility::is_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Add Shiro to the Accessibility list and trigger the system permission
/// dialog. Returns whether access is already granted.
#[tauri::command]
pub fn cmd_request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::accessibility::prompt_trust()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Open System Settings → Privacy & Security → Screen & System Audio Recording.
#[tauri::command]
pub fn cmd_open_screen_recording_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Open System Settings directly to Privacy & Security → Accessibility.
#[tauri::command]
pub fn cmd_open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn cmd_get_items(db: tauri::State<Db>, filter: Option<String>) -> Result<Vec<Item>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;

    let (sql, type_filter) = build_list_query(&filter);

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params_from_iter(type_filter.iter()), row_to_item)
        .map_err(|e| e.to_string())?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[tauri::command]
pub fn cmd_delete_item(
    db: tauri::State<Db>,
    storage: tauri::State<StorageDir>,
    id: String,
) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    // Find the on-disk artifacts (the .md / file / screenshot) before removing
    // the row, so we can delete them from the storage folder too.
    let (file_path, image_path): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT file_path, image_path FROM items WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((None, None));
    conn.execute("DELETE FROM items WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    drop(conn);

    let dir = storage.lock().map_err(|e| e.to_string())?.clone();
    for rel in [file_path, image_path].into_iter().flatten() {
        if rel.trim().is_empty() {
            continue;
        }
        let p = std::path::Path::new(&rel);
        let full = if p.is_absolute() {
            p.to_path_buf()
        } else {
            dir.join(p)
        };
        let _ = std::fs::remove_file(full);
    }
    Ok(())
}

#[tauri::command]
pub fn cmd_update_notes(db: tauri::State<Db>, id: String, notes: String) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE items SET notes = ?1, updated_at = ?2 WHERE id = ?3",
        params![notes, now, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cmd_search(db: tauri::State<Db>, query: String) -> Result<Vec<Item>, String> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(vec![]);
    }

    let conn = db.lock().map_err(|e| e.to_string())?;

    // ── FTS pass ─────────────────────────────────────────────────────────────
    // Indexes: title, text, notes, url, file_path.
    // Each term is quoted (so special chars don't parse as FTS operators) + a
    // prefix wildcard so "goo" matches "google".
    let fts_query = q
        .split_whitespace()
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");

    let mut fts_results: Vec<Item> = Vec::new();
    if !fts_query.is_empty() {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT i.id, i.type, i.url, i.title, i.text, i.html, i.file_path, i.notes,
                    i.image_path, i.created_at, i.updated_at, i.remind_at
             FROM items i
             JOIN items_fts f ON i.rowid = f.rowid
             WHERE items_fts MATCH ?1
             ORDER BY rank",
        ) {
            if let Ok(rows) = stmt.query_map(params![fts_query], row_to_item) {
                fts_results = rows.filter_map(|r| r.ok()).collect();
            }
        }
    }

    // ── LIKE fallback ─────────────────────────────────────────────────────────
    // FTS tokenises on word boundaries (splits dots, slashes, hyphens), so
    // "google.com", "Q3-report.pdf", or a URL path are all tokenised into
    // multiple pieces. A LIKE scan on url + file_path catches those patterns
    // when they appear as a substring and deduplicates against FTS results.
    let pattern = format!("%{}%", q.to_lowercase());
    let fts_ids: std::collections::HashSet<String> =
        fts_results.iter().map(|i| i.id.clone()).collect();

    let mut like_extras: Vec<Item> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, type, url, title, text, html, file_path, notes,
                image_path, created_at, updated_at, remind_at
         FROM items
         WHERE lower(url) LIKE ?1
            OR lower(file_path) LIKE ?1
            OR lower(title) LIKE ?1
         ORDER BY created_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map(params![pattern], row_to_item) {
            like_extras = rows
                .filter_map(|r| r.ok())
                .filter(|i| !fts_ids.contains(&i.id))
                .collect();
        }
    }

    // FTS results first (ranked by relevance), then LIKE-only additions.
    fts_results.extend(like_extras);
    Ok(fts_results)
}

use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
static LAST_SHOWN_MS: AtomicU64 = AtomicU64::new(0);
// PID of the app that was frontmost when the hotkey fired, so we can hand focus
// back to it on save/close instead of surfacing Shiro's own window.
static PREV_APP_PID: AtomicI32 = AtomicI32::new(0);

/// Re-activate the app the user was in before the popup. Must run on the main
/// thread (AppKit). No-op if we never captured a PID.
#[cfg(target_os = "macos")]
pub(crate) fn reactivate_prev_app() {
    use objc::runtime::{Class, Object, BOOL};
    use objc::{msg_send, sel, sel_impl};

    let pid = PREV_APP_PID.load(Ordering::Relaxed);
    if pid <= 0 {
        return;
    }
    unsafe {
        let Some(cls) = Class::get("NSRunningApplication") else {
            return;
        };
        let app: *mut Object = msg_send![cls, runningApplicationWithProcessIdentifier: pid];
        if app.is_null() {
            return;
        }
        // NSApplicationActivateIgnoringOtherApps (1<<1).
        let _: BOOL = msg_send![app, activateWithOptions: 1usize << 1];
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// True if the popup was shown very recently — used to ignore the spurious
/// blur that can fire while the window is being presented, so it doesn't
/// immediately close itself.
pub(crate) fn popup_recently_shown() -> bool {
    now_ms().saturating_sub(LAST_SHOWN_MS.load(Ordering::Relaxed)) < 350
}

// Not a `#[tauri::command]` — only ever called from Rust (trigger_capture /
// trigger_screenshot), never invoked from the frontend.
pub fn cmd_show_popup(app: AppHandle, data: PopupData) -> Result<(), String> {
    // Window is pre-created at startup and kept alive — just update content and show.
    let win = app
        .get_webview_window("popup")
        .ok_or_else(|| "popup window not found".to_string())?;

    LAST_SHOWN_MS.store(now_ms(), Ordering::Relaxed);

    // React's listen() handler is already registered (window is pre-loaded) — no race.
    let _ = win.emit("popup-data", &data);

    // The action bar sits at the BOTTOM of the window (content stacks above it),
    // so anchor the window's bottom — not its top — near the cursor. Offsets are
    // physical (cursor_position is physical); scale converts the logical 440x360
    // window. Don't clamp: cursor coords go negative on displays left of/above
    // the primary monitor.
    if let Ok(pos) = win.cursor_position() {
        let scale = win.scale_factor().unwrap_or(1.0);
        let off_x = (200.0 * scale) as i32; // half of 400 → centered on cursor
                                            // window_height(340) - bottom_pad(14) - bar_height(44) = 282
                                            // bar_top = cursor_y + 10  →  window_y = cursor_y - 272
        let off_y = (250.0 * scale) as i32;
        let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
            x: pos.x as i32 - off_x,
            y: pos.y as i32 - off_y,
        }));
    }

    // On macOS, show as a non-activating panel (works over full-screen apps).
    // AppKit calls MUST happen on the main thread — run_on_main_thread marshals
    // there safely from this background command thread.
    #[cfg(target_os = "macos")]
    {
        let w = win.clone();
        app.run_on_main_thread(move || present_panel(&w))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// A custom NSPanel subclass whose only job is to return YES from
// canBecomeKeyWindow — a borderless window otherwise refuses key status, which
// would stop the popup's text fields from receiving keystrokes.
#[cfg(target_os = "macos")]
fn shiro_panel_class() -> *const objc::runtime::Class {
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel, BOOL, YES};
    use objc::{sel, sel_impl};
    use std::sync::Once;

    static mut CLS: *const Class = std::ptr::null();
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        let superclass = Class::get("NSPanel").expect("NSPanel class");
        let mut decl = ClassDecl::new("ShiroPanel", superclass).expect("declare ShiroPanel");
        extern "C" fn yes(_: &Object, _: Sel) -> BOOL {
            YES
        }
        decl.add_method(
            sel!(canBecomeKeyWindow),
            yes as extern "C" fn(&Object, Sel) -> BOOL,
        );
        CLS = decl.register();
    });
    unsafe { CLS }
}

/// Turn the popup's NSWindow into a non-activating NSPanel and show it over the
/// current Space — including another app's *native full-screen* Space — without
/// activating Shiro (which would yank the user out of full-screen). MUST run on
/// the main thread; AppKit is not thread-safe.
#[cfg(target_os = "macos")]
pub(crate) fn present_panel(win: &tauri::WebviewWindow) {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    extern "C" {
        fn object_setClass(obj: *mut Object, cls: *const Class) -> *const Class;
    }

    let Ok(ns_window) = win.ns_window() else {
        return;
    };
    let ns_window = ns_window as *mut Object;

    unsafe {
        // Re-class to our NSPanel subclass (idempotent — NSPanel adds no ivars
        // over NSWindow, so reassigning the class of a live window is safe).
        object_setClass(ns_window, shiro_panel_class());

        // NSWindowStyleMaskNonactivatingPanel (1<<7): become key without
        // activating the app or switching Spaces.
        let mask: usize = msg_send![ns_window, styleMask];
        let _: () = msg_send![ns_window, setStyleMask: mask | (1 << 7)];

        const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
        const FULLSCREEN_AUX: usize = 1 << 8;

        // NSPopUpMenuWindowLevel — above a full-screen app.
        let _: () = msg_send![ns_window, setLevel: 101i64];
        let _: () = msg_send![ns_window, setHidesOnDeactivate: false];
        // Kill the native window shadow — AppKit draws it as an ugly hard edge on
        // a transparent rounded window; we render a soft shadow in CSS instead.
        let _: () = msg_send![ns_window, setHasShadow: false];

        // Step 1: CanJoinAllSpaces pulls this persistent window onto whatever
        // Space is active right now (including a full-screen Space).
        let _: () =
            msg_send![ns_window, setCollectionBehavior: CAN_JOIN_ALL_SPACES | FULLSCREEN_AUX];

        // Show + key WITHOUT activating the app (no Space switch).
        let _: () = msg_send![ns_window, orderFrontRegardless];
        let _: () = msg_send![ns_window, makeKeyWindow];

        // Step 2: drop CanJoinAllSpaces so the window is now *pinned* to the
        // current Space. Switching Spaces leaves it behind → it disappears the
        // instant you swipe away instead of lingering in the next Space.
        let _: () = msg_send![ns_window, setCollectionBehavior: FULLSCREEN_AUX];
    }
}

#[tauri::command]
pub fn cmd_close_popup(app: AppHandle, restore_focus: Option<bool>) -> Result<(), String> {
    // Hide (not close) so it stays pre-loaded for the next hotkey press.
    if let Some(win) = app.get_webview_window("popup") {
        let _ = win.emit("popup-reset", ());
        win.hide().map_err(|e| e.to_string())?;
    }
    // On save/cancel/Escape, hand focus back to the app the user came from so the
    // popup dissolves silently instead of surfacing Shiro's own window. On
    // click-outside we skip this — focus has already gone where the user clicked.
    #[cfg(target_os = "macos")]
    if restore_focus.unwrap_or(true) {
        let _ = app.run_on_main_thread(reactivate_prev_app);
    }
    Ok(())
}

// ─── alarm / reminder ────────────────────────────────────────────────────────

/// Load one item by id (same column order as the list queries).
fn load_item(conn: &rusqlite::Connection, id: &str) -> Option<Item> {
    conn.query_row(
        "SELECT id, type, url, title, text, html, file_path, notes, image_path, created_at, updated_at, remind_at
         FROM items WHERE id = ?1",
        params![id],
        row_to_item,
    )
    .ok()
}

/// Find the earliest item whose reminder is now due, clear its reminder (a
/// reminder is one-shot), and return the item so the caller can ring it.
/// Compares as *parsed* timestamps, so it doesn't matter whether the stored
/// string ends in `Z` or `+00:00`. Called from the poll thread.
pub fn take_due_reminder(db: &Db) -> Option<Item> {
    let conn = db.lock().ok()?;
    let now = chrono::Utc::now();

    let due_id = {
        let mut stmt = conn
            .prepare("SELECT id, remind_at FROM items WHERE remind_at IS NOT NULL")
            .ok()?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .ok()?;
        let mut best: Option<(String, chrono::DateTime<chrono::Utc>)> = None;
        for (id, at) in rows.flatten() {
            if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&at) {
                let ts = ts.with_timezone(&chrono::Utc);
                if ts <= now && best.as_ref().is_none_or(|(_, b)| ts < *b) {
                    best = Some((id, ts));
                }
            }
        }
        best?.0
    };

    conn.execute("UPDATE items SET remind_at = NULL WHERE id = ?1", params![due_id])
        .ok()?;
    load_item(&conn, &due_id)
}

/// Whether the alarm is currently ringing — gates the native sound loop so
/// Stop/Snooze can silence it.
static ALARM_RINGING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ring a native, looping alarm sound (macOS `afplay`) for up to 30s or until
/// stopped. NATIVE on purpose: the alarm fires with no user gesture in its
/// webview, and WKWebView suspends WebAudio without one — so a JS beep would be
/// silent. Like Infinity's NSSound loop, this is guaranteed audible.
fn start_alarm_sound() {
    use std::sync::atomic::Ordering;
    if ALARM_RINGING.swap(true, Ordering::Relaxed) {
        return; // already ringing
    }
    std::thread::spawn(|| {
        let start = std::time::Instant::now();
        while ALARM_RINGING.load(Ordering::Relaxed) && start.elapsed().as_secs() < 30 {
            // Blocks ~1s per play; the flag is re-checked between loops so Stop
            // silences within ~1s. ponytail: 1s stop-tail is fine, no child-kill.
            let _ = std::process::Command::new("afplay")
                .arg("/System/Library/Sounds/Funk.aiff")
                .status();
        }
        ALARM_RINGING.store(false, Ordering::Relaxed);
    });
}

fn stop_alarm_sound() {
    ALARM_RINGING.store(false, std::sync::atomic::Ordering::Relaxed);
}

/// Show the floating alarm panel for a due item. Reuses the same non-activating
/// NSPanel trick as the capture pill, so it floats over full-screen apps without
/// stealing focus or surfacing Shiro. Anchored top-left, just below the menu bar.
pub fn show_alarm(app: &AppHandle, item: Item) {
    start_alarm_sound();
    let Some(win) = app.get_webview_window("alarm") else {
        return;
    };
    let _ = win.emit("alarm-data", &item);

    // Fixed top-right, just below the menu bar. (Alarm window is 340 logical wide.)
    let scale = win.scale_factor().unwrap_or(2.0);
    let panel_w = 340.0 * scale;
    let (mon_x, mon_w, mon_y) = match win.primary_monitor() {
        Ok(Some(mon)) => (mon.position().x as f64, mon.size().width as f64, mon.position().y as f64),
        _ => (0.0, 1440.0 * scale, 0.0),
    };
    let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: (mon_x + mon_w - panel_w - 16.0 * scale) as i32,
        y: (mon_y + 32.0 * scale) as i32,
    }));

    #[cfg(target_os = "macos")]
    {
        let w = win.clone();
        let _ = app.run_on_main_thread(move || present_panel(&w));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = win.show();
    }
}

/// Clicking the alarm card: silence it, hide the panel, then surface the main
/// window and tell the UI to open that item's note.
#[tauri::command]
pub fn cmd_open_item(app: AppHandle, id: String) -> Result<(), String> {
    stop_alarm_sound();
    if let Some(al) = app.get_webview_window("alarm") {
        let _ = al.hide();
    }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        let _ = win.emit("open-item", &id);
    }
    Ok(())
}

/// Stop button: silence the sound + hide the alarm panel (the reminder was
/// already cleared when it fired).
#[tauri::command]
pub fn cmd_dismiss_alarm(app: AppHandle) -> Result<(), String> {
    stop_alarm_sound();
    if let Some(win) = app.get_webview_window("alarm") {
        win.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Snooze button: re-arm the reminder `minutes` from now and hide the panel. The
/// poll thread will ring it again when it comes due.
#[tauri::command]
pub fn cmd_snooze_alarm(
    db: tauri::State<Db>,
    app: AppHandle,
    id: String,
    minutes: i64,
) -> Result<(), String> {
    stop_alarm_sound();
    let when = (chrono::Utc::now() + chrono::Duration::minutes(minutes.max(1))).to_rfc3339();
    {
        let conn = db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE items SET remind_at = ?1 WHERE id = ?2",
            params![when, id],
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(win) = app.get_webview_window("alarm") {
        let _ = win.hide();
    }
    Ok(())
}

// ─── helpers ────────────────────────────────────────────────────────────────

fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get(0)?,
        item_type: row.get(1)?,
        url: row.get(2)?,
        title: row.get(3)?,
        text: row.get(4)?,
        html: row.get(5)?,
        file_path: row.get(6)?,
        notes: row.get(7)?,
        image_path: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        remind_at: row.get(11)?,
    })
}

fn build_list_query(filter: &Option<String>) -> (String, Vec<String>) {
    let mut params: Vec<String> = vec![];
    let mut where_sql = String::new();

    if let Some(t) = filter {
        match t.as_str() {
            "" | "all" => {}
            // "Images" = anything with a screenshot attached (incl. highlights).
            "image" => where_sql = "WHERE i.image_path IS NOT NULL".to_string(),
            other => {
                where_sql = "WHERE i.type = ?1".to_string();
                params.push(other.to_string());
            }
        }
    }

    let sql = format!(
        "SELECT i.id, i.type, i.url, i.title, i.text, i.html, i.file_path, i.notes, i.image_path, i.created_at, i.updated_at, i.remind_at
         FROM items i
         {where_sql} ORDER BY i.created_at DESC"
    );

    (sql, params)
}

/// Rebuild / top up the search index from the folders, so the .md files are the
/// real source of truth: anything on disk that isn't indexed gets added. Runs at
/// startup; safe to run repeatedly (highlights/links keyed by their front-matter
/// id, files keyed by path).
pub fn rebuild_index(conn: &rusqlite::Connection, dir: &Path) {
    for (folder, default_type) in [
        (storage::highlights_dir(dir), "highlight"),
        (storage::links_dir(dir), "link"),
    ] {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (fm, body) = parse_front_matter(&raw);
            let id = match fm.get("id") {
                Some(id) if !id.is_empty() => id.clone(),
                _ => continue, // no id → not ours, skip
            };
            let rel = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let req = SaveRequest {
                item_type: fm
                    .get("type")
                    .cloned()
                    .unwrap_or_else(|| default_type.to_string()),
                url: fm.get("source").cloned(),
                title: fm.get("title").cloned(),
                text: if body.trim().is_empty() {
                    None
                } else {
                    Some(body)
                },
                html: None,
                file_path: Some(rel),
                notes: None,
                image_path: fm.get("image").cloned(),
                remind_at: fm.get("remind").cloned(),
            };
            let created = fm
                .get("created")
                .cloned()
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
            let _ = db::insert_item(conn, &id, &req, &created);
        }
    }

    // Files: index any file not already recorded (e.g. dropped in via Finder).
    if let Ok(entries) = std::fs::read_dir(storage::files_dir(dir)) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let fname = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
                .to_string();
            let rel = format!("Files/{fname}");
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM items WHERE file_path = ?1 LIMIT 1",
                    params![rel],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if exists {
                continue;
            }
            let created = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0)
                        .unwrap_or_else(chrono::Utc::now)
                        .to_rfc3339()
                })
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
            let req = SaveRequest {
                item_type: "file".to_string(),
                url: None,
                title: Some(fname),
                text: None,
                html: None,
                file_path: Some(rel),
                notes: None,
                image_path: None,
                remind_at: None,
            };
            let _ = db::insert_item(conn, &uuid::Uuid::new_v4().to_string(), &req, &created);
        }
    }
}

/// Minimal front-matter parser: returns (key→value map, body). Handles the
/// `---\n key: value …\n---\n body` shape we write; tolerant of missing fences.
fn parse_front_matter(raw: &str) -> (std::collections::HashMap<String, String>, String) {
    let mut map = std::collections::HashMap::new();
    let rest = raw
        .strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"));
    let Some(rest) = rest else {
        return (map, raw.to_string());
    };
    // Find the closing fence.
    if let Some(end) = rest.find("\n---") {
        let (header, after) = rest.split_at(end);
        for line in header.lines() {
            if let Some((k, v)) = line.split_once(':') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        let body = after
            .trim_start_matches('\n')
            .strip_prefix("---")
            .unwrap_or("")
            .trim_start_matches(['\n', '\r'])
            .to_string();
        (map, body)
    } else {
        (map, rest.to_string())
    }
}

// ─── hotkey ──────────────────────────────────────────────────────────────────

pub const DEFAULT_HOTKEY: &str = "Meta+KeyE";

#[tauri::command]
pub fn cmd_get_hotkey(db: tauri::State<Db>) -> Result<String, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    Ok(db::get_setting(&conn, "hotkey")
        .unwrap_or(None)
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string()))
}

#[tauri::command]
pub fn cmd_set_hotkey(app: AppHandle, db: tauri::State<Db>, hotkey: String) -> Result<(), String> {
    // Unregister all currently registered shortcuts
    if let Err(e) = app.global_shortcut().unregister_all() {
        eprintln!("unregister_all: {e}");
    }

    // Parse and re-register
    register_hotkey(&app, &hotkey)?;

    // Persist
    let conn = db.lock().map_err(|e| e.to_string())?;
    db::set_setting(&conn, "hotkey", &hotkey).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let (mods, code) = parse_shortcut_parts(hotkey)?;

    // Main hotkey → always open the capture pill.
    let main = Shortcut::new(if mods.is_empty() { None } else { Some(mods) }, code);
    let h1 = app.clone();
    app.global_shortcut()
        .on_shortcut(main, move |_app, _sc, event| {
            if event.state() == ShortcutState::Pressed {
                let h = h1.clone();
                std::thread::spawn(move || trigger_capture(&h));
            }
        })
        .map_err(|e| e.to_string())?;

    // Hotkey + Shift → fire the screenshot selector immediately (skip if the
    // user's hotkey already includes Shift, to avoid registering it twice).
    if !mods.contains(Modifiers::SHIFT) {
        let shot = Shortcut::new(Some(mods | Modifiers::SHIFT), code);
        let h2 = app.clone();
        let _ = app
            .global_shortcut()
            .on_shortcut(shot, move |_app, _sc, event| {
                if event.state() == ShortcutState::Pressed {
                    let h = h2.clone();
                    std::thread::spawn(move || trigger_screenshot(&h));
                }
            });
    }
    Ok(())
}

fn parse_shortcut_parts(s: &str) -> Result<(Modifiers, Code), String> {
    let parts: Vec<&str> = s.split('+').collect();
    let key_str = parts.last().ok_or("empty shortcut string")?;

    let code = str_to_code(key_str).ok_or_else(|| format!("unknown key code: {key_str}"))?;

    let mut mods = Modifiers::empty();
    for part in &parts[..parts.len() - 1] {
        match *part {
            "Meta" => mods |= Modifiers::META,
            "Shift" => mods |= Modifiers::SHIFT,
            "Alt" => mods |= Modifiers::ALT,
            "Control" | "Ctrl" => mods |= Modifiers::CONTROL,
            _ => {}
        }
    }

    Ok((mods, code))
}

fn str_to_code(s: &str) -> Option<Code> {
    Some(match s {
        "KeyA" => Code::KeyA,
        "KeyB" => Code::KeyB,
        "KeyC" => Code::KeyC,
        "KeyD" => Code::KeyD,
        "KeyE" => Code::KeyE,
        "KeyF" => Code::KeyF,
        "KeyG" => Code::KeyG,
        "KeyH" => Code::KeyH,
        "KeyI" => Code::KeyI,
        "KeyJ" => Code::KeyJ,
        "KeyK" => Code::KeyK,
        "KeyL" => Code::KeyL,
        "KeyM" => Code::KeyM,
        "KeyN" => Code::KeyN,
        "KeyO" => Code::KeyO,
        "KeyP" => Code::KeyP,
        "KeyQ" => Code::KeyQ,
        "KeyR" => Code::KeyR,
        "KeyS" => Code::KeyS,
        "KeyT" => Code::KeyT,
        "KeyU" => Code::KeyU,
        "KeyV" => Code::KeyV,
        "KeyW" => Code::KeyW,
        "KeyX" => Code::KeyX,
        "KeyY" => Code::KeyY,
        "KeyZ" => Code::KeyZ,
        "Digit0" => Code::Digit0,
        "Digit1" => Code::Digit1,
        "Digit2" => Code::Digit2,
        "Digit3" => Code::Digit3,
        "Digit4" => Code::Digit4,
        "Digit5" => Code::Digit5,
        "Digit6" => Code::Digit6,
        "Digit7" => Code::Digit7,
        "Digit8" => Code::Digit8,
        "Digit9" => Code::Digit9,
        "Space" => Code::Space,
        "Enter" => Code::Enter,
        "Escape" => Code::Escape,
        "Backspace" => Code::Backspace,
        "Tab" => Code::Tab,
        "Comma" => Code::Comma,
        "Period" => Code::Period,
        "Slash" => Code::Slash,
        "Semicolon" => Code::Semicolon,
        "Quote" => Code::Quote,
        "BracketLeft" => Code::BracketLeft,
        "BracketRight" => Code::BracketRight,
        "Backslash" => Code::Backslash,
        "Minus" => Code::Minus,
        "Equal" => Code::Equal,
        "Backquote" => Code::Backquote,
        "F1" => Code::F1,
        "F2" => Code::F2,
        "F3" => Code::F3,
        "F4" => Code::F4,
        "F5" => Code::F5,
        "F6" => Code::F6,
        "F7" => Code::F7,
        "F8" => Code::F8,
        "F9" => Code::F9,
        "F10" => Code::F10,
        "F11" => Code::F11,
        "F12" => Code::F12,
        _ => return None,
    })
}

// ─── capture trigger (shared with lib.rs) ────────────────────────────────────

pub fn trigger_capture(app: &AppHandle) {
    // ALWAYS open the pill, pre-filled with any detected selection/context.
    // A screenshot is never auto-fired — it's a deliberate action (the camera
    // button in the pill, or the hotkey+Shift variant → trigger_screenshot).
    let data = capture_context().unwrap_or_default();
    let _ = cmd_show_popup(app.clone(), data);
}

/// hotkey+Shift: fire the region selector immediately, then open the pill with
/// the screenshot attached. Cancelling the selector (Escape) opens nothing.
pub fn trigger_screenshot(app: &AppHandle) {
    if let Some(shot) = take_screenshot_to_temp() {
        let data = PopupData {
            screenshot: Some(shot.data_url),
            screenshot_path: Some(shot.path),
            ..Default::default()
        };
        let _ = cmd_show_popup(app.clone(), data);
    }
}

fn capture_context() -> Result<PopupData, String> {
    // Frontmost app, in-process (no subprocess spawn). Gives name + pid; the pid
    // is what we reactivate on save to return focus to where the user was, and
    // `front` decides whether the address-bar URL grab applies (browsers only).
    #[cfg(target_os = "macos")]
    let (front, pid) = crate::accessibility::frontmost_app().unwrap_or_default();
    #[cfg(not(target_os = "macos"))]
    let (front, pid): (String, i32) = (String::new(), 0);
    if pid > 0 {
        PREV_APP_PID.store(pid, Ordering::Relaxed);
    }

    // NOTE: Finder file capture by hotkey was removed on purpose — it needed a
    // `tell application "Finder"` Apple event, which triggers a second
    // ("Automation") permission prompt. Files are added by dragging them onto the
    // window or via the file picker, neither of which needs any extra permission.

    // Capture the selection from the CLIPBOARD first: one ⌘C grabs the plain
    // text, the HTML flavour (rich-text formatting) AND any image in a single
    // shot — that's what preserves formatting and copied images. The
    // Accessibility API only ever exposes plain text (no formatting, no image),
    // so it's a *fallback* for the rare app that blocks ⌘C but exposes
    // AXSelectedText. (Using AX first silently dropped html + images — that was
    // the "formatting/images don't capture" regression.)
    let mut cap = capture_clipboard();
    if cap.text.is_none() && cap.image_png.is_none() {
        #[cfg(target_os = "macos")]
        if let Some(t) = crate::accessibility::selected_text() {
            if !t.is_empty() {
                cap.text = Some(t);
            }
        }
    }
    let text = cap.text.filter(|s| !s.is_empty());
    let clip_html = cap.html.filter(|s| !s.trim().is_empty());

    // A pure image copy ("Copy Image") → pre-attach it like a screenshot so the
    // pill shows it and saving drops it into Images/.
    let (mut screenshot, mut screenshot_path) = (None, None);
    #[cfg(target_os = "macos")]
    if let Some(png) = cap.image_png {
        if let Some((path, data_url)) = clipboard_image_to_temp(&png) {
            screenshot = Some(data_url);
            screenshot_path = Some(path);
        }
    }

    // URL via the address-bar keyboard shortcut (browsers only). ONLY when the
    // user actually selected text — grabbing the URL means hijacking the address
    // bar (Cmd+L/Cmd+C/Esc), which is disruptive and pointless on a bare hotkey
    // press with nothing selected. With a selection, the URL is attached as the
    // source. No selection → no URL grab, no browser disruption, clean pill.
    #[cfg(target_os = "macos")]
    let url = if text.is_some() { browser_url_via_keyboard(&front) } else { None };
    #[cfg(not(target_os = "macos"))]
    let url: Option<String> = None;
    let title: Option<String> = None;

    Ok(PopupData {
        url: url.filter(|s| !s.is_empty()),
        title: title.filter(|s| !s.is_empty()),
        text,
        html: clip_html,
        files: vec![],
        screenshot,
        screenshot_path,
    })
}

/// Browsers we'll fire the address-bar shortcut into. Gated so Cmd+L can't be
/// sent into an arbitrary app.
#[cfg(target_os = "macos")]
const BROWSERS: &[&str] = &[
    "Safari",
    "Safari Technology Preview",
    "Orion",
    "Google Chrome",
    "Google Chrome Canary",
    "Chromium",
    "Brave Browser",
    "Microsoft Edge",
    "Arc",
    "Vivaldi",
    "Opera",
    "Opera GX",
    "ChatGPT Atlas",
    "Atlas",
    "Dia",
    "SigmaOS",
    "Comet",
];

/// Grab the current page URL by simulating the address-bar shortcut (⌘L → ⌘C →
/// Esc). Native CGEvent keystrokes — **Accessibility permission only, no
/// Automation prompt**; ⌘L is universal across Safari/Chromium. Gated to known
/// browsers so the shortcut can't be fired into an arbitrary app.
#[cfg(target_os = "macos")]
fn browser_url_via_keyboard(front: &str) -> Option<String> {
    if !BROWSERS.contains(&front) {
        return None;
    }
    crate::mac_input::browser_url()
}

/// What a clipboard capture produced. Any combination may be present; a pure
/// image copy (e.g. "Copy Image") yields only `image_png`.
#[derive(Default)]
struct ClipCapture {
    text: Option<String>,
    html: Option<String>,
    image_png: Option<Vec<u8>>,
}

/// Capture the current selection through the clipboard, **natively**. Sends ⌘C
/// with CGEvent (Accessibility permission only — no Automation prompt) and reads
/// the result off NSPasteboard, polling the actual *contents* so we never read
/// before the copy has landed (that race is what broke an earlier native
/// attempt). Text, HTML, and image bytes are all read off the one ⌘C. The
/// clipboard is restored only when nothing at all was captured.
#[cfg(target_os = "macos")]
fn capture_clipboard() -> ClipCapture {
    use crate::mac_input as mi;
    use std::time::Duration;

    let saved = mi::clipboard_text();
    mi::clear_clipboard();
    mi::send_cmd_key(mi::KEY_C);

    // Poll contents (~240ms max). A real selection lands in a frame or two; this
    // only runs to the end when there's nothing to copy.
    let mut text = None;
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(20));
        if let Some(s) = mi::clipboard_text() {
            if !s.trim().is_empty() {
                text = Some(s);
                break;
            }
        }
        if mi::clipboard_has_image() {
            break;
        }
    }

    // HTML flavour (rich-text formatting + any inline images) rides along with the
    // text selection. This is what carries images that are part of a text capture.
    let html = if text.is_some() {
        mi::clipboard_html()
    } else {
        None
    };

    // An image our own ⌘C produced — i.e. the selection itself WAS an image
    // (e.g. an image selected in Preview/Photos). We deliberately do NOT grab a
    // pre-existing "Copy Image" off the clipboard: images come along with a text
    // capture (via the HTML), not from a separate copy-then-hotkey step.
    let image_png = mi::read_image_png();

    // Nothing landed (no selection) → put back whatever we cleared.
    if text.is_none() && image_png.is_none() {
        if let Some(s) = saved {
            mi::set_clipboard_text(&s);
        }
    }

    ClipCapture {
        text: text.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        html: html.filter(|s| !s.trim().is_empty()),
        image_png,
    }
}

#[cfg(not(target_os = "macos"))]
fn capture_clipboard() -> ClipCapture {
    ClipCapture::default()
}

/// Write clipboard PNG bytes to a temp file; returns (path, data-url preview).
#[cfg(target_os = "macos")]
fn clipboard_image_to_temp(png: &[u8]) -> Option<(String, String)> {
    let tmp = std::env::temp_dir().join(format!("shiro-clip-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, png).ok()?;
    Some((
        tmp.to_string_lossy().to_string(),
        format!("data:image/png;base64,{}", b64(png)),
    ))
}
