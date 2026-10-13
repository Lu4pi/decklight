//! Tempo des spielenden Decks geht als MA3-Befehl per OSC ans Pult, auch ohne
//! Mixer und ohne Sync-Master (ein einzelner CDJ).

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
        while Instant::now() < end {
            if let Ok(n) = desk.recv(&mut buf) {
                let (addr, args) = osc::decode(&buf[..n]).unwrap();
                if let (true, Some(OscArg::String(cmd))) = (addr == "/gma3/cmd", args.first()) {
                    if cmd.starts_with("Master 3.1 At") {
                        return Some(cmd.clone());
                    }
                }
            }
        }
        None
    })
    .await
    .unwrap();
    sim.abort();
    assert_eq!(got.as_deref(), Some("Master 3.1 At 128.0"));
}
