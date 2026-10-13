//! Letzter bekannter Zustand aller Geräte, zusammengesetzt aus dem Event-Strom.
//! Grundlage für die CLI-Ansicht; die Engine baut darauf ihren eigenen Deck-Zustand.

use crate::device::Device;
use crate::net::LinkEvent;
use crate::packet::{Beat, CdjStatus, MixerStatus, OnAirChannels, Packet, PrecisePosition};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize)]
pub struct PlayerSnapshot {
    pub number: u8,
    pub name: String,
    pub status: Option<CdjStatus>,
    pub position: Option<PrecisePosition>,
    pub last_beat: Option<Beat>,
}

impl PlayerSnapshot {
    /// On-Air laut Mixer, falls bekannt, sonst laut Player-Flag.
    pub fn on_air(&self, mixer: Option<&OnAirChannels>) -> bool {
        let from_mixer = mixer
            .and_then(|m| m.channels.get(usize::from(self.number).wrapping_sub(1)))
            .copied();
        from_mixer.unwrap_or_else(|| self.status.as_ref().is_some_and(|s| s.on_air))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LinkSnapshot {
    pub devices: BTreeMap<u8, Device>,
    pub players: BTreeMap<u8, PlayerSnapshot>,
    pub mixer: Option<MixerStatus>,
    pub on_air: Option<OnAirChannels>,
}

impl LinkSnapshot {
    pub fn apply(&mut self, event: &LinkEvent) {
        match event {
            LinkEvent::DeviceFound(d) => {
                self.devices.insert(d.number, d.clone());
            }
            LinkEvent::DeviceLost(d) => {
                self.devices.remove(&d.number);
                self.players.remove(&d.number);
            }
            LinkEvent::NumberConflict { .. } | LinkEvent::NoInterface { .. } => {}
            LinkEvent::Packet { packet, .. } => self.apply_packet(packet),
        }
    }

    pub fn apply_packet(&mut self, packet: &Packet) {
        match packet {
            Packet::CdjStatus(s) => self.player(s.number, &s.name).status = Some(s.clone()),
            Packet::PrecisePosition(p) => self.player(p.number, &p.name).position = Some(p.clone()),
            Packet::Beat(b) if b.number <= 6 => {
                self.player(b.number, &b.name).last_beat = Some(b.clone())
            }
            Packet::MixerStatus(m) => self.mixer = Some(m.clone()),
            Packet::OnAir(o) => self.on_air = Some(o.clone()),
            _ => {}
        }
    }

    fn player(&mut self, number: u8, name: &str) -> &mut PlayerSnapshot {
        self.players
            .entry(number)
            .or_insert_with(|| PlayerSnapshot {
                number,
                name: name.to_string(),
                ..Default::default()
            })
    }
}
