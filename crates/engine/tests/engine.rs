use decklight_engine::record::{read_records, replay, write_record, InputRecord};
use decklight_engine::{Engine, EngineEvent, MasterMode};
use decklight_prolink::packet;
use decklight_simulator::{Scenario, Simulator};
use std::net::Ipv4Addr;

/// Lässt die Demo laufen und sammelt Eingänge und Ereignisse.
fn run_demo(seconds: u32) -> (Vec<InputRecord>, Vec<(f64, EngineEvent)>) {
    let mut sim = Simulator::new(Scenario::demo(), Ipv4Addr::LOCALHOST);
    let mut engine = Engine::default();
    let mut records = Vec::new();
    let mut events = Vec::new();
    for i in 1..=seconds * 100 {
        let t = f64::from(i) / 100.0;
        for o in sim.step(10.0) {
            let p = packet::parse(o.channel, &o.bytes).unwrap();
            events.extend(engine.handle_packet(t, &p).into_iter().map(|e| (t, e)));
            records.push(InputRecord { t, packet: p });
        }
        events.extend(engine.tick(t).into_iter().map(|e| (t, e)));
    }
    (records, events)
}

#[test]
fn demo_produces_expected_event_sequence() {
    let (_, events) = run_demo(100);
    let find =
        |pred: &dyn Fn(&EngineEvent) -> bool| events.iter().find(|(_, e)| pred(e)).map(|(t, _)| *t);

    assert!(find(&|e| matches!(e, EngineEvent::TrackLoaded { deck: 1, .. })).unwrap() < 0.3);
    assert!(find(&|e| matches!(e, EngineEvent::TrackLoaded { deck: 2, .. })).unwrap() < 0.3);
    let start1 = find(&|e| matches!(e, EngineEvent::PlayStarted { deck: 1, .. })).unwrap();
    assert!((0.5..0.8).contains(&start1), "Start Deck 1 bei {start1}");
    let first_master =
        find(&|e| matches!(e, EngineEvent::MasterChanged { to: Some(1), .. })).unwrap();
    assert!(first_master < 1.0);
    // Demo: Deck 2 ab 56 s on air, Deck 1 ab 72 s aus → Master-Wechsel kurz danach.
    let handover = find(&|e| {
        matches!(
            e,
            EngineEvent::MasterChanged {
                from: Some(1),
                to: Some(2)
            }
        )
    })
    .unwrap();
    assert!((72.0..73.0).contains(&handover), "Wechsel bei {handover}");
    let master_changes = events
        .iter()
        .filter(|(_, e)| matches!(e, EngineEvent::MasterChanged { .. }))
        .count();
    assert_eq!(master_changes, 2, "kein Flackern");
    assert!(find(&|e| matches!(e, EngineEvent::PlayStopped { deck: 1, .. })).unwrap() >= 75.0);
    assert!(events
        .iter()
        .any(|(_, e)| matches!(e, EngineEvent::Beat { deck: 2, .. })));
}

#[test]
fn tempo_master_mode_follows_player_flag() {
    let mut sim = Simulator::new(Scenario::demo(), Ipv4Addr::LOCALHOST);
    let mut engine = Engine::default();
    engine.set_master_mode(MasterMode::TempoMaster);
    let mut masters = Vec::new();
    for i in 1..=8000 {
        let t = f64::from(i) / 100.0;
        for o in sim.step(10.0) {
            for e in engine.handle_packet(t, &packet::parse(o.channel, &o.bytes).unwrap()) {
                if let EngineEvent::MasterChanged { to, .. } = e {
                    masters.push((t, to));
                }
            }
        }
    }
    assert_eq!(
        masters.iter().map(|m| m.1).collect::<Vec<_>>(),
        vec![Some(1), Some(2)]
    );
    assert!((70.0..71.0).contains(&masters[1].0));
}

#[test]
fn record_and_replay_reproduce_events() {
    let (records, events) = run_demo(20);
    let mut buf = Vec::new();
    for r in &records {
        write_record(&mut buf, r).unwrap();
    }
    let back = read_records(&buf[..]).unwrap();
    assert_eq!(back.len(), records.len());
    let replayed = replay(&mut Engine::default(), &back);
    let direct: Vec<_> = events
        .iter()
        .filter(|(_, e)| {
            !matches!(
                e,
                EngineEvent::MasterChanged { .. } | EngineEvent::OnAirChanged { .. }
            )
        })
        .map(|(_, e)| e.clone())
        .collect();
    let again: Vec<_> = replayed
        .iter()
        .map(|s| s.event.clone())
        .filter(|e| {
            !matches!(
                e,
                EngineEvent::MasterChanged { .. } | EngineEvent::OnAirChanged { .. }
            )
        })
        .collect();
    assert_eq!(direct, again);
}
