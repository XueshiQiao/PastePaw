use tauri::{AppHandle, Emitter, Listener};
// Import functions directly from the crate root
use crate::database::Database;
#[cfg(target_os = "windows")]
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use clipboard_rs::common::RustImage;
use clipboard_rs::{Clipboard, ClipboardContext};
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
#[cfg(target_os = "windows")]
use std::ffi::OsStr;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;
use tauri_plugin_clipboard_x::{read_text, start_listening};
use uuid::Uuid;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetObjectW, ReleaseDC, SelectObject, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP,
};
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::ProcessStatus::{GetModuleBaseNameW, GetModuleFileNameExW};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_CONTROL, VK_INSERT,
    VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SHIFT,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, DrawIconEx, GetForegroundWindow, GetIconInfo, GetWindowThreadProcessId, DI_NORMAL,
    ICONINFO,
};

// GLOBAL STATE: Store the hash of the clip we just pasted ourselves.
// If the next clipboard change matches this hash, we ignore it (don't update timestamp).
static IGNORE_HASH: Lazy<parking_lot::Mutex<Option<String>>> =
    Lazy::new(|| parking_lot::Mutex::new(None));
static LAST_STABLE_HASH: Lazy<parking_lot::Mutex<Option<String>>> =
    Lazy::new(|| parking_lot::Mutex::new(None));
pub static CLIPBOARD_SYNC: Lazy<Arc<tokio::sync::Mutex<()>>> =
    Lazy::new(|| Arc::new(tokio::sync::Mutex::new(())));

use std::sync::atomic::{AtomicU64, Ordering};
static DEBOUNCE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn set_ignore_hash(hash: String) {
    let mut lock = IGNORE_HASH.lock();
    *lock = Some(hash);
}

pub fn set_last_stable_hash(hash: String) {
    let mut lock = LAST_STABLE_HASH.lock();
    *lock = Some(hash);
}

pub fn compute_stable_hash(content_hash: &str, html: Option<&[u8]>, rtf: Option<&[u8]>) -> String {
    if html.is_none() && rtf.is_none() {
        return content_hash.to_string();
    }
    let html_hash = html.map(calculate_hash).unwrap_or_default();
    let rtf_hash = rtf.map(calculate_hash).unwrap_or_default();
    calculate_hash(format!("{}|html:{}|rtf:{}", content_hash, html_hash, rtf_hash).as_bytes())
}

fn build_text_metadata(
    existing_meta: Option<&str>,
    has_html: bool,
    has_rtf: bool,
) -> Option<String> {
    let mut val: serde_json::Value = existing_meta
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    if let Some(obj) = val.as_object_mut() {
        if has_html || has_rtf {
            obj.insert("has_rich_text".to_string(), serde_json::json!(true));
            obj.insert("has_html".to_string(), serde_json::json!(has_html));
            obj.insert("has_rtf".to_string(), serde_json::json!(has_rtf));
        } else {
            obj.remove("has_rich_text");
            obj.remove("has_html");
            obj.remove("has_rtf");
        }
        if obj.is_empty() {
            None
        } else {
            Some(val.to_string())
        }
    } else {
        None
    }
}

pub fn init(app: &AppHandle, db: Arc<Database>) {
    let app_clone = app.clone();
    let db_clone = db.clone();

    // Start monitor
    // tauri-plugin-clipboard-x exposes start_listening(app_handle)
    // It returns impl Future, so we need to spawn it or block.
    // Since init is synchronous here, we spawn it.
    let app_for_start = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = start_listening(app_for_start).await {
            log::error!("CLIPBOARD: Failed to start listener: {}", e);
        }
    });

    // Listen to clipboard changes
    // The event name found in source code: "plugin:clipboard-x://clipboard_changed"
    let event_name = "plugin:clipboard-x://clipboard_changed";

    app.listen(event_name, move |_event| {
        let app = app_clone.clone();
        let db = db_clone.clone();

        // Capture source app info IMMEDIATELY at event time, before debounce delay.
        // If we wait until after the delay, the user may have already switched to PastePaw,
        // causing frontmostApplication to return our own app instead of the real source.
        let source_app_info = get_clipboard_owner_app_info();

        // DEBOUNCE LOGIC:
        let current_count = DEBOUNCE_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;

        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;

            if DEBOUNCE_COUNTER.load(Ordering::SeqCst) != current_count {
                log::debug!(
                    "CLIPBOARD: Debounce: Aborting older event, current_count:{}",
                    current_count
                );
                return;
            }

            process_clipboard_change(app, db, source_app_info).await;
        });
    });
}

type SourceAppInfo = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
);

struct ClipboardImageRead {
    png_bytes: Vec<u8>,
    width: u32,
    height: u32,
    raw_hash: String,
    decode_ms: u128,
    source_type: &'static str,
}

fn read_clipboard_image_with_clipboard_rs(
    source_type: &'static str,
) -> Result<ClipboardImageRead, String> {
    let ctx = ClipboardContext::new().map_err(|e| e.to_string())?;
    let image = ctx.get_image().map_err(|e| e.to_string())?;
    let (width, height) = image.get_size();

    let dynamic_image = image.get_dynamic_image().map_err(|e| e.to_string())?;
    let raw_hash = calculate_hash(dynamic_image.as_bytes());

    let png_bytes = image
        .to_png()
        .map_err(|e| e.to_string())?
        .get_bytes()
        .to_vec();

    Ok(ClipboardImageRead {
        png_bytes,
        width,
        height,
        raw_hash,
        decode_ms: 0,
        source_type,
    })
}

fn read_clipboard_image_fast() -> Result<ClipboardImageRead, String> {
    read_clipboard_image_with_clipboard_rs("clipboard-rs-image")
}

async fn process_clipboard_change(
    app: AppHandle,
    db: Arc<Database>,
    source_app_info: SourceAppInfo,
) {
    let started = std::time::Instant::now();
    let mut image_read_ms = 0u128;
    let mut image_decode_ms = 0u128;
    let mut text_read_ms = 0u128;
    let mut was_existing = false;
    let _guard = CLIPBOARD_SYNC.lock().await;

    let mut clip_type = "text";
    let mut clip_content = Vec::new();
    let mut full_image_content: Option<Vec<u8>> = None;
    let mut rich_html_content: Option<Vec<u8>> = None;
    let mut rich_rtf_content: Option<Vec<u8>> = None;
    let mut clip_preview = String::new();
    let mut clip_hash = String::new();
    let mut metadata = String::new();
    let mut found_content = false;

    // Check settings (cached via SettingsManager)
    use crate::settings_manager::SettingsManager;
    use tauri::Manager;
    let manager = app.state::<Arc<SettingsManager>>();
    let settings = manager.get();

    // Try Image (in-memory path, no temp file write).
    log::debug!("CLIPBOARD: Attempting to read image from clipboard");
    let image_read_started = std::time::Instant::now();
    if let Ok(read_image_result) = read_clipboard_image_fast() {
        image_read_ms = image_read_started.elapsed().as_millis();
        log::debug!(
            "CLIPBOARD: Image read successfully, source_type={}, takes {} ms",
            read_image_result.source_type,
            image_read_ms
        );

        let bytes = read_image_result.png_bytes;
        let width = read_image_result.width;
        let height = read_image_result.height;
        image_decode_ms = read_image_result.decode_ms;
        let size_bytes = bytes.len();
        clip_hash = read_image_result.raw_hash;
        clip_content = Vec::new();
        full_image_content = Some(bytes);
        clip_type = "image";
        clip_preview = "[Image]".to_string();
        metadata = serde_json::json!({
            "width": width,
            "height": height,
            "format": "png",
            "size_bytes": size_bytes
        })
        .to_string();
        found_content = true;
        log::debug!(
            "CLIPBOARD: Found image: {}x{}, source_type={}, png_bytes={}",
            width,
            height,
            read_image_result.source_type,
            size_bytes
        );
    }

    if !found_content {
        // Try Text (and optional HTML / RTF rich formats)
        let text_read_started = std::time::Instant::now();
        if let Ok(text) = read_text().await {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                if settings.preserve_formatting {
                    let (html, rtf) = read_clipboard_rich_formats();
                    rich_html_content = html;
                    rich_rtf_content = rtf;
                }
                text_read_ms = text_read_started.elapsed().as_millis();
                clip_content = trimmed.as_bytes().to_vec();
                clip_hash = calculate_hash(&clip_content);
                clip_type = "text";
                clip_preview = trimmed.chars().take(200).collect::<String>();
                found_content = true;
                log::debug!(
                    "CLIPBOARD: Found text (html={}, rtf={}): {}",
                    rich_html_content.is_some(),
                    rich_rtf_content.is_some(),
                    clip_preview
                );
            }
        }
    }

    if !found_content {
        return;
    }

    let stable_hash = compute_stable_hash(
        &clip_hash,
        rich_html_content.as_deref(),
        rich_rtf_content.as_deref(),
    );

    // Stable Hash Check
    {
        let mut lock = LAST_STABLE_HASH.lock();
        if let Some(ref last_hash) = *lock {
            if last_hash == &stable_hash {
                return;
            }
        }
        *lock = Some(stable_hash);
    }

    // Check ignore self-paste
    let mut is_self_paste = false;
    {
        let mut lock = IGNORE_HASH.lock();
        if let Some(ignore_hash) = lock.take() {
            if ignore_hash == clip_hash {
                is_self_paste = true;
                if !settings.move_to_top_on_paste {
                    log::info!(
                        "CLIPBOARD: Ignoring self-paste timestamp update for hash {} (move_to_top_on_paste=false)",
                        ignore_hash
                    );
                    return;
                }
                log::info!(
                    "CLIPBOARD: Detected self-paste for hash {}, proceeding to update timestamp",
                    ignore_hash
                );
            }
        }
    }

    // Source app info was captured at event time (before debounce) to avoid race conditions
    let (source_app, source_icon, exe_name, full_path, is_explicit_owner) = source_app_info;
    log::info!(
        "CLIPBOARD: Source app: {:?}, exe_name: {:?}, full_path: {:?}, explicit: {}",
        source_app,
        exe_name,
        full_path,
        is_explicit_owner
    );

    if settings.ignore_ghost_clips && !is_explicit_owner {
        log::info!("CLIPBOARD: Ignoring ghost clip (unknown owner)");
        return;
    }

    // Check if the app is in the ignore list (Case Insensitive)
    let is_ignored = |name: &str| {
        let name_lower = name.to_lowercase();
        settings
            .ignored_apps
            .iter()
            .any(|app| app.to_lowercase() == name_lower)
    };

    if let Some(ref path) = full_path {
        if is_ignored(path) {
            log::info!(
                "CLIPBOARD: Ignoring content from ignored app (path match): {}",
                path
            );
            return;
        }
    }

    if let Some(ref exe) = exe_name {
        if is_ignored(exe) {
            log::info!(
                "CLIPBOARD: Ignoring content from ignored app (exe match): {}",
                exe
            );
            return;
        }
    }

    // DB Logic
    let pool = &db.pool;

    let db_lookup_started = std::time::Instant::now();
    let existing_clip: Option<(String, Option<String>)> =
        sqlx::query_as::<_, (String, Option<String>)>(
            r#"SELECT uuid, metadata FROM clips WHERE content_hash = ?"#,
        )
        .bind(&clip_hash)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    let db_lookup_ms = db_lookup_started.elapsed().as_millis();

    let has_html = rich_html_content.is_some();
    let has_rtf = rich_rtf_content.is_some();

    let is_own_app = exe_name
        .as_deref()
        .map(|e| {
            e.eq_ignore_ascii_case("PastePaw.exe") || e.eq_ignore_ascii_case("msedgewebview2.exe")
        })
        .unwrap_or(false);

    let db_write_started = std::time::Instant::now();
    let emitted_id = if let Some((existing_id, existing_meta)) = existing_clip {
        if is_self_paste || is_own_app {
            if !settings.move_to_top_on_paste {
                log::info!(
                    "CLIPBOARD: Skipping bump for existing clip {} from own paste (move_to_top_on_paste=false)",
                    existing_id
                );
                return;
            }
            was_existing = true;
            let _ = sqlx::query(
                r#"UPDATE clips SET created_at = CURRENT_TIMESTAMP, is_deleted = 0 WHERE uuid = ?"#,
            )
            .bind(&existing_id)
            .execute(pool)
            .await;
            existing_id
        } else {
            was_existing = true;
            if clip_type == "image" {
                let _ = sqlx::query(
                    r#"
                UPDATE clips
                SET created_at = CURRENT_TIMESTAMP,
                    is_deleted = 0,
                    source_app = ?,
                    source_icon = ?,
                    content = ?,
                    text_preview = ?,
                    metadata = ?,
                    is_thumbnail = 0
                WHERE uuid = ?
                "#,
                )
                .bind(&source_app)
                .bind(&source_icon)
                .bind(&clip_content)
                .bind(&clip_preview)
                .bind(Some(metadata.clone()))
                .bind(&existing_id)
                .execute(pool)
                .await;

                if let Some(full_bytes) = &full_image_content {
                    match persist_full_image_file(&existing_id, full_bytes) {
                        Ok(file_path) => {
                            let _ = sqlx::query(
                            r#"
                            INSERT OR REPLACE INTO clip_images (clip_uuid, full_content, file_path, file_size, storage_kind, mime_type, created_at)
                            VALUES (?, x'', ?, ?, 'file', 'image/png', CURRENT_TIMESTAMP)
                            "#,
                        )
                        .bind(&existing_id)
                        .bind(&file_path)
                        .bind(full_bytes.len() as i64)
                        .execute(pool)
                        .await;
                        }
                        Err(e) => {
                            log::error!(
                                "Failed to persist full image file for existing clip {}: {}",
                                existing_id,
                                e
                            );
                        }
                    }
                }
            } else {
                let updated_meta = build_text_metadata(existing_meta.as_deref(), has_html, has_rtf);
                let _ = sqlx::query(
                r#"UPDATE clips SET created_at = CURRENT_TIMESTAMP, is_deleted = 0, source_app = ?, source_icon = ?, metadata = ? WHERE uuid = ?"#,
            )
            .bind(&source_app)
            .bind(&source_icon)
            .bind(&updated_meta)
            .bind(&existing_id)
            .execute(pool)
            .await;

                if has_html || has_rtf {
                    let _ = sqlx::query(
                    r#"
                    INSERT OR REPLACE INTO clip_formats (clip_uuid, html_content, rtf_content, created_at)
                    VALUES (?, ?, ?, CURRENT_TIMESTAMP)
                    "#,
                )
                .bind(&existing_id)
                .bind(&rich_html_content)
                .bind(&rich_rtf_content)
                .execute(pool)
                .await;
                } else {
                    let _ = sqlx::query(r#"DELETE FROM clip_formats WHERE clip_uuid = ?"#)
                        .bind(&existing_id)
                        .execute(pool)
                        .await;
                }
            }
            existing_id
        }
    } else {
        let clip_uuid = Uuid::new_v4().to_string();
        let initial_meta = if clip_type == "image" {
            Some(metadata)
        } else {
            build_text_metadata(None, has_html, has_rtf)
        };

        let _ = sqlx::query(
            r#"
            INSERT INTO clips (uuid, clip_type, content, text_preview, content_hash, folder_id, is_deleted, is_thumbnail, source_app, source_icon, metadata, created_at, last_accessed)
            VALUES (?, ?, ?, ?, ?, NULL, 0, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(&clip_uuid)
        .bind(clip_type)
        .bind(&clip_content)
        .bind(&clip_preview)
        .bind(&clip_hash)
        .bind(false)
        .bind(&source_app)
        .bind(&source_icon)
        .bind(initial_meta)
        .execute(pool)
        .await;

        if clip_type == "image" {
            if let Some(full_bytes) = &full_image_content {
                match persist_full_image_file(&clip_uuid, full_bytes) {
                    Ok(file_path) => {
                        let _ = sqlx::query(
                            r#"
                            INSERT OR REPLACE INTO clip_images (clip_uuid, full_content, file_path, file_size, storage_kind, mime_type, created_at)
                            VALUES (?, x'', ?, ?, 'file', 'image/png', CURRENT_TIMESTAMP)
                            "#,
                        )
                        .bind(&clip_uuid)
                        .bind(&file_path)
                        .bind(full_bytes.len() as i64)
                        .execute(pool)
                        .await;
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to persist full image file for new clip {}, dropping clip: {}",
                            clip_uuid,
                            e
                        );
                        let _ = sqlx::query(r#"DELETE FROM clips WHERE uuid = ?"#)
                            .bind(&clip_uuid)
                            .execute(pool)
                            .await;
                        return;
                    }
                }
            }
        } else if has_html || has_rtf {
            let _ = sqlx::query(
                r#"
                INSERT OR REPLACE INTO clip_formats (clip_uuid, html_content, rtf_content, created_at)
                VALUES (?, ?, ?, CURRENT_TIMESTAMP)
                "#,
            )
            .bind(&clip_uuid)
            .bind(&rich_html_content)
            .bind(&rich_rtf_content)
            .execute(pool)
            .await;
        }
        clip_uuid
    };
    let db_write_ms = db_write_started.elapsed().as_millis();

    let emit_started = std::time::Instant::now();
    let _ = app.emit(
        "clipboard-change",
        &serde_json::json!({
            "id": emitted_id,
            "content": clip_preview,
            "clip_type": clip_type,
            "source_app": source_app,
            "source_icon": source_icon,
            "created_at": chrono::Utc::now().to_rfc3339()
        }),
    );
    let emit_ms = emit_started.elapsed().as_millis();

    log::info!(
        "[perf][clipboard_ingest] type={} existing={} full_bytes={} thumb_bytes={} image_read_ms={} decode_ms={} text_read_ms={} db_lookup_ms={} db_write_ms={} emit_ms={} total_ms={}",
        clip_type,
        was_existing,
        full_image_content.as_ref().map(|v| v.len()).unwrap_or(0),
        if clip_type == "image" { clip_content.len() } else { 0 },
        image_read_ms,
        image_decode_ms,
        text_read_ms,
        db_lookup_ms,
        db_write_ms,
        emit_ms,
        started.elapsed().as_millis()
    );
}
fn calculate_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let result = hasher.finalize();
    format!("{:x}", result)
}

fn get_image_store_dir() -> std::path::PathBuf {
    let current_dir = std::env::current_dir().unwrap_or(std::path::PathBuf::from("."));
    let app_data_dir = match dirs::data_dir() {
        Some(path) => path.join("PastePaw"),
        None => current_dir.join("PastePaw"),
    };
    app_data_dir.join("images")
}

pub fn persist_full_image_file(clip_uuid: &str, png_bytes: &[u8]) -> Result<String, String> {
    let dir = get_image_store_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file_path = dir.join(format!("{}.png", clip_uuid));
    std::fs::write(&file_path, png_bytes).map_err(|e| e.to_string())?;
    Ok(file_path.to_string_lossy().to_string())
}

pub fn read_full_image_file(file_path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(file_path).map_err(|e| e.to_string())
}

pub fn remove_full_image_file(file_path: &str) {
    if let Err(e) = std::fs::remove_file(file_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("Failed to delete image file {}: {}", file_path, e);
        }
    }
}

#[cfg(target_os = "windows")]
fn get_clipboard_owner_app_info() -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
) {
    unsafe {
        let (hwnd, is_explicit) = match GetClipboardOwner() {
            Ok(h) if !h.0.is_null() => (h, true),
            Err(e) => {
                log::info!(
                    "CLIPBOARD: GetClipboardOwner failed: {:?}, falling back to foreground window",
                    e
                );
                (GetForegroundWindow(), false)
            }
            Ok(_) => {
                log::info!(
                    "CLIPBOARD: GetClipboardOwner returned null, falling back to foreground window"
                );
                (GetForegroundWindow(), false)
            }
        };

        if hwnd.0.is_null() {
            return (None, None, None, None, false);
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        if process_id == 0 {
            return (None, None, None, None, false);
        }

        let process_handle = match OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            process_id,
        ) {
            Ok(h) => h,
            Err(_) => return (None, None, None, None, false),
        };

        let mut name_buffer = [0u16; MAX_PATH as usize];
        let name_size = GetModuleBaseNameW(process_handle, None, &mut name_buffer);
        let exe_name = if name_size > 0 {
            String::from_utf16_lossy(&name_buffer[..name_size as usize])
        } else {
            String::new()
        };

        let mut path_buffer = [0u16; MAX_PATH as usize];
        let path_size = GetModuleFileNameExW(Some(process_handle), None, &mut path_buffer);
        let (app_name, app_icon, full_path) = if path_size > 0 {
            let full_path_str = String::from_utf16_lossy(&path_buffer[..path_size as usize]);

            let desc = get_app_description(&full_path_str);
            let final_name = if let Some(d) = desc {
                Some(d)
            } else {
                if !exe_name.is_empty() {
                    Some(exe_name.clone())
                } else {
                    None
                }
            };

            let icon = extract_icon(&full_path_str);
            (final_name, icon, Some(full_path_str))
        } else {
            (
                if !exe_name.is_empty() {
                    Some(exe_name.clone())
                } else {
                    None
                },
                None,
                None,
            )
        };

        let exe_val = if !exe_name.is_empty() {
            Some(exe_name)
        } else {
            None
        };
        (app_name, app_icon, exe_val, full_path, is_explicit)
    }
}

#[cfg(target_os = "windows")]
unsafe fn get_app_description(path: &str) -> Option<String> {
    use std::ffi::c_void;

    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let size = GetFileVersionInfoSizeW(windows::core::PCWSTR(wide_path.as_ptr()), None);
    if size == 0 {
        return None;
    }

    let mut data = vec![0u8; size as usize];
    if GetFileVersionInfoW(
        windows::core::PCWSTR(wide_path.as_ptr()),
        Some(0),
        size,
        data.as_mut_ptr() as *mut _,
    )
    .is_err()
    {
        return None;
    }

    let mut lang_ptr: *mut c_void = std::ptr::null_mut();
    let mut lang_len: u32 = 0;

    let translation_query = OsStr::new("\\VarFileInfo\\Translation")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();

    if !VerQueryValueW(
        data.as_ptr() as *const _,
        windows::core::PCWSTR(translation_query.as_ptr()),
        &mut lang_ptr,
        &mut lang_len,
    )
    .as_bool()
    {
        return None;
    }

    if lang_len < 4 {
        return None;
    }

    let pairs = std::slice::from_raw_parts(lang_ptr as *const u16, (lang_len / 2) as usize);
    let num_pairs = (lang_len / 4) as usize;

    let mut lang_code = pairs[0];
    let mut charset_code = pairs[1];

    for i in 0..num_pairs {
        let code = pairs[i * 2];
        let charset = pairs[i * 2 + 1];

        if code == 0x0804 {
            lang_code = code;
            charset_code = charset;
        }
    }

    let keys = ["FileDescription", "ProductName"];

    for key in keys {
        let query_str = format!(
            "\\StringFileInfo\\{:04x}{:04x}\\{}",
            lang_code, charset_code, key
        );
        let query = OsStr::new(&query_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>();

        let mut desc_ptr: *mut c_void = std::ptr::null_mut();
        let mut desc_len: u32 = 0;

        if VerQueryValueW(
            data.as_ptr() as *const _,
            windows::core::PCWSTR(query.as_ptr()),
            &mut desc_ptr,
            &mut desc_len,
        )
        .as_bool()
        {
            let desc = std::slice::from_raw_parts(desc_ptr as *const u16, desc_len as usize);
            let len = if desc.last() == Some(&0) {
                desc.len() - 1
            } else {
                desc.len()
            };
            if len > 0 {
                return Some(String::from_utf16_lossy(&desc[..len]));
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
unsafe fn extract_icon(path: &str) -> Option<String> {
    use image::ImageEncoder;

    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut shfi = SHFILEINFOW::default();

    SHGetFileInfoW(
        windows::core::PCWSTR(wide_path.as_ptr()),
        windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
        Some(&mut shfi as *mut _),
        std::mem::size_of::<SHFILEINFOW>() as u32,
        SHGFI_ICON | SHGFI_LARGEICON | SHGFI_USEFILEATTRIBUTES,
    );

    if shfi.hIcon.is_invalid() {
        return None;
    }

    let icon = shfi.hIcon;
    struct IconGuard(windows::Win32::UI::WindowsAndMessaging::HICON);
    impl Drop for IconGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyIcon(self.0);
            }
        }
    }
    let _guard = IconGuard(icon);

    let mut icon_info = ICONINFO::default();
    if GetIconInfo(icon, &mut icon_info).is_err() {
        return None;
    }

    struct BitmapGuard(HBITMAP);
    impl Drop for BitmapGuard {
        fn drop(&mut self) {
            unsafe {
                if !self.0.is_invalid() {
                    let _ = DeleteObject(self.0.into());
                }
            }
        }
    }
    let _bm_mask = BitmapGuard(icon_info.hbmMask);
    let _bm_color = BitmapGuard(icon_info.hbmColor);

    let mut bm = BITMAP::default();
    if GetObjectW(
        icon_info.hbmMask.into(),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut _ as *mut _),
    ) == 0
    {
        return None;
    }

    let width = bm.bmWidth;
    let height = if !icon_info.hbmColor.is_invalid() {
        bm.bmHeight
    } else {
        bm.bmHeight / 2
    };

    let screen_dc = GetDC(None);
    let mem_dc = CreateCompatibleDC(Some(screen_dc));
    let mem_bm = CreateCompatibleBitmap(screen_dc, width, height);

    let old_obj = SelectObject(mem_dc, mem_bm.into());

    let _ = DrawIconEx(mem_dc, 0, 0, icon, width, height, 0, None, DI_NORMAL);

    let bi = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let mut pixels = vec![0u8; (width * height * 4) as usize];

    GetDIBits(
        mem_dc,
        mem_bm,
        0,
        height as u32,
        Some(pixels.as_mut_ptr() as *mut _),
        &mut BITMAPINFO {
            bmiHeader: bi,
            ..Default::default()
        },
        DIB_RGB_COLORS,
    );

    SelectObject(mem_dc, old_obj);
    let _ = DeleteDC(mem_dc);
    let _ = DeleteObject(mem_bm.into());
    let _ = ReleaseDC(None, screen_dc);

    for chunk in pixels.chunks_exact_mut(4) {
        let b = chunk[0];
        let r = chunk[2];
        chunk[0] = r;
        chunk[2] = b;
    }

    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder
        .write_image(
            &pixels,
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )
        .ok()?;

    Some(BASE64.encode(&png_data))
}

unsafe fn wait_for_modifier_keys_release(timeout_ms: u64) {
    let modifiers = [
        VK_CONTROL,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_SHIFT,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_MENU,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
    ];

    let started = std::time::Instant::now();
    loop {
        let any_down = modifiers
            .iter()
            .any(|vk| (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0);
        if !any_down || started.elapsed().as_millis() as u64 >= timeout_ms {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(target_os = "windows")]
pub fn send_paste_input(method: &str) {
    unsafe {
        // Wait briefly for physical hotkey modifiers (e.g. Ctrl/Shift/Alt) to be released
        // so they don't collide with or cancel out the synthesized paste chord.
        wait_for_modifier_keys_release(350);

        let inputs = if method == "ctrl_v" {
            log::info!("send_paste_input: sending Ctrl+V");
            let ctrl_scan = MapVirtualKeyW(VK_LCONTROL.0 as u32, MAPVK_VK_TO_VSC) as u16;
            let v_scan = MapVirtualKeyW(0x56, MAPVK_VK_TO_VSC) as u16;
            vec![
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_LCONTROL,
                            wScan: ctrl_scan,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(0x56),
                            wScan: v_scan,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(0x56),
                            wScan: v_scan,
                            dwFlags: KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_LCONTROL,
                            wScan: ctrl_scan,
                            dwFlags: KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                },
            ]
        } else {
            log::info!("send_paste_input: sending Shift+Insert (with KEYEVENTF_EXTENDEDKEY)");
            let shift_scan = MapVirtualKeyW(VK_LSHIFT.0 as u32, MAPVK_VK_TO_VSC) as u16;
            let insert_scan = MapVirtualKeyW(VK_INSERT.0 as u32, MAPVK_VK_TO_VSC) as u16;
            vec![
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_LSHIFT,
                            wScan: shift_scan,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_INSERT,
                            wScan: insert_scan,
                            dwFlags: KEYEVENTF_EXTENDEDKEY,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_INSERT,
                            wScan: insert_scan,
                            dwFlags: KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_LSHIFT,
                            wScan: shift_scan,
                            dwFlags: KEYEVENTF_KEYUP,
                            ..Default::default()
                        },
                    },
                },
            ]
        };

        let result = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        log::info!("send_paste_input: SendInput returned {}", result);
    }
}

const CF_UNICODETEXT_ID: u32 = 13;
const MAX_RICH_FORMAT_BYTES: usize = 10 * 1024 * 1024;

struct Win32ClipboardGuard;

impl Win32ClipboardGuard {
    fn open_with_retry(attempts: usize, delay_ms: u64) -> Result<Self, String> {
        let mut last_err = String::new();
        for i in 0..attempts {
            match unsafe { OpenClipboard(None) } {
                Ok(()) => return Ok(Self),
                Err(e) => {
                    last_err = e.to_string();
                    if i + 1 < attempts {
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    }
                }
            }
        }
        Err(format!("OpenClipboard failed: {}", last_err))
    }
}

impl Drop for Win32ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

unsafe fn read_registered_format_bytes(format_id: u32) -> Option<Vec<u8>> {
    if format_id == 0 || IsClipboardFormatAvailable(format_id).is_err() {
        return None;
    }

    let handle = GetClipboardData(format_id).ok()?;
    if handle.0.is_null() {
        return None;
    }

    let hglobal = HGLOBAL(handle.0);
    let size = GlobalSize(hglobal);
    if size == 0 || size > MAX_RICH_FORMAT_BYTES {
        return None;
    }

    let ptr = GlobalLock(hglobal);
    if ptr.is_null() {
        return None;
    }

    let raw = std::slice::from_raw_parts(ptr as *const u8, size);
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    let result = if end > 0 {
        Some(raw[..end].to_vec())
    } else {
        None
    };

    let _ = GlobalUnlock(hglobal);
    result
}

pub fn read_clipboard_rich_formats() -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    unsafe {
        let cf_html = RegisterClipboardFormatW(windows::core::w!("HTML Format"));
        let cf_rtf = RegisterClipboardFormatW(windows::core::w!("Rich Text Format"));

        let html_avail = cf_html != 0 && IsClipboardFormatAvailable(cf_html).is_ok();
        let rtf_avail = cf_rtf != 0 && IsClipboardFormatAvailable(cf_rtf).is_ok();
        if !html_avail && !rtf_avail {
            return (None, None);
        }

        let _guard = match Win32ClipboardGuard::open_with_retry(5, 10) {
            Ok(g) => g,
            Err(e) => {
                log::warn!(
                    "CLIPBOARD: Failed to open clipboard for rich formats: {}",
                    e
                );
                return (None, None);
            }
        };

        let html = if html_avail {
            read_registered_format_bytes(cf_html)
        } else {
            None
        };
        let rtf = if rtf_avail {
            read_registered_format_bytes(cf_rtf)
        } else {
            None
        };

        (html, rtf)
    }
}

unsafe fn set_hglobal_bytes(format_id: u32, bytes_with_terminator: &[u8]) -> Result<(), String> {
    let hglobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes_with_terminator.len())
        .map_err(|e| format!("GlobalAlloc failed: {}", e))?;

    let ptr = GlobalLock(hglobal);
    if ptr.is_null() {
        let _ = GlobalFree(Some(hglobal));
        return Err("GlobalLock returned null".to_string());
    }

    std::ptr::copy_nonoverlapping(
        bytes_with_terminator.as_ptr(),
        ptr as *mut u8,
        bytes_with_terminator.len(),
    );
    let _ = GlobalUnlock(hglobal);

    if let Err(e) = SetClipboardData(format_id, Some(HANDLE(hglobal.0))) {
        let _ = GlobalFree(Some(hglobal));
        return Err(format!("SetClipboardData({}) failed: {}", format_id, e));
    }

    Ok(())
}

pub fn write_clipboard_with_rich_formats(
    plain_text: &str,
    html_bytes: Option<&[u8]>,
    rtf_bytes: Option<&[u8]>,
) -> Result<(), String> {
    let _guard = Win32ClipboardGuard::open_with_retry(5, 25)?;

    unsafe {
        EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {}", e))?;

        let mut utf16_bytes = Vec::with_capacity((plain_text.len() + 1) * 2);
        for unit in plain_text.encode_utf16().chain(std::iter::once(0u16)) {
            utf16_bytes.extend_from_slice(&unit.to_le_bytes());
        }
        set_hglobal_bytes(CF_UNICODETEXT_ID, &utf16_bytes)?;

        if let Some(html) = html_bytes.filter(|b| !b.is_empty()) {
            let cf_html = RegisterClipboardFormatW(windows::core::w!("HTML Format"));
            if cf_html != 0 {
                let mut buf = Vec::with_capacity(html.len() + 1);
                buf.extend_from_slice(html);
                buf.push(0);
                if let Err(e) = set_hglobal_bytes(cf_html, &buf) {
                    log::warn!("CLIPBOARD: Failed to set HTML Format: {}", e);
                }
            }
        }

        if let Some(rtf) = rtf_bytes.filter(|b| !b.is_empty()) {
            let cf_rtf = RegisterClipboardFormatW(windows::core::w!("Rich Text Format"));
            if cf_rtf != 0 {
                let mut buf = Vec::with_capacity(rtf.len() + 1);
                buf.extend_from_slice(rtf);
                buf.push(0);
                if let Err(e) = set_hglobal_bytes(cf_rtf, &buf) {
                    log::warn!("CLIPBOARD: Failed to set Rich Text Format: {}", e);
                }
            }
        }
    }

    Ok(())
}
