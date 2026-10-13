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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionOutConfig {
    /// OSC-Ziel, z. B. die grandMA3 auf Port 8000.
    pub osc: Option<SocketAddr>,
    /// Präfix aus den OSC-Einstellungen des Pults, z. B. `gma3`.
    #[serde(default)]
    pub osc_prefix: String,
    /// MIDI-Ausgang für Note/CC (Teil des Namens).
    #[serde(default)]
    pub midi_port: Option<String>,
    /// MA3-Befehl, der bei jeder Tempoänderung des Master-Decks per OSC geht,
    /// `{bpm}` wird ersetzt (z. B. `Master 3.1 At BPM {bpm}` für Speed-Master 1).
    /// Leer = aus. Läuft auch bei Not-Aus weiter, wie der Timecode.
    #[serde(
        default = "default_bpm_command",
        deserialize_with = "bpm_command_upgrade"
    )]
    pub bpm_command: Option<String>,
    /// MA3-Befehl für den Phasen-Sync, genau auf dem Beat gesendet: auf jeder
    /// „1“ (alle 4 Beats), während einer Tempoänderung auf jedem Beat. Danach
    /// geht sofort der Tempo-Befehl hinterher. Leer = aus.
    #[serde(default = "default_phase_command")]
    pub phase_command: Option<String>,
}

/// Speed-Master 1 der grandMA3 folgt dem Tempo des Master-Decks. Ohne
/// „BPM“ setzt die MA3 den Fader-Wert statt des Tempos.
pub const DEFAULT_BPM_COMMAND: &str = "Master 3.1 At BPM {bpm}";

/// Frühere Voreinstellung ohne „BPM“; gespeicherte Einstellungen damit werden
/// beim Laden auf [`DEFAULT_BPM_COMMAND`] gehoben.
pub const OLD_BPM_COMMAND: &str = "Master 3.1 At {bpm}";

fn bpm_command_upgrade<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v = Option::<String>::deserialize(d)?;
    Ok(match v.as_deref() {
        Some(OLD_BPM_COMMAND) => default_bpm_command(),
        _ => v,
    })
}

fn default_bpm_command() -> Option<String> {
    Some(DEFAULT_BPM_COMMAND.into())
}

/// „Learn“ auf dem Beat richtet die Phase von Speed-Master 1 aus.
pub const DEFAULT_PHASE_COMMAND: &str = "Learn Master 3.1";

fn default_phase_command() -> Option<String> {
    Some(DEFAULT_PHASE_COMMAND.into())
}

/// So lange nach der letzten Tempoänderung auf jedem Beat synchronisieren.
const TEMPO_CHANGE_WINDOW: Duration = Duration::from_secs(3);

impl Default for ActionOutConfig {
    fn default() -> Self {
        Self {
            osc: None,
            osc_prefix: String::new(),
            midi_port: None,
            bpm_command: default_bpm_command(),
            phase_command: default_phase_command(),
        }
    }
}

/// Tempo höchstens so oft an das Pult schicken.
const BPM_INTERVAL: Duration = Duration::from_millis(100);

/// `{bpm}` im Befehl ersetzen, eine Nachkommastelle.
pub fn bpm_command(template: &str, bpm: f64) -> String {
    template.replace("{bpm}", &format!("{bpm:.1}"))
}

pub(crate) struct TriggerThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TriggerThread {
    /// Platzhalter ohne Thread.
    pub(crate) fn idle() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(true)),
            handle: None,
        }
    }
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
    let bpm_template = cfg.bpm_command.clone().filter(|c| !c.trim().is_empty());
    let mut bpm_sent: Option<(f64, Instant)> = None;
    let phase_template = cfg.phase_command.clone().filter(|c| !c.trim().is_empty());
    let mut tempo_changed: Option<Instant> = None;

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
            // Tempoquelle: Master-Deck, sonst (ein CDJ ohne Sync-Master) das
            // spielende Deck.
            let source = master
                .and_then(|m| s.engine.deck(m))
                .or_else(|| s.engine.decks().find(|d| d.moving));
            let source_number = source.map(|d| d.number);
            let bpm = source
                .and_then(|d| d.effective_bpm())
                .map(|b| (b * 10.0).round() / 10.0);
            // Phasen-Sync genau auf dem Beat des Quell-Decks.
            if let (Some(phase), Some(osc)) = (&phase_template, dispatcher.osc.as_ref()) {
                let changing = tempo_changed.is_some_and(|at| at.elapsed() < TEMPO_CHANGE_WINDOW);
                let on_beat = deck_events.iter().any(|e| {
                    matches!(e, EngineEvent::Beat { deck, beat_in_bar, .. }
                        if Some(*deck) == source_number && (*beat_in_bar == 1 || changing))
                });
                if on_beat {
                    if let Err(e) = osc.command(phase) {
                        tracing::debug!(error = %e, "Phasen-Sync nicht gesendet");
                    }
                    if let (Some(template), Some(b)) = (&bpm_template, bpm) {
                        let _ = osc.command(&bpm_command(template, b));
                        bpm_sent = Some((b, Instant::now()));
                    }
                }
            }
            if let Some(template) = &bpm_template {
                let due = bpm_sent.is_none_or(|(last, at)| {
                    bpm.is_some_and(|b| (b - last).abs() >= 0.05) && at.elapsed() >= BPM_INTERVAL
                });
                if let (Some(b), true, Some(osc)) = (bpm, due, dispatcher.osc.as_ref()) {
                    if let Err(e) = osc.command(&bpm_command(template, b)) {
                        tracing::debug!(error = %e, "Tempo nicht gesendet");
                    }
                    if bpm_sent.is_some() {
                        tempo_changed = Some(Instant::now());
                    }
                    bpm_sent = Some((b, Instant::now()));
                }
            }
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
            if let Some(e) = &error {
                s.monitor
                    .warn(crate::monitor::Level::Error, format!("{}: {e}", f.name()));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_bpm_default_is_upgraded() {
        let c: ActionOutConfig =
            serde_json::from_str(r#"{"osc":null,"bpm_command":"Master 3.1 At {bpm}"}"#).unwrap();
        assert_eq!(c.bpm_command.as_deref(), Some(DEFAULT_BPM_COMMAND));
        let c: ActionOutConfig = serde_json::from_str(r#"{"osc":null}"#).unwrap();
        assert_eq!(c.bpm_command.as_deref(), Some(DEFAULT_BPM_COMMAND));
        let c: ActionOutConfig =
            serde_json::from_str(r#"{"osc":null,"bpm_command":"Master 3.2 At BPM {bpm}"}"#)
                .unwrap();
        assert_eq!(c.bpm_command.as_deref(), Some("Master 3.2 At BPM {bpm}"));
        assert_eq!(
            bpm_command(DEFAULT_BPM_COMMAND, 127.96),
            "Master 3.1 At BPM 128.0"
        );
    }
}
