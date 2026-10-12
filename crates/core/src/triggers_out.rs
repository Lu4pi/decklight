//! Trigger-Thread: prüft alle 2 ms die Cue-Punkte, reagiert auf Deck-Ereignisse
//! und schickt die Aktionen per OSC oder MIDI raus.

use crate::runtime::Shared;
use decklight_engine::{EngineEvent, Stamped};
use decklight_outputs::midi::MidiOut;
use decklight_outputs::osc::OscSender;
use decklight_show::{Action, TrackKey};
use decklight_triggers::{DeckInput, Firing, FiringSource, LogEntry, TriggerEngine};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

/// Wohin Aktionen gehen.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionOutConfig {
    /// OSC-Ziel, z. B. die grandMA3 auf Port 8000.
    pub osc: Option<SocketAddr>,
    /// Präfix aus den OSC-Einstellungen des Pults, z. B. `gma3`.
    #[serde(default)]
    pub osc_prefix: String,
    /// MIDI-Ausgang für Note/CC (Teil des Namens).
    #[serde(default)]
    pub midi_port: Option<String>,
}

pub(crate) struct TriggerThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for TriggerThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Von Hand ausgelöste Aktionen (Test-Button, Cue aus der Oberfläche).
pub(crate) struct Manual {
    pub name: String,
    pub deck: Option<u8>,
    pub actions: Vec<Action>,
}

pub(crate) fn spawn(
    cfg: ActionOutConfig,
    epoch: Instant,
    shared: Arc<Mutex<Shared>>,
    events: broadcast::Sender<Stamped>,
    manual: mpsc::Receiver<Manual>,
) -> TriggerThread {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handle = std::thread::Builder::new()
        .name("decklight-trigger".into())
        .spawn(move || run(cfg, epoch, shared, events, manual, flag))
        .expect("Trigger-Thread konnte nicht starten");
    TriggerThread {
        stop,
        handle: Some(handle),
    }
}

struct Dispatcher {
    osc: Option<OscSender>,
    osc_error: Option<String>,
    midi: Option<MidiOut>,
    midi_error: Option<String>,
}

impl Dispatcher {
    fn new(cfg: &ActionOutConfig) -> Self {
        let (osc, osc_error) = match cfg.osc {
            None => (None, Some("kein OSC-Ziel eingestellt".to_string())),
            Some(addr) => match OscSender::new(addr, &cfg.osc_prefix) {
                Ok(s) => (Some(s), None),
                Err(e) => (None, Some(format!("OSC: {e}"))),
            },
        };
        let (midi, midi_error) = match cfg.midi_port.as_deref() {
            None => (None, Some("kein MIDI-Ausgang eingestellt".to_string())),
            Some(name) => match MidiOut::open(name) {
                Ok(m) => (Some(m), None),
                Err(e) => (None, Some(e.to_string())),
            },
        };
        for e in [&osc_error, &midi_error].into_iter().flatten() {
            tracing::info!("Aktionen: {e}");
        }
        Self {
            osc,
            osc_error,
            midi,
            midi_error,
        }
    }

    fn send(&mut self, action: &Action) -> Result<(), String> {
        match action {
            Action::Osc { command } => match &self.osc {
                Some(o) => o.command(command).map_err(|e| e.to_string()),
                None => Err(self.osc_error.clone().unwrap_or_default()),
            },
            Action::OscMessage { address, args } => match &self.osc {
                Some(o) => o.send(address, args).map_err(|e| e.to_string()),
                None => Err(self.osc_error.clone().unwrap_or_default()),
            },
            Action::Midi { message } => match &mut self.midi {
                Some(m) => m.send(&message.bytes()).map_err(|e| e.to_string()),
                None => Err(self.midi_error.clone().unwrap_or_default()),
            },
        }
    }
}

fn run(
    cfg: ActionOutConfig,
    epoch: Instant,
    shared: Arc<Mutex<Shared>>,
    events: broadcast::Sender<Stamped>,
    manual: mpsc::Receiver<Manual>,
    stop: Arc<AtomicBool>,
) {
    let mut dispatcher = Dispatcher::new(&cfg);
    let mut engine = TriggerEngine::new();
    let mut rx = events.subscribe();
    let tick = Duration::from_millis(2);
    let mut next = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        next += tick;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(50) {
            next = now;
        }
        let t = epoch.elapsed().as_secs_f64();
        let mut firings: Vec<Firing> = Vec::new();

        while let Ok(m) = manual.try_recv() {
            firings.push(Firing {
                deck: m.deck,
                source: FiringSource::Manual { name: m.name },
                actions: m.actions,
                late_ms: 0.0,
            });
        }
        let mut deck_events = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(s) => deck_events.push(s.event),
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    tracing::warn!(n, "Trigger: Ereignisse übersprungen");
                }
                Err(_) => break,
            }
        }
        {
            let Ok(s) = shared.lock() else { return };
            let master = s.engine.master();
            for ev in &deck_events {
                firings.extend(engine.on_event(ev, master, &s.triggers));
            }
            let keys: Vec<Option<TrackKey>> = s
                .engine
                .decks()
                .map(|d| d.track.map(|r| s.key_for(&r)))
                .collect();
            let inputs: Vec<DeckInput<'_>> = s
                .engine
                .decks()
                .zip(&keys)
                .map(|(d, key)| DeckInput {
                    number: d.number,
                    track: key.as_ref(),
                    position_ms: d.position_at(t),
                    moving: d.moving,
                    rate: d.rate(),
                    grid: d.grid.as_ref(),
                    precise: d.has_precise_position,
                })
                .collect();
            firings.extend(engine.update(t, &inputs, master, &s.triggers));
        }
        if firings.is_empty() {
            continue;
        }

        let mut done = Vec::with_capacity(firings.len());
        for f in firings {
            let errors: Vec<String> = f
                .actions
                .iter()
                .filter_map(|a| dispatcher.send(a).err())
                .collect();
            let error = (!errors.is_empty()).then(|| errors.join("; "));
            if let Some(e) = &error {
                tracing::warn!(name = f.name(), error = %e, "Aktion nicht gesendet");
            }
            done.push((f, error));
        }
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        let Ok(mut s) = shared.lock() else { return };
        for (f, error) in done {
            let actions: Vec<String> = f.actions.iter().map(Action::describe).collect();
            s.trigger_log.push(LogEntry {
                t,
                unix_ms,
                deck: f.deck,
                source: f.name().to_string(),
                actions: actions.clone(),
                late_ms: f.late_ms,
                error: error.clone(),
            });
            let _ = events.send(Stamped {
                t,
                event: EngineEvent::TriggerFired {
                    deck: f.deck,
                    name: f.name().to_string(),
                    actions,
                    late_ms: f.late_ms,
                    error,
                },
            });
        }
    }
}
