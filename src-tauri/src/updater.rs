use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::RwLock;

#[derive(Clone, Default)]
pub struct UpdateManager {
  available_version: Arc<RwLock<Option<String>>>,
  is_checking: Arc<AtomicBool>,
}

impl UpdateManager {
  pub fn new() -> Self {
    Self {
      available_version: Arc::new(RwLock::new(None)),
      is_checking: Arc::new(AtomicBool::new(false)),
    }
  }

  pub async fn get_available_version(&self, app: &AppHandle) -> Option<String> {
    let current = self.available_version.read().await.clone();
    eprintln!("[Rust:UpdateManager] get_available_version called. Cached state: {:?}", current);
    log::info!("[UpdateManager] get_available_version called. Cached state: {:?}", current);
    if current.is_some() {
      return current;
    }
    eprintln!("[Rust:UpdateManager] Cached state is None. Triggering check_for_updates...");
    log::info!("[UpdateManager] Cached state is None. Triggering check_for_updates...");
    let res = self.check_for_updates(app).await.ok().flatten();
    eprintln!("[Rust:UpdateManager] check_for_updates finished with: {:?}", res);
    log::info!("[UpdateManager] check_for_updates finished with: {:?}", res);
    res
  }

  pub async fn check_for_updates(&self, app: &AppHandle) -> Result<Option<String>, String> {
    if self
      .is_checking
      .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
      .is_err()
    {
      let cached = self.available_version.read().await.clone();
      eprintln!("[Rust:UpdateManager] Check already in progress. Returning cached: {:?}", cached);
      log::info!("[UpdateManager] Check already in progress. Returning cached: {:?}", cached);
      return Ok(cached);
    }

    let result = self.do_check(app).await;
    self.is_checking.store(false, Ordering::SeqCst);
    result
  }

  async fn do_check(&self, app: &AppHandle) -> Result<Option<String>, String> {
    eprintln!("[updater] do_check started...");
    let updater = match app.updater() {
      Ok(u) => u,
      Err(e) => {
        eprintln!("[updater] app.updater() error: {:?}", e);
        log::error!("app.updater() error: {:?}", e);
        return Err(e.to_string());
      }
    };
    eprintln!("[updater] querying updater.check().await...");
    match updater.check().await {
      Ok(Some(update)) => {
        let version = update.version.clone();
        eprintln!("[updater] Update found: v{}", version);
        log::info!("Update available: v{}", version);
        *self.available_version.write().await = Some(version.clone());
        let _ = app.emit("update-available", Some(version.clone()));
        Ok(Some(version))
      }
      Ok(None) => {
        eprintln!("[updater] check() returned None (on latest version)");
        log::info!("No update available, on latest version");
        *self.available_version.write().await = None;
        let _ = app.emit("update-available", None::<String>);
        Ok(None)
      }
      Err(e) => {
        eprintln!("[updater] check() failed with error: {:?}", e);
        log::warn!("Failed to check for updates: {:?}", e);
        Err(e.to_string())
      }
    }
  }

  pub async fn install_update(&self, app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
      .check()
      .await
      .map_err(|e| e.to_string())?
      .ok_or_else(|| "No update available to install".to_string())?;

    log::info!("Downloading and installing update v{}...", update.version);
    update
      .download_and_install(|_chunk_length, _content_length| {}, || {})
      .await
      .map_err(|e| e.to_string())?;

    log::info!("Update installed successfully, restarting app...");
    app.restart();
  }

  pub fn start_background_loop(&self, app: AppHandle) {
    let manager = self.clone();
    tokio::spawn(async move {
      // Initial startup check after 1 second
      tokio::time::sleep(Duration::from_secs(1)).await;
      let _ = manager.check_for_updates(&app).await;

      // Periodic check every 4 hours
      let mut interval = tokio::time::interval(Duration::from_secs(4 * 3600));
      interval.tick().await; // consume first instant tick
      loop {
        interval.tick().await;
        let _ = manager.check_for_updates(&app).await;
      }
    });
  }
}
