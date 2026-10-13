//! Einstellungen und Show-Daten als JSON-Dateien: laden, automatisch sichern.

use crate::library::ShowFile;
use crate::settings::Settings;
use crate::{App, Server, ServerConfig, ShowData, UiAssets, DEFAULT_PORT};
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Datei lesen; fehlt sie, gibt es die Standardwerte.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    read(path)
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("{} lesen", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("{} ist kein gültiges JSON", path.display()))
}

/// Erst in eine Nachbardatei schreiben, dann umbenennen: nie halb geschriebene Dateien.
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).with_context(|| format!("{} schreiben", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("{} schreiben", path.display()))?;
    Ok(())
}

/// Arbeitskopie der laufenden Show laden: Name und Show-Daten.
/// Liest auch ältere Dateien, die nur die Show-Daten enthalten.
pub fn load_working_copy(path: &Path) -> Result<(String, ShowData)> {
    if !path.exists() {
        return Ok(("Neue Show".into(), ShowData::default()));
    }
    if let Ok(f) = read::<ShowFile>(path) {
        f.check()?;
        return Ok((f.name, f.show));
    }
    Ok(("Neue Show".into(), read(path)?))
}

/// Was beim Start aus dem Datenordner geladen wurde.
pub struct StartData {
    pub settings: Settings,
    pub name: String,
    pub show: ShowData,
    /// Hinweise für das Monitoring, z. B. reparierte Dateien.
    pub warnings: Vec<String>,
}

/// Einstellungen und Arbeitskopie laden. Eine defekte Datei verhindert den
/// Start nicht: Sie wird als `*.defekt-<Zeit>` beiseitegelegt, die Show kommt
/// aus der neuesten lesbaren Sicherung, die Einstellungen auf Standard.
pub fn load_start(data_dir: &Path) -> StartData {
    let settings_path = data_dir.join("settings.json");
    let show_path = data_dir.join("show.json");
    let mut warnings = Vec::new();
    let settings = load::<Settings>(&settings_path).unwrap_or_else(|e| {
        let moved = set_aside(&settings_path);
        warnings.push(format!(
            "Einstellungen nicht lesbar ({e:#}), Standardwerte geladen. Alte Datei: {moved}"
        ));
        Settings::default()
    });
    let (name, show) = load_working_copy(&show_path).unwrap_or_else(|e| {
        let moved = set_aside(&show_path);
        let lib = crate::library::Library::new(data_dir);
        let restored = lib
            .backups()
            .into_iter()
            .find_map(|b| lib.open_backup(&b.file).ok().map(|show| (b.file, show)));
        match restored {
            Some((file, show)) => {
                warnings.push(format!(
                    "Show-Datei nicht lesbar ({e:#}), Sicherung {file} geladen. Alte Datei: {moved}"
                ));
                ("Wiederhergestellt".into(), show)
            }
            None => {
                warnings.push(format!(
                    "Show-Datei nicht lesbar ({e:#}), keine Sicherung vorhanden, leere Show. Alte Datei: {moved}"
                ));
                ("Neue Show".into(), ShowData::default())
            }
        }
    });
    for w in &warnings {
        tracing::error!("{w}");
    }
    StartData {
        settings,
        name,
        show,
        warnings,
    }
}

/// Datei umbenennen, damit sie beim nächsten Sichern nicht überschrieben wird.
fn set_aside(path: &Path) -> String {
    let target = path.with_extension(format!(
        "json.defekt-{}",
        decklight_core::monitor::unix_ms()
    ));
    match std::fs::rename(path, &target) {
        Ok(()) => target.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

/// Mindestabstand zwischen zwei automatischen Sicherungskopien.
pub const BACKUP_EVERY: Duration = Duration::from_secs(300);

/// Sichert Einstellungen und Show-Daten, sobald sich etwas geändert hat, und legt
/// regelmäßig Sicherungskopien an.
/// Zuletzt geschriebener Stand (Show mit Namen, Einstellungen).
type LastSaved = (Option<(String, ShowData)>, Option<Settings>);

pub struct Persist {
    app: Arc<App>,
    pub settings_path: PathBuf,
    pub show_path: PathBuf,
    last: Mutex<LastSaved>,
}

impl Persist {
    pub fn new(app: Arc<App>, settings_path: PathBuf, show_path: PathBuf) -> Self {
        Self {
            last: Mutex::new((
                Some((app.show_name(), app.show_data())),
                Some(app.settings()),
            )),
            app,
            settings_path,
            show_path,
        }
    }

    /// Geänderte Daten schreiben. `force`: immer schreiben.
    pub fn save(&self, force: bool) -> Result<()> {
        let show = (self.app.show_name(), self.app.show_data());
        let settings = self.app.settings();
        let mut last = self.last.lock().map_err(|_| anyhow::anyhow!("Sperre"))?;
        if force || last.0.as_ref() != Some(&show) {
            save(
                &self.show_path,
                &ShowFile::new(&show.0, show.1.clone(), None),
            )?;
            if let Some(lib) = self.app.library() {
                lib.backup(&show.1, BACKUP_EVERY)?;
            }
            last.0 = Some(show);
        }
        if force || last.1.as_ref() != Some(&settings) {
            save(&self.settings_path, &settings)?;
            last.1 = Some(settings);
        }
        Ok(())
    }

    /// Alle paar Sekunden sichern, bis die Aufgabe beendet wird.
    pub async fn run(self: Arc<Self>, every: Duration) {
        let mut tick = tokio::time::interval(every);
        loop {
            tick.tick().await;
            if let Err(e) = self.save(false) {
                tracing::error!(error = %format!("{e:#}"), "Sichern fehlgeschlagen");
            }
        }
    }
}

/// Desktop-App: alles im Datenordner, Server auf Port 8750 im ganzen Netz.
/// `ui` sind die eingebauten Dateien der App, damit Handys `/monitor` laden können.
pub async fn start_desktop(
    data_dir: &Path,
    ui: Option<UiAssets>,
) -> Result<(Arc<App>, Server, Arc<Persist>)> {
    let settings_path = data_dir.join("settings.json");
    let show_path = data_dir.join("show.json");
    let start = load_start(data_dir);
    let app = App::start(start.settings, start.show).await?;
    app.set_show_name(&start.name);
    app.set_library(data_dir);
    for w in start.warnings {
        app.warn(decklight_core::Level::Error, w);
    }
    let server = match Server::start(
        app.clone(),
        ServerConfig {
            bind: SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT)),
            ui_dir: None,
            ui_assets: ui.clone(),
        },
    )
    .await
    {
        Ok(s) => s,
        // Netzweit belegt (z. B. zweite Instanz): wenigstens lokal versuchen.
        Err(_) => {
            Server::start(
                app.clone(),
                ServerConfig {
                    bind: SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)),
                    ui_dir: None,
                    ui_assets: ui,
                },
            )
            .await?
        }
    };
    let persist = Arc::new(Persist::new(app.clone(), settings_path, show_path));
    tokio::spawn(persist.clone().run(Duration::from_secs(5)));
    Ok((app, server, persist))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("decklight-files-{}", std::process::id()));
        let path = dir.join("sub").join("show.json");
        let mut show = ShowData::default();
        show.offsets
            .assign(&decklight_show::TrackKey("A".into()), "Artist – A");
        save(&path, &show).unwrap();
        let back: ShowData = load(&path).unwrap();
        assert_eq!(back, show);
        let missing: ShowData = load(&dir.join("fehlt.json")).unwrap();
        assert_eq!(missing, ShowData::default());
        std::fs::write(&path, "{kaputt").unwrap();
        assert!(load::<ShowData>(&path).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn broken_files_are_set_aside_and_backup_is_restored() {
        let dir = std::env::temp_dir().join(format!("decklight-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Nichts da: Standard, keine Hinweise.
        let s = load_start(&dir);
        assert!(s.warnings.is_empty());
        assert_eq!(s.name, "Neue Show");

        let mut show = ShowData::default();
        show.offsets
            .assign(&decklight_show::TrackKey("A".into()), "Artist – A");
        crate::library::Library::new(&dir)
            .backup(&show, Duration::ZERO)
            .unwrap();
        std::fs::write(dir.join("show.json"), "{halb gesch").unwrap();
        std::fs::write(dir.join("settings.json"), "\0\0\0").unwrap();

        let s = load_start(&dir);
        assert_eq!(s.show, show);
        assert_eq!(s.settings, Settings::default());
        assert_eq!(s.warnings.len(), 2, "{:?}", s.warnings);
        assert!(!dir.join("show.json").exists());
        let aside = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".defekt-"))
            .count();
        assert_eq!(aside, 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
