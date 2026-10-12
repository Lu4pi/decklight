//! Show-Dateien (`.decklight`) und Sicherungskopien.
//!
//! Eine Show-Datei enthält Offsets, Cues, Trigger und die Ausgangs-Einstellungen
//! (Timecode, OSC, MIDI, Master-Auswahl). Netz- und Web-Einstellungen gehören zum
//! Rechner und bleiben draußen. Gespeicherte Shows liegen in `shows/`, automatische
//! Sicherungen der laufenden Show in `backups/`.

use crate::files::{load, read, save};
use crate::settings::Settings;
use crate::ShowData;
use anyhow::{bail, Context, Result};
use decklight_core::{ActionOutConfig, TimecodeOutConfig};
use decklight_engine::MasterMode;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const EXTENSION: &str = "decklight";
pub const FORMAT: &str = "decklight-show";
pub const VERSION: u32 = 1;
/// So viele automatische Sicherungen bleiben liegen.
pub const KEEP_BACKUPS: usize = 30;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSettings {
    pub master_mode: MasterMode,
    pub timecode_enabled: bool,
    pub timecode: TimecodeOutConfig,
    pub actions: ActionOutConfig,
}

impl OutputSettings {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            master_mode: s.master_mode,
            timecode_enabled: s.timecode_enabled,
            timecode: s.timecode.clone(),
            actions: s.actions.clone(),
        }
    }

    /// In vorhandene Einstellungen übernehmen (Netz und Web bleiben).
    pub fn apply_to(&self, s: &Settings) -> Settings {
        Settings {
            master_mode: self.master_mode,
            timecode_enabled: self.timecode_enabled,
            timecode: self.timecode.clone(),
            actions: self.actions.clone(),
            ..s.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowFile {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub saved_unix_ms: u64,
    pub show: ShowData,
    #[serde(default)]
    pub outputs: Option<OutputSettings>,
}

impl ShowFile {
    pub fn new(name: &str, show: ShowData, outputs: Option<OutputSettings>) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            name: name.into(),
            saved_unix_ms: decklight_core::monitor::unix_ms(),
            show,
            outputs,
        }
    }

    pub fn check(&self) -> Result<()> {
        if self.format != FORMAT {
            bail!("Keine DeckLight-Show-Datei");
        }
        if self.version > VERSION {
            bail!(
                "Show-Datei aus einer neueren DeckLight-Version ({}), bitte aktualisieren",
                self.version
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShowInfo {
    pub name: String,
    pub file: String,
    pub saved_unix_ms: u64,
    pub tracks: usize,
    pub cues: usize,
    pub triggers: usize,
}

impl ShowInfo {
    fn of(file: &str, name: &str, saved_unix_ms: u64, show: &ShowData) -> Self {
        Self {
            name: name.into(),
            file: file.into(),
            saved_unix_ms,
            tracks: show.offsets.entries.len(),
            cues: show.triggers.tracks.values().map(Vec::len).sum(),
            triggers: show.triggers.globals.len(),
        }
    }
}

/// Erlaubte Zeichen für Shownamen (auch Dateiname): keine Pfade, keine Sonderzeichen.
pub fn clean_name(name: &str) -> Result<String> {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || " -_.()&+".contains(*c))
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    if cleaned.is_empty() {
        bail!("Bitte einen Namen für die Show angeben");
    }
    Ok(cleaned.chars().take(80).collect())
}

pub struct Library {
    dir: PathBuf,
}

impl Library {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn shows_dir(&self) -> PathBuf {
        self.dir.join("shows")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.dir.join("backups")
    }

    fn show_path(&self, name: &str) -> Result<PathBuf> {
        Ok(self
            .shows_dir()
            .join(format!("{}.{EXTENSION}", clean_name(name)?)))
    }

    pub fn save(&self, file: &ShowFile) -> Result<PathBuf> {
        let path = self.show_path(&file.name)?;
        save(&path, file)?;
        Ok(path)
    }

    pub fn open(&self, name: &str) -> Result<ShowFile> {
        let path = self.show_path(name)?;
        if !path.exists() {
            bail!("Show „{name}“ nicht gefunden");
        }
        let f: ShowFile = read(&path)?;
        f.check()?;
        Ok(f)
    }

    pub fn list(&self) -> Vec<ShowInfo> {
        let mut out: Vec<ShowInfo> = read_dir(&self.shows_dir(), EXTENSION)
            .into_iter()
            .filter_map(|(file, path)| {
                let f: ShowFile = read(&path).ok()?;
                Some(ShowInfo::of(&file, &f.name, f.saved_unix_ms, &f.show))
            })
            .collect();
        out.sort_by_key(|a| a.name.to_lowercase());
        out
    }

    /// Sicherung der laufenden Show, höchstens alle `min_interval`.
    pub fn backup(&self, show: &ShowData, min_interval: Duration) -> Result<Option<PathBuf>> {
        let now = decklight_core::monitor::unix_ms();
        let newest = self.backups().first().map(|b| b.saved_unix_ms);
        let min_ms = u64::try_from(min_interval.as_millis()).unwrap_or(u64::MAX);
        if newest.is_some_and(|t| now.saturating_sub(t) < min_ms) {
            return Ok(None);
        }
        let path = self.backups_dir().join(format!("show-{now}.json"));
        save(&path, show)?;
        for old in self.backups().into_iter().skip(KEEP_BACKUPS) {
            let _ = std::fs::remove_file(self.backups_dir().join(old.file));
        }
        Ok(Some(path))
    }

    /// Sicherungen, neueste zuerst.
    pub fn backups(&self) -> Vec<ShowInfo> {
        let mut out: Vec<ShowInfo> = read_dir(&self.backups_dir(), "json")
            .into_iter()
            .filter_map(|(file, path)| {
                let t: u64 = file
                    .strip_prefix("show-")?
                    .strip_suffix(".json")?
                    .parse()
                    .ok()?;
                let show: ShowData = load(&path).ok()?;
                Some(ShowInfo::of(&file, "Sicherung", t, &show))
            })
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.saved_unix_ms));
        out
    }

    pub fn open_backup(&self, file: &str) -> Result<ShowData> {
        let valid = file
            .strip_prefix("show-")
            .and_then(|f| f.strip_suffix(".json"))
            .is_some_and(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()));
        if !valid {
            bail!("Ungültige Sicherung");
        }
        load(&self.backups_dir().join(file)).context("Sicherung nicht lesbar")
    }
}

fn read_dir(dir: &Path, ext: &str) -> Vec<(String, PathBuf)> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|x| x.to_str()) == Some(ext))
                .filter_map(|p| Some((p.file_name()?.to_str()?.to_string(), p)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use decklight_show::{Cue, TrackKey};

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("decklight-lib-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Club Night 2026 ").unwrap(), "Club Night 2026");
        assert_eq!(clean_name("../../etc/passwd").unwrap(), "etcpasswd");
        assert_eq!(clean_name("Fête & Bühne").unwrap(), "Fête & Bühne");
        assert!(clean_name("///").is_err());
    }

    #[test]
    fn save_list_open_roundtrip() {
        let dir = temp("roundtrip");
        let lib = Library::new(&dir);
        let mut show = ShowData::default();
        show.triggers
            .add_cue(&TrackKey("A".into()), Cue::at_ms("Drop", 1.0, vec![]));
        show.offsets.assign(&TrackKey("A".into()), "A");
        let outputs = OutputSettings::from_settings(&Settings::default());
        lib.save(&ShowFile::new("Club Night", show.clone(), Some(outputs)))
            .unwrap();
        let list = lib.list();
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].tracks, list[0].cues), (1, 1));
        let back = lib.open("Club Night").unwrap();
        assert_eq!(back.show, show);
        assert!(lib.open("Fehlt").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn foreign_or_newer_files_are_refused() {
        let mut f = ShowFile::new("x", ShowData::default(), None);
        f.format = "etwas".into();
        assert!(f.check().is_err());
        let mut f = ShowFile::new("x", ShowData::default(), None);
        f.version = VERSION + 1;
        assert!(f.check().is_err());
    }

    #[test]
    fn backups_are_throttled_and_pruned() {
        let dir = temp("backups");
        let lib = Library::new(&dir);
        let show = ShowData::default();
        assert!(lib
            .backup(&show, Duration::from_secs(300))
            .unwrap()
            .is_some());
        assert!(lib
            .backup(&show, Duration::from_secs(300))
            .unwrap()
            .is_none());
        for i in 0..40u64 {
            save(
                &lib.backups_dir().join(format!("show-{}.json", 1000 + i)),
                &show,
            )
            .unwrap();
        }
        lib.backup(&show, Duration::ZERO).unwrap();
        let list = lib.backups();
        assert_eq!(list.len(), KEEP_BACKUPS);
        assert!(list[0].saved_unix_ms > 1_000_000);
        assert!(lib.open_backup(&list[0].file).is_ok());
        assert!(lib.open_backup("../settings.json").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
