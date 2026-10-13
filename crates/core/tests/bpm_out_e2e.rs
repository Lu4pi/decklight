//! Tempo des spielenden Decks geht als MA3-Befehl per OSC ans Pult, auch ohne
//! Mixer und ohne Sync-Master (ein einzelner CDJ). Phasen-Sync auf jeder „1“.

use decklight_core::{ActionOutConfig, Core, CoreConfig};
use decklight_outputs::osc::{self, OscArg};
use decklight_prolink::{LinkConfig, PortConfig};
use decklight_simulator::{Command, Scenario, SimTrack, Simulator, Step, Target};
use std::net::{Ipv4Addr, UdpSocket};
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn master_bpm_reaches_the_desk() {
    let desk = UdpSocket::bind("127.0.0.1:0").unwrap();
    desk.set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let track = SimTrack {
        id: 1,
        title: "Tempo".into(),
        artist: "Sim".into(),
        length_ms: 300_000,
        bpm: 128.0,
        first_beat_ms: 0.0,
        file: None,
    };
    let scenario = Scenario {
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
    };
    let core = Core::start(CoreConfig {
        link: LinkConfig {
            ports: PortConfig {
                announce: 0,
                beat: 0,
                status: 0,
            },
            ..Default::default()
        },
        actions: ActionOutConfig {
            osc: Some(desk.local_addr().unwrap()),
            osc_prefix: "gma3".into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .await
    .unwrap();
    let sim = tokio::spawn(decklight_simulator::run(
        Simulator::new(scenario, Ipv4Addr::LOCALHOST),
        Target {
            ip: Ipv4Addr::LOCALHOST,
            ports: core.link().local_ports(),
        },
        Duration::from_millis(5),
    ));
    let got = tokio::task::spawn_blocking(move || {
        let end = Instant::now() + Duration::from_secs(5);
        let mut buf = [0u8; 256];
        let mut out = Vec::new();
        while Instant::now() < end {
            if let Ok(n) = desk.recv(&mut buf) {
                let (addr, args) = osc::decode(&buf[..n]).unwrap();
                if let (true, Some(OscArg::String(cmd))) = (addr == "/gma3/cmd", args.first()) {
                    out.push(cmd.clone());
                }
            }
        }
        out
    })
    .await
    .unwrap();
    sim.abort();
    assert_eq!(got.first().map(String::as_str), Some("Master 3.1 At 128.0"));
    // 128 BPM: eine „1“ alle 1,875 s, also in 5 s zwei bis drei Syncs, jeweils
    // gefolgt vom Tempo.
    let learns: Vec<usize> = got
        .iter()
        .enumerate()
        .filter(|(_, c)| *c == "Learn Master 3.1")
        .map(|(i, _)| i)
        .collect();
    assert!((2..=3).contains(&learns.len()), "{got:?}");
    for i in learns {
        assert_eq!(
            got.get(i + 1).map(String::as_str),
            Some("Master 3.1 At 128.0")
        );
    }
}
