//! Cues gegen einen simulierten CDJ mit Netzwerk-Jitter: Die Engine-Schätzung muss
//! genau dieselben Cues auslösen wie der echte Playhead des Simulators, jeden genau
//! einmal pro Durchlauf und höchstens einen Frame daneben. Mit Pause, Pitch,
//! Loop, Cue-Taste, Sprung nach vorn und Sprung direkt auf einen Cue.

use decklight_engine::Engine;
use decklight_prolink::packet;
use decklight_show::{Action, Cue, TrackKey, TriggerTable};
use decklight_simulator::{Command, Scenario, SimTrack, Simulator, Step};
use decklight_triggers::{DeckInput, TriggerEngine};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::net::Ipv4Addr;

const FRAME_MS: f64 = 40.0;

fn scenario(legacy: bool) -> Scenario {
    let track = SimTrack {
        id: 9,
        title: "Cue-Test".into(),
        artist: "Sim".into(),
        length_ms: 8 * 60_000,
        bpm: 124.0,
        first_beat_ms: 0.0,
    };
    use Command::*;
    let steps = vec![
        (0, Load { deck: 1, track }),
        (1_000, Play { deck: 1 }),
        (
            30_000,
            Pitch {
                deck: 1,
                percent: 4.0,
            },
        ),
        (60_000, Pause { deck: 1 }),
        (63_000, Play { deck: 1 }),
        (
            90_000,
            Pitch {
                deck: 1,
                percent: -5.0,
            },
        ),
        (
            120_000,
            Seek {
                deck: 1,
                ms: 99_000.0,
            },
        ),
        (
            120_000,
            Loop {
                deck: 1,
                start_ms: 100_000.0,
                end_ms: 104_000.0,
            },
        ),
        (140_000, ExitLoop { deck: 1 }),
        (160_000, Cue { deck: 1 }),
        (162_000, Play { deck: 1 }),
        // Sprung nach vorn über mehrere Cues hinweg.
        (
            200_000,
            Seek {
                deck: 1,
                ms: 250_000.0,
            },
        ),
        // Hot Cue genau auf einen Cue.
        (
            230_000,
            Seek {
                deck: 1,
                ms: 300_000.0,
            },
        ),
    ];
    Scenario {
        decks: 1,
        mixer: false,
        repeat_after_ms: None,
        legacy_decks: if legacy { vec![1] } else { vec![] },
        steps: steps
            .into_iter()
            .map(|(at_ms, command)| Step { at_ms, command })
            .collect(),
    }
}

fn table() -> TriggerTable {
    let mut t = TriggerTable::default();
    let key = TrackKey("Cue-Test".into());
    // Absichtlich nicht auf dem Beat; einer im Loop, einer auf dem Hot-Cue-Ziel.
    let mut points: Vec<f64> = (0..60).map(|i| 3_100.0 + f64::from(i) * 7_300.0).collect();
    points.push(102_000.0);
    points.push(300_000.0);
    for (i, ms) in points.into_iter().enumerate() {
        t.add_cue(
            &key,
            Cue::at_ms(
                &format!("C{i}"),
                ms,
                vec![Action::Osc {
                    command: format!("Go+ Exec {i}"),
                }],
            ),
        );
    }
    t
}

struct Lcg(u64);
impl Lcg {
    fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// (Cue-Name, Auslösezeit in s) für die Wahrheit und für die Schätzung.
type Fired = Vec<(String, f64)>;

fn run(legacy: bool, minutes: u64) -> (Fired, Fired) {
    let mut sim = Simulator::new(scenario(legacy), Ipv4Addr::LOCALHOST);
    let mut engine = Engine::default();
    let table = table();
    let key = TrackKey("Cue-Test".into());
    let (mut truth_eval, mut est_eval) = (TriggerEngine::new(), TriggerEngine::new());
    let (mut truth, mut est) = (Vec::new(), Vec::new());
    let mut rng = Lcg(11);
    let mut queue: BinaryHeap<Reverse<(u64, u64, usize)>> = BinaryHeap::new();
    let mut store: Vec<Option<(packet::Channel, Vec<u8>)>> = Vec::new();
    let mut seq = 0u64;

    let mut now_us = 0u64;
    while now_us < minutes * 60_000_000 {
        now_us += 1000;
        for o in sim.step(1.0) {
            let delay_us = 300 + (rng.next_f64() * 3700.0) as u64;
            store.push(Some((o.channel, o.bytes)));
            queue.push(Reverse((now_us + delay_us, seq, store.len() - 1)));
            seq += 1;
        }
        while let Some(Reverse((at, _, idx))) = queue.peek().copied() {
            if at > now_us {
                break;
            }
            queue.pop();
            let (channel, bytes) = store[idx].take().unwrap();
            let p = packet::parse(channel, &bytes).unwrap();
            engine.handle_packet(at as f64 / 1e6, &p);
        }
        // Auswertung alle 2 ms wie im Trigger-Thread.
        if now_us % 2000 != 0 {
            continue;
        }
        let t = now_us as f64 / 1e6;
        let d = sim.deck(1).unwrap();
        let real = DeckInput {
            number: 1,
            track: Some(&key),
            position_ms: Some(d.position_ms),
            moving: d.is_moving(),
            rate: d.rate(),
            grid: None,
            precise: true,
        };
        for f in truth_eval.update(t, &[real], Some(1), &table) {
            truth.push((f.name().to_string(), t - f.late_ms / 1000.0));
        }
        let Some(deck) = engine.deck(1) else { continue };
        let guess = DeckInput {
            number: 1,
            track: deck.track.map(|_| &key),
            position_ms: deck.position_at(t),
            moving: deck.moving,
            rate: deck.rate(),
            grid: None,
            precise: deck.has_precise_position,
        };
        for f in est_eval.update(t, &[guess], Some(1), &table) {
            est.push((f.name().to_string(), t - f.late_ms / 1000.0));
        }
    }
    (truth, est)
}

fn compare(truth: &Fired, est: &Fired, tolerance_ms: f64) -> f64 {
    let names = |v: &Fired| v.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    assert_eq!(names(est), names(truth), "andere Cues ausgelöst");
    let mut max = 0.0f64;
    for ((n, a), (_, b)) in truth.iter().zip(est) {
        let err = (a - b).abs() * 1000.0;
        assert!(err < tolerance_ms, "{n}: {err:.1} ms daneben");
        max = max.max(err);
    }
    max
}

#[test]
fn cdj3000_cues_fire_exactly_once_and_within_one_frame() {
    let (truth, est) = run(false, 5);
    // Loop: C-Loop (102 s) in jeder der fünf Runden; Hot-Cue-Ziel einmal.
    let loop_hits = truth.iter().filter(|(n, _)| n == "C60").count();
    assert!(loop_hits >= 4, "Loop-Cue {loop_hits}×");
    assert_eq!(truth.iter().filter(|(n, _)| n == "C61").count(), 1);
    let max = compare(&truth, &est, FRAME_MS);
    println!("CDJ-3000: {} Cues, max {max:.1} ms", est.len());
    assert!(est.len() > 30);
}

#[test]
fn nxs2_cues_fire_exactly_once() {
    // Ältere Player kennen die Position nur über Beat-Pakete: dieselben Cues,
    // aber nach Start oder Sprung bis zum nächsten Beat ungenauer.
    let (truth, est) = run(true, 5);
    compare(&truth, &est, 500.0);
    // Im normalen Lauf ebenfalls unter einem Frame; nur der Hot-Cue-Sprung (C61)
    // wird erst mit dem nächsten Paket erkannt.
    let running: Fired = truth.iter().filter(|(n, _)| n != "C61").cloned().collect();
    let running_est: Fired = est.iter().filter(|(n, _)| n != "C61").cloned().collect();
    let max = compare(&running, &running_est, FRAME_MS);
    println!("NXS2: {} Cues, max {max:.1} ms im Lauf", est.len());
}
