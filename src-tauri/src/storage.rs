use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager};

/// Where the user's data lives (db + files). Shared, mutable so the
/// user can relocate it at runtime.
pub type StorageDir = Arc<Mutex<PathBuf>>;

/// Path to the tiny bootstrap config that records where storage lives. This
/// always sits in the OS app-data dir — it's the one thing we must find before
/// we know the (user-chosen) storage location.
fn config_path(app: &AppHandle) -> PathBuf {
    let base = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("/tmp/shiro"));
    let _ = std::fs::create_dir_all(&base);
    base.join("config.json")
}

/// The default storage location used on first run, before the user picks one.
fn default_storage_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .desktop_dir()
        .unwrap_or_else(|_| {
            app.path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("/tmp/shiro"))
        })
        .join("Shiro")
}

/// Resolve the configured storage dir (or the default), creating the folder
/// layout. Called once at startup.
pub fn resolve(app: &AppHandle) -> PathBuf {
    let dir = read_config(app).unwrap_or_else(|| default_storage_dir(app));
    ensure_layout(&dir);
    dir
}

pub fn read_config(app: &AppHandle) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(config_path(app)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("storage_dir")
        .and_then(|s| s.as_str())
        .map(PathBuf::from)
}

pub fn write_config(app: &AppHandle, dir: &Path) -> Result<(), String> {
    let json = serde_json::json!({ "storage_dir": dir.to_string_lossy() });
    std::fs::write(
        config_path(app),
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())
}

/// Human-browsable layout: typed folders hold the source-of-truth artifacts
/// (.md for highlights/links, originals for files), and a hidden `.shiro/`
/// holds the rebuildable SQLite search index.
pub fn ensure_layout(dir: &Path) {
    for sub in [".shiro", "Highlights", "Links", "Files", "Images"] {
        let _ = std::fs::create_dir_all(dir.join(sub));
    }
}

/// Path to the SQLite index. It's a cache: if deleted, it's rebuilt by scanning
/// the typed folders.
pub fn index_db_path(dir: &Path) -> PathBuf {
    dir.join(".shiro").join("index.db")
}

pub fn highlights_dir(dir: &Path) -> PathBuf {
    dir.join("Highlights")
}

pub fn links_dir(dir: &Path) -> PathBuf {
    dir.join("Links")
}

pub fn files_dir(dir: &Path) -> PathBuf {
    dir.join("Files")
}

pub fn images_dir(dir: &Path) -> PathBuf {
    dir.join("Images")
}

/// Make a string safe to use as a file name (no path separators / illegal
/// chars), trimmed and length-capped. Falls back to "untitled".
pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    let base = if trimmed.is_empty() {
        "untitled"
    } else {
        trimmed
    };
    base.chars().take(80).collect()
}

/// `<dir>/<stem>.<ext>` (or `<dir>/<stem>` if ext is empty), suffixing
/// " 2", " 3"… if the name is taken.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let name = |stem: &str| {
        if ext.is_empty() {
            stem.to_string()
        } else {
            format!("{stem}.{ext}")
        }
    };
    let mut p = dir.join(name(stem));
    let mut n = 2;
    while p.exists() {
        p = dir.join(name(&format!("{stem} {n}")));
        n += 1;
    }
    p
}

/// Recursively copy `from` into `to`. Used when relocating storage.
pub fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}
