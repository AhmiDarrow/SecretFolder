mod commands;
mod crypto;
mod error;
mod vault;

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, RunEvent, WindowEvent,
};

use commands::AppState;

/// When false, Close on the main window hides to tray instead of exiting.
static ALLOW_EXIT: AtomicBool = AtomicBool::new(false);

/// Allow a real process exit (tray Quit / quit_app command).
pub(crate) fn request_exit() {
    ALLOW_EXIT.store(true, Ordering::SeqCst);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .setup(|app| {
            let state = AppState::new()
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            app.manage(state);

            build_tray(app.handle())?;

            // Idle-lock poller — locks vault and notifies UI.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(30));
                if let Some(state) = handle.try_state::<AppState>() {
                    if let Ok(mut guard) = state.vault.lock() {
                        if guard.check_idle_lock() {
                            let _ = handle.emit("vault-locked", ());
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::setup_vault,
            commands::unlock_vault,
            commands::unlock_with_recovery,
            commands::lock_vault,
            commands::touch_activity,
            commands::set_idle_lock_secs,
            commands::change_password,
            commands::list_items,
            commands::folder_path,
            commands::get_item,
            commands::create_folder,
            commands::move_item,
            commands::create_text,
            commands::update_text,
            commands::rename_item,
            commands::folder_content_count,
            commands::delete_item,
            commands::import_path,
            commands::import_bytes,
            commands::export_path,
            commands::max_file_bytes,
            commands::show_main_window,
            commands::hide_main_window,
            commands::quit_app,
            commands::open_external_url,
        ])
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if !ALLOW_EXIT.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building SecretFolder");

    app.run(|app_handle, event| {
        if let RunEvent::ExitRequested { api, .. } = event {
            if !ALLOW_EXIT.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
        // Keep handle used on all platforms.
        let _ = app_handle;
    });
}

fn build_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let lock_i = MenuItem::with_id(app, "lock", "Lock", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &lock_i, &quit_i])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("SecretFolder")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                show_main(app);
            }
            "lock" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut guard) = state.vault.lock() {
                        guard.lock();
                    }
                }
                let _ = app.emit("vault-locked", ());
                show_main(app);
            }
            "quit" => {
                // Scrub session then real exit.
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut guard) = state.vault.lock() {
                        guard.lock();
                    }
                }
                ALLOW_EXIT.store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

// Re-export emit trait used above.
use tauri::Emitter;
