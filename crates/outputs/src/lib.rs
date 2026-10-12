//! Ausgänge von DeckLight: Timecode (Art-Net, MTC) und Steuerbefehle (OSC, MIDI).

pub mod artnet;
pub mod generator;
pub mod midi;
pub mod mtc;
pub mod osc;
pub mod timecode;

pub use generator::{GeneratorConfig, StopMode, TcInput, TcMessage, TimecodeGenerator};
pub use midi::MidiMessage;
pub use timecode::{FrameRate, Timecode};
