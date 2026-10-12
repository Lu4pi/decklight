//! Engine von DeckLight: Deck-Zustand, Timecode-Uhr mit Jitterfilter,
//! Master-Auswahl und Ereignisse. Rein rechnerisch, ohne Netz und ohne Uhr:
//! jede Funktion bekommt die aktuelle Zeit übergeben. Das macht alles im
//! Simulator und mit Aufzeichnungen exakt testbar.

pub mod beatgrid;
pub mod clock;
pub mod deck;
pub mod engine;
pub mod events;
pub mod master;
pub mod record;

pub use beatgrid::BeatGrid;
pub use clock::{ClockConfig, Jump, TimecodeClock};
pub use deck::{DeckState, TrackRef};
pub use engine::{DeckSnapshot, DeviceInfo, Engine, EngineSnapshot};
pub use events::{EngineEvent, Stamped};
pub use master::{MasterMode, MasterSelector};
