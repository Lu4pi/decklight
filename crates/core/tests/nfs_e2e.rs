//! Player ohne dbserver-Antwort (etwa ein einzelner CDJ-3000): DeckLight liest
//! Titel, Grid und Wellenform per NFS direkt vom Stick im Player.

use decklight_core::{Core, CoreConfig};
use decklight_engine::EngineEvent;
use decklight_prolink::{LinkConfig, PortConfig};
use decklight_simulator::{Command, Scenario, SimTrack, Simulator, Step, Target};
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;

fn scenario() -> Scenario {
    let track = SimTrack {
        id: 1,
        title: String::new(),
        artist: String::new(),
        length_ms: 172_000,
        bpm: 128.0,
        first_beat_ms: 0.0,
        file: None,
    };
    Scenario {
        decks: 1,
        mixer: false,
        repeat_after_ms: None,
        legacy_decks: vec![],
        steps: vec![
            Step {
                at_ms: 0,
                command: Command::Load { deck: 1, track },
            },
            Step {
                at_ms: 100,
                command: Command::Play { deck: 1 },
            },
        ],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reads_the_track_from_the_players_media() {
    // Ein Port, auf dem niemand antwortet.
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/tests/fixtures/rekordbox-demo");
    let nfs = decklight_library::nfs::server::serve(vec![
        ("/B/".into(), fixture.clone()),
        ("/C/".into(), fixture),
    ])
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
        db_query_port: Some(closed),
        nfs_portmap_port: Some(nfs.portmap_port),
        ..Default::default()
    })
    .await
    .unwrap();
    let mut events = core.subscribe();
    let sim = tokio::spawn(decklight_simulator::run(
        Simulator::new(scenario(), Ipv4Addr::LOCALHOST),
        Target {
            ip: Ipv4Addr::LOCALHOST,
            ports: core.link().local_ports(),
        },
        Duration::from_millis(5),
    ));
    let title = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Ok(s) = events.recv().await {
                if let EngineEvent::TrackInfo { title, .. } = s.event {
                    return title;
                }
            }
        }
    })
    .await
    .expect("keine Metadaten vom Medium");
    assert_eq!(title, "Demo Track 1");
    core.with(|s| {
        let deck = s.engine.deck(1).unwrap();
        let data = s.track_data(&deck.track.unwrap()).unwrap();
        assert_eq!(data.metadata.artist, "Loopmasters");
        assert_eq!(data.beat_grid.len(), 368);
        assert!(deck.grid.is_some());
        assert!(data.waveform.is_some());
    })
    .unwrap();
    sim.abort();
}
