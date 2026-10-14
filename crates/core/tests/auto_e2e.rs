//! Ende-zu-Ende Auto-Pilot: simulierter CDJ spielt einen Track mit Breakdown,
//! Build-up, Pause und Drop. Eine „MA3“ schaltet Auto per OSC ein, DeckLight
//! schickt Build, Blackout und Drop samt Hit genau auf die Eins.

use decklight_auto::AutoConfig;
use decklight_core::{ActionOutConfig, Core, CoreConfig};
use decklight_engine::EngineEvent;
use decklight_library::dbserver::server::DbServer;
use decklight_outputs::osc::{self, OscArg};
use decklight_prolink::{LinkConfig, PortConfig};
use decklight_simulator::{Command, Scenario, SimLibrary, SimTrack, Simulator, Step};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 128 BPM: Takt 1875 ms. Breakdown ab Takt 32, Build-up ab 56, Drop auf Takt 64.
const BAR_MS: f64 = 1875.0;
const DROP_MS: f64 = 64.0 * BAR_MS;

fn scenario() -> Scenario {
    let track = SimTrack {
        id: 5,
        title: "Auto".into(),
        artist: "Sim".into(),
        length_ms: 360_000,
        bpm: 128.0,
        first_beat_ms: 0.0,
        file: None,
    };
    use Command::*;
    Scenario {
        decks: 1,
        mixer: true,
        repeat_after_ms: None,
        legacy_decks: vec![],
        steps: [
            (0, Load { deck: 1, track }),
            (0, OnAir { deck: 1, on: true }),
            (
                100,
                Seek {
                    deck: 1,
                    ms: 61.0 * BAR_MS,
                },
            ),
            (2_000, Play { deck: 1 }),
        ]
        .into_iter()
        .map(|(at_ms, command)| Step { at_ms, command })
        .collect(),
    }
}

fn free_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auto_pilot_fires_build_pause_and_drop_on_the_one() {
    let desk = UdpSocket::bind("127.0.0.1:0").unwrap();
    desk.set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let scenario = scenario();
    let db = DbServer::start(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
        Arc::new(SimLibrary::from_scenario(&scenario)),
    )
    .await
    .unwrap();
    let osc_in = free_port();
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
        actions: ActionOutConfig {
            osc: Some(desk.local_addr().unwrap()),
            osc_prefix: "gma3".into(),
            midi_port: None,
            bpm_command: None,
            phase_command: None,
            auto: AutoConfig {
                osc_in_port: Some(osc_in),
                ..Default::default()
            },
        },
        ..Default::default()
    })
    .await
    .unwrap();
    let mut events = core.subscribe();
    let ports = core.link().local_ports();

    let sim = Arc::new(Mutex::new(Simulator::new(scenario, Ipv4Addr::LOCALHOST)));
    let truth: Arc<Mutex<Vec<(Instant, f64)>>> = Arc::default();
    let (sim2, truth2) = (sim.clone(), truth.clone());
    let sender = tokio::spawn(async move {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut last = Instant::now();
        let mut tick = tokio::time::interval(Duration::from_millis(2));
        loop {
            tick.tick().await;
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f64() * 1000.0;
            last = now;
            let (packets, pos) = {
                let mut s = sim2.lock().unwrap();
                let p = s.step(dt);
                (p, s.deck(1).unwrap().position_ms)
            };
            truth2.lock().unwrap().push((now, pos));
            for p in packets {
                let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, ports.port(p.channel)));
                let _ = sock.send_to(&p.bytes, addr).await;
            }
        }
    });

    // Warten, bis die Track-Daten da sind, dann „MA3-Taste“: Auto an.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(ev) = events.recv().await {
                if matches!(ev.event, EngineEvent::TrackInfo { .. }) {
                    return;
                }
            }
        }
    })
    .await
    .expect("keine Track-Daten");
    let ma3 = UdpSocket::bind("127.0.0.1:0").unwrap();
    let on = osc::encode("/decklight/auto", &[OscArg::Int(1)]).unwrap();
    ma3.send_to(&on, ("127.0.0.1", osc_in)).unwrap();

    let core = Arc::new(core);
    let core2 = core.clone();
    let truth3 = truth.clone();
    let received = tokio::task::spawn_blocking(move || {
        let mut out: Vec<(Instant, String)> = Vec::new();
        let end = Instant::now() + Duration::from_secs(12);
        let mut buf = [0u8; 512];
        let mut off_sent = false;
        while Instant::now() < end {
            if let Ok(n) = desk.recv(&mut buf) {
                let (_, args) = osc::decode(&buf[..n]).unwrap();
                if let Some(OscArg::String(cmd)) = args.first().cloned() {
                    out.push((Instant::now(), cmd));
                }
            }
            let pos = truth3.lock().unwrap().last().map_or(0.0, |(_, p)| *p);
            if !off_sent && pos >= DROP_MS + 2_000.0 {
                off_sent = true;
                core2.set_auto(Some(false));
            }
            if off_sent && out.iter().any(|(_, c)| c.starts_with("Off Sequence")) {
                std::thread::sleep(Duration::from_millis(100));
                while let Ok(n) = desk.recv(&mut buf) {
                    if let Some((_, args)) = osc::decode(&buf[..n]) {
                        if let Some(OscArg::String(cmd)) = args.first().cloned() {
                            out.push((Instant::now(), cmd));
                        }
                    }
                }
                break;
            }
        }
        out
    })
    .await
    .unwrap();
    sender.abort();

    let cmds: Vec<&str> = received
        .iter()
        .map(|(_, c)| c.as_str())
        .filter(|c| !c.starts_with("Master 3.2"))
        .collect();
    println!("{cmds:#?}");
    assert_eq!(
        cmds,
        [
            "Goto Sequence \"DeckLight Auto\" Cue 20",
            "Goto Sequence \"DeckLight Auto\" Cue 50",
            "Goto Sequence \"DeckLight Auto\" Cue 30",
            "Go+ Sequence \"DeckLight Hit\"",
            "Off Sequence \"DeckLight Auto\"",
            "Off Sequence \"DeckLight Hit\"",
        ]
    );
    // Build-up stufenlos auf Speed-Master 2, am Ende zurück auf 0.
    let levels: Vec<u32> = received
        .iter()
        .filter_map(|(_, c)| c.strip_prefix("Master 3.2 At "))
        .map(|v| v.parse().unwrap())
        .collect();
    assert!(levels.len() >= 5, "{levels:?}");
    assert!(levels
        .windows(2)
        .take(levels.len() - 2)
        .all(|w| w[0] <= w[1]));
    assert_eq!(levels.last(), Some(&0));

    // Drop genau auf der Eins: Vorlauf 20 ms, höchstens ein Frame daneben.
    let truth = truth.lock().unwrap().clone();
    let crossed = truth
        .iter()
        .find(|(_, p)| *p >= DROP_MS)
        .map(|(t, _)| *t)
        .unwrap();
    let got = received
        .iter()
        .find(|(_, c)| c.ends_with("Cue 30"))
        .unwrap()
        .0;
    let err_ms = if got >= crossed {
        got.duration_since(crossed).as_secs_f64() * 1000.0
    } else {
        -(crossed.duration_since(got).as_secs_f64() * 1000.0)
    };
    println!("Drop kam {err_ms:+.1} ms zur echten Eins (Vorlauf 20 ms)");
    assert!((-60.0..20.0).contains(&err_ms), "{err_ms:.1} ms daneben");

    // Log zeigt Grund und Quelle.
    let log: Vec<String> = core
        .with(|s| s.trigger_log.entries().map(|e| e.source.clone()).collect())
        .unwrap();
    assert!(
        log.iter()
            .any(|l| l.starts_with("Auto: Drop (Phrasen + Wellenform")),
        "{log:?}"
    );
    assert!(!core.with(|s| s.auto_status.active).unwrap());
}
