//! Befehle der Oberfläche: Offsets, Cues, Trigger, Not-Aus, Einstellungen.

use crate::settings::Settings;
use decklight_core::Core;
use decklight_engine::MasterMode;
use decklight_outputs::FrameRate;
use decklight_show::{Action, Cue, FireScope, GlobalTrigger, TrackKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    SetOffset {
        key: TrackKey,
        offset_ms: f64,
    },
    /// Festen Offset verwerfen und neu automatisch vergeben.
    RemoveOffset {
        key: TrackKey,
    },
    ImportOffsets {
        csv: String,
        #[serde(default)]
        rate: Option<FrameRate>,
    },
    SetMasterMode {
        mode: MasterMode,
    },
    AddCue {
        key: TrackKey,
        cue: Cue,
    },
    /// Cue mit gleicher ID ersetzen.
    UpdateCue {
        cue: Cue,
    },
    AddGlobal {
        trigger: GlobalTrigger,
    },
    UpdateGlobal {
        trigger: GlobalTrigger,
    },
    /// Cue oder globalen Trigger löschen.
    Remove {
        id: u32,
    },
    /// Einzelnen Cue/Trigger (`id`) oder alles (`id` leer) scharf schalten/sperren.
    SetArmed {
        #[serde(default)]
        id: Option<u32>,
        armed: bool,
    },
    Panic,
    SetScope {
        scope: FireScope,
    },
    SetImportHotCues {
        on: bool,
    },
    /// Aktionen sofort senden (Test-Button).
    Fire {
        name: String,
        actions: Vec<Action>,
    },
    /// Cue von Hand auslösen.
    FireCue {
        id: u32,
    },
    UpdateSettings {
        settings: Box<Settings>,
    },
    ClearHistory,
    /// Windows: Firewall-Regel für DeckLight anlegen (nur vom eigenen Rechner).
    FixFirewall,
    /// Laufende Show unter einem Namen speichern (ohne Namen: unter dem aktuellen).
    ShowSave {
        #[serde(default)]
        name: Option<String>,
    },
    ShowOpen {
        name: String,
    },
    /// Leere Show; Ausgangs-Einstellungen bleiben.
    ShowNew {
        name: String,
    },
    ShowImport {
        file: Box<crate::library::ShowFile>,
    },
    BackupRestore {
        file: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

impl Reply {
    pub fn ok() -> Self {
        Self {
            ok: true,
            ..Default::default()
        }
    }

    pub fn err(e: impl ToString) -> Self {
        Self {
            ok: false,
            error: Some(e.to_string()),
            ..Default::default()
        }
    }

    fn found(found: bool, what: &str) -> Self {
        if found {
            Self::ok()
        } else {
            Self::err(format!("{what} nicht gefunden"))
        }
    }
}

/// Befehle, die nur den Kern betreffen (Einstellungen behandelt der Server).
pub fn apply(core: &Core, cmd: Command, rate: FrameRate) -> Reply {
    match cmd {
        Command::Panic => {
            core.panic();
            Reply::ok()
        }
        Command::FixFirewall => Reply::err("Firewall-Freigabe läuft über den Server"),
        Command::SetArmed { id: None, armed } => {
            core.set_armed(armed);
            Reply::ok()
        }
        Command::Fire { name, actions } => {
            core.fire(&name, None, actions);
            Reply::ok()
        }
        Command::FireCue { id } => {
            let cue = core
                .with(|s| {
                    s.triggers
                        .tracks
                        .values()
                        .flatten()
                        .find(|c| c.id == id)
                        .cloned()
                })
                .flatten();
            match cue {
                Some(c) => {
                    core.fire(&c.name, None, c.actions);
                    Reply::ok()
                }
                None => Reply::err("Cue nicht gefunden"),
            }
        }
        Command::SetMasterMode { mode } => {
            core.with(|s| s.engine.set_master_mode(mode));
            Reply::ok()
        }
        other => core
            .with(|s| match other {
                Command::SetOffset { key, offset_ms } => {
                    if !offset_ms.is_finite() || offset_ms < 0.0 {
                        return Reply::err("Offset muss ≥ 0 sein");
                    }
                    s.offsets.set(&key, offset_ms);
                    Reply::ok()
                }
                Command::RemoveOffset { key } => {
                    let label = s
                        .offsets
                        .get(&key)
                        .map(|e| e.label.clone())
                        .unwrap_or_default();
                    s.offsets.remove(&key);
                    s.offsets.assign(&key, &label);
                    Reply::ok()
                }
                Command::ImportOffsets { csv, rate: r } => {
                    match s.offsets.import_csv(&csv, r.unwrap_or(rate)) {
                        Ok(n) => Reply {
                            count: Some(n),
                            ..Reply::ok()
                        },
                        Err(e) => Reply::err(e),
                    }
                }
                Command::AddCue { key, cue } => Reply {
                    id: Some(s.triggers.add_cue(&key, cue)),
                    ..Reply::ok()
                },
                Command::UpdateCue { cue } => match s.triggers.cue_mut(cue.id) {
                    Some(c) => {
                        *c = cue;
                        Reply::ok()
                    }
                    None => Reply::err("Cue nicht gefunden"),
                },
                Command::AddGlobal { trigger } => Reply {
                    id: Some(s.triggers.add_global(trigger)),
                    ..Reply::ok()
                },
                Command::UpdateGlobal { trigger } => {
                    let g = s.triggers.globals.iter_mut().find(|g| g.id == trigger.id);
                    let found = g.is_some();
                    if let Some(g) = g {
                        *g = trigger;
                    }
                    Reply::found(found, "Trigger")
                }
                Command::Remove { id } => Reply::found(s.triggers.remove(id), "Eintrag"),
                Command::SetArmed {
                    id: Some(id),
                    armed,
                } => Reply::found(s.triggers.set_armed(id, armed), "Eintrag"),
                Command::SetScope { scope } => {
                    s.triggers.scope = scope;
                    Reply::ok()
                }
                Command::SetImportHotCues { on } => {
                    s.triggers.import_hot_cues = on;
                    Reply::ok()
                }
                Command::ClearHistory => {
                    s.monitor.history.clear();
                    Reply::ok()
                }
                _ => Reply::err("Befehl hier nicht möglich"),
            })
            .unwrap_or_else(|| Reply::err("Kern nicht verfügbar")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_parse_from_ui_json() {
        let c: Command = serde_json::from_str(
            r#"{"cmd":"add_cue","key":"A – B [3:00]","cue":{"id":0,"name":"Drop","position":{"type":"ms","ms":61000},"actions":[{"type":"osc","command":"Go+ Exec 1"}]}}"#,
        )
        .unwrap();
        assert!(matches!(c, Command::AddCue { .. }));
        let c: Command = serde_json::from_str(r#"{"cmd":"set_armed","armed":false}"#).unwrap();
        assert_eq!(
            c,
            Command::SetArmed {
                id: None,
                armed: false
            }
        );
        let c: Command = serde_json::from_str(r#"{"cmd":"panic"}"#).unwrap();
        assert_eq!(c, Command::Panic);
        let c: Command =
            serde_json::from_str(r#"{"cmd":"set_master_mode","mode":{"mode":"manual","deck":2}}"#)
                .unwrap();
        assert_eq!(
            c,
            Command::SetMasterMode {
                mode: MasterMode::Manual(2)
            }
        );
    }
}
