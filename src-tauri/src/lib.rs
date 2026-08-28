#![allow(non_snake_case)] // crate name PastePaw is intentional
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    Manager,
};
use tauri_plugin_aptabase::EventTracker;
#[cfg(not(feature = "app-store"))]
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

static IS_ANIMATING: AtomicBool = AtomicBool::new(false);
static LAST_SHOW_TIME: AtomicI64 = AtomicI64::new(0);

mod ai;
mod clipboard;
mod commands;
mod constants;
mod database;
mod models;
mod settings_commands;
mod settings_manager;
pub mod updater;

use database::Database;
use models::get_runtime;
use settings_manager::SettingsManager;

pub fn run_app() {
    let data_dir = get_data_dir();
    fs::create_dir_all(&data_dir).ok();
    let db_path = data_dir.join("paste_paw.db");
    let db_path_str = db_path.to_str().unwrap_or("paste_paw.db").to_string();

    let rt = get_runtime().expect("Failed to get global tokio runtime");
    let _guard = rt.enter();

    let db = rt.block_on(async { Database::new(&db_path_str).await });

    rt.block_on(async {
        db.migrate().await.ok();
    });

    let db_arc = Arc::new(db);

    let mut log_builder = tauri_plugin_log::Builder::default()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}][{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .level_for("sqlx", log::LevelFilter::Warn);

    #[cfg(debug_assertions)]
    {
        log_builder = log_builder.targets([
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
        ]);
    }

    #[cfg(not(debug_assertions))]
    {
        log_builder = log_builder.targets([
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: None }),
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
        ]);
    }

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    #[cfg(not(feature = "app-store"))]
    {
        builder = builder
            .plugin(tauri_plugin_autostart::init(
                MacosLauncher::LaunchAgent,
                Some(vec!["--flag1", "--flag2"]),
            ))
            .plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .plugin(log_builder.build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("Second instance detected. Sending notification and exiting.");
            use tauri_plugin_notification::NotificationExt;
            if let Err(e) = app.notification()
                .builder()
                .title("PastePaw")
                .body("PastePaw is already running")
                .show() {
                log::error!("Failed to send notification: {:?}", e);
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_x::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_aptabase::Builder::new("A-US-2920723583").build())
        .manage(db_arc.clone())
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::ThemeChanged(theme) => {
                    log::info!("THEME:System theme changed to: {:?}, win.theme(): {:?}", theme, window.theme());
                    let label = window.label().to_string();
                    let app_handle = window.app_handle().clone();
                    let theme_ = theme.clone();

                    // Update tray icon to match new system theme
                    if let Some(tray) = app_handle.tray_by_id("main") {
                        update_tray_icon(&tray, &theme_);
                    }

                    // Use SettingsManager
                    let manager = window.state::<Arc<SettingsManager>>();
                    let settings = manager.get();

                    tauri::async_runtime::spawn(async move {
                        let current_theme = settings.theme;
                        let mica_effect = settings.mica_effect;
                        let round_corners = settings.round_corners;

                        log::info!("THEME:Re-applying window effect due to theme change. Current theme setting: {:?}, system theme: {:?}, mica_effect setting: {:?}", current_theme, theme_, mica_effect);
                        if let Some(webview_win) = app_handle.get_webview_window(&label) {
                            let effective_theme = get_effective_theme(&webview_win, &current_theme);
                            crate::apply_window_effect(&webview_win, &mica_effect, &effective_theme, round_corners);
                        }
                    });
                }
                tauri::WindowEvent::Focused(focused) => {
                    if !focused {
                        let label = window.label();
                        // Only auto-hide the main window
                        if label == "main" {
                            if window.app_handle().get_webview_window("settings").is_some() {
                                // Settings window is open, keep main window visible
                                return;
                            }

                            // Debounce: Ignore blur events immediately after showing
                            let last_show = LAST_SHOW_TIME.load(Ordering::SeqCst);
                            let now = chrono::Local::now().timestamp_millis();
                            let debounce_ms = 500;
                            if now - last_show < debounce_ms {
                                return;
                            }

                        if let Some(win) = window.app_handle().get_webview_window(label) {
                                 // Safety checks:
                                 // 1. If we are already animating (e.g. hiding via hotkey), don't interfere.
                                 if IS_ANIMATING.load(Ordering::SeqCst) {
                                     return;
                                 }
                                 // 2. If the window is not visible (e.g. just hidden programmatically), don't try to move/show it.
                                 if !win.is_visible().unwrap_or(false) {
                                     return;
                                 }

                                 // Check if cursor is on a different monitor
                                 let current_monitor = win.current_monitor().ok().flatten();
                                 let cursor_monitor = get_monitor_at_cursor(&win);

                                 let mut moved_screens = false;
                                 if let (Some(cm), Some(crm)) = (&current_monitor, &cursor_monitor) {
                                     if cm.position().x != crm.position().x || cm.position().y != crm.position().y {
                                         moved_screens = true;
                                     }
                                 }

                                 if moved_screens {
                                     // User clicked on another screen, move window there immediately
                                     position_window_at_bottom(&win);
                                     let _ = win.show();
                                     let _ = win.set_focus();
                                 } else {
                                     // Normal blur handling (hide)
                                     if win.is_visible().unwrap_or(false) {
                                         let win_clone = win.clone();
                                         std::thread::spawn(move || {
                                             crate::animate_window_hide(&win_clone, None);
                                         });
                                     }
                                 }
                            }
                        }
                    }
                }
                _ => {}
            }
        })
        .setup(move |app| {
            log::info!("PastePaw starting...");

            // Initialize Settings Manager
            let db_for_settings = db_arc.clone();
            let settings_manager = get_runtime().unwrap().block_on(async {
                SettingsManager::new(app.handle(), &db_for_settings).await
            });
            app.manage(Arc::new(settings_manager));

            // Initialize Update Manager and start background check loop
            let update_manager = Arc::new(updater::UpdateManager::new());
            update_manager.start_background_loop(app.handle().clone());
            app.manage(update_manager);

            let _ = app.track_event("startup", None);
            log::info!("Database path: {}", db_path_str);
            if let Ok(log_dir) = app.path().app_log_dir() {
                log::info!("Log directory: {:?}", log_dir);
            }
            let handle = app.handle().clone();
            let db_for_clipboard = db_arc.clone();

            let version = env!("CARGO_PKG_VERSION");
            let title = format!("v{}", version);
            let title_i = MenuItem::with_id(app, "title", &title, false, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit PastePaw", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let separator_i = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&title_i, &show_i, &separator_i, &quit_i])?;

            // Pick icon based on current system theme: white for dark, black for light
            let is_dark = dark_light::detect().map(|m| m == dark_light::Mode::Dark).unwrap_or(false);
            let icon_data: &[u8] = if is_dark {
                include_bytes!("../icons/tray_white.png")
            } else {
                include_bytes!("../icons/tray.png")
            };
            let icon = Image::from_bytes(icon_data).map_err(|e| {
                log::info!("Failed to load icon: {:?}", e);
                e
            })?;

            let tray_builder = TrayIconBuilder::with_id("main")
                .icon(icon)
                .menu(&menu);

            let _tray = tray_builder
                .tooltip("PastePaw")
                .on_menu_event(move |app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    } else if event.id.as_ref() == "show" {
                        if let Some(win) = app.get_webview_window("main") {
                            position_window_at_bottom(&win);
                        }
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                        if let Some(win) = tray.app_handle().get_webview_window("main") {
                            position_window_at_bottom(&win);
                        }
                    }
                })
                .build(app)?;

            let app_handle = handle.clone();
            let win = app_handle.get_webview_window("main").unwrap();

            {
                let manager = app_handle.state::<Arc<SettingsManager>>();
                let settings = manager.get();
                let mica_effect = settings.mica_effect;
                let theme = settings.theme;
                let round_corners = settings.round_corners;

                let current_theme = get_effective_theme(&win, &theme);

                log::info!("THEME:Applying window effect: {} with theme: {:?} (setting:{:?})", mica_effect, current_theme, theme);

                crate::apply_window_effect(&win, &mica_effect, &current_theme, round_corners);
            }

            // Load saved hotkey from database or use default
            let manager = app_handle.state::<Arc<SettingsManager>>();
            let saved_hotkey = manager.get().hotkey;

            log::info!("Registering hotkey: {}", saved_hotkey);

            // Parse the hotkey string into a Shortcut
            use std::str::FromStr;
            use tauri_plugin_global_shortcut::Shortcut;

            if let Ok(shortcut) = Shortcut::from_str(&saved_hotkey) {
                let win_clone = win.clone();
                let _ = app_handle.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if win_clone.is_visible().unwrap_or(false) && win_clone.is_focused().unwrap_or(false) {
                            crate::animate_window_hide(&win_clone, None);
                        } else {
                            position_window_at_bottom(&win_clone);
                        }
                    }
                });
            } else {
                log::error!("Failed to parse hotkey: {}", saved_hotkey);
            }

            let handle_for_clip = app_handle.clone();
            let db_for_clip = db_for_clipboard.clone();
            clipboard::init(&handle_for_clip, db_for_clip);

            // Start background image migration
            let db_for_migration = db_for_clipboard.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = commands::migrate_images_to_files(&db_for_migration.pool).await {
                    log::error!("Background image migration failed: {}", e);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_clips,
            commands::get_clip,
            commands::get_clip_detail,
            commands::paste_clip,
            commands::delete_clip,
            commands::move_to_folder,
            commands::create_folder,
            commands::rename_folder,
            commands::delete_folder,
            commands::search_clips,
            commands::get_folders,
            // Replaced by settings_commands
            settings_commands::get_settings,
            settings_commands::save_settings,
            commands::hide_window,
            commands::get_clipboard_history_size,
            commands::clear_clipboard_history,
            commands::clear_all_clips,
            commands::remove_duplicate_clips,
            commands::register_global_shortcut,
            commands::show_window,
            settings_commands::add_ignored_app,
            settings_commands::remove_ignored_app,
            settings_commands::get_ignored_apps,
            commands::pick_file,
            commands::get_layout_config,
            commands::test_log,
            commands::ai_process_clip,
            commands::focus_window,
            commands::refresh_window,
            commands::get_available_update,
            commands::check_update_now,
            commands::install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn get_effective_theme(window: &tauri::WebviewWindow, theme_setting: &str) -> tauri::Theme {
    match theme_setting {
        "dark" => tauri::Theme::Dark,
        "light" => tauri::Theme::Light,
        _ => {
            if let Ok(mode) = dark_light::detect() {
                match mode {
                    dark_light::Mode::Dark => tauri::Theme::Dark,
                    _ => tauri::Theme::Light,
                }
            } else {
                window.theme().unwrap_or(tauri::Theme::Light)
            }
        }
    }
}

pub fn position_window_at_bottom(window: &tauri::WebviewWindow) {
    let (mica_effect, theme_setting, round_corners) = {
        let manager = window.state::<Arc<crate::settings_manager::SettingsManager>>();
        let s = manager.get();
        (s.mica_effect, s.theme, s.round_corners)
    };
    let effective_theme = get_effective_theme(window, &theme_setting);
    apply_window_effect(window, &mica_effect, &effective_theme, round_corners);

    animate_window_show(window);
}

pub fn animate_window_show(window: &tauri::WebviewWindow) {
    // Atomically check if false and set to true. If already true, return.
    if IS_ANIMATING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    LAST_SHOW_TIME.store(chrono::Local::now().timestamp_millis(), Ordering::SeqCst);

    let window = window.clone();

    let (side_margin, bottom_margin, float_above_taskbar, card_size) = {
        let manager = window.state::<Arc<crate::settings_manager::SettingsManager>>();
        let s = manager.get();
        let is_mica = s.mica_effect != "clear";
        let no_corners = !s.round_corners;
        let side = if is_mica && no_corners {
            0.0
        } else {
            constants::WINDOW_MARGIN
        };
        let bottom = if is_mica && no_corners {
            0.0
        } else {
            constants::WINDOW_MARGIN
        };
        (side, bottom, s.float_above_taskbar, s.card_size.clone())
    };

    std::thread::spawn(move || {
        if let Some(monitor) = get_monitor_at_cursor(&window) {
            let scale_factor = monitor.scale_factor();
            let monitor_pos = monitor.position();
            let monitor_size = monitor.size();
            let work_area = monitor.work_area();

            let base_height = match card_size.as_str() {
                "medium" => 236.0,
                _ => constants::WINDOW_HEIGHT, // 266.0
            };
            let window_height_px = (base_height * scale_factor).round() as u32;
            let side_margin_px = (side_margin * scale_factor) as i32;
            let bottom_margin_px = (bottom_margin * scale_factor) as i32;

            // Use full monitor height when floating above taskbar, otherwise work area
            let reference_bottom = if float_above_taskbar {
                monitor_pos.y + monitor_size.height as i32
            } else {
                work_area.position.y + work_area.size.height as i32
            };

            let target_width = (work_area.size.width as i32 - side_margin_px * 2).max(1) as i32;
            let target_height = window_height_px as i32;
            let target_x = work_area.position.x + side_margin_px;

            let target_y = reference_bottom - target_height - bottom_margin_px;
            let start_y = reference_bottom;

            // Atomically set initial position and size at target monitor coordinates
            // to avoid DPI double-scaling bugs when moving across monitors with different DPIs.
            if let Ok(handle) = window.hwnd() {
                use windows::Win32::Foundation::HWND;
                use windows::Win32::UI::WindowsAndMessaging::{
                    SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
                };
                let hwnd = HWND(handle.0 as _);
                let z_order = if float_above_taskbar {
                    Some(HWND(-1 as _)) // HWND_TOPMOST
                } else {
                    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
                    let taskbar_hwnd =
                        unsafe { FindWindowW(windows::core::w!("Shell_TrayWnd"), None).ok() };
                    if let Some(hwnd) = taskbar_hwnd {
                        Some(hwnd)
                    } else {
                        Some(HWND(1 as _)) // HWND_BOTTOM
                    }
                };
                let flags = SWP_NOACTIVATE
                    | if z_order.is_some() {
                        windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(0)
                    } else {
                        SWP_NOZORDER
                    };
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        z_order,
                        target_x,
                        start_y,
                        target_width,
                        target_height,
                        flags,
                    );
                }
            } else {
                let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                    width: target_width as u32,
                    height: target_height as u32,
                }));
                let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                    x: target_x,
                    y: start_y,
                }));
            }

            let _ = window.show();
            let _ = window.set_focus();

            // When floating above taskbar, ensure window stays on top
            if float_above_taskbar {
                if let Ok(handle) = window.hwnd() {
                    use windows::Win32::Foundation::HWND;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                    };
                    let hwnd = HWND(handle.0 as _);
                    let hwnd_topmost = HWND(-1 as _); // HWND_TOPMOST
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            Some(hwnd_topmost),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                }
            }

            let steps = 15;
            let duration = std::time::Duration::from_millis(10);
            let dy = (target_y - start_y) as f64 / steps as f64;

            for i in 1..=steps {
                let current_y = (start_y as f64 + dy * i as f64) as i32;
                if let Ok(handle) = window.hwnd() {
                    use windows::Win32::Foundation::HWND;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
                    };
                    let hwnd = HWND(handle.0 as _);
                    unsafe {
                        let _ = SetWindowPos(
                            hwnd,
                            None,
                            target_x,
                            current_y,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                } else {
                    let _ =
                        window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                            x: target_x,
                            y: current_y,
                        }));
                }
                std::thread::sleep(duration);
            }

            // Ensure final position is exact
            if let Ok(handle) = window.hwnd() {
                use windows::Win32::Foundation::HWND;
                use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE};
                let hwnd = HWND(handle.0 as _);
                let hwnd_topmost = HWND(-1 as _); // HWND_TOPMOST
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(hwnd_topmost),
                        target_x,
                        target_y,
                        target_width,
                        target_height,
                        SWP_NOACTIVATE,
                    );
                }
            } else {
                let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                    x: target_x,
                    y: target_y,
                }));
            }
        }
        IS_ANIMATING.store(false, Ordering::SeqCst);
    });
}

pub fn animate_window_hide(
    window: &tauri::WebviewWindow,
    on_done: Option<Box<dyn FnOnce() + Send>>,
) {
    // Atomically check if false and set to true. If already true, return.
    if IS_ANIMATING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let (side_margin, bottom_margin, float_above_taskbar, card_size) = {
        let manager = window.state::<Arc<crate::settings_manager::SettingsManager>>();
        let s = manager.get();
        let is_mica = s.mica_effect != "clear";
        let no_corners = !s.round_corners;
        let side = if is_mica && no_corners {
            0.0
        } else {
            constants::WINDOW_MARGIN
        };
        let bottom = if is_mica && no_corners {
            0.0
        } else {
            constants::WINDOW_MARGIN
        };
        (side, bottom, s.float_above_taskbar, s.card_size.clone())
    };

    let window = window.clone();

    std::thread::spawn(move || {
        let monitor = window
            .current_monitor()
            .ok()
            .flatten()
            .or_else(|| get_monitor_at_cursor(&window));

        if let Some(monitor) = monitor {
            let scale_factor = monitor.scale_factor();
            let monitor_pos = monitor.position();
            let monitor_size = monitor.size();
            let work_area = monitor.work_area();

            let base_height = match card_size.as_str() {
                "medium" => 236.0,
                _ => constants::WINDOW_HEIGHT, // 266.0
            };
            let window_height_px = (base_height * scale_factor).round() as u32;
            let side_margin_px = (side_margin * scale_factor) as i32;
            let bottom_margin_px = (bottom_margin * scale_factor) as i32;

            let reference_bottom = if float_above_taskbar {
                monitor_pos.y + monitor_size.height as i32
            } else {
                work_area.position.y + work_area.size.height as i32
            };

            let target_x = work_area.position.x + side_margin_px;
            let start_y = reference_bottom - window_height_px as i32 - bottom_margin_px;
            let target_y = reference_bottom;

            let steps = 15;
            let duration = std::time::Duration::from_millis(10);
            let dy = (target_y - start_y) as f64 / steps as f64;

            if let Ok(handle) = window.hwnd() {
                use windows::Win32::Foundation::HWND;
                use windows::Win32::UI::WindowsAndMessaging::{
                    SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                };
                let hwnd = HWND(handle.0 as _);
                let hwnd_topmost = HWND(-1 as _); // HWND_TOPMOST
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(hwnd_topmost),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }

            let mut dropped_z = false;
            let taskbar_hwnd = {
                use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
                unsafe { FindWindowW(windows::core::w!("Shell_TrayWnd"), None).ok() }
            };

            for i in 1..=steps {
                let current_y = (start_y as f64 + dy * i as f64) as i32;

                let mut z_order = None;
                if !float_above_taskbar && !dropped_z {
                    // Check if window's bottom has reached taskbar top
                    let window_bottom = current_y + window_height_px as i32;
                    let taskbar_top = work_area.position.y + work_area.size.height as i32;
                    if window_bottom > taskbar_top {
                        if let Some(hwnd) = taskbar_hwnd {
                            z_order = Some(hwnd);
                        } else {
                            use windows::Win32::Foundation::HWND;
                            z_order = Some(HWND(1 as _)); // HWND_BOTTOM
                        }
                        dropped_z = true;
                    }
                }

                if let Ok(handle) = window.hwnd() {
                    use windows::Win32::Foundation::HWND;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
                    };
                    let hwnd = HWND(handle.0 as _);
                    let flags = SWP_NOSIZE
                        | SWP_NOACTIVATE
                        | if z_order.is_some() {
                            windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS(0)
                        } else {
                            SWP_NOZORDER
                        };
                    unsafe {
                        let _ = SetWindowPos(hwnd, z_order, target_x, current_y, 0, 0, flags);
                    }
                } else {
                    let _ =
                        window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                            x: target_x,
                            y: current_y,
                        }));
                }
                std::thread::sleep(duration);
            }

            let _ = window.hide();
        } else {
            let _ = window.hide();
        }
        IS_ANIMATING.store(false, Ordering::SeqCst);

        if let Some(callback) = on_done {
            callback();
        }
    });
}

fn get_data_dir() -> std::path::PathBuf {
    let current_dir = std::env::current_dir().unwrap_or(std::path::PathBuf::from("."));
    match dirs::data_dir() {
        Some(path) => path.join("PastePaw"),
        None => current_dir.join("PastePaw"),
    }
}

pub fn get_monitor_at_cursor(window: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point).is_ok() } {
        let hmonitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        if !hmonitor.is_invalid() {
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if unsafe { GetMonitorInfoW(hmonitor, &mut info).as_bool() } {
                if let Ok(monitors) = window.available_monitors() {
                    // Try exact match by monitor rectangle coordinates
                    for m in &monitors {
                        let pos = m.position();
                        if pos.x == info.rcMonitor.left && pos.y == info.rcMonitor.top {
                            return Some(m.clone());
                        }
                    }
                    // Fallback to bounding box check against cursor point
                    for m in &monitors {
                        let pos = m.position();
                        let size = m.size();
                        if point.x >= pos.x
                            && point.x < pos.x + size.width as i32
                            && point.y >= pos.y
                            && point.y < pos.y + size.height as i32
                        {
                            return Some(m.clone());
                        }
                    }
                }
            }
        }
    }

    window.current_monitor().ok().flatten()
}

pub fn apply_window_effect(
    window: &tauri::WebviewWindow,
    effect: &str,
    theme: &tauri::Theme,
    round_corners: bool,
) {
    log::info!(
        "THEME:apply_window_effect called: effect={}, theme={:?}, round_corners={}",
        effect,
        theme,
        round_corners
    );
    use window_vibrancy::{apply_mica, apply_tabbed, clear_mica};

    match effect {
        "clear" => {
            if let Err(e) = clear_mica(window) {
                log::error!("THEME:Failed to clear mica: {:?}", e);
            }
            log::info!("THEME:Mica effect cleared");
        }
        "mica" | "dark" => {
            if let Err(e) = clear_mica(window) {
                log::error!("THEME:Failed to clear mica: {:?}", e);
            }
            if let Err(e) = apply_mica(window, Some(matches!(theme, tauri::Theme::Dark))) {
                log::error!("THEME:Failed to apply mica: {:?}", e);
            }
            log::info!("THEME:Applied Mica effect (Theme: {})", theme);
        }
        "mica_alt" | "auto" | _ => {
            if let Err(e) = clear_mica(window) {
                log::error!("THEME:Failed to clear mica: {:?}", e);
            }
            if let Err(e) = apply_tabbed(window, Some(matches!(theme, tauri::Theme::Dark))) {
                log::error!("THEME:Failed to apply tabbed: {:?}", e);
            }
            log::info!("THEME:Applied Tabbed effect (Theme: {})", theme);
        }
    }

    // Apply DWM rounded corners on Windows 11.
    // "clear" always rounds; Mica/Mica-Alt follow the user setting.
    let use_rounded = effect == "clear" || round_corners;
    if let Ok(handle) = window.hwnd() {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND,
        };
        let hwnd = HWND(handle.0 as _);
        let corner_pref = if use_rounded {
            DWMWCP_ROUND.0
        } else {
            DWMWCP_DONOTROUND.0
        };
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner_pref as *const _ as *const _,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

pub fn update_tray_icon(tray: &TrayIcon, theme: &tauri::Theme) {
    let icon_data: &[u8] = match theme {
        tauri::Theme::Dark => include_bytes!("../icons/tray_white.png"),
        _ => include_bytes!("../icons/tray.png"),
    };
    if let Ok(icon) = Image::from_bytes(icon_data) {
        let _ = tray.set_icon(Some(icon));
    }
}
pub fn update_window_size(window: &tauri::WebviewWindow) {
    let (side_margin, bottom_margin, float_above_taskbar, card_size) = {
        let manager = window.state::<std::sync::Arc<crate::settings_manager::SettingsManager>>();
        let s = manager.get();
        let is_mica = s.mica_effect != "clear";
        let no_corners = !s.round_corners;
        let side = if is_mica && no_corners { 0.0 } else { constants::WINDOW_MARGIN };
        let bottom = if is_mica && no_corners { 0.0 } else { constants::WINDOW_MARGIN };
        (side, bottom, s.float_above_taskbar, s.card_size.clone())
    };

    if let Some(monitor) = window.current_monitor().ok().flatten().or_else(|| crate::get_monitor_at_cursor(&window)) {
        let scale_factor = monitor.scale_factor();
        let monitor_pos = monitor.position();
        let monitor_size = monitor.size();
        let work_area = monitor.work_area();

        let base_height = match card_size.as_str() {
            "medium" => 236.0,
            _ => constants::WINDOW_HEIGHT,
        };
        let window_height_px = (base_height * scale_factor).round() as u32;
        let side_margin_px = (side_margin * scale_factor) as i32;
        let bottom_margin_px = (bottom_margin * scale_factor) as i32;

        let reference_bottom = if float_above_taskbar {
            monitor_pos.y + monitor_size.height as i32
        } else {
            work_area.position.y + work_area.size.height as i32
        };

        let target_width = (work_area.size.width as i32 - side_margin_px * 2).max(1) as i32;
        let target_height = window_height_px as i32;
        let target_x = work_area.position.x + side_margin_px;
        let target_y = reference_bottom - target_height - bottom_margin_px;

        if let Ok(handle) = window.hwnd() {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
            let hwnd = HWND(handle.0 as _);
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    target_x,
                    target_y,
                    target_width,
                    target_height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        } else {
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: target_x, y: target_y }));
            let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width: target_width as u32, height: target_height as u32 }));
        }
    }
}
