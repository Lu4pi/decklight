//! Timecode-Offsets je Track: automatische Vergabe, Bearbeiten, CSV.

use decklight_outputs::{FrameRate, Timecode};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Stabile Kennung eines Tracks in der Show.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackKey(pub String);

impl fmt::Display for TrackKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OffsetEntry {
    pub key: TrackKey,
    /// Anzeigename, z. B. "Artist – Titel".
    pub label: String,
    /// Timecode am Track-Anfang in Millisekunden.
    pub offset_ms: f64,
    /// Automatisch vergeben (true) oder von Hand gesetzt.
    pub auto: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutoRule {
    /// Stunde des ersten Tracks (Standard 1 → 01:00:00:00).
    pub first_hour: u32,
    /// Abstand zwischen Tracks in Stunden.
    pub step_hours: u32,
}

impl Default for AutoRule {
    fn default() -> Self {
        Self {
            first_hour: 1,
            step_hours: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OffsetTable {
    #[serde(default)]
    pub rule: AutoRule,
    #[serde(default)]
    pub entries: Vec<OffsetEntry>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CsvError {
    #[error("Zeile {line}: {msg}")]
    Line { line: usize, msg: String },
}

const HOUR_MS: f64 = 3_600_000.0;

impl OffsetTable {
    pub fn get(&self, key: &TrackKey) -> Option<&OffsetEntry> {
        self.entries.iter().find(|e| &e.key == key)
    }

    /// Offset eines Tracks; unbekannte Tracks bekommen die nächste freie Stunde.
    pub fn assign(&mut self, key: &TrackKey, label: &str) -> f64 {
        if let Some(e) = self.entries.iter_mut().find(|e| &e.key == key) {
            if e.label.is_empty() && !label.is_empty() {
                e.label = label.to_string();
            }
            return e.offset_ms;
        }
        let offset_ms = self.next_free();
        self.entries.push(OffsetEntry {
            key: key.clone(),
            label: label.to_string(),
            offset_ms,
            auto: true,
        });
        offset_ms
    }

    /// Nächste freie Stunde nach der Regel (24 h Timecode-Raum, danach von vorn).
    pub fn next_free(&self) -> f64 {
        let step = f64::from(self.rule.step_hours.max(1)) * HOUR_MS;
        let mut candidate = f64::from(self.rule.first_hour) * HOUR_MS;
        while self
            .entries
            .iter()
            .any(|e| (e.offset_ms - candidate).abs() < step / 2.0)
        {
            candidate += step;
            if candidate >= 24.0 * HOUR_MS {
                break;
            }
        }
        candidate % (24.0 * HOUR_MS)
    }

    pub fn set(&mut self, key: &TrackKey, offset_ms: f64) {
        match self.entries.iter_mut().find(|e| &e.key == key) {
            Some(e) => {
                e.offset_ms = offset_ms;
                e.auto = false;
            }
            None => self.entries.push(OffsetEntry {
                key: key.clone(),
                label: String::new(),
                offset_ms,
                auto: false,
            }),
        }
    }

    pub fn remove(&mut self, key: &TrackKey) {
        self.entries.retain(|e| &e.key != key);
    }

    /// CSV mit Semikolon: `key;label;offset` (Offset als HH:MM:SS:FF).
    pub fn to_csv(&self, rate: FrameRate) -> String {
        let mut s = String::from("key;label;offset\n");
        for e in &self.entries {
            s.push_str(&format!(
                "{};{};{}\n",
                csv_field(&e.key.0),
                csv_field(&e.label),
                Timecode::from_ms(e.offset_ms, rate)
            ));
        }
        s
    }

    /// Übernimmt Offsets aus CSV. Bestehende Einträge werden überschrieben.
    pub fn import_csv(&mut self, text: &str, rate: FrameRate) -> Result<usize, CsvError> {
        let mut n = 0;
        for (i, line) in text.lines().enumerate() {
            let line_no = i + 1;
            if line.trim().is_empty() || (i == 0 && line.to_lowercase().starts_with("key")) {
                continue;
            }
            let fields = split_csv(line);
            let err = |msg: &str| CsvError::Line {
                line: line_no,
                msg: msg.to_string(),
            };
            let [key, label, offset] = &fields[..] else {
                return Err(err("erwartet drei Felder: key;label;offset"));
            };
            let tc = Timecode::parse(offset, rate)
                .ok_or_else(|| err("Offset ist kein Timecode HH:MM:SS:FF"))?;
            let key = TrackKey(key.clone());
            self.set(&key, tc.to_ms());
            if let Some(e) = self.entries.iter_mut().find(|e| e.key == key) {
                e.label.clone_from(label);
            }
            n += 1;
        }
        Ok(n)
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([';', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (';', false) => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> TrackKey {
        TrackKey(s.into())
    }

    #[test]
    fn auto_assign_new_hour_per_track() {
        let mut t = OffsetTable::default();
        assert_eq!(t.assign(&key("a"), "A"), HOUR_MS);
        assert_eq!(t.assign(&key("b"), "B"), 2.0 * HOUR_MS);
        assert_eq!(t.assign(&key("a"), "A"), HOUR_MS);
        t.set(&key("c"), 3.0 * HOUR_MS);
        assert_eq!(t.assign(&key("d"), "D"), 4.0 * HOUR_MS);
        t.remove(&key("a"));
        assert_eq!(t.assign(&key("e"), "E"), HOUR_MS);
    }

    #[test]
    fn csv_roundtrip_with_quotes() {
        let mut t = OffsetTable::default();
        t.assign(&key("p1-usb-17"), "Artist; \"Live\" – Song");
        t.set(
            &key("p2-usb-4"),
            Timecode::parse("05:30:00:12", FrameRate::Fps25)
                .unwrap()
                .to_ms(),
        );
        let csv = t.to_csv(FrameRate::Fps25);
        assert!(csv.contains("\"Artist; \"\"Live\"\" – Song\""));
        let mut back = OffsetTable::default();
        assert_eq!(back.import_csv(&csv, FrameRate::Fps25), Ok(2));
        assert_eq!(
            back.get(&key("p1-usb-17")).unwrap().label,
            "Artist; \"Live\" – Song"
        );
        assert_eq!(
            back.get(&key("p2-usb-4")).unwrap().offset_ms,
            t.get(&key("p2-usb-4")).unwrap().offset_ms
        );
        assert!(back
            .import_csv("key;label;offset\nx;y;25:00:00:00\n", FrameRate::Fps25)
            .is_err());
    }
}
