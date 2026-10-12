//! Lädt Metadaten, Cover, Beatgrid, Wellenform und Cues für jeden neu geladenen Track.

use crate::runtime::{content_key, track_key, Shared};
use decklight_engine::{BeatGrid, EngineEvent, Stamped, TrackRef};
use decklight_library::dbserver::client::asking_player;
use decklight_library::{load_track, TrackData, TrackLocation};
use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, Clone, Copy)]
pub(crate) struct FetchRequest {
    pub deck: u8,
    pub track: TrackRef,
}

const ATTEMPTS: u32 = 3;
/// So lange auf den Keep-Alive eines Players warten.
const DEVICE_WAIT: Duration = Duration::from_secs(10);

pub(crate) async fn run(
    mut rx: mpsc::UnboundedReceiver<FetchRequest>,
    shared: Arc<Mutex<Shared>>,
    events: broadcast::Sender<Stamped>,
    epoch: Instant,
    own_number: u8,
    query_port: u16,
) {
    let in_flight: Arc<Mutex<HashSet<TrackRef>>> = Arc::default();
    while let Some(req) = rx.recv().await {
        let Ok(mut busy) = in_flight.lock() else {
            return;
        };
        if !busy.insert(req.track) {
            continue;
        }
        drop(busy);
        let (shared, events, in_flight) = (shared.clone(), events.clone(), in_flight.clone());
        tokio::spawn(async move {
            fetch(req, &shared, &events, epoch, own_number, query_port).await;
            if let Ok(mut busy) = in_flight.lock() {
                busy.remove(&req.track);
            }
        });
    }
}

async fn fetch(
    req: FetchRequest,
    shared: &Arc<Mutex<Shared>>,
    events: &broadcast::Sender<Stamped>,
    epoch: Instant,
    own_number: u8,
    query_port: u16,
) {
    let r = req.track;
    let loc = TrackLocation {
        slot: r.slot.to_byte(),
        track_type: 1,
        rekordbox_id: r.rekordbox_id,
    };
    // Status-Pakete kommen oft vor dem ersten Keep-Alive: auf die IP des Players
    // warten, ohne das als Fehlversuch zu zählen.
    let wait_for_device = Instant::now() + DEVICE_WAIT;
    let mut attempt = 0;
    while attempt < ATTEMPTS {
        let target = {
            let Ok(s) = shared.lock() else { return };
            if s.keys.contains_key(&r) {
                return;
            }
            let ip = s
                .engine
                .devices()
                .find(|d| d.number == r.source_player)
                .and_then(|d| d.ip.parse::<IpAddr>().ok());
            let present: Vec<u8> = s.engine.decks().map(|d| d.number).collect();
            ip.map(|ip| (ip, asking_player(own_number, r.source_player, &present)))
        };
        let Some((ip, asking)) = target else {
            if Instant::now() > wait_for_device {
                tracing::warn!(
                    player = r.source_player,
                    "Player unbekannt, keine Metadaten"
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        };
        attempt += 1;
        match load_track(ip, query_port, asking, loc).await {
            Ok(data) => {
                store(req, data, shared, events, epoch);
                return;
            }
            Err(e) => {
                tracing::warn!(attempt, player = r.source_player, id = r.rekordbox_id, error = %e, "Metadaten nicht geladen");
                if attempt == ATTEMPTS {
                    if let Ok(mut s) = shared.lock() {
                        s.monitor.warn(
                            crate::monitor::Level::Warning,
                            format!(
                                "Metadaten für Track {} von Player {} nicht geladen: {e}",
                                r.rekordbox_id, r.source_player
                            ),
                        );
                    }
                }
                tokio::time::sleep(Duration::from_secs(u64::from(attempt))).await;
            }
        }
    }
}

fn store(
    req: FetchRequest,
    data: TrackData,
    shared: &Arc<Mutex<Shared>>,
    events: &broadcast::Sender<Stamped>,
    epoch: Instant,
) {
    let r = req.track;
    let Ok(mut s) = shared.lock() else { return };
    let key = content_key(&data.metadata);
    let label = data.metadata.label();
    s.offsets.rekey(&track_key(&r), &key, &label);
    s.triggers.rekey(&track_key(&r), &key);
    let imported = s
        .triggers
        .import_templates(&key, decklight_triggers::hot_cue_templates(&data.cues));
    if imported > 0 {
        tracing::info!(imported, track = %key, "Rekordbox-Cues als Vorlagen übernommen");
    }
    let data = Arc::new(data);
    s.tracks.insert(key.clone(), data.clone());
    s.keys.insert(r, key);
    let decks: Vec<u8> = s
        .engine
        .decks()
        .filter(|d| d.track == Some(r))
        .map(|d| d.number)
        .collect();
    for deck in decks {
        apply_grid(&mut s, deck, &r, &data);
    }
    tracing::info!(title = %data.metadata.title, artist = %data.metadata.artist, "Metadaten geladen");
    let _ = events.send(Stamped {
        t: epoch.elapsed().as_secs_f64(),
        event: EngineEvent::TrackInfo {
            track: r,
            title: data.metadata.title.clone(),
            artist: data.metadata.artist.clone(),
        },
    });
    let _ = req.deck;
}

/// Beatgrid und Länge in den Deck-Zustand übernehmen (genauere Position bei NXS2).
pub(crate) fn apply_grid(s: &mut Shared, deck: u8, track: &TrackRef, data: &TrackData) {
    let Some(d) = s.engine.deck_mut(deck) else {
        return;
    };
    if d.track != Some(*track) {
        return;
    }
    if !data.beat_grid.is_empty() {
        d.grid = Some(BeatGrid {
            beats: data
                .beat_grid
                .iter()
                .map(|b| decklight_engine::beatgrid::GridBeat {
                    time_ms: f64::from(b.time_ms),
                    beat_in_bar: b.beat_in_bar,
                    bpm: b.bpm,
                })
                .collect(),
        });
    }
    if d.track_length_ms.is_none() && data.metadata.duration_s > 0 {
        d.track_length_ms = Some(data.metadata.duration_s * 1000);
    }
}
