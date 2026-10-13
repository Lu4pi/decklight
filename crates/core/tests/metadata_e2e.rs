//! Simulierte Player mit dbserver → DeckLight lädt Titel, Cover, Beatgrid, Wellen, Cues.

use decklight_core::{Core, CoreConfig};
use decklight_engine::EngineEvent;
use decklight_library::dbserver::server::DbServer;
use decklight_prolink::{LinkConfig, PortConfig};
use decklight_simulator::{Scenario, SimLibrary, Simulator, Target};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn loads_metadata_for_every_loaded_track() {
    let scenario = Scenario::demo();
    let db = DbServer::start(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
        Arc::new(SimLibrary::from_scenario(&scenario)),
    )
    .await
    .unwrap();
    let core = Core::start(CoreConfig {
        link: LinkConfig {
            ports: PortConfig {
                announce: 0,
                beat: 0,
                status: 0,
            },
            ..Default::default()
        },
        db_query_port: Some(db.query_port),
        ..Default::default()
    })
    .await
    .unwrap();
    let mut events = core.subscribe();
    let target = Target {
        ip: Ipv4Addr::LOCALHOST,
        ports: core.link().local_ports(),
    };
    let sim = tokio::spawn(decklight_simulator::run(
        Simulator::new(scenario, Ipv4Addr::LOCALHOST),
        target,
        Duration::from_millis(5),
    ));

    let mut titles = Vec::new();
    // F3: Metadaten spätestens 2 s nach dem Laden des Tracks.
    let mut loaded_at = std::collections::HashMap::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while titles.len() < 2 {
        let ev = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("keine Metadaten in 5 s")
            .unwrap();
        match ev.event {
            EngineEvent::TrackLoaded { track, .. } => {
                loaded_at.entry(track).or_insert(ev.t);
            }
            EngineEvent::TrackInfo { title, track, .. } => {
                let since = ev.t - loaded_at.get(&track).copied().unwrap_or(0.0);
                assert!(since < 2.0, "{title}: {since:.2} s nach dem Laden");
                titles.push(title);
            }
            _ => {}
        }
    }
    titles.sort();
    assert_eq!(titles, ["Midnight Signal", "Second Light"]);

    core.with(|s| {
        assert_eq!(s.tracks.len(), 2);
        let deck1 = s.engine.deck(1).unwrap();
        let data = s.track_data(&deck1.track.unwrap()).unwrap();
        assert_eq!(data.metadata.artist, "Sim Artist");
        assert!(data
            .artwork
            .as_ref()
            .is_some_and(|a| a.starts_with(b"\x89PNG")));
        assert!(data.beat_grid.len() > 700);
        assert!(data
            .waveform
            .as_ref()
            .is_some_and(|w| w.columns.len() == 360 * 150));
        assert_eq!(
            data.cues
                .iter()
                .filter(|c| matches!(c.kind, decklight_library::CueKind::Hot(_)))
                .count(),
            4
        );
        assert!(deck1.grid.is_some(), "Beatgrid in die Engine übernommen");
        // Offsets laufen jetzt unter der Inhalts-Kennung mit lesbarem Namen.
        let labels: Vec<_> = s.offsets.entries.iter().map(|e| e.label.clone()).collect();
        assert!(
            labels.contains(&"Sim Artist – Midnight Signal".to_string()),
            "{labels:?}"
        );
        assert_eq!(s.offsets.entries.len(), 2);
        // Rekordbox-Cues als Vorlagen (4 Hot Cues + 1 Memory Cue je Track).
        assert_eq!(s.triggers.tracks.len(), 2);
        assert!(s.triggers.tracks.values().all(|c| c.len() == 5));
    })
    .unwrap();
    sim.abort();
}
