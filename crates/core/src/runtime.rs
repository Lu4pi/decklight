//! Startet und verbindet alle Teile.

use crate::metadata::{self, FetchRequest};
use crate::monitor::{Level, Monitor};
use crate::timecode_out::{self, TimecodeOutConfig, TimecodeStatus, TimecodeThread};
use crate::triggers_out::{self, ActionOutConfig, Manual, TriggerThread};
use decklight_engine::{ClockConfig, Engine, EngineEvent, MasterMode, Stamped, TrackRef};
use decklight_library::{TrackData, TrackMetadata};
use decklight_prolink::{Link, LinkConfig, LinkEvent};
use decklight_show::{Action, OffsetTable, TrackKey, TriggerTable};
use decklight_triggers::ActionLog;
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

/// Vorläufige Kennung eines geladenen Tracks, solange keine Metadaten da sind.
pub fn track_key(r: &TrackRef) -> TrackKey {
    let slot = serde_json::to_value(r.slot)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "?".into());
    TrackKey(format!("p{}-{}-{}", r.source_player, slot, r.rekordbox_id))
}

/// Kennung aus den Metadaten: gleich für denselben Song auf jedem USB-Stick.
pub fn content_key(md: &TrackMetadata) -> TrackKey {
    TrackKey(format!(
        "{} [{}:{:02}]",
        md.label(),
        md.duration_s / 60,
        md.duration_s % 60
    ))
}

#[derive(Debug, Clone, Default)]
pub struct CoreConfig {
    pub link: LinkConfig,
    pub master_mode: MasterMode,
    pub clock: ClockConfig,
    /// `None`: kein Timecode-Ausgang.
    pub timecode: Option<TimecodeOutConfig>,
    pub offsets: OffsetTable,
    /// Abfrage-Port des dbservers (`None` = 12523 wie bei echten Playern).
    pub db_query_port: Option<u16>,
    pub triggers: TriggerTable,
    /// Ziele für OSC- und MIDI-Aktionen.
    pub actions: ActionOutConfig,
    /// Rekordbox-Exporte (Wurzel eines Sticks oder einer Kopie) als Rückfall,
    /// wenn ein Player keine Metadaten liefert.
    pub usb_exports: Vec<std::path::PathBuf>,
    /// Portmapper der Player für den NFS-Zugriff auf ihre Medien (`None` = aus;
    /// echte Player: 111). Wird genutzt, wenn der dbserver nichts liefert.
    pub nfs_portmap_port: Option<u16>,
}

/// NFS nur gegen echte Player (Standard-dbserver-Port), nicht im Simulator.
pub fn nfs_port_for(db_port: u16) -> Option<u16> {
    (db_port == decklight_library::dbserver::QUERY_PORT)
        .then_some(decklight_library::nfs::PORTMAP_PORT)
}

/// Gemeinsamer Zustand, geschützt durch einen Mutex.
pub struct Shared {
    pub engine: Engine,
    pub offsets: OffsetTable,
    pub timecode: TimecodeStatus,
    /// Geladene Track-Daten nach Kennung.
    pub tracks: HashMap<TrackKey, Arc<TrackData>>,
    /// Welche Kennung ein Track auf einem Medium hat.
    pub keys: HashMap<TrackRef, TrackKey>,
    pub triggers: TriggerTable,
    pub trigger_log: ActionLog,
    pub monitor: Monitor,
    /// Seit wann das Master-Deck ohne Metadaten läuft (für die History).
    unnamed_since: Option<(TrackRef, f64)>,
}

impl Shared {
    pub fn key_for(&self, r: &TrackRef) -> TrackKey {
        self.keys.get(r).cloned().unwrap_or_else(|| track_key(r))
    }

    pub fn track_data(&self, r: &TrackRef) -> Option<Arc<TrackData>> {
        self.tracks.get(self.keys.get(r)?).cloned()
    }
}

pub struct Core {
    epoch: Instant,
    shared: Arc<Mutex<Shared>>,
    events: broadcast::Sender<Stamped>,
    link: Link,
    tasks: Vec<JoinHandle<()>>,
    outputs: Mutex<Outputs>,
}

/// Ausgangs-Threads; werden bei geänderten Einstellungen neu gestartet.
struct Outputs {
    manual: std::sync::mpsc::Sender<Manual>,
    _timecode: Option<TimecodeThread>,
    _triggers: TriggerThread,
}

impl Core {
    pub async fn start(cfg: CoreConfig) -> io::Result<Self> {
        let epoch = Instant::now();
        let shared = Arc::new(Mutex::new(Shared {
            engine: Engine::new(cfg.clock, cfg.master_mode),
            offsets: cfg.offsets,
            timecode: TimecodeStatus::default(),
            tracks: HashMap::new(),
            keys: HashMap::new(),
            triggers: cfg.triggers,
            trigger_log: ActionLog::default(),
            monitor: Monitor::default(),
            unnamed_since: None,
        }));
        let (events, _) = broadcast::channel(4096);
        let own_number = cfg.link.virtual_player.as_ref().map_or(0, |v| v.number);
        let link = Link::start(cfg.link).await?;
        let (fetch_tx, fetch_rx) = mpsc::unbounded_channel::<FetchRequest>();

        let mut tasks = Vec::new();
        let mut rx = link.subscribe();
        let (sh, tx, ftx) = (shared.clone(), events.clone(), fetch_tx.clone());
        tasks.push(tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        let at = match &ev {
                            LinkEvent::Packet { at, .. } => *at,
                            _ => Instant::now(),
                        };
                        let t = at.saturating_duration_since(epoch).as_secs_f64();
                        let out = match sh.lock() {
                            Ok(mut s) => handle(&mut s, &ftx, |e| e.handle_link_event(t, &ev)),
                            Err(_) => return,
                        };
                        for event in out {
                            let _ = tx.send(Stamped { t, event });
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(n, "Engine kommt nicht hinterher");
                    }
                    Err(_) => return,
                }
            }
        }));
        let (sh, tx, ftx) = (shared.clone(), events.clone(), fetch_tx.clone());
        tasks.push(tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(20));
            loop {
                tick.tick().await;
                let t = epoch.elapsed().as_secs_f64();
                let out = match sh.lock() {
                    Ok(mut s) => {
                        let out = handle(&mut s, &ftx, |e| e.tick(t));
                        record_history(&mut s, t);
                        out
                    }
                    Err(_) => return,
                };
                for event in out {
                    let _ = tx.send(Stamped { t, event });
                }
            }
        }));
        tasks.push(tokio::spawn(metadata::run(
            fetch_rx,
            shared.clone(),
            events.clone(),
            epoch,
            own_number,
            cfg.db_query_port
                .unwrap_or(decklight_library::dbserver::QUERY_PORT),
            Arc::new(metadata::Exports::new(
                cfg.usb_exports.clone(),
                cfg.nfs_portmap_port.map(|port| metadata::Media {
                    port,
                    cache: decklight_library::MediaCache::new(
                        std::env::temp_dir().join("decklight-media"),
                    ),
                }),
            )),
        )));

        let outputs = Outputs::spawn(cfg.timecode, cfg.actions, epoch, &shared, &events);
        Ok(Self {
            epoch,
            shared,
            events,
            link,
            tasks,
            outputs: Mutex::new(outputs),
        })
    }

    /// Timecode- und Aktionsausgänge mit neuen Einstellungen neu starten.
    pub fn set_outputs(&self, timecode: Option<TimecodeOutConfig>, actions: ActionOutConfig) {
        if let Ok(mut o) = self.outputs.lock() {
            // Erst die alten Threads beenden, damit MIDI-Ports frei werden.
            *o = Outputs {
                manual: std::sync::mpsc::channel().0,
                _timecode: None,
                _triggers: TriggerThread::idle(),
            };
            *o = Outputs::spawn(timecode, actions, self.epoch, &self.shared, &self.events);
        }
    }

    /// Sekunden seit Start (Zeitbasis aller Ereignisse).
    pub fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    pub fn epoch(&self) -> Instant {
        self.epoch
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Stamped> {
        self.events.subscribe()
    }

    /// Eigene Ereignisse einspeisen (z. B. aus Triggern oder der Oberfläche).
    pub fn publish(&self, event: EngineEvent) {
        let _ = self.events.send(Stamped {
            t: self.now(),
            event,
        });
    }

    pub fn shared(&self) -> Arc<Mutex<Shared>> {
        self.shared.clone()
    }

    pub fn link(&self) -> &Link {
        &self.link
    }

    /// Not-Aus: ab sofort löst kein Cue und kein Trigger mehr aus.
    pub fn panic(&self) {
        self.set_armed(false);
    }

    /// Trigger global scharf schalten oder sperren.
    pub fn set_armed(&self, armed: bool) {
        let changed = self
            .with(|s| std::mem::replace(&mut s.triggers.armed, armed) != armed)
            .unwrap_or(false);
        if changed {
            tracing::info!(armed, "Trigger");
            self.with(|s| {
                if armed {
                    s.monitor.warn(Level::Info, "Trigger wieder scharf");
                } else {
                    s.monitor.warn(Level::Warning, "Not-Aus: Trigger gesperrt");
                }
            });
            self.publish(EngineEvent::ArmedChanged { armed });
        }
    }

    /// Aktionen sofort senden (Test-Button, Cue von Hand). Läuft auch bei Not-Aus.
    pub fn fire(&self, name: &str, deck: Option<u8>, actions: Vec<Action>) {
        if let Ok(o) = self.outputs.lock() {
            let _ = o.manual.send(Manual {
                name: name.to_string(),
                deck,
                actions,
            });
        }
    }

    /// Kurzer Zugriff auf den gemeinsamen Zustand.
    pub fn with<R>(&self, f: impl FnOnce(&mut Shared) -> R) -> Option<R> {
        self.shared.lock().ok().map(|mut s| f(&mut s))
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

impl Outputs {
    fn spawn(
        timecode: Option<TimecodeOutConfig>,
        actions: ActionOutConfig,
        epoch: Instant,
        shared: &Arc<Mutex<Shared>>,
        events: &broadcast::Sender<Stamped>,
    ) -> Self {
        let timecode = timecode.map(|tc| timecode_out::spawn(tc, epoch, shared.clone()));
        let (manual, manual_rx) = std::sync::mpsc::channel();
        let triggers =
            triggers_out::spawn(actions, epoch, shared.clone(), events.clone(), manual_rx);
        Self {
            manual,
            _timecode: timecode,
            _triggers: triggers,
        }
    }
}

/// So lange spielt ein Track ohne Metadaten, bevor er trotzdem in die History kommt.
const UNNAMED_AFTER_S: f64 = 5.0;

/// Läuft das Master-Deck, kommt sein Track in die History.
fn record_history(s: &mut Shared, t: f64) {
    let Some(n) = s.engine.master() else { return };
    let Some(deck) = s.engine.deck(n) else { return };
    let (Some(track), true) = (deck.track, deck.moving) else {
        return;
    };
    let key = s.key_for(&track);
    if let Some(data) = s.track_data(&track) {
        s.unnamed_since = None;
        let (title, artist) = (data.metadata.title.clone(), data.metadata.artist.clone());
        s.monitor.played(n, key, &title, &artist);
        return;
    }
    match s.unnamed_since {
        Some((r, since)) if r == track => {
            if t - since >= UNNAMED_AFTER_S {
                let title = format!("Track {}", track.rekordbox_id);
                s.monitor.played(n, key, &title, "");
            }
        }
        _ => s.unnamed_since = Some((track, t)),
    }
}

/// Engine-Aufruf plus Folgeschritte, die Show-Daten brauchen.
fn handle(
    s: &mut Shared,
    fetch: &mpsc::UnboundedSender<FetchRequest>,
    f: impl FnOnce(&mut Engine) -> Vec<EngineEvent>,
) -> Vec<EngineEvent> {
    let out = f(&mut s.engine);
    for e in &out {
        s.monitor.observe(e);
        if let EngineEvent::TrackLoaded { deck, track } = e {
            let key = s.key_for(track);
            let label = s
                .tracks
                .get(&key)
                .map(|t| t.metadata.label())
                .unwrap_or_default();
            s.offsets.assign(&key, &label);
            match s.track_data(track) {
                Some(data) => metadata::apply_grid(s, *deck, track, &data),
                None => {
                    let _ = fetch.send(FetchRequest {
                        deck: *deck,
                        track: *track,
                    });
                }
            }
        }
    }
    out
}
