//! Was die Oberfläche zu sehen bekommt: kompakte JSON-Ansichten des Zustands.

use base64::Engine as _;
use decklight_core::{HistoryEntry, Shared, Warning};
use decklight_engine::{DeckSnapshot, DeviceInfo, MasterMode};
use decklight_library::{CuePoint, TrackData, TrackMetadata};
use decklight_outputs::{FrameRate, Timecode};
use decklight_show::{Cue, OffsetTable, TrackKey, TriggerTable};
use decklight_triggers::LogEntry;
use serde::Serialize;

/// Kurzinfo zum Track auf einem Deck.
#[derive(Debug, Clone, Serialize)]
pub struct TrackSummary {
    pub key: TrackKey,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub musical_key: String,
    pub duration_s: u32,
    pub bpm: Option<f64>,
    pub has_artwork: bool,
    /// Metadaten schon geladen.
    pub loaded: bool,
    pub offset_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckView {
    #[serde(flatten)]
    pub deck: DeckSnapshot,
    pub master: bool,
    pub info: Option<TrackSummary>,
    /// Timecode an der aktuellen Position (Offset + Position).
    pub timecode_ms: Option<f64>,
    /// Derselbe Timecode als HH:MM:SS:FF in der eingestellten Bildrate.
    pub timecode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimecodeView {
    pub text: Option<String>,
    pub deck: Option<u8>,
    pub running: bool,
    pub errors: u64,
    pub enabled: bool,
    pub rate: FrameRate,
}

/// Zustand für jeden Bildschirm-Frame (30×/s).
#[derive(Debug, Clone, Serialize)]
pub struct StateView {
    pub t: f64,
    pub master: Option<u8>,
    pub master_mode: MasterMode,
    pub armed: bool,
    pub decks: Vec<DeckView>,
    pub timecode: TimecodeView,
    pub mixer_bpm: Option<f64>,
}

pub fn state(s: &Shared, t: f64, timecode_enabled: bool, rate: FrameRate) -> StateView {
    let snap = s.engine.snapshot(t);
    let decks = snap
        .decks
        .into_iter()
        .map(|d| {
            let info = d.track.map(|r| {
                let key = s.key_for(&r);
                let offset_ms = s.offsets.get(&key).map(|e| e.offset_ms);
                match s.track_data(&r) {
                    Some(data) => summary(&key, &data, offset_ms),
                    None => TrackSummary {
                        title: format!("Track {}", r.rekordbox_id),
                        key,
                        artist: String::new(),
                        album: String::new(),
                        musical_key: String::new(),
                        duration_s: d.track_length_ms.unwrap_or(0) / 1000,
                        bpm: d.track_bpm,
                        has_artwork: false,
                        loaded: false,
                        offset_ms,
                    },
                }
            });
            let timecode_ms = info
                .as_ref()
                .and_then(|i| Some(i.offset_ms? + d.position_ms?));
            let timecode = timecode_ms.map(|ms| Timecode::from_ms(ms.max(0.0), rate).to_string());
            DeckView {
                master: snap.master == Some(d.number),
                deck: d,
                info,
                timecode_ms,
                timecode,
            }
        })
        .collect();
    StateView {
        t,
        master: snap.master,
        master_mode: snap.master_mode,
        armed: s.triggers.armed,
        decks,
        timecode: TimecodeView {
            text: s.timecode.timecode.map(|tc| tc.to_string()),
            deck: s.timecode.deck,
            running: s.timecode.running,
            errors: s.timecode.errors,
            enabled: timecode_enabled,
            rate,
        },
        mixer_bpm: snap.mixer_bpm,
    }
}

pub fn summary(key: &TrackKey, data: &TrackData, offset_ms: Option<f64>) -> TrackSummary {
    let m: &TrackMetadata = &data.metadata;
    TrackSummary {
        key: key.clone(),
        title: m.title.clone(),
        artist: m.artist.clone(),
        album: m.album.clone(),
        musical_key: m.key.clone(),
        duration_s: m.duration_s,
        bpm: m.bpm,
        has_artwork: data.artwork.is_some(),
        loaded: true,
        offset_ms,
    }
}

/// Seltener aktualisierte Daten fürs Monitoring (2×/s).
#[derive(Debug, Clone, Serialize)]
pub struct MonitorView {
    pub devices: Vec<DeviceInfo>,
    pub warnings: Vec<Warning>,
    pub history: Vec<HistoryEntry>,
    pub log: Vec<LogEntry>,
}

const MONITOR_TAIL: usize = 200;

pub fn monitor(s: &Shared) -> MonitorView {
    let tail = |n: usize| n.saturating_sub(MONITOR_TAIL);
    let warnings: Vec<Warning> = s.monitor.warnings().cloned().collect();
    let log: Vec<LogEntry> = s.trigger_log.entries().cloned().collect();
    MonitorView {
        devices: s.engine.devices().cloned().collect(),
        warnings: warnings[tail(warnings.len())..].to_vec(),
        history: s.monitor.history.clone(),
        log: log[tail(log.len())..].to_vec(),
    }
}

/// Show-Daten (Offsets und Cues), gesendet bei jeder Änderung.
#[derive(Debug, Clone, Serialize)]
pub struct ShowView {
    pub offsets: OffsetTable,
    pub triggers: TriggerTable,
    /// Bekannte Tracks mit Namen für Setlist und Editor.
    pub tracks: Vec<TrackSummary>,
}

pub fn show(s: &Shared) -> ShowView {
    let mut tracks: Vec<TrackSummary> = s
        .tracks
        .iter()
        .map(|(k, d)| summary(k, d, s.offsets.get(k).map(|e| e.offset_ms)))
        .collect();
    tracks.sort_by(|a, b| a.key.cmp(&b.key));
    ShowView {
        offsets: s.offsets.clone(),
        triggers: s.triggers.clone(),
        tracks,
    }
}

/// Alles für Wellenform und Cue-Editor eines Tracks.
#[derive(Debug, Clone, Serialize)]
pub struct TrackDetail {
    pub summary: TrackSummary,
    /// Beat-Zeiten in ms und Schlag im Takt.
    pub beats: Vec<(u32, u8)>,
    pub rekordbox_cues: Vec<CuePoint>,
    pub cues: Vec<Cue>,
    pub waveform: Option<WaveformView>,
}

/// Wellenform kompakt: je Spalte vier Bytes (Höhe 0–31, R, G, B), Base64.
/// Dazu, falls vorhanden, die drei Bänder (Bass, Mitten, Höhen; je 0–31).
#[derive(Debug, Clone, Serialize)]
pub struct WaveformView {
    pub per_second: f64,
    pub columns: usize,
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bands: Option<String>,
}

pub fn track_detail(s: &Shared, key: &TrackKey) -> Option<TrackDetail> {
    let data = s.tracks.get(key)?;
    let waveform = data.waveform.as_ref().map(|w| {
        let mut bytes = Vec::with_capacity(w.columns.len() * 4);
        for c in &w.columns {
            bytes.extend_from_slice(&[c.height, c.rgb[0], c.rgb[1], c.rgb[2]]);
        }
        let b64 = &base64::engine::general_purpose::STANDARD;
        WaveformView {
            per_second: w.per_second,
            columns: w.columns.len(),
            data: b64.encode(bytes),
            bands: (w.bands.len() == w.columns.len() && !w.bands.is_empty())
                .then(|| b64.encode(w.bands.concat())),
        }
    });
    Some(TrackDetail {
        summary: summary(key, data, s.offsets.get(key).map(|e| e.offset_ms)),
        beats: data
            .beat_grid
            .iter()
            .map(|b| (b.time_ms, b.beat_in_bar))
            .collect(),
        rekordbox_cues: data.cues.clone(),
        cues: s.triggers.cues(key).to_vec(),
        waveform,
    })
}
