//! Wertet Cue-Punkte und globale Trigger aus.
//!
//! Ein Cue löst aus, wenn der Playhead eines berechtigten Decks im normalen Lauf
//! über seine Position läuft. Pause, Scrubben im Stillstand und Sprünge nach vorn
//! lösen nichts aus. Ein ausgelöster Cue wird erst wieder scharf, wenn der Playhead
//! deutlich davor steht (Loop, Hot Cue, Zurückspulen), damit kleine Korrekturen
//! der Uhr beim Stoppen nie doppelt auslösen.

mod evaluator;
mod log;
mod template;

pub use evaluator::{
    DeckInput, Firing, FiringSource, TriggerEngine, JUMP_LANDING_LEGACY_MS, JUMP_LANDING_MS,
    REARM_MS,
};
pub use log::{ActionLog, LogEntry};
pub use template::{cue_position_ms, hot_cue_templates};
