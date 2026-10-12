//! Typisierte Ereignisse der Engine. UI, Trigger und Ausgänge hören darauf.

use crate::deck::TrackRef;
use decklight_prolink::DeviceKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    DeviceFound {
        number: u8,
        name: String,
        kind: DeviceKind,
    },
    DeviceLost {
        number: u8,
        name: String,
    },
    /// Ein anderes Gerät nutzt unsere virtuelle Player-Nummer.
    NumberConflict {
        number: u8,
        name: String,
    },
    TrackLoaded {
        deck: u8,
        track: TrackRef,
    },
    TrackUnloaded {
        deck: u8,
    },
    /// Metadaten (Titel, Artist, Beatgrid …) eines Tracks sind da.
    TrackInfo {
        track: TrackRef,
        title: String,
        artist: String,
    },
    PlayStarted {
        deck: u8,
        position_ms: f64,
    },
    PlayStopped {
        deck: u8,
        position_ms: f64,
    },
    Beat {
        deck: u8,
        beat_in_bar: u8,
        bpm: Option<f64>,
    },
    /// Harte Positionsänderung (Cue, Hot Cue, Loop, Suchen).
    Jump {
        deck: u8,
        from_ms: f64,
        to_ms: f64,
    },
    OnAirChanged {
        deck: u8,
        on_air: bool,
    },
    MasterChanged {
        from: Option<u8>,
        to: Option<u8>,
    },
    /// Ein Cue oder Trigger hat Aktionen ausgelöst (kommt aus dem Trigger-Teil).
    TriggerFired {
        deck: Option<u8>,
        name: String,
        actions: Vec<String>,
        late_ms: f64,
        error: Option<String>,
    },
    /// Trigger global scharf geschaltet oder gesperrt (Not-Aus).
    ArmedChanged {
        armed: bool,
    },
}

/// Ereignis mit Zeitstempel (Sekunden auf der Engine-Uhr).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stamped {
    pub t: f64,
    #[serde(flatten)]
    pub event: EngineEvent,
}
