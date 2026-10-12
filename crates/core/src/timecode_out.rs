//! Timecode-Ausgang in einem eigenen Thread mit Millisekunden-Takt.

use crate::runtime::{track_key, Shared};
use decklight_outputs::artnet::{ArtNetSender, ARTNET_PORT};
use decklight_outputs::{GeneratorConfig, TcInput, TcMessage, Timecode, TimecodeGenerator};
use decklight_show::TrackKey;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimecodeOutConfig {
    pub generator: GeneratorConfig,
    /// Ziel für Art-Net-Timecode (Broadcast-Adresse oder Pult-IP).
    pub artnet: Option<Ipv4Addr>,
    #[serde(default = "artnet_port")]
    pub artnet_port: u16,
    /// MIDI-Ausgang für MTC (Teil des Namens).
    pub mtc_port: Option<String>,
}

fn artnet_port() -> u16 {
    ARTNET_PORT
}

impl Default for TimecodeOutConfig {
    fn default() -> Self {
        Self {
            generator: GeneratorConfig::default(),
            artnet: None,
            artnet_port: ARTNET_PORT,
            mtc_port: None,
        }
    }
}

/// Was der Timecode-Ausgang gerade tut (für UI und Monitoring).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TimecodeStatus {
    pub timecode: Option<Timecode>,
    pub deck: Option<u8>,
    pub track: Option<TrackKey>,
    pub offset_ms: Option<f64>,
    pub running: bool,
    pub errors: u64,
}

pub(crate) struct TimecodeThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for TimecodeThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

pub(crate) fn spawn(
    cfg: TimecodeOutConfig,
    epoch: Instant,
    shared: Arc<Mutex<Shared>>,
) -> TimecodeThread {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handle = std::thread::Builder::new()
        .name("decklight-timecode".into())
        .spawn(move || run(cfg, epoch, shared, flag))
        .expect("Timecode-Thread konnte nicht starten");
    TimecodeThread {
        stop,
        handle: Some(handle),
    }
}

fn run(cfg: TimecodeOutConfig, epoch: Instant, shared: Arc<Mutex<Shared>>, stop: Arc<AtomicBool>) {
    let artnet = cfg.artnet.and_then(|ip| {
        ArtNetSender::new(ip, cfg.artnet_port)
            .map_err(|e| tracing::error!(error = %e, "Art-Net-Ausgang nicht verfügbar"))
            .ok()
    });
    let mut midi = cfg.mtc_port.as_deref().and_then(|name| {
        decklight_outputs::midi::MidiOut::open(name)
            .map_err(|e| tracing::error!(error = %e, "MTC-Ausgang nicht verfügbar"))
            .ok()
    });

    let mut generator = TimecodeGenerator::new(cfg.generator);
    let tick = Duration::from_millis(1);
    let mut next = Instant::now();
    let mut errors = 0u64;
    let mut last_status = Instant::now() - Duration::from_secs(1);

    while !stop.load(Ordering::Relaxed) {
        next += tick;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(50) {
            next = now; // nach einem Hänger nicht aufholen
        }
        let t = epoch.elapsed().as_secs_f64();

        let (input, deck, track, offset) = {
            let Ok(mut s) = shared.lock() else { return };
            master_input(&mut s, t)
        };
        for msg in generator.update(t, input) {
            let result = match msg {
                TcMessage::ArtNet(tc) => artnet.as_ref().map(|a| a.send(&tc)).unwrap_or(Ok(())),
                TcMessage::MtcQuarter(q) => midi_send(&mut midi, &q),
                TcMessage::MtcFull(tc) => {
                    midi_send(&mut midi, &decklight_outputs::mtc::full_frame(&tc))
                }
            };
            if let Err(e) = result {
                errors += 1;
                if errors % 1000 == 1 {
                    tracing::warn!(error = %e, "Timecode konnte nicht gesendet werden");
                }
            }
        }
        if last_status.elapsed() >= Duration::from_millis(20) {
            last_status = Instant::now();
            if let Ok(mut s) = shared.lock() {
                s.timecode = TimecodeStatus {
                    timecode: generator.current().filter(|_| input.position_ms.is_some()),
                    deck,
                    track,
                    offset_ms: offset,
                    running: input.moving,
                    errors,
                };
            }
        }
    }
}

fn midi_send(
    midi: &mut Option<decklight_outputs::midi::MidiOut>,
    bytes: &[u8],
) -> std::io::Result<()> {
    match midi {
        Some(m) => m
            .send(bytes)
            .map_err(|e| std::io::Error::other(e.to_string())),
        None => Ok(()),
    }
}

/// Position des Master-Decks plus Offset seines Tracks.
pub(crate) fn master_input(
    s: &mut Shared,
    t: f64,
) -> (TcInput, Option<u8>, Option<TrackKey>, Option<f64>) {
    let none = TcInput {
        position_ms: None,
        moving: false,
        reverse: false,
    };
    let Some(n) = s.engine.master() else {
        return (none, None, None, None);
    };
    let Some(deck) = s.engine.deck(n) else {
        return (none, Some(n), None, None);
    };
    let (Some(track), Some(pos)) = (deck.track, deck.position_at(t)) else {
        return (none, Some(n), None, None);
    };
    let moving = deck.moving;
    let key = track_key(&track);
    let offset = s.offsets.assign(&key, "");
    (
        TcInput {
            position_ms: Some(offset + pos),
            moving,
            reverse: false,
        },
        Some(n),
        Some(key),
        Some(offset),
    )
}
