//! Zustand eines Players, zusammengeführt aus Status-, Beat- und Positionspaketen.

use crate::beatgrid::BeatGrid;
use crate::clock::{ClockConfig, TimecodeClock};
use crate::events::EngineEvent;
use decklight_prolink::packet::{Beat, CdjStatus, PrecisePosition, TrackSlot};
use decklight_prolink::PlayState;
use serde::{Deserialize, Serialize};

/// Woher ein geladener Track stammt. Die Rekordbox-ID gilt nur je Medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackRef {
    pub source_player: u8,
    pub slot: TrackSlot,
    pub rekordbox_id: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeckState {
    pub number: u8,
    pub name: String,
    pub track: Option<TrackRef>,
    pub play_state: PlayState,
    pub moving: bool,
    pub pitch_percent: f64,
    pub track_bpm: Option<f64>,
    pub beat_number: Option<u32>,
    pub beat_in_bar: u8,
    pub tempo_master: bool,
    /// On-Air laut Player-Flag; die Engine bevorzugt die Mixer-Kanäle.
    pub on_air_flag: bool,
    pub on_air: bool,
    pub on_air_since: Option<f64>,
    pub track_length_ms: Option<u32>,
    /// CDJ-3000 liefern präzise Positionen; ältere Player nur Beats.
    pub has_precise_position: bool,
    pub grid: Option<BeatGrid>,
    pub clock: TimecodeClock,
    /// Nummer des nächsten Beats, wenn sie aus dem Status bekannt ist.
    pending_beat: Option<u32>,
    pub last_seen: f64,
}

impl DeckState {
    pub fn new(number: u8, name: &str, clock: ClockConfig) -> Self {
        Self {
            number,
            name: name.to_string(),
            track: None,
            play_state: PlayState::NoTrack,
            moving: false,
            pitch_percent: 0.0,
            track_bpm: None,
            beat_number: None,
            beat_in_bar: 0,
            tempo_master: false,
            on_air_flag: false,
            on_air: false,
            on_air_since: None,
            track_length_ms: None,
            has_precise_position: false,
            grid: None,
            clock: TimecodeClock::new(clock),
            pending_beat: None,
            last_seen: 0.0,
        }
    }

    pub fn rate(&self) -> f64 {
        1.0 + self.pitch_percent / 100.0
    }

    pub fn effective_bpm(&self) -> Option<f64> {
        self.track_bpm.map(|b| b * self.rate())
    }

    pub fn position_at(&self, t: f64) -> Option<f64> {
        self.track.and(self.clock.position_at(t))
    }

    /// Zeitpunkt eines Beats: aus dem Beatgrid oder, solange keins geladen ist,
    /// aus einem gleichmäßigen Raster mit dem ersten Beat bei 0.
    fn beat_time(&self, beat: u32) -> Option<f64> {
        if let Some(g) = &self.grid {
            return g.time_of(beat);
        }
        let bpm = self.track_bpm?;
        Some(f64::from(beat.saturating_sub(1)) * 60_000.0 / bpm)
    }

    fn beat_at(&self, pos_ms: f64) -> Option<u32> {
        if let Some(g) = &self.grid {
            return Some(g.beat_at(pos_ms));
        }
        let bpm = self.track_bpm?;
        Some((pos_ms.max(0.0) * bpm / 60_000.0).floor() as u32 + 1)
    }

    fn observe(&mut self, t: f64, pos_ms: f64, out: &mut Vec<EngineEvent>) {
        if let Some(j) = self.clock.observe(t, pos_ms, self.rate(), self.moving) {
            out.push(EngineEvent::Jump {
                deck: self.number,
                from_ms: j.from_ms,
                to_ms: j.to_ms,
            });
        }
    }

    pub fn apply_status(&mut self, t: f64, s: &CdjStatus) -> Vec<EngineEvent> {
        let mut out = Vec::new();
        self.last_seen = t;
        self.name.clone_from(&s.name);

        let track = (s.rekordbox_id != 0).then_some(TrackRef {
            source_player: s.track_source_player,
            slot: s.track_slot,
            rekordbox_id: s.rekordbox_id,
        });
        if track != self.track {
            if self.track.is_some() {
                out.push(EngineEvent::TrackUnloaded { deck: self.number });
            }
            self.track = track;
            self.grid = None;
            self.track_length_ms = None;
            self.beat_number = None;
            self.pending_beat = None;
            self.clock.reset();
            if let Some(track) = track {
                out.push(EngineEvent::TrackLoaded {
                    deck: self.number,
                    track,
                });
            }
        }

        let rate_before = self.rate();
        let moving_before = self.moving;
        self.play_state = s.play_state;
        self.moving = track.is_some() && s.play_state.is_moving();
        self.pitch_percent = s.pitch_percent;
        self.track_bpm = s.track_bpm;
        self.tempo_master = s.master;
        self.on_air_flag = s.on_air;
        if s.beat_in_bar != 0 {
            self.beat_in_bar = s.beat_in_bar;
        }

        if track.is_some() {
            let predicted = self.clock.position_at(t);
            if self.has_precise_position {
                // Position kommt aus den präzisen Paketen; hier nur Start/Stopp und Tempo.
                if let Some(p) = predicted {
                    if self.moving != moving_before || (self.rate() - rate_before).abs() > 1e-9 {
                        self.observe(t, p, &mut out);
                    }
                }
            } else if let Some(b) = s.beat_number {
                let clock_beat = predicted.and_then(|p| self.beat_at(p));
                let off_grid = clock_beat.is_none_or(|cb| cb.abs_diff(b) > 1);
                if off_grid || (!self.moving && clock_beat != Some(b)) {
                    // Bekannt ist nur die Beat-Nummer: Beat-Mitte annehmen (höchstens
                    // ein halber Beat daneben), das nächste Beat-Paket rastet exakt ein.
                    if let (Some(start), Some(bpm)) = (self.beat_time(b), self.track_bpm) {
                        self.observe(t, start + 30_000.0 / bpm, &mut out);
                    }
                    self.pending_beat = Some(b + 1);
                } else if let Some(p) = predicted {
                    if self.moving != moving_before || (self.rate() - rate_before).abs() > 1e-9 {
                        self.observe(t, p, &mut out);
                    }
                }
                if self.moving && !moving_before {
                    self.pending_beat = Some(b + 1);
                }
                // Status-Pakete hinken den Beat-Paketen hinterher: im Lauf nur vorwärts übernehmen.
                self.beat_number = match self.beat_number {
                    Some(n) if self.moving && !off_grid => Some(n.max(b)),
                    _ => Some(b),
                };
            }
        }

        if self.moving != moving_before {
            let position_ms = self.clock.position_at(t).unwrap_or(0.0);
            out.push(if self.moving {
                EngineEvent::PlayStarted {
                    deck: self.number,
                    position_ms,
                }
            } else {
                EngineEvent::PlayStopped {
                    deck: self.number,
                    position_ms,
                }
            });
        }
        out
    }

    pub fn apply_precise_position(&mut self, t: f64, p: &PrecisePosition) -> Vec<EngineEvent> {
        let mut out = Vec::new();
        self.last_seen = t;
        if !self.has_precise_position {
            // Bisher nur aus Beats geschätzt: ab jetzt gilt die präzise Position, ohne Sprung.
            self.clock.reset();
            self.has_precise_position = true;
        }
        self.track_length_ms = Some(p.track_length_s.saturating_mul(1000));
        if self.track.is_some() {
            self.pitch_percent = p.pitch_percent;
            self.observe(t, f64::from(p.position_ms), &mut out);
        }
        out
    }

    pub fn apply_beat(&mut self, t: f64, b: &Beat) -> Vec<EngineEvent> {
        let mut out = Vec::new();
        self.last_seen = t;
        self.beat_in_bar = b.beat_in_bar;
        self.pitch_percent = b.pitch_percent;
        if b.track_bpm.is_some() {
            self.track_bpm = b.track_bpm;
        }
        if !self.has_precise_position && self.track.is_some() && self.moving {
            // Ein Beat-Paket heißt: ein Beat beginnt genau jetzt. Welcher, sagt die
            // Uhr (nächstgelegener Beat-Anfang); Status- und Beat-Pakete können sich
            // im Netz überholen, deshalb nicht einfach weiterzählen.
            let half_beat = self.track_bpm.map_or(0.0, |bpm| 30_000.0 / bpm);
            let next = match (self.pending_beat.take(), self.clock.position_at(t)) {
                (Some(n), _) => n,
                (None, Some(p)) => self.beat_at(p + half_beat).unwrap_or(1),
                (None, None) => self.beat_number.map_or(1, |n| n + 1),
            };
            if let Some(pos) = self.beat_time(next) {
                self.observe(t, pos, &mut out);
            }
            self.beat_number = Some(next);
        }
        out.push(EngineEvent::Beat {
            deck: self.number,
            beat_in_bar: b.beat_in_bar,
            bpm: b.effective_bpm(),
        });
        out
    }
}
