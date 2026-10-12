// Unter Windows im Release kein zusätzliches Konsolenfenster öffnen.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Desktop-App: startet Kern und Webserver im selben Prozess. Die Oberfläche
//! spricht über http://127.0.0.1:8750 mit dem Kern, genau wie /monitor im Netz.

use decklight_server::files::{start_desktop, Persist};
use std::sync::{Arc, OnceLock};
use tauri::Manager;

static PERSIST: OnceLock<Arc<Persist>> = OnceLock::new();

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            tauri::async_runtime::spawn(async move {
                match start_desktop(&dir).await {
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
