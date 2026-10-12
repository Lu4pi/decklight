use decklight_library::dbserver::client::DbClient;
use decklight_library::dbserver::server::{DbServer, TrackSource};
use decklight_library::*;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

struct One(TrackData);
impl TrackSource for One {
    fn track(&self, slot: u8, id: u32) -> Option<TrackData> {
        (slot == 3 && id == self.0.metadata.rekordbox_id).then(|| self.0.clone())
    }
}

fn sample() -> TrackData {
    TrackData {
        metadata: TrackMetadata {
            rekordbox_id: 4711,
            title: "Nachtlicht".into(),
            artist: "Größenwahn & Co".into(),
            album: "Live".into(),
            genre: "Techno".into(),
            key: "8A".into(),
            comment: "Drop bei 1:04".into(),
            duration_s: 372,
            bpm: Some(126.5),
            rating: 4,
            artwork_id: 4711,
        },
        artwork: Some(vec![0xff, 0xd8, 0xff, 0xe0, 1, 2, 3]),
        beat_grid: (0..100)
            .map(|i| GridBeat {
                time_ms: 100 + i * 474,
                beat_in_bar: (i % 4) as u8 + 1,
                bpm: 126.5,
            })
            .collect(),
        cues: vec![
            CuePoint {
                kind: CueKind::Memory,
                position_ms: 100,
                loop_end_ms: None,
                color: None,
                comment: String::new(),
            },
            CuePoint {
                kind: CueKind::Hot(1),
                position_ms: 64_000,
                loop_end_ms: None,
                color: Some([255, 0, 0]),
                comment: "Drop".into(),
            },
            CuePoint {
                kind: CueKind::Hot(3),
                position_ms: 90_000,
                loop_end_ms: Some(97_584),
                color: None,
                comment: String::new(),
            },
        ],
        waveform: Some(Waveform {
            per_second: 150.0,
            columns: (0..1500)
                .map(|i| WaveColumn {
                    height: (i % 32) as u8,
                    rgb: [0, 109, 255],
                })
                .collect(),
        }),
    }
}

#[tokio::test]
async fn loads_everything_from_a_dbserver() {
    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let server = DbServer::start(local, 0, Arc::new(One(sample())))
        .await
        .unwrap();
    let loc = TrackLocation {
        slot: 3,
        track_type: 1,
        rekordbox_id: 4711,
    };
    let data = load_track(local, server.query_port, 1, loc).await.unwrap();
    let want = sample();
    assert_eq!(data.metadata, want.metadata);
    assert_eq!(data.artwork, want.artwork);
    assert_eq!(data.beat_grid, want.beat_grid);
    assert_eq!(data.cues, want.cues);
    // Farben werden auf 3 Bit je Kanal quantisiert.
    let w = data.waveform.unwrap();
    assert_eq!(w.columns.len(), 1500);
    assert_eq!(w.columns[31].height, 31);
    assert_eq!(w.columns[0].rgb, [0, 109, 255]);
}

#[tokio::test]
async fn unknown_track_is_not_found() {
    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let server = DbServer::start(local, 0, Arc::new(One(sample())))
        .await
        .unwrap();
    let mut c = DbClient::connect_via(local, server.query_port, 2)
        .await
        .unwrap();
    let loc = TrackLocation {
        slot: 3,
        track_type: 1,
        rekordbox_id: 1,
    };
    assert!(matches!(
        c.metadata(loc).await,
        Err(decklight_library::dbserver::DbError::NotFound)
    ));
    assert!(c.beat_grid(loc).await.is_err());
}

#[test]
fn asking_player_never_uses_target_or_high_numbers() {
    use decklight_library::dbserver::client::asking_player;
    assert_eq!(asking_player(2, 1, &[1, 2]), 2);
    assert_eq!(asking_player(7, 1, &[1, 2, 3]), 2);
    assert_eq!(asking_player(7, 2, &[2]), 4);
    assert_eq!(asking_player(7, 4, &[4]), 3);
    assert_eq!(asking_player(5, 1, &[1, 5, 6]), 4);
}
