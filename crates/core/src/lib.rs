//! Laufzeit von DeckLight. Verbindet PRO DJ LINK, Engine, Show-Daten und
//! Ausgänge; Desktop-App, Webserver und Kommandozeile bauen darauf auf.

mod metadata;
pub mod runtime;
pub mod timecode_out;
pub mod triggers_out;

pub use runtime::{content_key, track_key, Core, CoreConfig, Shared};
pub use timecode_out::{TimecodeOutConfig, TimecodeStatus};
pub use triggers_out::ActionOutConfig;
