//! Track-Daten für DeckLight: Metadaten, Cover, Beatgrid, Wellenformen und Cues
//! vom dbserver der Player, mit Zwischenspeicher.

pub mod anlz;
pub mod cache;
pub mod dbserver;
pub mod loader;
pub mod track;

pub use cache::{TrackCache, TrackId};
pub use dbserver::client::TrackLocation;
pub use loader::load_track;
pub use track::{CueKind, CuePoint, GridBeat, TrackData, TrackMetadata, WaveColumn, Waveform};
