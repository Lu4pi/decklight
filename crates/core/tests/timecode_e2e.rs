//! Ende-zu-Ende: simulierter CDJ über UDP → DeckLight → Art-Net-Timecode über UDP.
//! Ersatz für den Test mit grandMA3 onPC: ein Empfänger prüft, dass jeder
//! empfangene Timecode zur echten Position des simulierten Decks passt.

use decklight_core::{Core, CoreConfig, TimecodeOutConfig};
use decklight_outputs::artnet;
use decklight_outputs::{FrameRate, GeneratorConfig, StopMode, Timecode};
use decklight_prolink::{LinkConfig, PortConfig};
use decklight_simulator::{Command, Scenario, SimTrack, Simulator, Step};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
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
    };
    use Command::*;
    let steps = vec![
        (0, Load { deck: 1, track }),
        (0, OnAir { deck: 1, on: true }),
        (300, Play { deck: 1 }),
        (
            1_500,
            Pitch {
                deck: 1,
                percent: 4.0,
            },
        ),
        (
            3_000,
            Seek {
                deck: 1,
                ms: 120_000.0,
            },
        ),
    ];
    Scenario {
        decks: 1,
        mixer: true,
        repeat_after_ms: None,
        legacy_decks: vec![],
        steps: steps
            .into_iter()
            .map(|(at_ms, command)| Step { at_ms, command })
            .collect(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn artnet_timecode_follows_simulated_cdj_within_one_frame() {
    let rate = FrameRate::Fps25;
    let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
    rx.set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let rx_port = rx.local_addr().unwrap().port();

    let core = Core::start(CoreConfig {
        link: LinkConfig {
            ports: PortConfig {
                announce: 0,
                beat: 0,
                status: 0,
            },
            ..Default::default()
        },
        timecode: Some(TimecodeOutConfig {
            generator: GeneratorConfig {
                rate,
                stop_mode: StopMode::Freeze,
            },
            artnet: Some(Ipv4Addr::LOCALHOST),
            artnet_port: rx_port,
            mtc_port: None,
        }),
        ..Default::default()
    })
    .await
    .unwrap();
    let ports = core.link().local_ports();

    // Simulator im Echtzeittakt; die Wahrheit wird mit Zeitstempel protokolliert.
    let sim = Arc::new(Mutex::new(Simulator::new(scenario(), Ipv4Addr::LOCALHOST)));
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

    let received = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        let end = Instant::now() + Duration::from_secs(5);
        let mut buf = [0u8; 64];
        while Instant::now() < end {
            if let Ok(n) = rx.recv(&mut buf) {
                if let Some((h, m, s, f, code)) = artnet::decode(&buf[..n]) {
                    assert_eq!(code, rate.code());
                    out.push((
                        Instant::now(),
                        Timecode {
                            hours: h,
                            minutes: m,
                            seconds: s,
                            frames: f,
                            rate,
                        },
                    ));
                }
            }
        }
        out
    })
    .await
    .unwrap();
    sender.abort();

    let truth = truth.lock().unwrap().clone();
    let truth_at = |at: Instant| -> Option<f64> {
        let i = truth.partition_point(|(t, _)| *t <= at);
        let (t0, p0) = *truth.get(i.checked_sub(1)?)?;
        let (t1, p1) = *truth.get(i)?;
        // Echte Sprünge (Seek) nicht interpolieren.
        if (p1 - p0).abs() > 100.0 {
            return None;
        }
        let f = at.duration_since(t0).as_secs_f64() / t1.duration_since(t0).as_secs_f64().max(1e-9);
        Some(p0 + (p1 - p0) * f)
    };

    let offset = 3_600_000.0; // erster Track: 01:00:00:00
    let frame_ms = rate.frame_ms();
    let mut checked = 0;
    let mut worst: f64 = 0.0;
    let seek_at = truth.iter().find(|(_, p)| *p > 100_000.0).map(|(t, _)| *t);
    for (at, tc) in &received {
        // Erste Sekunde (Anlauf) und 100 ms nach dem Sprung auslassen.
        if at.duration_since(truth[0].0) < Duration::from_millis(1000) {
            continue;
        }
        if seek_at.is_some_and(|s| *at >= s && at.duration_since(s) < Duration::from_millis(100)) {
            continue;
        }
        let Some(pos) = truth_at(*at) else { continue };
        // Ein empfangener Frame muss gerade laufen: Wahrheit liegt in [Frame-Anfang, Frame-Ende).
        let into_frame = offset + pos - tc.to_ms();
        let err = if into_frame < 0.0 {
            -into_frame
        } else {
            (into_frame - frame_ms).max(0.0)
        };
        worst = worst.max(err);
        checked += 1;
    }
    println!("{checked} Art-Net-Frames geprüft, größte Abweichung {worst:.2} ms");
    assert!(checked > 60, "nur {checked} Frames empfangen");
    assert!(worst < frame_ms, "Abweichung {worst:.2} ms ≥ 1 Frame");
    let offsets = core.with(|s| s.offsets.entries.len()).unwrap();
    assert_eq!(offsets, 1);
}
