//! Auto-Pilot für Momente ohne Lichtoperator.
//!
//! [`structure`] zerlegt einen Track beim Laden in Abschnitte (Intro, Groove,
//! Breakdown, Build-up, Drop, Outro) und findet Pausen vor dem Drop, aus der
//! rekordbox-Phrasenanalyse und der Drei-Band-Wellenform. [`pilot`] verfolgt
//! im Betrieb das führende Deck und meldet Wechsel genau zur richtigen Zeit;
//! [`config`] macht daraus grandMA3-Befehle.

pub mod config;
pub mod pilot;
pub mod structure;

pub use config::{AutoCommands, AutoConfig, ManualCues};
pub use pilot::{AutoCue, AutoFiring, AutoStatus, DeckView, Pilot, Upcoming};
pub use structure::{analyze, Pause, Section, Segment, Source, Structure};
