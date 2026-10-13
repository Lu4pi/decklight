//! Ende-zu-Ende: simulierter CDJ über UDP → DeckLight → OSC an ein "Pult" über UDP.
//! Prüft Zeitpunkt (unter einem Frame), einmaliges Auslösen, globale Trigger,
//! Test-Button und Not-Aus.

use decklight_core::{ActionOutConfig, Core, CoreConfig};
use decklight_engine::EngineEvent;
use decklight_library::dbserver::server::DbServer;
use decklight_outputs::osc::{self, OscArg};
use decklight_prolink::{LinkConfig, PortConfig};
use decklight_show::{Action, Cue, DeckFilter, GlobalEvent, GlobalTrigger, TrackKey, TriggerTable};
use decklight_simulator::{Command, Scenario, SimLibrary, SimTrack, Simulator, Step};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn scenario() -> Scenario {
    let track = SimTrack {
        id: 77,
        title: "E2E".into(),
        artist: "Sim".into(),
        length_ms: 600_000,
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
            (300, Play { deck: 1 }),
        ]
        .into_iter()
        .map(|(at_ms, command)| Step { at_ms, command })
        .collect(),
    }
}

fn osc(cmd: &str) -> Vec<Action> {
    vec![Action::Osc {
        command: cmd.into(),
    }]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn osc_cues_fire_once_on_time_and_stop_on_panic() {
    let desk = UdpSocket::bind("127.0.0.1:0").unwrap();
    desk.set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let desk_addr = desk.local_addr().unwrap();

    let scenario = scenario();
    let db = DbServer::start(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
        Arc::new(SimLibrary::from_scenario(&scenario)),
    )
    .await
    .unwrap();

    let key = TrackKey("Sim – E2E [10:00]".into());
    let mut triggers = TriggerTable::default();
    triggers.add_cue(&key, Cue::at_ms("Intro", 1_500.0, osc("Go+ Exec 201")));
    triggers.add_cue(&key, Cue::at_ms("Drop", 2_600.0, osc("Go+ Exec 202")));
    triggers.add_global(GlobalTrigger {
        id: 0,
        name: "Play".into(),
        on: GlobalEvent::Play,
        decks: DeckFilter::Any,
        actions: osc("Go+ Exec 10{deck}"),
        armed: true,
    });

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
        triggers,
        actions: ActionOutConfig {
            osc: Some(desk_addr),
            osc_prefix: "gma3".into(),
            midi_port: None,
            bpm_command: None,
            phase_command: None,
        },
        ..Default::default()
    })
    .await
    .unwrap();
    let mut events = core.subscribe();
    let ports = core.link().local_ports();

    // Simulator im Echtzeittakt, Wahrheit mit Zeitstempel.
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

    // Pult: alle OSC-Befehle mit Empfangszeit. Nach "Intro" Not-Aus, dann Test-Button.
    let core = Arc::new(core);
    let core2 = core.clone();
    let received = tokio::task::spawn_blocking(move || {
        let mut out: Vec<(Instant, String, String)> = Vec::new();
        let end = Instant::now() + Duration::from_millis(3_600);
        let mut buf = [0u8; 256];
        let mut panicked = false;
        while Instant::now() < end {
            if let Ok(n) = desk.recv(&mut buf) {
                let (addr, args) = osc::decode(&buf[..n]).unwrap();
                let Some(OscArg::String(cmd)) = args.first().cloned() else {
                    panic!("Befehl ohne Text");
                };
                out.push((Instant::now(), addr, cmd));
            }
            if !panicked && out.iter().any(|(_, _, c)| c == "Go+ Exec 201") {
                panicked = true;
                core2.panic();
                core2.fire("Test", None, osc("Go+ Exec 999"));
            }
        }
        out
    })
    .await
    .unwrap();
    sender.abort();

    let cmds: Vec<&str> = received.iter().map(|(_, _, c)| c.as_str()).collect();
    assert_eq!(cmds, ["Go+ Exec 101", "Go+ Exec 201", "Go+ Exec 999"]);
    assert!(received.iter().all(|(_, a, _)| a == "/gma3/cmd"));

    // Zeitpunkt: Empfang höchstens einen Frame (40 ms) nach dem echten Durchlauf.
    let truth = truth.lock().unwrap().clone();
    let crossed = truth
        .iter()
        .find(|(_, p)| *p >= 1_500.0)
        .map(|(t, _)| *t)
        .unwrap();
    let got = received[1].0;
    let err_ms = if got >= crossed {
        got.duration_since(crossed).as_secs_f64() * 1000.0
    } else {
        -(crossed.duration_since(got).as_secs_f64() * 1000.0)
    };
    println!("Cue \"Intro\" kam {err_ms:+.1} ms nach dem echten Durchlauf");
    assert!(err_ms.abs() < 40.0, "{err_ms:.1} ms daneben");

    // Log und Ereignisse.
    let log: Vec<(String, Option<String>)> = core
        .with(|s| {
            s.trigger_log
                .entries()
                .map(|e| (e.source.clone(), e.error.clone()))
                .collect()
        })
        .unwrap();
    let names: Vec<&str> = log.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Play", "Intro", "Test"]);
    assert!(log.iter().all(|(_, e)| e.is_none()));
    let mut armed_off = false;
    let mut fired = 0;
    while let Ok(ev) = events.try_recv() {
        match ev.event {
            EngineEvent::ArmedChanged { armed: false } => armed_off = true,
            EngineEvent::TriggerFired { .. } => fired += 1,
            _ => {}
        }
    }
    assert!(armed_off);
    assert_eq!(fired, 3);
}
