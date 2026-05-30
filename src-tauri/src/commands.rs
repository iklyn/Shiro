use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::db::{self, Db, SaveRequest};
use crate::storage::{self, StorageDir};

/// Shared waker: the reminder thread sleeps on this condvar. Any code that
/// adds or removes a reminder signals it so the thread re-evaluates its sleep.
pub type ReminderWaker = Arc<(Mutex<()>, Condvar)>;

pub(crate) fn signal_reminder_thread(waker: &ReminderWaker) {
    let (lock, cvar) = &**waker;
    let _guard = lock.lock().unwrap();
    cvar.notify_one();
}

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
    waker: tauri::State<ReminderWaker>,
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
            let name = dest.file_name().and_then(|s| s.to_str()).unwrap_or("shot.png");
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
    // Wake the reminder thread so it can recalculate its next sleep duration.
    if req.remind_at.is_some() {
        signal_reminder_thread(&waker);
    }
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
fn write_markdown(dir: &Path, id: &str, req: &SaveRequest, created: &str) -> Result<String, String> {
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

/// Relocate the storage folder: copy db + files/ + pages/ into `new_dir`,
/// reopen the database there, and remember the choice. Old copies are removed.
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
    std::fs::create_dir_all(&new).map_err(|e| e.to_string())?;
    storage::ensure_layout(&new);

    let old = storage.lock().map_err(|e| e.to_string())?.clone();
    if old == new {
        return Ok(());
    }

    // Fold the WAL back into the index db so a plain copy is consistent.
    {
        let conn = db.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }

    // Copy every typed folder + the hidden index.
    for sub in [".shiro", "Highlights", "Links", "Pages", "Files", "Images"] {
        storage::copy_dir(&old.join(sub), &new.join(sub)).map_err(|e| e.to_string())?;
    }

    // Reopen at the new location and swap the connection inside the shared Mutex.
    let new_conn = db::open(&new).map_err(|e| e.to_string())?;
    {
        let mut guard = db.lock().map_err(|e| e.to_string())?;
        *guard = new_conn;
    }
    *storage.lock().map_err(|e| e.to_string())? = new.clone();
    storage::write_config(&app, &new)?;

    // Best-effort cleanup of the old copies.
    for sub in [".shiro", "Highlights", "Links", "Pages", "Files", "Images"] {
        let _ = std::fs::remove_dir_all(old.join(sub));
    }

    let _ = app.emit("storage-changed", new.to_string_lossy().to_string());
    Ok(())
}

/// Open a stored item on disk. Accepts an absolute path or a path relative to
/// the storage folder (as stored in `file_path`).
#[tauri::command]
pub fn cmd_open_path(storage: tauri::State<StorageDir>, path: String) -> Result<(), String> {
    let p = Path::new(&path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        storage.lock().map_err(|e| e.to_string())?.join(p)
    };
    std::process::Command::new("open")
        .arg(&full)
        .spawn()
        .map_err(|e| e.to_string())?;
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

/// Read a stored image (relative to the storage folder, or absolute) as a
/// data-URL, for display in the detail panel.
#[tauri::command]
pub fn cmd_read_image(storage: tauri::State<StorageDir>, path: String) -> Result<String, String> {
    let p = Path::new(&path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        storage.lock().map_err(|e| e.to_string())?.join(p)
    };
    let bytes = std::fs::read(&full).map_err(|e| e.to_string())?;
    let mime = match full.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Ok(format!("data:{mime};base64,{}", b64(&bytes)))
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

/// Whether the user has enabled the reminders feature (stored in settings).
/// Defaults to false — the alarm button is hidden until explicitly turned on.
#[tauri::command]
pub fn cmd_get_reminders_enabled(db: tauri::State<Db>) -> Result<bool, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    Ok(db::get_setting(&conn, "reminders_enabled")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false))
}

#[tauri::command]
pub fn cmd_set_reminders_enabled(db: tauri::State<Db>, enabled: bool) -> Result<(), String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    db::set_setting(&conn, "reminders_enabled", if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())
}

/// Open System Settings → Notifications so the user can grant permission.
#[tauri::command]
pub fn cmd_open_notification_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.notifications")
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
pub fn cmd_get_items(
    db: tauri::State<Db>,
    filter: Option<String>,
) -> Result<Vec<Item>, String> {
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
        let full = if p.is_absolute() { p.to_path_buf() } else { dir.join(p) };
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
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let conn = db.lock().map_err(|e| e.to_string())?;
    // Wrap each whitespace-separated term in double quotes (escaping any embedded
    // quotes) and append a prefix wildcard. Without this, characters that FTS5
    // treats as operators (", *, :, -, etc.) raise a syntax error and search
    // silently returns nothing.
    let fts_query = query
        .split_whitespace()
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");
    if fts_query.is_empty() {
        return Ok(vec![]);
    }

    let mut stmt = conn
        .prepare(
            "SELECT i.id, i.type, i.url, i.title, i.text, i.html, i.file_path, i.notes, i.image_path, i.created_at, i.updated_at, i.remind_at
             FROM items i
             JOIN items_fts f ON i.rowid = f.rowid
             WHERE items_fts MATCH ?1
             ORDER BY rank",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![fts_query], row_to_item)
        .map_err(|e| e.to_string())?;

    Ok(rows.filter_map(|r| r.ok()).collect())
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

#[tauri::command]
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

    let Ok(ns_window) = win.ns_window() else { return };
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
        let _: () = msg_send![ns_window, setCollectionBehavior: CAN_JOIN_ALL_SPACES | FULLSCREEN_AUX];

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
                item_type: fm.get("type").cloned().unwrap_or_else(|| default_type.to_string()),
                url: fm.get("source").cloned(),
                title: fm.get("title").cloned(),
                text: if body.trim().is_empty() { None } else { Some(body) },
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
    let rest = raw.strip_prefix("---\n").or_else(|| raw.strip_prefix("---\r\n"));
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
        let _ = app.global_shortcut().on_shortcut(shot, move |_app, _sc, event| {
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
        "KeyA" => Code::KeyA, "KeyB" => Code::KeyB, "KeyC" => Code::KeyC,
        "KeyD" => Code::KeyD, "KeyE" => Code::KeyE, "KeyF" => Code::KeyF,
        "KeyG" => Code::KeyG, "KeyH" => Code::KeyH, "KeyI" => Code::KeyI,
        "KeyJ" => Code::KeyJ, "KeyK" => Code::KeyK, "KeyL" => Code::KeyL,
        "KeyM" => Code::KeyM, "KeyN" => Code::KeyN, "KeyO" => Code::KeyO,
        "KeyP" => Code::KeyP, "KeyQ" => Code::KeyQ, "KeyR" => Code::KeyR,
        "KeyS" => Code::KeyS, "KeyT" => Code::KeyT, "KeyU" => Code::KeyU,
        "KeyV" => Code::KeyV, "KeyW" => Code::KeyW, "KeyX" => Code::KeyX,
        "KeyY" => Code::KeyY, "KeyZ" => Code::KeyZ,
        "Digit0" => Code::Digit0, "Digit1" => Code::Digit1, "Digit2" => Code::Digit2,
        "Digit3" => Code::Digit3, "Digit4" => Code::Digit4, "Digit5" => Code::Digit5,
        "Digit6" => Code::Digit6, "Digit7" => Code::Digit7, "Digit8" => Code::Digit8,
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
        "F1" => Code::F1, "F2" => Code::F2, "F3" => Code::F3, "F4" => Code::F4,
        "F5" => Code::F5, "F6" => Code::F6, "F7" => Code::F7, "F8" => Code::F8,
        "F9" => Code::F9, "F10" => Code::F10, "F11" => Code::F11, "F12" => Code::F12,
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
    // IMPORTANT: AppleScript resolves each `tell application "X"` dictionary at
    // COMPILE time. Referencing a browser that isn't installed makes the whole
    // script fail to compile ("Expected end of line but found property"). So we
    // detect the frontmost app first, then run a script that only ever talks to
    // apps we know are present (System Events / Finder) or the one browser that
    // is actually frontmost.
    // Frontmost app in-process (no osascript spawn). Gives name + pid; the pid is
    // also what we reactivate on save to return focus to where the user was.
    #[cfg(target_os = "macos")]
    let (front, pid) = crate::accessibility::frontmost_app().unwrap_or_default();
    #[cfg(not(target_os = "macos"))]
    let (front, pid): (String, i32) = (String::new(), 0);
    if pid > 0 {
        PREV_APP_PID.store(pid, Ordering::Relaxed);
    }

    // Finder: capture the file selection, nothing else.
    if front == "Finder" {
        let out = run_osascript(
            r#"set out to ""
            tell application "Finder" to set theSel to selection
            repeat with anItem in theSel
                try
                    set out to out & POSIX path of (anItem as alias) & linefeed
                end try
            end repeat
            return out"#,
        )
        .unwrap_or_default();
        let files = out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>();
        return Ok(PopupData { files, ..Default::default() });
    }

    // Browser URL + title — only for the frontmost browser, built dynamically.
    let (url, title) = browser_url_title(&front);

    // Selected text: Accessibility API first (instant, in-process; covers native
    // apps). If it finds nothing, fall back to the clipboard ⌘C method, which
    // also catches selections in browsers and Electron apps (Slack/VS Code/…).
    // The hotkey no longer auto-screenshots, so this only affects pill speed.
    let ax_text = {
        #[cfg(target_os = "macos")]
        { crate::accessibility::selected_text() }
        #[cfg(not(target_os = "macos"))]
        { None }
    };
    let text = ax_text.or_else(capture_selected_text_via_clipboard);

    Ok(PopupData {
        url: url.filter(|s| !s.is_empty()),
        title: title.filter(|s| !s.is_empty()),
        text: text.filter(|s| !s.is_empty()),
        files: vec![],
        ..Default::default()
    })
}

fn run_osascript(script: &str) -> Result<String, String> {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        eprintln!("Shiro capture: osascript failed: {err}");
        return Err(err);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_string())
}

/// Ask the frontmost browser for its active tab's URL + title. Returns
/// (None, None) for non-browsers. The script only references the one app that
/// is frontmost, so it always compiles.
fn browser_url_title(front: &str) -> (Option<String>, Option<String>) {
    // (tab accessor, title property) per browser family.
    let (tab, title_prop) = match front {
        "Google Chrome" | "Arc" | "Brave Browser" | "Microsoft Edge" | "Chromium"
        | "Vivaldi" | "Opera" => ("active tab", "title"),
        "Safari" | "Safari Technology Preview" => ("current tab", "name"),
        _ => return (None, None),
    };

    let script = format!(
        r#"tell application "{front}"
            set u to URL of {tab} of front window
            set t to {title_prop} of {tab} of front window
            return u & "|||" & t
        end tell"#
    );

    match run_osascript(&script) {
        Ok(out) => {
            let mut parts = out.splitn(2, "|||");
            (
                parts.next().map(|s| s.trim().to_string()),
                parts.next().map(|s| s.trim().to_string()),
            )
        }
        Err(_) => (None, None),
    }
}

/// Copy the current selection from the frontmost app: clear the clipboard, send
/// Cmd+C, poll for new contents, and restore the clipboard if nothing was
/// selected. Only talks to System Events, so it always compiles.
/// Hybrid selection capture: ask the Accessibility API for the focused
/// element's selected text first (clean, no clipboard). That works for native
/// apps; browsers/Electron usually don't expose it, so we fall back to the
/// Cmd+C method. Both use the same Accessibility permission.

fn capture_selected_text_via_clipboard() -> Option<String> {
    // Two modes:
    //  • Accessibility granted → clear clipboard, send Cmd+C, poll for the new
    //    selection (accurate even if it matches what was already copied).
    //  • Keystroke blocked (no Accessibility) → the keystroke is caught by
    //    `try`, so instead we fall back to whatever the user copied manually.
    //    Copy-then-hotkey works without any permission.
    let script = r#"
        try
            set oldClip to the clipboard as text
        on error
            set oldClip to ""
        end try
        set selText to ""
        set didCopy to false
        try
            set the clipboard to ""
            tell application "System Events" to keystroke "c" using command down
            set didCopy to true
        end try
        if didCopy then
            -- Poll briefly (~240ms max), exiting the instant the clipboard fills.
            -- A real selection lands within ~100ms; this keeps the no-selection
            -- case (which precedes the screenshot) from stalling.
            repeat 8 times
                delay 0.02
                try
                    set cur to the clipboard as text
                on error
                    set cur to ""
                end try
                if cur is not "" then
                    set selText to cur
                    exit repeat
                end if
            end repeat
            if selText is "" then
                set the clipboard to oldClip
            end if
        else
            set the clipboard to oldClip
            set selText to oldClip
        end if
        return selText
    "#;
    run_osascript(script).ok().map(|s| s.trim().to_string())
}
