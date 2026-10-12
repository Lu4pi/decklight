//! Monitoring: Track-History (was lief wann) und Warnungen.

use decklight_engine::EngineEvent;
use decklight_show::TrackKey;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Ein Track, der als Master gespielt hat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub unix_ms: u64,
    pub deck: u8,
    pub key: TrackKey,
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    pub unix_ms: u64,
    pub level: Level,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Monitor {
    pub history: Vec<HistoryEntry>,
    warnings: VecDeque<Warning>,
}

const MAX_WARNINGS: usize = 500;

impl Monitor {
    pub fn warn(&mut self, level: Level, text: impl Into<String>) {
        if self.warnings.len() == MAX_WARNINGS {
            self.warnings.pop_front();
        }
        self.warnings.push_back(Warning {
            unix_ms: unix_ms(),
            level,
            text: text.into(),
        });
    }

    pub fn warnings(&self) -> impl DoubleEndedIterator<Item = &Warning> {
        self.warnings.iter()
    }

    /// Neuer Eintrag, wenn ein anderer Track als zuletzt auf dem Master läuft.
    pub fn played(&mut self, deck: u8, key: TrackKey, title: &str, artist: &str) -> bool {
        if self.history.last().is_some_and(|h| h.key == key) {
            return false;
        }
        self.history.push(HistoryEntry {
            unix_ms: unix_ms(),
            deck,
            key,
            title: title.to_string(),
            artist: artist.to_string(),
        });
        true
    }

    /// Warnungen zu Engine-Ereignissen.
    pub fn observe(&mut self, ev: &EngineEvent) {
        match ev {
            EngineEvent::DeviceFound { number, name, .. } => {
                self.warn(Level::Info, format!("{name} (#{number}) erkannt"));
            }
            EngineEvent::DeviceLost { number, name } => {
                self.warn(
                    Level::Warning,
                    format!("{name} (#{number}) nicht mehr erreichbar"),
                );
            }
            EngineEvent::NumberConflict {
                number,
                name,
                switched_to: Some(n),
            } => self.warn(
                Level::Warning,
                format!("{name} benutzt Player-Nummer {number}; DeckLight ist auf {n} ausgewichen"),
            ),
            EngineEvent::NumberConflict { number, name, .. } => self.warn(
                Level::Error,
                format!("{name} benutzt dieselbe Player-Nummer {number} wie DeckLight, keine Nummer frei"),
            ),
            EngineEvent::MasterChanged { to: None, .. } => {
                self.warn(Level::Warning, "Kein Master-Deck: Timecode steht");
            }
            EngineEvent::TriggerFired {
                name,
                error: Some(e),
                ..
            } => self.warn(Level::Error, format!("{name}: {e}")),
            EngineEvent::ArmedChanged { armed: false } => {
                self.warn(Level::Warning, "Not-Aus: Trigger gesperrt");
            }
            _ => {}
        }
    }

    /// CSV mit Semikolon: Uhrzeit (ms seit 1970), Deck, Artist, Titel.
    pub fn history_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = String::from("unix_ms;deck;artist;title\n");
        for h in &self.history {
            out.push_str(&format!(
                "{};{};{};{}\n",
                h.unix_ms,
                h.deck,
                q(&h.artist),
                q(&h.title)
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_skips_repeats_and_exports_csv() {
        let mut m = Monitor::default();
        let k = |s: &str| TrackKey(s.into());
        assert!(m.played(1, k("a"), "Song \"A\"", "X"));
        assert!(!m.played(2, k("a"), "Song \"A\"", "X"));
        assert!(m.played(2, k("b"), "B", "Y"));
        assert!(m.played(1, k("a"), "Song \"A\"", "X"));
        let csv = m.history_csv();
        assert_eq!(csv.lines().count(), 4);
        assert!(csv.contains(";1;\"X\";\"Song \"\"A\"\"\""));
    }

    #[test]
    fn warnings_are_bounded() {
        let mut m = Monitor::default();
        for i in 0..600 {
            m.warn(Level::Info, format!("{i}"));
        }
        assert_eq!(m.warnings().count(), MAX_WARNINGS);
        assert_eq!(m.warnings().next().unwrap().text, "100");
    }
}
