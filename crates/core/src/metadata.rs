//! Lädt Metadaten, Cover, Beatgrid, Wellenform und Cues für jeden neu geladenen Track.

use crate::runtime::{content_key, track_key, Shared};
use decklight_engine::{BeatGrid, EngineEvent, Stamped, TrackRef};
use decklight_library::dbserver::client::asking_player;
use decklight_library::export::Export;
use decklight_library::{load_track, TrackData, TrackLocation};
use decklight_show::{OffsetTable, TrackKey, TriggerTable};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::PathBuf;
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

/// Rekordbox-Exporte (Stick-Kopien oder eingesteckte Sticks) als Rückfall, wenn
/// ein Player keine Metadaten liefert. Geöffnete Exporte bleiben im Speicher, bis
/// sich die `export.pdb` ändert.
/// Geöffneter Export mit Änderungszeit der `export.pdb`.
type OpenExport = (Option<std::time::SystemTime>, Arc<Export>);

#[derive(Default)]
pub(crate) struct Exports {
    roots: Vec<PathBuf>,
    open: Mutex<HashMap<PathBuf, OpenExport>>,
    /// NFS-Zugriff auf die Medien in den Playern.
    media: Option<Media>,
}

impl Exports {
    pub(crate) fn new(roots: Vec<PathBuf>, media: Option<Media>) -> Self {
        Self {
            roots,
            open: Mutex::default(),
            media,
        }
    }

    fn export(&self, root: &PathBuf) -> Option<Arc<Export>> {
        let pdb = root.join("PIONEER").join("rekordbox").join("export.pdb");
        let mtime = std::fs::metadata(&pdb).and_then(|m| m.modified()).ok();
        let mut open = self.open.lock().ok()?;
        if let Some((t, ex)) = open.get(root) {
            if *t == mtime {
                return Some(ex.clone());
            }
        }
        match Export::open(root) {
            Ok(ex) => {
                let ex = Arc::new(ex);
                open.insert(root.clone(), (mtime, ex.clone()));
                Some(ex)
            }
            Err(e) => {
                tracing::debug!(root = %root.display(), error = %e, "Export nicht lesbar");
                None
            }
        }
    }

    /// Track in den Exporten suchen. Rekordbox-IDs gelten je Stick; passt die
    /// Länge nicht zur Angabe des Players, ist es ein anderer Track.
    fn find(&self, rekordbox_id: u32, length_s: Option<u32>) -> Option<(PathBuf, TrackData)> {
        self.roots.iter().find_map(|root| {
            let t = self.export(root)?.track(rekordbox_id)?;
            let fits = length_s.is_none_or(|l| l.abs_diff(t.metadata.duration_s) <= 1);
            fits.then(|| (root.clone(), t))
        })
    }
}

/// NFS-Zugriff auf die Medien in den Playern.
pub(crate) struct Media {
    pub port: u16,
    pub cache: decklight_library::MediaCache,
}

pub(crate) async fn run(
    mut rx: mpsc::UnboundedReceiver<FetchRequest>,
    shared: Arc<Mutex<Shared>>,
    events: broadcast::Sender<Stamped>,
    epoch: Instant,
    own_number: u8,
    query_port: u16,
    exports: Arc<Exports>,
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
        let exports = exports.clone();
        tokio::spawn(async move {
            let media = exports.media.as_ref();
            if !fetch(req, &shared, &events, epoch, own_number, query_port, media).await {
                from_export(req, &shared, &events, epoch, &exports).await;
            }
            if let Ok(mut busy) = in_flight.lock() {
                busy.remove(&req.track);
            }
        });
    }
}

/// Rückfall auf die Exporte, wenn der dbserver nichts geliefert hat.
async fn from_export(
    req: FetchRequest,
    shared: &Arc<Mutex<Shared>>,
    events: &broadcast::Sender<Stamped>,
    epoch: Instant,
    exports: &Arc<Exports>,
) {
    if exports.roots.is_empty() {
        return;
    }
    let length_s = shared.lock().ok().and_then(|s| {
        s.engine
            .decks()
            .find(|d| d.track == Some(req.track))
            .and_then(|d| d.track_length_ms)
            .map(|ms| (ms + 500) / 1000)
    });
    let ex = exports.clone();
    let id = req.track.rekordbox_id;
    let found = tokio::task::spawn_blocking(move || ex.find(id, length_s))
        .await
        .ok()
        .flatten();
    let Some((root, data)) = found else {
        return;
    };
    tracing::info!(id, root = %root.display(), "Metadaten aus dem Rekordbox-Export");
    if let Ok(mut s) = shared.lock() {
        s.monitor.warn(
            crate::monitor::Level::Info,
            format!(
                "„{}“ aus dem Export {} gelesen (Player lieferte nichts)",
                data.metadata.label(),
                root.display()
            ),
        );
    }
    store(req, data, shared, events, epoch);
}

/// `true`, wenn Metadaten da sind (geladen oder schon vorhanden).
async fn fetch(
    req: FetchRequest,
    shared: &Arc<Mutex<Shared>>,
    events: &broadcast::Sender<Stamped>,
    epoch: Instant,
    own_number: u8,
    query_port: u16,
    media: Option<&Media>,
) -> bool {
    let r = req.track;
    let loc = TrackLocation {
        slot: r.slot.to_byte(),
        track_type: if r.track_type == 0 { 1 } else { r.track_type },
        rekordbox_id: r.rekordbox_id,
    };
    // Status-Pakete kommen oft vor dem ersten Keep-Alive: auf die IP des Players
    // warten, ohne das als Fehlversuch zu zählen.
    let wait_for_device = Instant::now() + DEVICE_WAIT;
    let mut attempt = 0;
    while attempt < ATTEMPTS {
        let target = {
            let Ok(s) = shared.lock() else { return true };
            if s.keys.contains_key(&r) {
                return true;
            }
            let ip = s
                .engine
                .devices()
                .find(|d| d.number == r.source_player)
                .and_then(|d| d.ip.parse::<IpAddr>().ok());
            let present: Vec<u8> = s.engine.decks().map(|d| d.number).collect();
            ip.map(|ip| {
                let asking = asking_player(own_number, r.source_player, &present);
                // Ausgedachte Nummer (kein zweiter Player 1–4 da): der dbserver
                // lehnt das meist ab, also gleich vom Medium lesen.
                let invented = asking != own_number && !present.contains(&asking);
                (ip, asking, invented)
            })
        };
        let Some((ip, asking, invented)) = target else {
            if Instant::now() > wait_for_device {
                tracing::warn!(
                    player = r.source_player,
                    "Player unbekannt, keine Metadaten"
                );
                return false;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        };
        if let Some(m) = media.filter(|_| attempt == 0 && invented) {
            if from_media(req, ip, m, shared, events, epoch).await {
                return true;
            }
        }
        attempt += 1;
        match load_track(ip, query_port, asking, loc).await {
            Ok(data) => {
                store(req, data, shared, events, epoch);
                return true;
            }
            Err(e) => {
                tracing::warn!(attempt, player = r.source_player, id = r.rekordbox_id, error = %e, "Metadaten nicht geladen");
                if let Some(m) = media.filter(|_| attempt == 1 && !invented) {
                    if from_media(req, ip, m, shared, events, epoch).await {
                        return true;
                    }
                }
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
    false
}

/// Track vom Medium im Player lesen (NFS). `true` bei Erfolg.
async fn from_media(
    req: FetchRequest,
    ip: IpAddr,
    media: &Media,
    shared: &Arc<Mutex<Shared>>,
    events: &broadcast::Sender<Stamped>,
    epoch: Instant,
) -> bool {
    let r = req.track;
    match media
        .cache
        .track_via(ip, media.port, r.slot.to_byte(), r.rekordbox_id)
        .await
    {
        Ok(data) => {
            tracing::info!(
                player = r.source_player,
                id = r.rekordbox_id,
                "Metadaten per NFS vom Medium gelesen"
            );
            store(req, data, shared, events, epoch);
            true
        }
        Err(e) => {
            tracing::warn!(player = r.source_player, id = r.rekordbox_id, error = %e, "Medium per NFS nicht lesbar");
            false
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
    let s: &mut Shared = &mut s;
    adopt(
        &mut s.offsets,
        &mut s.triggers,
        &track_key(&r),
        &key,
        &label,
        data.fingerprint(),
    );
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
        apply_grid(s, deck, &r, &data);
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

/// Offsets und Cues auf die endgültige Kennung eines Tracks umhängen: von der
/// vorläufigen Kennung (vor den Metadaten) und, wenn derselbe Inhalt schon unter
/// anderem Namen bekannt ist (Tags geändert), von dieser.
pub(crate) fn adopt(
    offsets: &mut OffsetTable,
    triggers: &mut TriggerTable,
    provisional: &TrackKey,
    key: &TrackKey,
    label: &str,
    hash: Option<String>,
) {
    if let Some(known) = hash
        .as_deref()
        .and_then(|h| offsets.find_by_hash(h))
        .cloned()
    {
        if &known != key && offsets.get(key).is_none() {
            tracing::info!(from = %known, to = %key, "Track am Fingerabdruck wiedererkannt");
            offsets.rekey(&known, key, label);
            triggers.rekey(&known, key);
        }
    }
    offsets.rekey(provisional, key, label);
    offsets.set_hash(key, hash);
    triggers.rekey(provisional, key);
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

#[cfg(test)]
mod tests {
    use super::*;
    use decklight_show::Cue;

    fn key(s: &str) -> TrackKey {
        TrackKey(s.into())
    }

    #[test]
    fn renamed_track_keeps_offset_and_cues_via_fingerprint() {
        let mut offsets = OffsetTable::default();
        let mut triggers = TriggerTable::default();
        // Erster Abend: Track von Stick A, Offset von Hand gesetzt, ein Cue.
        adopt(
            &mut offsets,
            &mut triggers,
            &key("usb-a:12"),
            &key("Artist – Tittle [5:00]"),
            "Artist – Tittle",
            Some("abc".into()),
        );
        offsets.set(&key("Artist – Tittle [5:00]"), 5.0 * 3_600_000.0);
        triggers.add_cue(
            &key("Artist – Tittle [5:00]"),
            Cue::at_ms("Drop", 1.0, vec![]),
        );
        // Zweiter Abend: Tippfehler im Titel korrigiert, anderer Stick, gleicher Inhalt.
        adopt(
            &mut offsets,
            &mut triggers,
            &key("usb-b:7"),
            &key("Artist – Title [5:00]"),
            "Artist – Title",
            Some("abc".into()),
        );
        let e = offsets.get(&key("Artist – Title [5:00]")).unwrap();
        assert_eq!(e.offset_ms, 5.0 * 3_600_000.0);
        assert!(!e.auto);
        assert_eq!(offsets.entries.len(), 1);
        assert_eq!(triggers.tracks[&key("Artist – Title [5:00]")].len(), 1);

        // Anderer Inhalt bekommt einen eigenen Eintrag.
        adopt(
            &mut offsets,
            &mut triggers,
            &key("usb-b:8"),
            &key("Other [3:00]"),
            "Other",
            Some("def".into()),
        );
        assert_eq!(offsets.entries.len(), 2);
        assert_eq!(offsets.find_by_hash("def"), Some(&key("Other [3:00]")));
    }
}
