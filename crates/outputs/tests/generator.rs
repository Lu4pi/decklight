use decklight_outputs::mtc::QuarterFrameDecoder;
use decklight_outputs::*;

fn run(
    gen: &mut TimecodeGenerator,
    from_s: f64,
    to_s: f64,
    pos: impl Fn(f64) -> Option<f64>,
    moving: bool,
) -> Vec<(f64, TcMessage)> {
    let mut out = Vec::new();
    let mut t = from_s;
    while t < to_s {
        let input = TcInput {
            position_ms: pos(t),
            moving,
            reverse: false,
        };
        out.extend(gen.update(t, input).into_iter().map(|m| (t, m)));
        t += 0.001;
    }
    out
}

#[test]
fn running_timecode_is_continuous_for_all_rates() {
    for rate in FrameRate::ALL {
        let mut g = TimecodeGenerator::new(GeneratorConfig {
            rate,
            stop_mode: StopMode::Freeze,
        });
        let offset = 3_600_000.0;
        let msgs = run(&mut g, 0.0, 10.0, |t| Some(offset + t * 1000.0), true);

        let artnet: Vec<Timecode> = msgs
            .iter()
            .filter_map(|(_, m)| {
                if let TcMessage::ArtNet(tc) = m {
                    Some(*tc)
                } else {
                    None
                }
            })
            .collect();
        let expected = (10.0 * rate.fps()).round() as usize;
        assert!(
            artnet.len().abs_diff(expected) <= 1,
            "{rate}: {} Art-Net-Frames",
            artnet.len()
        );
        assert_eq!(artnet[0].to_string().get(..8), Some("01:00:00"));
        for w in artnet.windows(2) {
            let a = Timecode::frame_index(w[0].to_ms() + 0.01, rate);
            let b = Timecode::frame_index(w[1].to_ms() + 0.01, rate);
            assert_eq!(b - a, 1, "{rate}: {} -> {}", w[0], w[1]);
        }

        // Quarter-Frames: vier pro Frame, Decoder liefert nach jedem Durchlauf den
        // Wert von vor zwei Frames (Empfänger addieren die zwei Frames).
        let mut dec = QuarterFrameDecoder::default();
        let mut qf = 0usize;
        let mut decoded = 0;
        for (t, m) in &msgs {
            if let TcMessage::MtcQuarter(q) = m {
                qf += 1;
                if let Some((h, mi, s, f, code)) = dec.push(*q) {
                    decoded += 1;
                    assert_eq!(code, rate.code());
                    let sent = Timecode {
                        hours: h,
                        minutes: mi,
                        seconds: s,
                        frames: f,
                        rate,
                    };
                    let now = Timecode::frame_index(offset + t * 1000.0, rate);
                    let lag = now - Timecode::frame_index(sent.to_ms() + 0.01, rate);
                    assert!(
                        (1..=2).contains(&lag),
                        "{rate}: Versatz {lag} Frames bei {sent}"
                    );
                }
            }
        }
        assert!(
            qf.abs_diff(expected * 4) <= 8,
            "{rate}: {qf} Quarter-Frames"
        );
        assert!(decoded >= expected / 2 - 2);
    }
}

#[test]
fn jump_sends_full_frame_and_pause_freezes() {
    let mut g = TimecodeGenerator::new(GeneratorConfig::default());
    let msgs = run(&mut g, 0.0, 1.0, |t| Some(t * 1000.0), true);
    assert!(matches!(msgs[0].1, TcMessage::MtcFull(_)));
    // Sprung nach 2 h.
    let msgs = run(&mut g, 1.0, 1.1, |t| Some(7_200_000.0 + t * 1000.0), true);
    let full: Vec<_> = msgs
        .iter()
        .filter(|(_, m)| matches!(m, TcMessage::MtcFull(_)))
        .collect();
    assert_eq!(full.len(), 1);
    assert!(matches!(full[0].1, TcMessage::MtcFull(tc) if tc.hours == 2));

    // Pause: ein Full-Frame, dann Art-Net im Frametakt mit demselben Wert.
    let msgs = run(&mut g, 2.0, 3.0, |_| Some(7_201_100.0), false);
    let full = msgs
        .iter()
        .filter(|(_, m)| matches!(m, TcMessage::MtcFull(_)))
        .count();
    let artnet: Vec<_> = msgs
        .iter()
        .filter_map(|(_, m)| {
            if let TcMessage::ArtNet(tc) = m {
                Some(*tc)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(full, 1);
    assert!(
        (24..=26).contains(&artnet.len()),
        "{} Art-Net im Stillstand",
        artnet.len()
    );
    assert!(artnet.iter().all(|tc| *tc == artnet[0]));
    assert!(!msgs
        .iter()
        .any(|(_, m)| matches!(m, TcMessage::MtcQuarter(_))));
}

#[test]
fn stop_mode_goes_silent() {
    let mut g = TimecodeGenerator::new(GeneratorConfig {
        rate: FrameRate::Fps30,
        stop_mode: StopMode::Stop,
    });
    run(&mut g, 0.0, 0.5, |t| Some(t * 1000.0), true);
    let msgs = run(&mut g, 0.5, 2.0, |_| Some(500.0), false);
    assert_eq!(msgs.len(), 2, "{msgs:?}"); // Full-Frame + letztes Art-Net, dann Ruhe
    let none = run(&mut g, 2.0, 2.5, |_| None, false);
    assert!(none.is_empty());
}
