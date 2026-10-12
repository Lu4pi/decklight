//! Ein virtueller Player: Track, Playhead, Pitch, Loop.

use decklight_prolink::packet::{
    Beat, CdjStatus, KeepAlive, PlayState, PrecisePosition, TrackSlot,
};
use decklight_prolink::DeviceKind;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

/// Ein Track, wie ihn der Simulator kennt. Später auch aus Audiodatei + Beatgrid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimTrack {
    pub id: u32,
    pub title: String,
    pub artist: String,
    pub length_ms: u32,
    pub bpm: f64,
    /// Position des ersten Beats im Track.
    #[serde(default)]
    pub first_beat_ms: f64,
}

impl SimTrack {
    pub fn beat_len_ms(&self) -> f64 {
        60_000.0 / self.bpm
    }

    /// Beat-Nummer (ab 1) an einer Track-Position; vor dem ersten Beat 1.
    pub fn beat_at(&self, position_ms: f64) -> u32 {
        let n = ((position_ms - self.first_beat_ms) / self.beat_len_ms()).floor();
        if n < 0.0 {
            1
        } else {
            n as u32 + 1
        }
    }

    /// Track-Position, an der Beat `beat` (ab 1) beginnt.
    pub fn beat_position(&self, beat: u32) -> f64 {
        self.first_beat_ms + f64::from(beat.saturating_sub(1)) * self.beat_len_ms()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimDeck {
    pub number: u8,
    pub name: String,
    pub ip: Ipv4Addr,
    pub track: Option<SimTrack>,
    pub play_state: PlayState,
    pub pitch_percent: f64,
    /// Playhead in Track-Millisekunden.
    pub position_ms: f64,
    /// Rückwärtslauf (Reverse-Taste).
    pub reverse: bool,
    pub master: bool,
    pub on_air: bool,
    pub loop_range: Option<(f64, f64)>,
    packet_counter: u32,
}

impl SimDeck {
    pub fn new(number: u8, ip: Ipv4Addr) -> Self {
        Self {
            number,
            name: "CDJ-3000".into(),
            ip,
            track: None,
            play_state: PlayState::NoTrack,
            pitch_percent: 0.0,
            position_ms: 0.0,
            reverse: false,
            master: false,
            on_air: false,
            loop_range: None,
            packet_counter: 0,
        }
    }

    pub fn mac(&self) -> [u8; 6] {
        // Lokal administrierte Adresse "02:44:4c:..." ("DL"), eindeutig je Player.
        [0x02, 0x44, 0x4c, 0x00, 0x00, self.number]
    }

    pub fn is_moving(&self) -> bool {
        self.track.is_some() && self.play_state.is_moving()
    }

    /// Abspielrate relativ zur Echtzeit (Pitch, Richtung).
    pub fn rate(&self) -> f64 {
        let r = 1.0 + self.pitch_percent / 100.0;
        if self.reverse {
            -r
        } else {
            r
        }
    }

    pub fn load(&mut self, track: SimTrack) {
        self.track = Some(track);
        self.position_ms = 0.0;
        self.loop_range = None;
        self.play_state = PlayState::Cued;
    }

    pub fn play(&mut self) {
        if self.track.is_some() {
            self.play_state = if self.loop_range.is_some() {
                PlayState::Looping
            } else {
                PlayState::Playing
            };
        }
    }

    pub fn pause(&mut self) {
        if self.track.is_some() {
            self.play_state = PlayState::Paused;
        }
    }

    /// Zurück auf den Cue-Punkt (hier: Track-Anfang) und anhalten.
    pub fn cue(&mut self) {
        if self.track.is_some() {
            self.position_ms = 0.0;
            self.play_state = PlayState::Cued;
        }
    }

    pub fn seek(&mut self, ms: f64) {
        if let Some(t) = &self.track {
            self.position_ms = ms.clamp(0.0, f64::from(t.length_ms));
        }
    }

    pub fn set_loop(&mut self, range: Option<(f64, f64)>) {
        self.loop_range = range.filter(|(a, b)| b > a);
        if self.is_moving() {
            self.play();
        }
    }

    /// Bewegt den Playhead um `dt_ms` Echtzeit weiter. Liefert die Beat-Nummer,
    /// wenn dabei (vorwärts) eine Beat-Grenze überschritten wurde.
    pub fn advance(&mut self, dt_ms: f64) -> Option<u32> {
        if !self.is_moving() {
            return None;
        }
        let track = self.track.as_ref()?;
        let before = track.beat_at(self.position_ms);
        let mut pos = self.position_ms + dt_ms * self.rate();
        let mut wrapped = false;
        if let Some((start, end)) = self.loop_range {
            if pos >= end && self.position_ms < end {
                pos = start + (pos - end) % (end - start);
                wrapped = true;
            }
        }
        let length = f64::from(track.length_ms);
        if pos >= length {
            pos = length;
            self.play_state = PlayState::Ended;
        }
        if pos <= 0.0 {
            pos = 0.0;
            self.play_state = PlayState::Paused;
        }
        self.position_ms = pos;
        let after = track.beat_at(pos);
        let crossed_forward = !self.reverse && (after > before || (wrapped && after != before));
        // Genau auf dem ersten Beat (Startposition) zählt ebenfalls.
        crossed_forward.then_some(after)
    }

    pub fn keep_alive(&self, peers_seen: u8) -> KeepAlive {
        KeepAlive {
            name: self.name.clone(),
            number: self.number,
            kind: DeviceKind::Player,
            mac: self.mac(),
            ip: self.ip,
            peers_seen,
        }
    }

    pub fn status(&mut self) -> CdjStatus {
        self.packet_counter = self.packet_counter.wrapping_add(1);
        let track = self.track.as_ref();
        let beat = track.map(|t| t.beat_at(self.position_ms));
        CdjStatus {
            name: self.name.clone(),
            number: self.number,
            track_source_player: if track.is_some() { self.number } else { 0 },
            track_slot: if track.is_some() {
                TrackSlot::Usb
            } else {
                TrackSlot::None
            },
            track_type: u8::from(track.is_some()),
            rekordbox_id: track.map_or(0, |t| t.id),
            track_number: 0,
            play_state: self.play_state,
            playing: self.is_moving(),
            master: self.master,
            sync: false,
            on_air: self.on_air,
            pitch_percent: self.pitch_percent,
            track_bpm: track.map(|t| t.bpm),
            beat_number: beat,
            beat_in_bar: beat.map_or(0, beat_in_bar),
            packet_counter: self.packet_counter,
        }
    }

    pub fn precise_position(&self) -> Option<PrecisePosition> {
        let t = self.track.as_ref()?;
        Some(PrecisePosition {
            name: self.name.clone(),
            number: self.number,
            track_length_s: t.length_ms / 1000,
            position_ms: self.position_ms.round() as u32,
            pitch_percent: self.pitch_percent,
            effective_bpm: Some(t.bpm * (1.0 + self.pitch_percent / 100.0)),
        })
    }

    /// Beat-Paket für den Beat, der gerade begonnen hat.
    pub fn beat_packet(&self, beat: u32) -> Option<Beat> {
        let t = self.track.as_ref()?;
        let rate = self.rate().abs().max(0.01);
        let beat_real_ms = t.beat_len_ms() / rate;
        let into_beat = (self.position_ms - t.beat_position(beat)).max(0.0) / rate;
        let next_beat = (beat_real_ms - into_beat).max(0.0);
        let in_bar = beat_in_bar(beat);
        let next_bar = next_beat + f64::from(4 - in_bar) * beat_real_ms;
        Some(Beat {
            name: self.name.clone(),
            number: self.number,
            next_beat_ms: next_beat.round() as u32,
            next_bar_ms: next_bar.round() as u32,
            pitch_percent: self.pitch_percent,
            track_bpm: Some(t.bpm),
            beat_in_bar: in_bar,
        })
    }
}

pub fn beat_in_bar(beat: u32) -> u8 {
    ((beat.max(1) - 1) % 4 + 1) as u8
}
