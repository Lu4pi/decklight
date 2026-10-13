// Unter Windows im Release kein zusätzliches Konsolenfenster öffnen.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Desktop-App: startet Kern und Webserver im selben Prozess. Die Oberfläche
//! spricht über http://127.0.0.1:8750 mit dem Kern, genau wie /monitor im Netz.

use decklight_server::files::{start_desktop, Persist};
use decklight_server::UiAssets;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tauri::webview::DownloadEvent;
use tauri::{Manager, WebviewWindowBuilder};

static PERSIST: OnceLock<Arc<Persist>> = OnceLock::new();

/// Log auf die Konsole und nach `decklight.log` im Datenordner, damit sich
/// Probleme vor Ort hinterher nachvollziehen lassen (Monitoring → Log-Datei).
fn init_logging(dir: &Path) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let path = dir.join(decklight_server::LOG_FILE);
    let file = std::fs::create_dir_all(dir).ok().and_then(|_| {
        // Ab 5 MB neu anfangen, die alte Datei bleibt als .1 liegen.
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > 5_000_000) {
            let _ = std::fs::rename(&path, path.with_extension("log.1"));
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
    });
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer());
    match file {
        Some(f) => registry
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(std::sync::Mutex::new(f)),
            )
            .init(),
        None => registry.init(),
    }
}

fn main() {
    tauri::Builder::default()
        // Nur eine DeckLight-Instanz: ein zweiter Start holt das offene Fenster
        // nach vorn, statt um Port 8750 und die PRO-DJ-LINK-Ports zu streiten.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            // Fenster selbst bauen, damit Downloads (Show-Export, CSV) im
            // Download-Ordner landen statt im Nichts.
            let downloads = app.path().download_dir().ok();
            if let Some(cfg) = app.config().app.windows.first() {
                WebviewWindowBuilder::from_config(app.handle(), cfg)?
                    .on_download(move |_, event| on_download(downloads.as_deref(), event))
                    .build()?;
            }
            let dir = app.path().app_data_dir()?;
            init_logging(&dir);
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "DeckLight startet");
            // Die eingebaute Oberfläche auch übers Netz ausliefern: so öffnen
            // Handy und Tablet http://<Rechner>:8750/monitor ohne Extra-Installation.
            let resolver = app.asset_resolver();
            let ui = UiAssets(Arc::new(move |path: &str| {
                resolver
                    .get(path.to_string())
                    .map(|a| (a.bytes, a.mime_type))
            }));
            tauri::async_runtime::spawn(async move {
                match start_desktop(&dir, Some(ui)).await {
                    Ok((app, server, persist)) => {
                        tracing::info!(addr = %server.addr, dir = %dir.display(), "DeckLight-Kern läuft");
                        let _ = PERSIST.set(persist);
                        // Kern und Server leben so lange wie die App.
                        std::mem::forget((app, server));
                    }
                    Err(e) => tracing::error!(error = %format!("{e:#}"), "DeckLight-Kern startet nicht"),
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("DeckLight konnte nicht gestartet werden")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(p) = PERSIST.get() {
                    if let Err(e) = p.save(true) {
                        tracing::error!(error = %format!("{e:#}"), "Sichern beim Beenden fehlgeschlagen");
                    }
                }
            }
        });
}

/// Downloads in den Download-Ordner, ohne vorhandene Dateien zu überschreiben.
fn on_download(downloads: Option<&Path>, event: DownloadEvent<'_>) -> bool {
    match event {
        DownloadEvent::Requested { url, destination } => {
            let Some(dir) = downloads else {
                return true;
            };
            let name = destination
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| {
                    url.path_segments()
                        .and_then(|mut s| s.next_back().map(str::to_string))
                        .unwrap_or_else(|| "decklight-export".into())
                });
            *destination = unique(dir, &name);
            tracing::info!(%url, to = %destination.display(), "Download");
            true
        }
        DownloadEvent::Finished { url, success, .. } => {
            if !success {
                tracing::warn!(%url, "Download fehlgeschlagen");
            }
            true
        }
        _ => true,
    }
}

fn unique(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..1000)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|c| !c.exists())
        .unwrap_or(first)
}
