//! Die Engine: nimmt PRO-DJ-LINK-Pakete entgegen, hält den Zustand aller Decks,
//! wählt das Master-Deck und erzeugt typisierte Ereignisse.

use crate::clock::ClockConfig;
use crate::deck::DeckState;
use crate::events::EngineEvent;
use crate::master::{DeckFacts, MasterMode, MasterSelector};
use decklight_prolink::packet::{MixerStatus, Packet};
use decklight_prolink::{DeviceKind, LinkEvent};
use serde::Serialize;
use std::collections::BTreeMap;

/// Ohne Status so lange gilt ein Deck als verschwunden.
pub const DECK_TIMEOUT_S: f64 = 5.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeviceInfo {
    pub number: u8,
    pub name: String,
    pub kind: DeviceKind,
    pub ip: String,
}

#[derive(Debug, Clone)]
pub struct Engine {
    clock_cfg: ClockConfig,
    decks: BTreeMap<u8, DeckState>,
    devices: BTreeMap<u8, DeviceInfo>,
    mixer: Option<MixerStatus>,
    mixer_on_air: Option<Vec<bool>>,
    master: MasterSelector,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(ClockConfig::default(), MasterMode::default())
    }
}

impl Engine {
    pub fn new(clock_cfg: ClockConfig, mode: MasterMode) -> Self {
        Self {
            clock_cfg,
            decks: BTreeMap::new(),
            devices: BTreeMap::new(),
            mixer: None,
            mixer_on_air: None,
            master: MasterSelector::new(mode),
        }
    }

    pub fn decks(&self) -> impl Iterator<Item = &DeckState> {
        self.decks.values()
    }

    pub fn deck(&self, number: u8) -> Option<&DeckState> {
        self.decks.get(&number)
    }

    pub fn deck_mut(&mut self, number: u8) -> Option<&mut DeckState> {
        self.decks.get_mut(&number)
    }

    pub fn devices(&self) -> impl Iterator<Item = &DeviceInfo> {
        self.devices.values()
    }

    pub fn mixer(&self) -> Option<&MixerStatus> {
        self.mixer.as_ref()
    }

    pub fn master(&self) -> Option<u8> {
        self.master.current()
    }

    pub fn master_mode(&self) -> MasterMode {
        self.master.mode
    }

    pub fn set_master_mode(&mut self, mode: MasterMode) {
        self.master.set_mode(mode);
    }

    /// Position des Master-Decks zum Zeitpunkt `t`.
    pub fn master_position(&self, t: f64) -> Option<(u8, f64)> {
        let n = self.master()?;
        Some((n, self.decks.get(&n)?.position_at(t)?))
    }

    pub fn handle_link_event(&mut self, t: f64, ev: &LinkEvent) -> Vec<EngineEvent> {
        match ev {
            LinkEvent::DeviceFound(d) => {
                self.devices.insert(
                    d.number,
                    DeviceInfo {
                        number: d.number,
                        name: d.name.clone(),
                        kind: d.kind,
                        ip: d.ip.to_string(),
                    },
                );
                vec![EngineEvent::DeviceFound {
                    number: d.number,
                    name: d.name.clone(),
                    kind: d.kind,
                }]
            }
            LinkEvent::DeviceLost(d) => {
                self.devices.remove(&d.number);
                let mut out = vec![EngineEvent::DeviceLost {
                    number: d.number,
                    name: d.name.clone(),
                }];
                if self.decks.remove(&d.number).is_some() {
                    out.extend(self.tick(t));
                }
                out
            }
            LinkEvent::NumberConflict {
                number,
                other,
                switched_to,
            } => {
                vec![EngineEvent::NumberConflict {
                    number: *number,
                    name: other.name.clone(),
                    switched_to: *switched_to,
                }]
            }
            LinkEvent::Packet { packet, .. } => self.handle_packet(t, packet),
        }
    }

    pub fn handle_packet(&mut self, t: f64, packet: &Packet) -> Vec<EngineEvent> {
        let cfg = self.clock_cfg;
        let mut out = match packet {
            Packet::CdjStatus(s) if (1..=6).contains(&s.number) => self
                .decks
                .entry(s.number)
                .or_insert_with(|| DeckState::new(s.number, &s.name, cfg))
                .apply_status(t, s),
            Packet::PrecisePosition(p) => match self.decks.get_mut(&p.number) {
                Some(d) => d.apply_precise_position(t, p),
                None => Vec::new(),
            },
            Packet::Beat(b) => match self.decks.get_mut(&b.number) {
                Some(d) => d.apply_beat(t, b),
                None => Vec::new(),
            },
            Packet::OnAir(o) => {
                self.mixer_on_air = Some(o.channels.clone());
                Vec::new()
            }
            Packet::MixerStatus(m) => {
                self.mixer = Some(m.clone());
                Vec::new()
            }
            _ => Vec::new(),
        };
        out.extend(self.tick(t));
        out
    }

    /// On-Air auflösen, verschwundene Decks entfernen, Master neu bewerten.
    /// Regelmäßig aufrufen (z. B. alle 50 ms), damit die Hysterese greift.
    pub fn tick(&mut self, t: f64) -> Vec<EngineEvent> {
        let mut out = Vec::new();
        let stale: Vec<u8> = self
            .decks
            .values()
            .filter(|d| t - d.last_seen > DECK_TIMEOUT_S)
            .map(|d| d.number)
            .collect();
        for n in stale {
            self.decks.remove(&n);
            out.push(EngineEvent::TrackUnloaded { deck: n });
        }

        for d in self.decks.values_mut() {
            let from_mixer = self
                .mixer_on_air
                .as_ref()
                .and_then(|ch| ch.get(usize::from(d.number) - 1))
                .copied();
            let on_air = from_mixer.unwrap_or(d.on_air_flag);
            if on_air != d.on_air {
                d.on_air = on_air;
                d.on_air_since = on_air.then_some(t);
                out.push(EngineEvent::OnAirChanged {
                    deck: d.number,
                    on_air,
                });
            }
        }

        let facts: Vec<DeckFacts> = self
            .decks
            .values()
            .map(|d| DeckFacts {
                number: d.number,
                loaded: d.track.is_some(),
                playing: d.moving,
                on_air: d.on_air,
                tempo_master: d.tempo_master,
                on_air_since: d.on_air_since,
            })
            .collect();
        if let Some((from, to)) = self.master.update(t, &facts) {
            out.push(EngineEvent::MasterChanged { from, to });
        }
        out
    }

    pub fn snapshot(&self, t: f64) -> EngineSnapshot {
        EngineSnapshot {
            master: self.master(),
            master_mode: self.master.mode,
            devices: self.devices.values().cloned().collect(),
            decks: self
                .decks
                .values()
                .map(|d| DeckSnapshot {
                    number: d.number,
                    name: d.name.clone(),
                    track: d.track,
                    play_state: d.play_state,
                    moving: d.moving,
                    position_ms: d.position_at(t),
                    track_length_ms: d.track_length_ms,
                    pitch_percent: d.pitch_percent,
                    track_bpm: d.track_bpm,
                    effective_bpm: d.effective_bpm(),
                    beat_in_bar: d.beat_in_bar,
                    on_air: d.on_air,
                    tempo_master: d.tempo_master,
                    precise: d.has_precise_position,
                })
                .collect(),
            mixer_bpm: self.mixer.as_ref().and_then(|m| m.bpm),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeckSnapshot {
    pub number: u8,
    pub name: String,
    pub track: Option<crate::deck::TrackRef>,
    pub play_state: decklight_prolink::PlayState,
    pub moving: bool,
    pub position_ms: Option<f64>,
    pub track_length_ms: Option<u32>,
    pub pitch_percent: f64,
    pub track_bpm: Option<f64>,
    pub effective_bpm: Option<f64>,
    pub beat_in_bar: u8,
    pub on_air: bool,
    pub tempo_master: bool,
    pub precise: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineSnapshot {
    pub master: Option<u8>,
    pub master_mode: MasterMode,
    pub devices: Vec<DeviceInfo>,
    pub decks: Vec<DeckSnapshot>,
    pub mixer_bpm: Option<f64>,
}
