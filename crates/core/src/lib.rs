//! Laufzeit von DeckLight. Verbindet PRO DJ LINK, Engine, Show-Daten und
//! Ausgänge; Desktop-App, Webserver und Kommandozeile bauen darauf auf.

pub mod runtime;
pub mod timecode_out;

pub use runtime::{track_key, Core, CoreConfig, Shared};
pub use timecode_out::{TimecodeOutConfig, TimecodeStatus};
