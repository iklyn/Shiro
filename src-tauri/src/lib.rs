// objc 0.2's AppKit macros still emit cfg(cargo-clippy), which modern Rust
// reports as an unexpected cfg from our expansion sites. It is dependency noise.
#![allow(unexpected_cfgs)]

#[cfg(target_os = "macos")]
mod accessibility;
mod commands;
mod db;
#[cfg(target_os = "macos")]
mod mac_input;
mod storage;

use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WebviewWindowBuilder,
};

use commands::*;
use db::Db;
use storage::StorageDir;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // ── Storage location (user-configurable) ────────────────────────
            let storage_dir = storage::resolve(&handle);
            let storage_state: StorageDir = Arc::new(Mutex::new(storage_dir.clone()));
            app.manage(storage_state);

            // ── Database (lives inside the storage folder) ──────────────────
            let db: Db = match db::open(&storage_dir) {
                Ok(conn) => Arc::new(Mutex::new(conn)),
                Err(e) => {
                    eprintln!("DB open failed: {e}");
                    return Ok(());
                }
            };
            // Top up the index from the folders (.md files are the source of
            // truth) — picks up anything added/restored outside the app.
            if let Ok(conn) = db.lock() {
                commands::rebuild_index(&conn, &storage_dir);
            }
            app.manage(db.clone());

            // ── Main window: hide on close so the tray can re-open it ───────
            if let Some(main_win) = app.get_webview_window("main") {
                let hide_win = main_win.clone();
                main_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hide_win.hide();
                        // Window hidden → drop back to menu-bar-only (no Dock icon).
                        #[cfg(target_os = "macos")]
                        let _ = hide_win
                            .app_handle()
                            .set_activation_policy(tauri::ActivationPolicy::Accessory);
                    }
                });

                // Let the green button enter real macOS full-screen, not just
                // zoom. titleBarStyle:Overlay drops FullScreenPrimary from the
                // window's collection behaviour, so re-add it (main thread = ok).
                #[cfg(target_os = "macos")]
                {
                    use objc::{msg_send, sel, sel_impl};
                    if let Ok(ns) = main_win.ns_window() {
                        let ns = ns as *mut objc::runtime::Object;
                        unsafe {
                            let cur: usize = msg_send![ns, collectionBehavior];
                            // NSWindowCollectionBehaviorFullScreenPrimary = 1 << 7
                            let _: () = msg_send![ns, setCollectionBehavior: cur | (1usize << 7)];
                        }
                    }
                }
            }

            // ── Pre-create the capture popup (hidden, kept alive) ───────────
            let popup_url = if cfg!(debug_assertions) {
                "http://localhost:1420?window=popup"
            } else {
                "tauri://localhost?window=popup"
            };
            // The popup is converted into a non-activating NSPanel on first show
            // (see commands::present_panel) so it can float over full-screen apps.
            let popup = WebviewWindowBuilder::new(
                app,
                "popup",
                tauri::WebviewUrl::External(popup_url.parse().unwrap()),
            )
            .title("Shiro")
            .inner_size(400.0, 340.0)
            .resizable(false)
            .always_on_top(true)
            .decorations(false)
            .transparent(true)
            .skip_taskbar(true)
            .visible(false)
            .build();

            // Dismiss the popup when it loses focus — i.e. the user clicks
            // outside it or switches Spaces. The recently-shown guard ignores the
            // transient blur that fires while the panel is being presented.
            if let Ok(popup) = popup {
                let p = popup.clone();
                popup.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        if commands::popup_recently_shown() {
                            return;
                        }
                        // Let the frontend play its fade-out, then close without
                        // restoring focus (the user clicked elsewhere / changed Space).
                        let _ = p.emit("popup-dismiss", ());
                    }
                });
            }

            // ── Global hotkey (loaded from settings) ────────────────────────
            let hotkey = {
                let conn = db.lock().unwrap();
                db::get_setting(&conn, "hotkey")
                    .unwrap_or(None)
                    .unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
            };
            if let Err(e) = register_hotkey(app.handle(), &hotkey) {
                eprintln!("Could not register hotkey '{hotkey}': {e}");
            }

            // ── System tray ─────────────────────────────────────────────────
            if let (Ok(open_i), Ok(capture_i), Ok(quit_i)) = (
                MenuItem::with_id(app, "open", "Open Shiro", true, None::<&str>),
                MenuItem::with_id(app, "capture", "Quick Capture", true, None::<&str>),
                MenuItem::with_id(app, "quit", "Quit", true, None::<&str>),
            ) {
                if let Ok(menu) = Menu::with_items(app, &[&open_i, &capture_i, &quit_i]) {
                    {
                        // Menu-bar icon: just the Shiro mascot (transparent, no square).
                        // Not a template image, so its colours show as-is.
                        let tray_icon = tauri::include_image!("icons/tray.png");
                        let _ = TrayIconBuilder::with_id("shiro-tray")
                            .icon(tray_icon)
                            .icon_as_template(false)
                            .menu(&menu)
                            .show_menu_on_left_click(false)
                            .on_tray_icon_event(|tray, event| {
                                if let TrayIconEvent::Click {
                                    button: MouseButton::Left,
                                    button_state: MouseButtonState::Up,
                                    ..
                                } = event
                                {
                                    let app = tray.app_handle();
                                    if let Some(win) = app.get_webview_window("main") {
                                        let _ = win.show();
                                        let _ = win.set_focus();
                                        // Window visible → Regular so the menu bar and
                                        // native full-screen work (and Dock icon shows).
                                        #[cfg(target_os = "macos")]
                                        let _ = app.set_activation_policy(
                                            tauri::ActivationPolicy::Regular,
                                        );
                                    }
                                }
                            })
                            .on_menu_event(|app, event| match event.id.as_ref() {
                                "open" => {
                                    if let Some(win) = app.get_webview_window("main") {
                                        let _ = win.show();
                                        let _ = win.set_focus();
                                        #[cfg(target_os = "macos")]
                                        let _ = app.set_activation_policy(
                                            tauri::ActivationPolicy::Regular,
                                        );
                                    }
                                }
                                "capture" => {
                                    let app_h = app.clone();
                                    std::thread::spawn(move || trigger_capture(&app_h));
                                }
                                "quit" => app.exit(0),
                                _ => {}
                            })
                            .build(app);
                    }
                }
            }

            // ── macOS: dynamic activation policy ─────────────────────────────
            // The main window launches visible, so start in Regular (Dock icon +
            // menu bar) — required for native full-screen to behave. When the
            // window is hidden (close button) we flip to Accessory so background
            // capture stays a menu-bar-only utility with no Dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd_save_item,
            cmd_save_files,
            cmd_get_items,
            cmd_delete_item,
            cmd_update_notes,
            cmd_search,
            cmd_show_popup,
            cmd_close_popup,
            cmd_get_hotkey,
            cmd_set_hotkey,
            cmd_get_storage_dir,
            cmd_set_storage_dir,
            cmd_open_path,
            cmd_reveal_in_finder,
            cmd_take_screenshot,
            cmd_read_image,
            cmd_get_setting,
            cmd_set_setting,
            cmd_screen_capture_status,
            cmd_request_screen_capture,
            cmd_accessibility_status,
            cmd_request_accessibility,
            cmd_open_accessibility_settings,
            cmd_open_screen_recording_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running shiro");
}
