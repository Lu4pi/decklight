//! Track-Daten der simulierten Player: Metadaten, Cover, Beatgrid, Wellenform, Cues.
//! Damit lässt sich die ganze Oberfläche ohne Hardware vorführen.

use crate::deck::SimTrack;
use crate::scenario::{Command, Scenario};
use decklight_library::dbserver::server::TrackSource;
use decklight_library::{
    CueKind, CuePoint, GridBeat, TrackData, TrackMetadata, WaveColumn, Waveform,
};
use std::collections::HashMap;

impl SimTrack {
    pub fn track_data(&self) -> TrackData {
        let beat = self.beat_len_ms();
        let beats = ((f64::from(self.length_ms) - self.first_beat_ms) / beat)
            .floor()
            .max(0.0) as u32;
        let beat_grid = (0..beats)
            .map(|i| GridBeat {
                time_ms: (self.first_beat_ms + f64::from(i) * beat).round() as u32,
                beat_in_bar: (i % 4) as u8 + 1,
                bpm: self.bpm,
            })
            .collect();
        // Hot Cues A–D alle 32 Takte, ein Memory Cue auf dem ersten Beat.
        let bar = beat * 4.0;
        let mut cues = vec![CuePoint {
            kind: CueKind::Memory,
            position_ms: self.first_beat_ms.round() as u32,
            loop_end_ms: None,
            color: None,
            comment: String::new(),
        }];
        let colors = [[255, 64, 64], [64, 200, 255], [255, 170, 0], [140, 255, 90]];
        for (i, label) in ["Intro", "Break", "Drop", "Outro"].iter().enumerate() {
            let at = self.first_beat_ms + bar * 32.0 * (i as f64 + 1.0);
            if at < f64::from(self.length_ms) {
                cues.push(CuePoint {
                    kind: CueKind::Hot(i as u8 + 1),
                    position_ms: at.round() as u32,
                    loop_end_ms: None,
                    color: Some(colors[i]),
                    comment: (*label).to_string(),
                });
            }
        }
        TrackData {
            metadata: TrackMetadata {
                rekordbox_id: self.id,
                title: self.title.clone(),
                artist: self.artist.clone(),
                album: "DeckLight Simulator".into(),
                genre: "Electronic".into(),
                key: ["8A", "9A", "5A", "11B"][self.id as usize % 4].into(),
                comment: String::new(),
                duration_s: self.length_ms / 1000,
                bpm: Some(self.bpm),
                rating: 3,
                artwork_id: self.id,
            },
            artwork: Some(crate::art::cover(self.id)),
            beat_grid,
            cues,
            waveform: Some(self.waveform()),
        }
    }

    /// Erzeugte Wellenform: Kicks auf jedem Beat, Abschnitte mit Breaks.
    fn waveform(&self) -> Waveform {
        let n = (f64::from(self.length_ms) / 1000.0 * 150.0) as usize;
        let beat = self.beat_len_ms();
        let mut seed = self.id.wrapping_mul(2_654_435_761);
        let columns = (0..n)
            .map(|i| {
                let ms = i as f64 * 1000.0 / 150.0;
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let noise = f64::from(seed % 100) / 100.0;
                let phase = ((ms - self.first_beat_ms) / beat).rem_euclid(1.0);
                let section = ((ms - self.first_beat_ms) / (beat * 4.0 * 32.0)).floor() as i64;
                let breakdown = section % 4 == 1;
                let kick = if breakdown {
                    0.0
                } else {
                    (1.0 - phase * 4.0).max(0.0)
                };
                let body = if breakdown { 0.25 } else { 0.45 };
                let h = (body + 0.5 * kick + 0.15 * noise).min(1.0);
                let rgb = if kick > 0.3 {
                    [40, 90, 255]
                } else if breakdown {
                    [230, 240, 255]
                } else {
                    [120, 200, 255]
                };
                WaveColumn {
                    height: (h * 31.0) as u8,
                    rgb,
                }
            })
            .collect();
        Waveform {
            per_second: 150.0,
            columns,
        }
    }
}

/// Alle Tracks eines Szenarios, als dbserver-Quelle.
pub struct SimLibrary {
    tracks: HashMap<u32, TrackData>,
}

impl SimLibrary {
    pub fn from_scenario(s: &Scenario) -> Self {
        let tracks = s
            .steps
            .iter()
            .filter_map(|st| match &st.command {
                Command::Load { track, .. } => Some((track.id, track.track_data())),
                _ => None,
            })
            .collect();
        Self { tracks }
    }
}

impl TrackSource for SimLibrary {
    fn track(&self, _slot: u8, rekordbox_id: u32) -> Option<TrackData> {
        self.tracks.get(&rekordbox_id).cloned()
    }
}
