use decklight_prolink::packet::{self, Packet, PlayState};
use decklight_prolink::{Channel, Link, LinkConfig, LinkEvent, LinkSnapshot, PortConfig};
use decklight_simulator::{Command, Scenario, SimTrack, Simulator, Step, Target};
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

fn track(bpm: f64) -> SimTrack {
    SimTrack {
        id: 7,
        title: "T".into(),
        artist: "A".into(),
        length_ms: 300_000,
        bpm,
        first_beat_ms: 0.0,
    }
}

fn one_deck(steps: Vec<(u64, Command)>) -> Simulator {
    let scenario = Scenario {
        decks: 1,
        mixer: false,
        repeat_after_ms: None,
        legacy_decks: Vec::new(),
        steps: steps
            .into_iter()
            .map(|(at_ms, command)| Step { at_ms, command })
            .collect(),
    };
    Simulator::new(scenario, Ipv4Addr::LOCALHOST)
}

fn beats(out: &[decklight_simulator::Outgoing]) -> Vec<packet::Beat> {
    out.iter()
        .filter_map(|o| match packet::parse(o.channel, &o.bytes) {
            Some(Packet::Beat(b)) => Some(b),
            _ => None,
        })
        .collect()
}

#[test]
fn beats_follow_tempo_and_pitch() {
    // 120 BPM, +10 % Pitch → 132 BPM → 60 s ergeben 132 Beats.
    let mut sim = one_deck(vec![
        (
            0,
            Command::Load {
                deck: 1,
                track: track(120.0),
            },
        ),
        (
            0,
            Command::Pitch {
                deck: 1,
                percent: 10.0,
            },
        ),
        (0, Command::Play { deck: 1 }),
    ]);
    let mut all = Vec::new();
    for _ in 0..6000 {
        all.extend(beats(&sim.step(10.0)));
    }
    assert!((131..=132).contains(&all.len()), "{} Beats", all.len());
    let b = &all[10];
    assert!((b.effective_bpm().unwrap() - 132.0).abs() < 0.01);
    assert!(b.next_beat_ms <= 455, "next_beat {}", b.next_beat_ms);
    assert_eq!(all[0].beat_in_bar, 2, "erster gemeldeter Beat ist Beat 2");
    let pos = sim.deck(1).unwrap().position_ms;
    assert!((pos - 66_000.0).abs() < 1.0, "Position {pos}");
}

#[test]
fn pause_cue_seek_and_loop() {
    let mut sim = one_deck(vec![
        (
            0,
            Command::Load {
                deck: 1,
                track: track(120.0),
            },
        ),
        (0, Command::Play { deck: 1 }),
        (1000, Command::Pause { deck: 1 }),
        (
            2000,
            Command::Seek {
                deck: 1,
                ms: 10_000.0,
            },
        ),
        (
            2000,
            Command::Loop {
                deck: 1,
                start_ms: 10_000.0,
                end_ms: 12_000.0,
            },
        ),
        (2000, Command::Play { deck: 1 }),
    ]);
    for _ in 0..150 {
        sim.step(10.0);
    }
    let d = sim.deck(1).unwrap();
    assert_eq!(d.play_state, PlayState::Paused);
    assert!((d.position_ms - 1000.0).abs() < 10.1);

    for _ in 0..500 {
        sim.step(10.0);
    }
    // 2 s Loop, 5 s gespielt (ab t=2 s bis t=6,5 s = 4,5 s) → 10 s + 0,5 s.
    let d = sim.deck(1).unwrap();
    assert_eq!(d.play_state, PlayState::Looping);
    assert!(
        (d.position_ms - 10_500.0).abs() < 20.0,
        "Position {}",
        d.position_ms
    );

    sim.apply(&Command::Cue { deck: 1 });
    assert_eq!(sim.deck(1).unwrap().position_ms, 0.0);
    assert_eq!(sim.deck(1).unwrap().play_state, PlayState::Cued);
}

#[test]
fn emits_keep_alive_status_and_precise_position() {
    let mut sim = Simulator::new(Scenario::demo(), Ipv4Addr::LOCALHOST);
    let mut kinds = std::collections::HashMap::<(Channel, u8), usize>::new();
    for _ in 0..290 {
        for o in sim.step(10.0) {
            *kinds.entry((o.channel, o.bytes[0x0a])).or_default() += 1;
        }
    }
    // 2,9 s: Keep-Alive bei 0 und 1,5 s für 2 Decks + Mixer.
    assert_eq!(kinds[&(Channel::Announce, packet::kind::KEEP_ALIVE)], 6);
    assert_eq!(kinds[&(Channel::Status, packet::kind::CDJ_STATUS)], 2 * 15);
    assert_eq!(kinds[&(Channel::Status, packet::kind::MIXER_STATUS)], 15);
    assert_eq!(kinds[&(Channel::Beat, packet::kind::ON_AIR)], 15);
    assert!(kinds[&(Channel::Beat, packet::kind::PRECISE_POSITION)] >= 2 * 95);
    assert!(kinds[&(Channel::Beat, packet::kind::BEAT)] >= 4);
}

#[test]
fn scenario_json_roundtrip() {
    let demo = Scenario::demo();
    let json = serde_json::to_string_pretty(&demo).unwrap();
    assert_eq!(Scenario::from_json(&json).unwrap(), demo);
}

/// F1: Simulierte Geräte erscheinen über echtes UDP innerhalb von 3 s.
#[tokio::test(flavor = "multi_thread")]
async fn devices_are_discovered_over_udp_within_three_seconds() {
    let link = Link::start(LinkConfig {
        ports: PortConfig {
            announce: 0,
            beat: 0,
            status: 0,
        },
        ..Default::default()
    })
    .await
    .unwrap();
    let mut events = link.subscribe();
    let ports = link.local_ports();
    let sim = Simulator::new(Scenario::demo(), Ipv4Addr::LOCALHOST);
    let target = Target {
        ip: Ipv4Addr::LOCALHOST,
        ports,
    };
    let sim_task = tokio::spawn(decklight_simulator::run(
        sim,
        target,
        Duration::from_millis(5),
    ));

    let started = Instant::now();
    let mut snap = LinkSnapshot::default();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while snap.devices.len() < 3
        || snap.players.values().filter(|p| p.status.is_some()).count() < 2
        || snap.on_air.is_none()
    {
        let ev = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("Zeitlimit 3 s")
            .unwrap();
        snap.apply(&ev);
        if let LinkEvent::NumberConflict { .. } = ev {
            panic!("unerwarteter Nummernkonflikt");
        }
    }
    assert!(started.elapsed() < Duration::from_secs(3));
    let names: Vec<_> = snap.devices.values().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["CDJ-3000", "CDJ-3000", "DJM-900NXS2"]);
    assert_eq!(link.devices().len(), 3);
    let deck1 = &snap.players[&1];
    assert_eq!(deck1.status.as_ref().unwrap().rekordbox_id, 1001);
    assert!(deck1.on_air(snap.on_air.as_ref()));
    assert!(!snap.players[&2].on_air(snap.on_air.as_ref()));
    sim_task.abort();
}
