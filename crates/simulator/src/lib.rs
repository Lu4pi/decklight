//! Simulator für DeckLight: virtuelle CDJs und ein DJM, die dieselben Pakete wie
//! echte Geräte senden. Damit lassen sich Erkennung, Engine, Timecode und Trigger
//! ohne Hardware entwickeln und testen.
//!
//! [`Simulator::step`] ist deterministisch (keine Uhr, kein Netz) und liefert die
//! fälligen Pakete; [`run`] schickt sie in Echtzeit per UDP los.

pub mod deck;
pub mod scenario;

pub use deck::{SimDeck, SimTrack};
pub use scenario::{Command, Scenario, Step};

use decklight_prolink::encode;
use decklight_prolink::packet::{KeepAlive, MixerStatus, OnAirChannels, MIXER_DEVICE_NUMBER};
use decklight_prolink::{Channel, DeviceKind, PortConfig};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

pub const KEEP_ALIVE_EVERY_MS: f64 = 1500.0;
pub const STATUS_EVERY_MS: f64 = 200.0;
pub const PRECISE_POSITION_EVERY_MS: f64 = 30.0;

/// Ein zu sendendes Paket.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub channel: Channel,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Simulator {
    scenario: Scenario,
    ip: Ipv4Addr,
    pub decks: Vec<SimDeck>,
    pub mixer: bool,
    /// Zeit seit Start des aktuellen Durchlaufs.
    pub time_ms: f64,
    next_step: usize,
    next_keep_alive: f64,
    next_status: f64,
    next_position: f64,
}

impl Simulator {
    /// `ip` wird in die Keep-Alives der virtuellen Geräte geschrieben.
    pub fn new(scenario: Scenario, ip: Ipv4Addr) -> Self {
        let mut sim = Self {
            decks: Vec::new(),
            mixer: scenario.mixer,
            scenario,
            ip,
            time_ms: 0.0,
            next_step: 0,
            next_keep_alive: 0.0,
            next_status: 0.0,
            next_position: 0.0,
        };
        sim.reset();
        sim
    }

    fn reset(&mut self) {
        self.decks = (1..=self.scenario.decks.clamp(1, 6))
            .map(|n| {
                if self.scenario.legacy_decks.contains(&n) {
                    SimDeck::legacy(n, self.ip)
                } else {
                    SimDeck::new(n, self.ip)
                }
            })
            .collect();
        self.time_ms = 0.0;
        self.next_step = 0;
    }

    pub fn deck(&self, number: u8) -> Option<&SimDeck> {
        self.decks.iter().find(|d| d.number == number)
    }

    pub fn deck_mut(&mut self, number: u8) -> Option<&mut SimDeck> {
        self.decks.iter_mut().find(|d| d.number == number)
    }

    pub fn apply(&mut self, command: &Command) {
        use Command::*;
        match command.clone() {
            Load { deck, track } => self.with(deck, |d| d.load(track)),
            Play { deck } => self.with(deck, SimDeck::play),
            Pause { deck } => self.with(deck, SimDeck::pause),
            Cue { deck } => self.with(deck, SimDeck::cue),
            Seek { deck, ms } => self.with(deck, |d| d.seek(ms)),
            Pitch { deck, percent } => {
                self.with(deck, |d| d.pitch_percent = percent.clamp(-100.0, 100.0))
            }
            Reverse { deck, on } => self.with(deck, |d| d.reverse = on),
            Loop {
                deck,
                start_ms,
                end_ms,
            } => self.with(deck, |d| d.set_loop(Some((start_ms, end_ms)))),
            ExitLoop { deck } => self.with(deck, |d| d.set_loop(None)),
            Master { deck } => {
                for d in &mut self.decks {
                    d.master = d.number == deck;
                }
            }
            OnAir { deck, on } => self.with(deck, |d| d.on_air = on),
        }
    }

    fn with(&mut self, deck: u8, f: impl FnOnce(&mut SimDeck)) {
        match self.deck_mut(deck) {
            Some(d) => f(d),
            None => tracing::warn!(deck, "Simulator: Deck gibt es nicht"),
        }
    }

    /// Lässt `dt_ms` Zeit vergehen und liefert alle Pakete, die dabei fällig werden.
    pub fn step(&mut self, dt_ms: f64) -> Vec<Outgoing> {
        let mut out = Vec::new();
        self.time_ms += dt_ms;

        if let Some(after) = self.scenario.repeat_after_ms {
            if self.time_ms >= after as f64 {
                self.reset();
            }
        }
        while let Some(step) = self.scenario.steps.get(self.next_step) {
            if step.at_ms as f64 > self.time_ms {
                break;
            }
            let command = step.command.clone();
            self.next_step += 1;
            self.apply(&command);
        }

        for deck in &mut self.decks {
            if let Some(beat) = deck.advance(dt_ms) {
                if let Some(b) = deck.beat_packet(beat) {
                    out.push(Outgoing {
                        channel: Channel::Beat,
                        bytes: encode::beat(&b),
                    });
                }
            }
        }

        self.next_keep_alive -= dt_ms;
        if self.next_keep_alive <= 0.0 {
            self.next_keep_alive += KEEP_ALIVE_EVERY_MS;
            out.extend(self.keep_alives());
        }
        self.next_status -= dt_ms;
        if self.next_status <= 0.0 {
            self.next_status += STATUS_EVERY_MS;
            out.extend(self.statuses());
        }
        self.next_position -= dt_ms;
        if self.next_position <= 0.0 {
            self.next_position += PRECISE_POSITION_EVERY_MS;
            for d in &self.decks {
                if let Some(p) = d.precise_position() {
                    out.push(Outgoing {
                        channel: Channel::Beat,
                        bytes: encode::precise_position(&p),
                    });
                }
            }
        }
        out
    }

    fn keep_alives(&self) -> Vec<Outgoing> {
        let peers = u8::try_from(self.decks.len() + usize::from(self.mixer)).unwrap_or(u8::MAX);
        let mut out: Vec<Outgoing> = self
            .decks
            .iter()
            .map(|d| Outgoing {
                channel: Channel::Announce,
                bytes: encode::keep_alive(&d.keep_alive(peers)),
            })
            .collect();
        if self.mixer {
            let k = KeepAlive {
                name: "DJM-900NXS2".into(),
                number: MIXER_DEVICE_NUMBER,
                kind: DeviceKind::Mixer,
                mac: [0x02, 0x44, 0x4c, 0x00, 0x00, MIXER_DEVICE_NUMBER],
                ip: self.ip,
                peers_seen: peers,
            };
            out.push(Outgoing {
                channel: Channel::Announce,
                bytes: encode::keep_alive(&k),
            });
        }
        out
    }

    fn statuses(&mut self) -> Vec<Outgoing> {
        let mut out: Vec<Outgoing> = self
            .decks
            .iter_mut()
            .map(|d| Outgoing {
                channel: Channel::Status,
                bytes: encode::cdj_status(&d.status()),
            })
            .collect();
        if self.mixer {
            let master = self.decks.iter().find(|d| d.master && d.track.is_some());
            let m = MixerStatus {
                name: "DJM-900NXS2".into(),
                number: MIXER_DEVICE_NUMBER,
                master: false,
                bpm: master.and_then(|d| {
                    d.track
                        .as_ref()
                        .map(|t| t.bpm * (1.0 + d.pitch_percent / 100.0))
                }),
                beat_in_bar: master
                    .and_then(|d| {
                        d.track
                            .as_ref()
                            .map(|t| deck::beat_in_bar(t.beat_at(d.position_ms)))
                    })
                    .unwrap_or(0),
            };
            out.push(Outgoing {
                channel: Channel::Status,
                bytes: encode::mixer_status(&m),
            });
            let mut channels = vec![false; 4.max(self.decks.len())];
            for d in &self.decks {
                channels[usize::from(d.number) - 1] = d.on_air;
            }
            let o = OnAirChannels {
                name: "DJM-900NXS2".into(),
                number: MIXER_DEVICE_NUMBER,
                channels,
            };
            out.push(Outgoing {
                channel: Channel::Beat,
                bytes: encode::on_air(&o),
            });
        }
        out
    }
}

/// Wohin der Simulator sendet.
#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub ip: Ipv4Addr,
    pub ports: PortConfig,
}

/// Lässt den Simulator in Echtzeit laufen, bis der Task abgebrochen wird.
pub async fn run(mut sim: Simulator, target: Target, tick: Duration) -> std::io::Result<()> {
    let socket = tokio::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    socket.set_broadcast(true)?;
    let mut interval = tokio::time::interval(tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
    let mut last = tokio::time::Instant::now();
    loop {
        interval.tick().await;
        let now = tokio::time::Instant::now();
        let dt = now.duration_since(last).as_secs_f64() * 1000.0;
        last = now;
        for p in sim.step(dt) {
            let addr = SocketAddr::from((target.ip, target.ports.port(p.channel)));
            if let Err(e) = socket.send_to(&p.bytes, addr).await {
                tracing::warn!(error = %e, %addr, "Simulator: Senden fehlgeschlagen");
            }
        }
    }
}
