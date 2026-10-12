//! Meilenstein Phase 2: Timecode-Abweichung im Simulator unter einem Frame
//! über zehn Minuten, mit Netzwerk-Jitter, Pitch-Änderungen, Loop, Pause und Cue.

use decklight_engine::{Engine, EngineEvent};
use decklight_prolink::packet;
use decklight_simulator::{Command, Scenario, SimTrack, Simulator, Step};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::net::Ipv4Addr;

/// Ein Frame bei 25 fps.
const FRAME_MS: f64 = 40.0;

fn scenario(legacy: bool) -> Scenario {
    let track = SimTrack {
        id: 42,
        title: "Test".into(),
        artist: "Sim".into(),
        length_ms: 12 * 60_000,
        bpm: 124.0,
        first_beat_ms: 0.0,
    };
    use Command::*;
    let steps = vec![
        (0, Load { deck: 1, track }),
        (0, OnAir { deck: 1, on: true }),
        (1_000, Play { deck: 1 }),
        (
            60_000,
            Pitch {
                deck: 1,
                percent: 3.0,
            },
        ),
        (
            120_000,
            Pitch {
                deck: 1,
                percent: -6.5,
            },
        ),
        (180_000, Pause { deck: 1 }),
        (185_000, Play { deck: 1 }),
        (
            240_000,
            Seek {
                deck: 1,
                ms: 199_000.0,
            },
        ),
        (
            240_000,
            Loop {
                deck: 1,
                start_ms: 200_000.0,
                end_ms: 203_870.0,
            },
        ),
        (270_000, ExitLoop { deck: 1 }),
        (300_000, Cue { deck: 1 }),
        (302_000, Play { deck: 1 }),
        (
            360_000,
            Seek {
                deck: 1,
                ms: 400_000.0,
            },
        ),
        (
            420_000,
            Pitch {
                deck: 1,
                percent: 8.0,
            },
        ),
        (
            480_000,
            Pitch {
                deck: 1,
                percent: 0.0,
            },
        ),
    ];
    Scenario {
        decks: 1,
        mixer: true,
        repeat_after_ms: None,
        legacy_decks: if legacy { vec![1] } else { vec![] },
        steps: steps
            .into_iter()
            .map(|(at_ms, command)| Step { at_ms, command })
            .collect(),
    }
}

/// Einfacher deterministischer Zufall (LCG) für den Jitter.
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

struct Report {
    max_err_ms: f64,
    mean_err_ms: f64,
    samples: usize,
    jumps: usize,
}

fn run(legacy: bool, minutes: u64) -> Report {
    let mut sim = Simulator::new(scenario(legacy), Ipv4Addr::LOCALHOST);
    let mut engine = Engine::default();
    let mut rng = Lcg(7);
    // Pakete kommen 0,3–4 ms nach dem Senden an, in Empfangsreihenfolge.
    let mut queue: BinaryHeap<Reverse<(u64, u64, usize)>> = BinaryHeap::new();
    let mut store: Vec<Option<(packet::Channel, Vec<u8>)>> = Vec::new();
    let mut seq = 0u64;

    let total_us = minutes * 60_000_000;
    let step_us = 1000u64;
    let (mut max_err, mut sum_err, mut samples, mut jumps) = (0.0f64, 0.0f64, 0usize, 0usize);
    let mut last_truth = 0.0f64;
    let mut last_moving = false;
    let mut settle_until_us = 0u64;

    let mut now_us = 0u64;
    while now_us < total_us {
        now_us += step_us;
        for o in sim.step(step_us as f64 / 1000.0) {
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
            let t = at as f64 / 1e6;
            for ev in engine.handle_packet(t, &p) {
                if matches!(ev, EngineEvent::Jump { .. }) {
                    jumps += 1;
                }
            }
        }

        let truth_deck = sim.deck(1).unwrap();
        let truth = truth_deck.position_ms;
        let moving = truth_deck.is_moving();
        // Echte Sprünge (Cue, Loop, Suchen) kann niemand vorhersehen: bis das
        // nächste Paket eintrifft, wird nicht gemessen.
        let expected = if moving { 1.0 } else { 0.0 };
        let state_change = moving != last_moving;
        last_moving = moving;
        if (truth - last_truth - expected * truth_deck.rate()).abs() > 2.0
            || (legacy && state_change)
        {
            // Ältere Player kennen nach Sprung oder Start/Stopp erst wieder mit dem
            // nächsten Status- und Beat-Paket die genaue Position.
            settle_until_us = now_us + if legacy { 1_200_000 } else { 60_000 };
        }
        last_truth = truth;
        // Im Stillstand kennen ältere Player ihre Position nur auf den Beat genau.
        if now_us < settle_until_us || now_us < 2_000_000 || (legacy && !moving) {
            continue;
        }
        let t = now_us as f64 / 1e6;
        let Some(est) = engine.deck(1).and_then(|d| d.position_at(t)) else {
            continue;
        };
        let err = (est - truth).abs();
        max_err = max_err.max(err);
        sum_err += err;
        samples += 1;
    }
    Report {
        max_err_ms: max_err,
        mean_err_ms: sum_err / samples as f64,
        samples,
        jumps,
    }
}

#[test]
fn cdj3000_precise_position_stays_within_one_frame_for_ten_minutes() {
    let r = run(false, 10);
    println!(
        "CDJ-3000: max {:.2} ms, Mittel {:.2} ms, {} Messungen, {} Sprünge",
        r.max_err_ms, r.mean_err_ms, r.samples, r.jumps
    );
    assert!(r.samples > 500_000);
    assert!(r.max_err_ms < FRAME_MS, "max {:.2} ms", r.max_err_ms);
    assert!(r.mean_err_ms < 5.0, "Mittel {:.2} ms", r.mean_err_ms);
    // Loop-Rücksprünge, Cue und Suchen sind Sprünge, sonst nichts.
    assert!(r.jumps >= 3 && r.jumps < 20, "{} Sprünge", r.jumps);
}

#[test]
fn nxs2_beat_packets_stay_within_one_frame_for_ten_minutes() {
    let r = run(true, 10);
    println!(
        "NXS2: max {:.2} ms, Mittel {:.2} ms, {} Messungen, {} Sprünge",
        r.max_err_ms, r.mean_err_ms, r.samples, r.jumps
    );
    assert!(r.samples > 400_000);
    assert!(r.max_err_ms < FRAME_MS, "max {:.2} ms", r.max_err_ms);
}
