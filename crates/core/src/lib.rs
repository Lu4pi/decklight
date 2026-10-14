//! Laufzeit von DeckLight. Verbindet PRO DJ LINK, Engine, Show-Daten und
//! Ausgänge; Desktop-App, Webserver und Kommandozeile bauen darauf auf.

pub mod auto_in;
mod metadata;
pub mod monitor;
pub mod runtime;
pub mod timecode_out;
pub mod triggers_out;

pub use monitor::{HistoryEntry, Level, Monitor, Warning};
pub use runtime::{content_key, nfs_port_for, track_key, Core, CoreConfig, Shared};
pub use timecode_out::{TimecodeOutConfig, TimecodeStatus};
pub use triggers_out::ActionOutConfig;
