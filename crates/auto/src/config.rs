//! Einstellungen des Auto-Piloten und die grandMA3-Befehle je Abschnitt.

use crate::pilot::AutoCue;
use crate::structure::Section;
use decklight_show::Action;
use serde::{Deserialize, Serialize};

/// Befehle je Abschnitt. Bei Abschnitten wechseln mehrere Zeilen sich ab
/// (Rotation: jeder neue Drop nimmt den nächsten Look); bei Ereignissen
/// (Hit, Pause, Stopp, Übergabe) gehen alle Zeilen raus.
///
/// Platzhalter: `{seq}` Sequenz, `{deck}` führendes Deck, `{bpm}` Tempo,
/// `{pct}` Build-Fortschritt 0–100.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoCommands {
    pub intro: Vec<String>,
    pub groove: Vec<String>,
    pub breakdown: Vec<String>,
    pub build: Vec<String>,
    pub drop: Vec<String>,
    pub outro: Vec<String>,
    /// Zusätzlich genau auf der Eins des Drops.
    pub hit: Vec<String>,
    /// Pause im Track (Stille vor dem Drop).
    pub pause: Vec<String>,
    /// DJ stoppt, nichts läuft mehr.
    pub stop: Vec<String>,
    /// Stufenlos während des Build-ups (`{pct}`), am Ende mit 0.
    pub build_level: String,
    /// Beim Ausschalten: Licht an den Operator zurückgeben.
    pub release: Vec<String>,
}

fn cue(n: u32) -> String {
    format!("Goto Sequence {{seq}} Cue {n}")
}

impl Default for AutoCommands {
    fn default() -> Self {
        Self {
            intro: vec![cue(5)],
            groove: vec![cue(1), cue(2), cue(3)],
            breakdown: vec![cue(10)],
            build: vec![cue(20)],
            drop: vec![cue(30), cue(31), cue(32)],
            outro: vec![cue(60)],
            hit: vec![format!("Go+ Sequence {DEFAULT_HIT_SEQUENCE}")],
            pause: vec![cue(50)],
            stop: vec![cue(50)],
            build_level: "Master 3.2 At {pct}".into(),
            release: vec![
                "Off Sequence {seq}".into(),
                format!("Off Sequence {DEFAULT_HIT_SEQUENCE}"),
            ],
        }
    }
}

pub const DEFAULT_SEQUENCE: &str = "\"DeckLight Auto\"";
pub const DEFAULT_HIT_SEQUENCE: &str = "\"DeckLight Hit\"";

impl AutoCommands {
    pub fn section(&self, s: Section) -> &[String] {
        match s {
            Section::Intro => &self.intro,
            Section::Groove => &self.groove,
            Section::Breakdown => &self.breakdown,
            Section::Build => &self.build,
            Section::Drop => &self.drop,
            Section::Outro => &self.outro,
        }
    }
}

/// Was tun, wenn ein Track eigene Cues in DeckLight hat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualCues {
    /// Auto läuft trotzdem, die eigenen Cues kommen dazu.
    #[default]
    Complement,
    /// Tracks mit eigenen Cues steuert Auto nicht.
    Yield,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoConfig {
    /// Nur anzeigen und ins Log schreiben, nichts senden.
    pub shadow: bool,
    /// Abschnitte unter dieser Sicherheit (0–100) zählen als Groove.
    pub min_confidence: u8,
    /// So viel früher senden (Netz, Pult, Lampen), in ms.
    pub lead_ms: f64,
    /// Sequenz für `{seq}`, mit Anführungszeichen bei Namen.
    pub sequence: String,
    /// Groove und Drop wechseln den Look alle so viele Takte (0 = nie).
    pub rotate_bars: u32,
    /// Hit höchstens einmal in so vielen Takten.
    pub hit_every_bars: u32,
    pub manual_cues: ManualCues,
    /// Ein- und Ausschalten per OSC (`/decklight/auto`) auf diesem Port.
    pub osc_in_port: Option<u16>,
    pub commands: AutoCommands,
}

pub const DEFAULT_OSC_IN_PORT: u16 = 8100;

impl Default for AutoConfig {
    fn default() -> Self {
        Self {
            shadow: false,
            min_confidence: 50,
            lead_ms: 20.0,
            sequence: DEFAULT_SEQUENCE.into(),
            rotate_bars: 16,
            hit_every_bars: 16,
            manual_cues: ManualCues::Complement,
            osc_in_port: Some(DEFAULT_OSC_IN_PORT),
            commands: AutoCommands::default(),
        }
    }
}

/// Werte für die Platzhalter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Fill {
    pub deck: Option<u8>,
    pub bpm: Option<f64>,
    pub pct: Option<u8>,
}

impl AutoConfig {
    fn fill(&self, template: &str, f: Fill) -> String {
        let mut s = template.replace("{seq}", self.sequence.trim());
        if let Some(d) = f.deck {
            s = s.replace("{deck}", &d.to_string());
        }
        if let Some(b) = f.bpm {
            s = s.replace("{bpm}", &format!("{b:.1}"));
        }
        if let Some(p) = f.pct {
            s = s.replace("{pct}", &p.to_string());
        }
        s
    }

    /// MA3-Befehle für ein Auto-Ereignis. `rotation` wählt bei Abschnitten
    /// die Zeile.
    pub fn actions(&self, cue: &AutoCue, rotation: usize, f: Fill) -> Vec<Action> {
        let c = &self.commands;
        let pick = |list: &[String]| -> Vec<String> {
            let list: Vec<&String> = list.iter().filter(|l| !l.trim().is_empty()).collect();
            if list.is_empty() {
                Vec::new()
            } else {
                vec![list[rotation % list.len()].clone()]
            }
        };
        let all = |list: &[String]| -> Vec<String> {
            list.iter()
                .filter(|l| !l.trim().is_empty())
                .cloned()
                .collect()
        };
        let lines = match cue {
            AutoCue::Section(s) => pick(c.section(*s)),
            AutoCue::Hit => all(&c.hit),
            AutoCue::Pause => all(&c.pause),
            AutoCue::Stop => all(&c.stop),
            AutoCue::Release => all(&c.release),
            AutoCue::BuildLevel(p) => {
                let f = Fill { pct: Some(*p), ..f };
                return (!c.build_level.trim().is_empty())
                    .then(|| Action::Osc {
                        command: self.fill(&c.build_level, f),
                    })
                    .into_iter()
                    .collect();
            }
        };
        lines
            .iter()
            .map(|l| Action::Osc {
                command: self.fill(l, f),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_render_ma3_commands() {
        let c = AutoConfig::default();
        let cmd = |a: &[Action]| match &a[0] {
            Action::Osc { command } => command.clone(),
            _ => unreachable!(),
        };
        let f = Fill::default();
        assert_eq!(
            cmd(&c.actions(&AutoCue::Section(Section::Drop), 0, f)),
            "Goto Sequence \"DeckLight Auto\" Cue 30"
        );
        assert_eq!(
            cmd(&c.actions(&AutoCue::Section(Section::Drop), 4, f)),
            "Goto Sequence \"DeckLight Auto\" Cue 31"
        );
        assert_eq!(
            cmd(&c.actions(&AutoCue::BuildLevel(42), 0, f)),
            "Master 3.2 At 42"
        );
        assert_eq!(c.actions(&AutoCue::Release, 0, f).len(), 2);
        // Ältere Einstellungen ohne Auto-Felder laden mit Standardwerten.
        let old: AutoConfig = serde_json::from_str(r#"{"shadow":true}"#).unwrap();
        assert!(old.shadow);
        assert_eq!(old.commands, AutoCommands::default());
    }
}
