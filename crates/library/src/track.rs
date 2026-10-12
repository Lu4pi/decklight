//! Was DeckLight über einen Track weiß.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub rekordbox_id: u32,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub key: String,
    pub comment: String,
    pub duration_s: u32,
    pub bpm: Option<f64>,
    pub rating: u8,
    /// Kennung des Covers beim dbserver (0 = keins).
    pub artwork_id: u32,
}

impl TrackMetadata {
    /// "Artist – Titel" für Listen und Overlays.
    pub fn label(&self) -> String {
        match (self.artist.is_empty(), self.title.is_empty()) {
            (false, false) => format!("{} – {}", self.artist, self.title),
            (true, false) => self.title.clone(),
            _ => format!("Track {}", self.rekordbox_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridBeat {
    pub time_ms: u32,
    /// 1–4
    pub beat_in_bar: u8,
    pub bpm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "slot", rename_all = "snake_case")]
pub enum CueKind {
    Memory,
    /// Hot Cue A = 1, B = 2, …
    Hot(u8),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CuePoint {
    pub kind: CueKind,
    pub position_ms: u32,
    /// Bei Loops das Ende.
    pub loop_end_ms: Option<u32>,
    pub color: Option<[u8; 3]>,
    pub comment: String,
}

/// Eine Spalte der farbigen Wellenform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveColumn {
    /// Höhe 0–31.
    pub height: u8,
    pub rgb: [u8; 3],
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Waveform {
    /// Spalten pro Sekunde (Detail: 150).
    pub per_second: f64,
    pub columns: Vec<WaveColumn>,
}

impl Waveform {
    /// Verkleinert auf `n` Spalten (Übersicht): Maximum der Höhe, Farbe der höchsten Spalte.
    pub fn overview(&self, n: usize) -> Waveform {
        if self.columns.is_empty() || n == 0 {
            return Waveform::default();
        }
        let len = self.columns.len();
        let columns = (0..n)
            .map(|i| {
                let a = i * len / n;
                let b = ((i + 1) * len / n).max(a + 1).min(len);
                *self.columns[a..b]
                    .iter()
                    .max_by_key(|c| c.height)
                    .unwrap_or(&self.columns[a])
            })
            .collect();
        let duration_s = len as f64 / self.per_second.max(1e-9);
        Waveform {
            per_second: n as f64 / duration_s.max(1e-9),
            columns,
        }
    }
}

/// Alles zu einem Track, wie es die Oberfläche braucht.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrackData {
    pub metadata: TrackMetadata,
    #[serde(skip)]
    pub artwork: Option<Vec<u8>>,
    pub beat_grid: Vec<GridBeat>,
    pub cues: Vec<CuePoint>,
    pub waveform: Option<Waveform>,
}

impl TrackData {
    /// Inhalts-Fingerabdruck aus Beatgrid und Wellenform der Analyse-Dateien.
    ///
    /// Er bleibt gleich, wenn derselbe Track auf einem anderen USB-Stick liegt oder
    /// seine Tags (Titel, Artist) geändert wurden, und ändert sich erst, wenn
    /// rekordbox den Track neu analysiert. `None`, wenn es weder Grid noch
    /// Wellenform gibt.
    pub fn fingerprint(&self) -> Option<String> {
        let wave = self.waveform.as_ref().filter(|w| !w.columns.is_empty());
        if self.beat_grid.len() < 8 && wave.is_none() {
            return None;
        }
        // FNV-1a, 64 Bit: stabil über Versionen und Plattformen.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        for b in &self.beat_grid {
            eat(&b.time_ms.to_le_bytes());
            eat(&[b.beat_in_bar]);
        }
        eat(b"|");
        if let Some(w) = wave {
            for c in &w.columns {
                eat(&[c.height, c.rgb[0], c.rgb[1], c.rgb[2]]);
            }
        }
        Some(format!("{h:016x}"))
    }
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;

    #[test]
    fn fingerprint_ignores_tags_and_follows_content() {
        let grid: Vec<GridBeat> = (0..64)
            .map(|i| GridBeat {
                time_ms: 500 * i,
                beat_in_bar: (i % 4) as u8 + 1,
                bpm: 120.0,
            })
            .collect();
        let a = TrackData {
            beat_grid: grid.clone(),
            ..Default::default()
        };
        let mut b = a.clone();
        b.metadata.title = "Anderer Titel".into();
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.fingerprint().unwrap().len(), 16);
        let mut c = a.clone();
        c.beat_grid[3].time_ms += 1;
        assert_ne!(a.fingerprint(), c.fingerprint());
        assert_eq!(TrackData::default().fingerprint(), None);
    }
}
