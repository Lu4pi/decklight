//! Ersatz für die Probe am Pult: zwei simulierte CDJs, eine „grandMA3“ aus
//! Art-Net-Empfänger und Timecode-Show. Geprüft wird der Ablauf aus der
//! Checkliste: zwei Tracks, Deck-Wechsel, Pitch ±8 %, Loop. Jeder empfangene
//! Frame muss zur echten Position des Decks passen, dessen Track er zeigt,
//! und jedes Show-Ereignis muss weniger als einen Frame zu spät oder zu früh
//! ausgelöst werden.

use decklight_core::{Core, CoreConfig, TimecodeOutConfig};
use decklight_engine::MasterMode;
use decklight_outputs::artnet;
use decklight_outputs::{FrameRate, GeneratorConfig, StopMode, Timecode};
use decklight_prolink::{LinkConfig, PortConfig};
use decklight_simulator::{Command, Scenario, SimTrack, Simulator, Step};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn track(id: u32, title: &str, bpm: f64) -> SimTrack {
    SimTrack {
        id,
        title: title.into(),
        artist: "Show".into(),
        length_ms: 600_000,
        bpm,
        first_beat_ms: 0.0,
        file: None,
    }
}

/// Deck 1 spielt mit +8 %, Deck 2 kommt mit −8 % dazu, übernimmt nach
/// 2,5 s den Master und läuft dann in einem 1-s-Loop.
fn scenario() -> Scenario {
    use Command::*;
    let steps = vec![
        (
            0,
            Load {
                deck: 1,
                track: track(11, "Alpha", 126.0),
            },
        ),
        (
            0,
            Load {
                deck: 2,
                track: track(22, "Beta", 124.0),
            },
        ),
        (0, OnAir { deck: 1, on: true }),
        (0, Master { deck: 1 }),
        (
            0,
            Pitch {
                deck: 1,
                percent: 8.0,
            },
        ),
        (300, Play { deck: 1 }),
        (
            1_500,
            Pitch {
                deck: 2,
                percent: -8.0,
            },
        ),
        (1_500, Play { deck: 2 }),
        (2_500, OnAir { deck: 2, on: true }),
        (2_500, Master { deck: 2 }),
        (2_700, OnAir { deck: 1, on: false }),
        (
            4_200,
            Loop {
                deck: 2,
                start_ms: 2_000.0,
                end_ms: 3_000.0,
            },
        ),
    ];
    Scenario {
        decks: 2,
        mixer: true,
        repeat_after_ms: None,
        legacy_decks: vec![],
        steps: steps
            .into_iter()
            .map(|(at_ms, command)| Step { at_ms, command })
            .collect(),
    }
}

type Truth = Vec<(Instant, [f64; 2])>;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ma3_timecode_show_follows_deck_change_pitch_and_loop() {
    let rate = FrameRate::Fps30;
    let frame_ms = rate.frame_ms();
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
        master_mode: MasterMode::OnAir,
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

    let sim = Arc::new(Mutex::new(Simulator::new(scenario(), Ipv4Addr::LOCALHOST)));
    let truth: Arc<Mutex<Truth>> = Arc::default();
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
                (
                    p,
                    [
                        s.deck(1).unwrap().position_ms,
                        s.deck(2).unwrap().position_ms,
                    ],
                )
            };
            truth2.lock().unwrap().push((now, pos));
            for p in packets {
                let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, ports.port(p.channel)));
                let _ = sock.send_to(&p.bytes, addr).await;
            }
        }
    });

    let received = tokio::task::spawn_blocking(move || {
        let mut out: Vec<(Instant, Timecode)> = Vec::new();
        let end = Instant::now() + Duration::from_millis(6_500);
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

    // Offsets so, wie DeckLight sie vergeben hat.
    let offsets: BTreeMap<String, f64> = core
        .with(|s| {
            s.offsets
                .entries
                .iter()
                .map(|e| (e.key.0.clone(), e.offset_ms))
                .collect()
        })
        .unwrap();
    // Ohne dbserver bleibt die vorläufige Kennung „p<Player>-<Slot>-<ID>“.
    let offset_of = |id: u32| -> f64 {
        *offsets
            .iter()
            .find(|(k, _)| k.ends_with(&format!("-{id}")))
            .unwrap_or_else(|| panic!("kein Offset für Track {id}: {offsets:?}"))
            .1
    };
    let (off_a, off_b) = (offset_of(11), offset_of(22));
    assert_ne!(off_a, off_b);

    let truth = truth.lock().unwrap().clone();
    let t0 = truth[0].0;
    let at_ms = |t: Instant| t.duration_since(t0).as_secs_f64() * 1000.0;
    // Wahrheit eines Decks zu einem Zeitpunkt; bei Sprüngen (Loop) keine Aussage.
    let truth_at = |at: Instant, deck: usize| -> Option<f64> {
        let i = truth.partition_point(|(t, _)| *t <= at);
        let (ta, pa) = *truth.get(i.checked_sub(1)?)?;
        let (tb, pb) = *truth.get(i)?;
        let (p0, p1) = (pa[deck], pb[deck]);
        if (p1 - p0).abs() > 100.0 {
            return None;
        }
        let f = at.duration_since(ta).as_secs_f64() / tb.duration_since(ta).as_secs_f64().max(1e-9);
        Some(p0 + (p1 - p0) * f)
    };
    // Zeitpunkte, an denen ein Deck springt (Loop-Ende).
    let jumps: Vec<Instant> = truth
        .windows(2)
        .filter(|w| (0..2).any(|d| (w[1].1[d] - w[0].1[d]).abs() > 100.0))
        .map(|w| w[1].0)
        .collect();
    assert!(
        jumps.len() >= 2,
        "Loop wurde nicht durchlaufen ({} Sprünge)",
        jumps.len()
    );

    // 1) Jeder Frame passt zum Deck, dessen Track er zeigt.
    let mut checked = [0usize; 2];
    let mut worst: f64 = 0.0;
    let mut first_b: Option<f64> = None;
    let mut a_after_switch = 0;
    for (at, tc) in &received {
        let ms = tc.to_ms();
        let (deck, offset) = if (ms - off_b).abs() < (ms - off_a).abs() {
            (1, off_b)
        } else {
            (0, off_a)
        };
        if deck == 1 {
            first_b.get_or_insert(at_ms(*at));
        } else if at_ms(*at) > 3_600.0 {
            a_after_switch += 1;
        }
        if at_ms(*at) < 1_000.0 {
            continue; // Anlauf
        }
        if jumps
            .iter()
            .any(|j| *at >= *j && at.duration_since(*j) < Duration::from_millis(100))
        {
            continue;
        }
        let Some(pos) = truth_at(*at, deck) else {
            continue;
        };
        let into = offset + pos - ms;
        let err = if into < 0.0 {
            -into
        } else {
            (into - frame_ms).max(0.0)
        };
        worst = worst.max(err);
        checked[deck] += 1;
    }
    let first_b = first_b.expect("Timecode wechselt nie auf Track B");
    println!(
        "Frames A {} / B {}, größte Abweichung {worst:.2} ms, Wechsel nach {first_b:.0} ms",
        checked[0], checked[1]
    );
    assert!(checked[0] > 40 && checked[1] > 60, "{checked:?}");
    assert!(worst < frame_ms, "Abweichung {worst:.2} ms ≥ 1 Frame");
    // Im On-Air-Modus gilt Deck 2, sobald Deck 1 bei 2,7 s aus dem Mix ist,
    // plus Hysterese (0,5 s) und Mixer-Takt (200 ms). Ohne Flackern.
    assert!(
        (2_700.0..3_600.0).contains(&first_b),
        "Wechsel bei {first_b:.0} ms"
    );
    assert_eq!(a_after_switch, 0, "nach dem Wechsel wieder Track A");

    // 2) Show-Ereignisse wie im MA3-Timecode-Pool, dort auf ganze Frames
    //    gerastert: alle 4 Frames in Track B ab Frame 54 (1,8 s), also nach
    //    dem Wechsel und im Loop-Bereich. Das Pult löst ein Ereignis aus,
    //    sobald es diesen Frame empfängt.
    let mut fired = 0;
    let mut worst_event: f64 = 0.0;
    for k in 0..8 {
        let ev_pos = f64::from(54 + 4 * k) * frame_ms;
        let ev_tc = off_b + ev_pos;
        let Some((got, _)) = received
            .iter()
            .find(|(_, tc)| (tc.to_ms() - ev_tc).abs() < 0.5)
        else {
            continue;
        };
        // Wann hat Deck 2 diese Stelle wirklich erreicht?
        let Some((real, _)) = truth.iter().find(|(_, p)| p[1] >= ev_pos) else {
            continue;
        };
        assert!(at_ms(*real) > first_b, "Ereignis vor dem Deck-Wechsel");
        let diff = (at_ms(*got) - at_ms(*real)).abs();
        println!(
            "  {ev_pos:6.1} ms: Pult {:.1} / echt {:.1}",
            at_ms(*got),
            at_ms(*real)
        );
        worst_event = worst_event.max(diff);
        fired += 1;
    }
    println!("{fired} Show-Ereignisse, größte Abweichung {worst_event:.2} ms");
    assert_eq!(fired, 8);
    assert!(
        worst_event < frame_ms,
        "Ereignis {worst_event:.2} ms daneben"
    );
}
