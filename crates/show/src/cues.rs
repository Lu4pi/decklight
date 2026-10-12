//! Cue-Punkte und globale Trigger: wann welche Aktion an Pult oder MIDI geht.

use crate::offsets::TrackKey;
use decklight_outputs::osc::OscArg;
use decklight_outputs::MidiMessage;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Was ausgelöst wird.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// grandMA3-Befehl über `/cmd`, z. B. `Go+ Exec 201`. `{deck}` wird ersetzt.
    Osc {
        command: String,
    },
    /// Beliebige OSC-Adresse (Präfix wird vorangestellt).
    OscMessage {
        address: String,
        #[serde(default)]
        args: Vec<OscArg>,
    },
    Midi {
        message: MidiMessage,
    },
}

impl Action {
    /// Platzhalter ersetzen: `{deck}` → Decknummer.
    pub fn render(&self, deck: Option<u8>) -> Action {
        let fill = |s: &str| match deck {
            Some(d) => s.replace("{deck}", &d.to_string()),
            None => s.to_string(),
        };
        match self {
            Action::Osc { command } => Action::Osc {
                command: fill(command),
            },
            Action::OscMessage { address, args } => Action::OscMessage {
                address: fill(address),
                args: args
                    .iter()
                    .map(|a| match a {
                        OscArg::String(s) => OscArg::String(fill(s)),
                        other => other.clone(),
                    })
                    .collect(),
            },
            Action::Midi { message } => Action::Midi { message: *message },
        }
    }

    /// Kurzbeschreibung für das Aktions-Log.
    pub fn describe(&self) -> String {
        match self {
            Action::Osc { command } => format!("OSC /cmd \"{command}\""),
            Action::OscMessage { address, args } => {
                let args: Vec<String> = args
                    .iter()
                    .map(|a| match a {
                        OscArg::Int(i) => i.to_string(),
                        OscArg::Float(f) => f.to_string(),
                        OscArg::String(s) => format!("\"{s}\""),
                    })
                    .collect();
                format!("OSC {address} {}", args.join(" "))
                    .trim_end()
                    .to_string()
            }
            Action::Midi { message } => match *message {
                MidiMessage::NoteOn {
                    channel,
                    note,
                    velocity,
                } => format!("MIDI Note {note} an, Vel {velocity}, Kanal {channel}"),
                MidiMessage::NoteOff { channel, note } => {
                    format!("MIDI Note {note} aus, Kanal {channel}")
                }
                MidiMessage::ControlChange {
                    channel,
                    controller,
                    value,
                } => format!("MIDI CC {controller} = {value}, Kanal {channel}"),
                MidiMessage::ProgramChange { channel, program } => {
                    format!("MIDI Program {program}, Kanal {channel}")
                }
            },
        }
    }
}

/// Wo im Track ein Cue liegt.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CuePosition {
    /// Millisekunden ab Track-Anfang.
    Ms { ms: f64 },
    /// Takt und Schlag (ab 1), gezählt ab dem ersten Taktanfang im Beatgrid.
    Bar { bar: u32, beat: u8 },
}

/// Welche Decks einen Cue auslösen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FireScope {
    /// Nur das Master-Deck (Standard: das Deck, das auch den Timecode liefert).
    #[default]
    Master,
    /// Jedes Deck, das den Track spielt.
    AnyDeck,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub id: u32,
    pub name: String,
    pub position: CuePosition,
    /// Vorlauf: so viele ms vor der Position auslösen (z. B. Pult-Latenz, Fades).
    #[serde(default)]
    pub lead_ms: f64,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default = "yes")]
    pub armed: bool,
    /// Eigene Einstellung statt der globalen.
    #[serde(default)]
    pub scope: Option<FireScope>,
    /// Auch auslösen, wenn ein Sprung (Hot Cue) genau auf dem Cue landet.
    #[serde(default = "yes")]
    pub fire_on_landing: bool,
}

fn yes() -> bool {
    true
}

impl Cue {
    pub fn at_ms(name: &str, ms: f64, actions: Vec<Action>) -> Self {
        Self {
            id: 0,
            name: name.to_string(),
            position: CuePosition::Ms { ms },
            lead_ms: 0.0,
            actions,
            color: None,
            armed: true,
            scope: None,
            fire_on_landing: true,
        }
    }
}

/// Deck-Ereignisse für globale Trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalEvent {
    TrackLoaded,
    /// Ein Deck wird Master.
    MasterChanged,
    Play,
    Stop,
    OnAir,
    OffAir,
}

/// Für welche Decks ein globaler Trigger gilt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", content = "deck", rename_all = "snake_case")]
pub enum DeckFilter {
    #[default]
    Any,
    Master,
    Deck(u8),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalTrigger {
    pub id: u32,
    pub name: String,
    pub on: GlobalEvent,
    #[serde(default)]
    pub decks: DeckFilter,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default = "yes")]
    pub armed: bool,
}

/// Alle Cues und Trigger einer Show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerTable {
    /// Globaler Schalter. Not-Aus setzt ihn auf `false`.
    #[serde(default = "yes")]
    pub armed: bool,
    #[serde(default)]
    pub scope: FireScope,
    /// Rekordbox-Hot-Cues neuer Tracks als (leere) Cue-Vorlagen übernehmen.
    #[serde(default = "yes")]
    pub import_hot_cues: bool,
    #[serde(default)]
    pub tracks: BTreeMap<TrackKey, Vec<Cue>>,
    #[serde(default)]
    pub globals: Vec<GlobalTrigger>,
    #[serde(default)]
    next_id: u32,
}

impl Default for TriggerTable {
    fn default() -> Self {
        Self {
            armed: true,
            scope: FireScope::Master,
            import_hot_cues: true,
            tracks: BTreeMap::new(),
            globals: Vec::new(),
            next_id: 0,
        }
    }
}

impl TriggerTable {
    fn new_id(&mut self) -> u32 {
        let used = self
            .tracks
            .values()
            .flatten()
            .map(|c| c.id)
            .chain(self.globals.iter().map(|g| g.id))
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(used) + 1;
        self.next_id
    }

    pub fn cues(&self, key: &TrackKey) -> &[Cue] {
        self.tracks.get(key).map_or(&[], Vec::as_slice)
    }

    /// Cue hinzufügen; bekommt eine neue, eindeutige ID.
    pub fn add_cue(&mut self, key: &TrackKey, mut cue: Cue) -> u32 {
        cue.id = self.new_id();
        let id = cue.id;
        self.tracks.entry(key.clone()).or_default().push(cue);
        id
    }

    pub fn add_global(&mut self, mut g: GlobalTrigger) -> u32 {
        g.id = self.new_id();
        let id = g.id;
        self.globals.push(g);
        id
    }

    pub fn cue(&self, id: u32) -> Option<&Cue> {
        self.tracks.values().flatten().find(|c| c.id == id)
    }

    pub fn cue_mut(&mut self, id: u32) -> Option<&mut Cue> {
        self.tracks.values_mut().flatten().find(|c| c.id == id)
    }

    /// Cue oder globalen Trigger entfernen.
    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.tracks.values().map(Vec::len).sum::<usize>() + self.globals.len();
        for cues in self.tracks.values_mut() {
            cues.retain(|c| c.id != id);
        }
        self.tracks.retain(|_, c| !c.is_empty());
        self.globals.retain(|g| g.id != id);
        before != self.tracks.values().map(Vec::len).sum::<usize>() + self.globals.len()
    }

    /// Einzelnen Cue oder Trigger scharf schalten oder sperren.
    pub fn set_armed(&mut self, id: u32, armed: bool) -> bool {
        if let Some(c) = self.cue_mut(id) {
            c.armed = armed;
            return true;
        }
        if let Some(g) = self.globals.iter_mut().find(|g| g.id == id) {
            g.armed = armed;
            return true;
        }
        false
    }

    /// Not-Aus: nichts wird mehr ausgelöst, bis wieder scharf geschaltet wird.
    pub fn panic(&mut self) {
        self.armed = false;
    }

    /// Vorlagen übernehmen, wenn der Track noch keine eigenen Cues hat.
    pub fn import_templates(&mut self, key: &TrackKey, templates: Vec<Cue>) -> usize {
        if !self.import_hot_cues || !self.cues(key).is_empty() {
            return 0;
        }
        let n = templates.len();
        for c in templates {
            self.add_cue(key, c);
        }
        n
    }

    /// Cues einer vorläufigen Kennung auf die endgültige umziehen. Hat die neue
    /// Kennung schon Cues (derselbe Song von einem anderen Stick), gewinnen diese.
    pub fn rekey(&mut self, old: &TrackKey, new: &TrackKey) {
        if old == new {
            return;
        }
        if let Some(cues) = self.tracks.remove(old) {
            self.tracks.entry(new.clone()).or_insert(cues);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> TrackKey {
        TrackKey(s.into())
    }

    #[test]
    fn ids_are_unique_and_survive_serde() {
        let mut t = TriggerTable::default();
        let a = t.add_cue(&key("A"), Cue::at_ms("Drop", 1000.0, vec![]));
        let b = t.add_global(GlobalTrigger {
            id: 0,
            name: "Laden".into(),
            on: GlobalEvent::TrackLoaded,
            decks: DeckFilter::Any,
            actions: vec![],
            armed: true,
        });
        assert_ne!(a, b);
        let json = serde_json::to_string(&t).unwrap();
        let mut back: TriggerTable = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
        let c = back.add_cue(&key("B"), Cue::at_ms("X", 0.0, vec![]));
        assert!(c > a && c > b);
    }

    #[test]
    fn minimal_json_uses_defaults() {
        let t: TriggerTable = serde_json::from_str(
            r#"{"tracks":{"A – B [3:00]":[{"id":1,"name":"Drop","position":{"type":"bar","bar":17,"beat":1},
                "actions":[{"type":"osc","command":"Go+ Exec 201"},
                           {"type":"midi","message":{"type":"note_on","channel":1,"note":60,"velocity":127}}]}]},
                "globals":[{"id":2,"name":"Master","on":"master_changed","decks":{"type":"deck","deck":2}}]}"#,
        )
        .unwrap();
        assert!(t.armed && t.import_hot_cues);
        let c = &t.cues(&key("A – B [3:00]"))[0];
        assert!(c.armed && c.fire_on_landing && c.lead_ms == 0.0);
        assert_eq!(c.position, CuePosition::Bar { bar: 17, beat: 1 });
        assert_eq!(t.globals[0].decks, DeckFilter::Deck(2));
    }

    #[test]
    fn rekey_keeps_existing_cues() {
        let mut t = TriggerTable::default();
        t.add_cue(&key("Song"), Cue::at_ms("alt", 1.0, vec![]));
        t.add_cue(&key("p1-usb-3"), Cue::at_ms("neu", 2.0, vec![]));
        t.rekey(&key("p1-usb-3"), &key("Song"));
        assert_eq!(t.cues(&key("Song"))[0].name, "alt");
        assert!(t.cues(&key("p1-usb-3")).is_empty());
        t.add_cue(&key("p2-usb-1"), Cue::at_ms("x", 2.0, vec![]));
        t.rekey(&key("p2-usb-1"), &key("Other"));
        assert_eq!(t.cues(&key("Other"))[0].name, "x");
    }

    #[test]
    fn templates_only_for_tracks_without_cues() {
        let mut t = TriggerTable::default();
        assert_eq!(
            t.import_templates(&key("A"), vec![Cue::at_ms("A", 1.0, vec![])]),
            1
        );
        assert_eq!(
            t.import_templates(&key("A"), vec![Cue::at_ms("B", 2.0, vec![])]),
            0
        );
        t.import_hot_cues = false;
        assert_eq!(
            t.import_templates(&key("C"), vec![Cue::at_ms("B", 2.0, vec![])]),
            0
        );
    }

    #[test]
    fn arm_remove_panic_and_render() {
        let mut t = TriggerTable::default();
        let id = t.add_cue(
            &key("A"),
            Cue::at_ms(
                "x",
                1.0,
                vec![Action::Osc {
                    command: "Go+ Exec 20{deck}".into(),
                }],
            ),
        );
        assert!(t.set_armed(id, false));
        assert!(!t.cues(&key("A"))[0].armed);
        let a = t.cues(&key("A"))[0].actions[0].render(Some(3));
        assert_eq!(a.describe(), "OSC /cmd \"Go+ Exec 203\"");
        t.panic();
        assert!(!t.armed);
        assert!(t.remove(id));
        assert!(!t.remove(id));
        assert!(t.tracks.is_empty());
    }
}
