//! MIDI-Nachrichten (Note, CC, Program Change) und Ausgabe über einen MIDI-Port.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MidiMessage {
    /// Kanal 1–16.
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
}

impl MidiMessage {
    pub fn bytes(&self) -> Vec<u8> {
        let ch = |c: u8| c.clamp(1, 16) - 1;
        match *self {
            Self::NoteOn {
                channel,
                note,
                velocity,
            } => vec![0x90 | ch(channel), note & 0x7f, velocity & 0x7f],
            Self::NoteOff { channel, note } => vec![0x80 | ch(channel), note & 0x7f, 0],
            Self::ControlChange {
                channel,
                controller,
                value,
            } => {
                vec![0xb0 | ch(channel), controller & 0x7f, value & 0x7f]
            }
            Self::ProgramChange { channel, program } => vec![0xc0 | ch(channel), program & 0x7f],
        }
    }
}

#[cfg(feature = "midi")]
pub use port::*;

#[cfg(feature = "midi")]
mod port {
    use midir::{MidiOutput, MidiOutputConnection};

    #[derive(Debug, thiserror::Error)]
    pub enum MidiError {
        #[error("MIDI nicht verfügbar: {0}")]
        Init(String),
        #[error("kein MIDI-Ausgang passt zu \"{0}\"")]
        NotFound(String),
        #[error("MIDI-Fehler: {0}")]
        Other(String),
    }

    pub fn output_ports() -> Result<Vec<String>, MidiError> {
        let out = MidiOutput::new("DeckLight").map_err(|e| MidiError::Init(e.to_string()))?;
        Ok(out
            .ports()
            .iter()
            .filter_map(|p| out.port_name(p).ok())
            .collect())
    }

    pub struct MidiOut {
        conn: MidiOutputConnection,
        pub name: String,
    }

    impl MidiOut {
        /// Öffnet den ersten Ausgang, dessen Name `wanted` enthält (ohne Groß/klein).
        pub fn open(wanted: &str) -> Result<Self, MidiError> {
            let out = MidiOutput::new("DeckLight").map_err(|e| MidiError::Init(e.to_string()))?;
            let needle = wanted.to_lowercase();
            let port = out
                .ports()
                .into_iter()
                .find(|p| {
                    out.port_name(p)
                        .is_ok_and(|n| n.to_lowercase().contains(&needle))
                })
                .ok_or_else(|| MidiError::NotFound(wanted.to_string()))?;
            let name = out.port_name(&port).unwrap_or_default();
            let conn = out
                .connect(&port, "DeckLight")
                .map_err(|e| MidiError::Other(e.to_string()))?;
            Ok(Self { conn, name })
        }

        /// Eigener virtueller Port (macOS, Linux). Unter Windows z. B. loopMIDI nutzen.
        #[cfg(unix)]
        pub fn virtual_port(name: &str) -> Result<Self, MidiError> {
            use midir::os::unix::VirtualOutput;
            let out = MidiOutput::new("DeckLight").map_err(|e| MidiError::Init(e.to_string()))?;
            let conn = out
                .create_virtual(name)
                .map_err(|e| MidiError::Other(e.to_string()))?;
            Ok(Self {
                conn,
                name: name.to_string(),
            })
        }

        pub fn send(&mut self, bytes: &[u8]) -> Result<(), MidiError> {
            self.conn
                .send(bytes)
                .map_err(|e| MidiError::Other(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_bytes() {
        assert_eq!(
            MidiMessage::NoteOn {
                channel: 1,
                note: 60,
                velocity: 127
            }
            .bytes(),
            [0x90, 60, 127]
        );
        assert_eq!(
            MidiMessage::ControlChange {
                channel: 16,
                controller: 7,
                value: 200
            }
            .bytes(),
            [0xbf, 7, 0x48]
        );
        assert_eq!(
            MidiMessage::ProgramChange {
                channel: 2,
                program: 5
            }
            .bytes(),
            [0xc1, 5]
        );
    }
}
