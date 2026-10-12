//! Startet und verbindet alle Teile.

use crate::timecode_out::{self, TimecodeOutConfig, TimecodeStatus, TimecodeThread};
use decklight_engine::{ClockConfig, Engine, EngineEvent, MasterMode, Stamped, TrackRef};
use decklight_prolink::{Link, LinkConfig, LinkEvent};
use decklight_show::{OffsetTable, TrackKey};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Kennung eines geladenen Tracks für Offsets und Cues.
/// Später ergänzt um Titel/Artist/Dauer, damit sie über USB-Sticks hinweg hält.
pub fn track_key(r: &TrackRef) -> TrackKey {
    let slot = serde_json::to_value(r.slot)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "?".into());
    TrackKey(format!("p{}-{}-{}", r.source_player, slot, r.rekordbox_id))
}

#[derive(Debug, Clone, Default)]
pub struct CoreConfig {
    pub link: LinkConfig,
    pub master_mode: MasterMode,
    pub clock: ClockConfig,
    /// `None`: kein Timecode-Ausgang.
    pub timecode: Option<TimecodeOutConfig>,
    pub offsets: OffsetTable,
}

/// Gemeinsamer Zustand, geschützt durch einen Mutex.
pub struct Shared {
    pub engine: Engine,
    pub offsets: OffsetTable,
    pub timecode: TimecodeStatus,
}

pub struct Core {
    epoch: Instant,
    shared: Arc<Mutex<Shared>>,
    events: broadcast::Sender<Stamped>,
    link: Link,
    tasks: Vec<JoinHandle<()>>,
    _timecode: Option<TimecodeThread>,
}

impl Core {
    pub async fn start(cfg: CoreConfig) -> io::Result<Self> {
        let epoch = Instant::now();
        let shared = Arc::new(Mutex::new(Shared {
            engine: Engine::new(cfg.clock, cfg.master_mode),
            offsets: cfg.offsets,
            timecode: TimecodeStatus::default(),
        }));
        let (events, _) = broadcast::channel(4096);
        let link = Link::start(cfg.link).await?;

        let mut tasks = Vec::new();
        let mut rx = link.subscribe();
        let (sh, tx) = (shared.clone(), events.clone());
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
                            Ok(mut s) => handle(&mut s, t, |e| e.handle_link_event(t, &ev)),
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
        let (sh, tx) = (shared.clone(), events.clone());
        tasks.push(tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(20));
            loop {
                tick.tick().await;
                let t = epoch.elapsed().as_secs_f64();
                let out = match sh.lock() {
                    Ok(mut s) => handle(&mut s, t, |e| e.tick(t)),
                    Err(_) => return,
                };
                for event in out {
                    let _ = tx.send(Stamped { t, event });
                }
            }
        }));

        let timecode = cfg
            .timecode
            .map(|tc| timecode_out::spawn(tc, epoch, shared.clone()));
        Ok(Self {
            epoch,
            shared,
            events,
            link,
            tasks,
            _timecode: timecode,
        })
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

    pub fn shared(&self) -> Arc<Mutex<Shared>> {
        self.shared.clone()
    }

    pub fn link(&self) -> &Link {
        &self.link
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

/// Engine-Aufruf plus Folgeschritte, die Show-Daten brauchen.
fn handle(
    s: &mut Shared,
    _t: f64,
    f: impl FnOnce(&mut Engine) -> Vec<EngineEvent>,
) -> Vec<EngineEvent> {
    let out = f(&mut s.engine);
    for e in &out {
        if let EngineEvent::TrackLoaded { track, .. } = e {
            s.offsets.assign(&track_key(track), "");
        }
    }
    out
}
