//! Track-Daten der simulierten Player: Metadaten, Cover, Beatgrid, Wellenform, Cues.
//! Damit lässt sich die ganze Oberfläche ohne Hardware vorführen.

use crate::deck::SimTrack;
use crate::scenario::{Command, Scenario};
use decklight_library::dbserver::server::TrackSource;
use decklight_library::{
    CueKind, CuePoint, GridBeat, Phrase, PhraseAnalysis, PhraseKind, PhraseMood, TrackData,
    TrackMetadata, WaveColumn, Waveform,
};
use std::collections::HashMap;

impl SimTrack {
    /// Track für die virtuellen Decks aus echten Track-Daten (Rekordbox-Export):
    /// Länge, Tempo und erster Taktanfang aus Metadaten und Beatgrid.
    pub fn from_track_data(d: &TrackData) -> Self {
        let bpm = d
            .metadata
            .bpm
            .or_else(|| d.beat_grid.first().map(|b| b.bpm))
            .filter(|b| *b > 0.0)
            .unwrap_or(120.0);
        let first_beat_ms = d
            .beat_grid
            .iter()
            .find(|b| b.beat_in_bar == 1)
            .map_or(0.0, |b| f64::from(b.time_ms));
        Self {
            id: d.metadata.rekordbox_id,
            title: d.metadata.title.clone(),
            artist: d.metadata.artist.clone(),
            length_ms: d.metadata.duration_s * 1000,
            bpm,
            first_beat_ms,
            file: None,
        }
    }

    /// Erzeugte Track-Daten (Grid aus BPM und erstem Beat, künstliche Wellenform).
    pub fn synthetic_data(&self) -> TrackData {
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
            phrases: Some(self.phrases(beats)),
        }
    }

    /// Phrasen passend zur erzeugten Wellenform (Stimmung „High“): je 32 Takte
    /// ein Abschnitt, jeder vierte ab dem zweiten ein Breakdown mit Build-up.
    fn phrases(&self, beats: u32) -> PhraseAnalysis {
        let section_beats = 128;
        let sections = beats.div_ceil(section_beats);
        let mut phrases = Vec::new();
        for i in 0..sections {
            let beat = i * section_beats + 1;
            let kind = match i {
                0 => PhraseKind::Intro,
                _ if i + 1 == sections => PhraseKind::Outro,
                _ if i % 4 == 1 => PhraseKind::Down,
                _ => PhraseKind::Chorus,
            };
            phrases.push(Phrase {
                beat,
                kind,
                fill_beat: None,
            });
            if kind == PhraseKind::Down && beat + 96 <= beats {
                phrases.push(Phrase {
                    beat: beat + 96,
                    kind: PhraseKind::Up,
                    fill_beat: Some(beat + 124),
                });
            }
        }
        PhraseAnalysis {
            mood: PhraseMood::High,
            end_beat: beats,
            phrases,
        }
    }

    /// Erzeugte Wellenform: Kicks auf jedem Beat, Abschnitte mit Breaks.
    fn waveform(&self) -> Waveform {
        let n = (f64::from(self.length_ms) / 1000.0 * 150.0) as usize;
        let beat = self.beat_len_ms();
        let mut seed = self.id.wrapping_mul(2_654_435_761);
        let bands: Vec<[u8; 3]> = (0..n)
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
                // Drei Bänder wie ein Club-Track: Kick im Bass, Flächen in den
                // Mitten, Hi-Hats auf den Achteln in den Höhen.
                let hat = (1.0 - ((phase * 2.0).fract() - 0.5).abs() * 8.0).max(0.0);
                // Breakdown: 24 Takte Fläche, dann 8 Takte Build-up mit
                // steigenden Höhen und einem Beat Pause vor dem Drop.
                let in_section = ((ms - self.first_beat_ms) / beat).rem_euclid(128.0);
                let (low, mid, high) = if breakdown && in_section >= 127.0 {
                    (0.0, 0.0, 0.0)
                } else if breakdown && in_section >= 96.0 {
                    let p = (in_section - 96.0) / 31.0;
                    (0.05, 0.4 + 0.3 * p, 0.15 + 0.6 * p * (0.6 + 0.4 * hat))
                } else if breakdown {
                    (0.05, 0.4 + 0.2 * noise, 0.1 * noise)
                } else {
                    (
                        0.3 + 0.7 * kick,
                        0.25 + 0.12 * noise + 0.15 * kick,
                        0.06 * noise + 0.18 * hat,
                    )
                };
                let q = |v: f64| (v.min(1.0) * 31.0).round() as u8;
                [q(low), q(mid), q(high)]
            })
            .collect();
        Waveform {
            per_second: 150.0,
            columns: bands.iter().map(|b| WaveColumn::from_bands(*b)).collect(),
            bands,
        }
    }
}

/// Alle Tracks eines Szenarios, als dbserver-Quelle.
pub struct SimLibrary {
    tracks: HashMap<u32, TrackData>,
}

impl SimLibrary {
    /// Track-Daten aller Tracks im Szenario. Tracks mit Audiodatei müssen vorher
    /// mit [`SimLibrary::load_audio`] analysiert sein, sonst gibt es erzeugte Daten.
    pub fn from_scenario(s: &Scenario) -> Self {
        let tracks = s
            .steps
            .iter()
            .filter_map(|st| match &st.command {
                Command::Load { track, .. } => Some((track.id, track.synthetic_data())),
                _ => None,
            })
            .collect();
        Self { tracks }
    }

    /// Audiodateien im Szenario analysieren: Länge, BPM und erster Beat landen im
    /// Szenario (für die Decks), Wellenform und Tags in der Bibliothek. Jede Datei
    /// wird nur einmal gelesen.
    pub fn load_audio(s: &mut Scenario) -> Result<Self, crate::audio::AudioError> {
        let mut lib = Self::from_scenario(s);
        let mut done: HashMap<u32, SimTrack> = HashMap::new();
        for st in &mut s.steps {
            let Command::Load { track, .. } = &mut st.command else {
                continue;
            };
            let Some(path) = track.file.clone() else {
                continue;
            };
            if let Some(t) = done.get(&track.id) {
                *track = t.clone();
                continue;
            }
            let (t, data) = SimTrack::from_audio(track.id, &path)?;
            lib.tracks.insert(t.id, data);
            done.insert(t.id, t.clone());
            *track = t;
        }
        Ok(lib)
    }

    /// Echte Track-Daten (z. B. aus einem Rekordbox-Export) für den dbserver.
    pub fn insert(&mut self, id: u32, data: TrackData) {
        self.tracks.insert(id, data);
    }

    pub fn get(&self, id: u32) -> Option<&TrackData> {
        self.tracks.get(&id)
    }
}

impl TrackSource for SimLibrary {
    fn track(&self, _slot: u8, rekordbox_id: u32) -> Option<TrackData> {
        self.tracks.get(&rekordbox_id).cloned()
    }
}
