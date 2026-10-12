//! Ablaufpläne für den Simulator: zeitgesteuerte Befehle an virtuelle Decks.

use crate::deck::SimTrack;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Command {
    Load {
        deck: u8,
        track: SimTrack,
    },
    Play {
        deck: u8,
    },
    Pause {
        deck: u8,
    },
    Cue {
        deck: u8,
    },
    Seek {
        deck: u8,
        ms: f64,
    },
    Pitch {
        deck: u8,
        percent: f64,
    },
    Reverse {
        deck: u8,
        on: bool,
    },
    Loop {
        deck: u8,
        start_ms: f64,
        end_ms: f64,
    },
    ExitLoop {
        deck: u8,
    },
    /// Tempo-Master an dieses Deck geben.
    Master {
        deck: u8,
    },
    /// Kanal-Fader am Mixer: Deck on air oder nicht.
    OnAir {
        deck: u8,
        on: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Zeitpunkt in Millisekunden ab Start.
    pub at_ms: u64,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub decks: u8,
    #[serde(default = "yes")]
    pub mixer: bool,
    /// Nach dem letzten Schritt wieder von vorn beginnen (Länge in ms).
    #[serde(default)]
    pub repeat_after_ms: Option<u64>,
    pub steps: Vec<Step>,
}

fn yes() -> bool {
    true
}

impl Scenario {
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Eingebaute Demo: zwei Decks, Übergang von Deck 1 auf Deck 2 nach einer
    /// Minute, mit Pitch-Änderung und Loop. Wiederholt sich alle drei Minuten.
    pub fn demo() -> Self {
        let a = SimTrack {
            id: 1001,
            title: "Midnight Signal".into(),
            artist: "Sim Artist".into(),
            length_ms: 6 * 60_000,
            bpm: 126.0,
            first_beat_ms: 120.0,
        };
        let b = SimTrack {
            id: 1002,
            title: "Second Light".into(),
            artist: "Other Sim".into(),
            length_ms: 5 * 60_000 + 30_000,
            bpm: 128.0,
            first_beat_ms: 80.0,
        };
        use Command::*;
        let steps = vec![
            (0, Load { deck: 1, track: a }),
            (0, Load { deck: 2, track: b }),
            (0, OnAir { deck: 1, on: true }),
            (0, Master { deck: 1 }),
            (500, Play { deck: 1 }),
            (
                20_000,
                Pitch {
                    deck: 1,
                    percent: 1.6,
                },
            ),
            (55_000, Play { deck: 2 }),
            (
                55_000,
                Pitch {
                    deck: 2,
                    percent: 0.0,
                },
            ),
            (56_000, OnAir { deck: 2, on: true }),
            (70_000, Master { deck: 2 }),
            (72_000, OnAir { deck: 1, on: false }),
            (75_000, Pause { deck: 1 }),
            (
                90_000,
                Loop {
                    deck: 2,
                    start_ms: 60_000.0,
                    end_ms: 67_500.0,
                },
            ),
            (110_000, ExitLoop { deck: 2 }),
            (120_000, Cue { deck: 1 }),
            (
                150_000,
                Pitch {
                    deck: 2,
                    percent: -2.0,
                },
            ),
        ];
        Self {
            decks: 2,
            mixer: true,
            repeat_after_ms: Some(180_000),
            steps: steps
                .into_iter()
                .map(|(at_ms, command)| Step { at_ms, command })
                .collect(),
        }
    }
}
