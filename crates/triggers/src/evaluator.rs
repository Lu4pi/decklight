//! Auslöse-Logik für Cues und globale Trigger.

use crate::template::cue_position_ms;
use decklight_engine::{BeatGrid, EngineEvent};
use decklight_show::{Action, Cue, DeckFilter, FireScope, GlobalEvent, TrackKey, TriggerTable};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

/// Ein ausgelöster Cue wird erst wieder scharf, wenn der Playhead so weit davor
/// steht. Größer als die Sprungschwelle der Uhr (120 ms), damit eine Korrektur
/// beim Stoppen nie ein zweites Auslösen erlaubt.
pub const REARM_MS: f64 = 250.0;

/// Ein Sprung (Hot Cue) auf einen Cue löst aus, wenn er höchstens so weit
/// dahinter landet.
pub const JUMP_LANDING_MS: f64 = 80.0;

/// Dasselbe für ältere Player ohne genaue Position (CDJ-2000NXS2): Sie melden
/// die neue Position erst mit dem nächsten Status- oder Beat-Paket.
pub const JUMP_LANDING_LEGACY_MS: f64 = 300.0;

/// Spielraum für Paket-Jitter: so viel weiter als erwartet gilt noch als Lauf.
const JUMP_SLACK_MS: f64 = 60.0;

/// Zustand eines Decks zum Auswertezeitpunkt.
#[derive(Debug, Clone, Copy)]
pub struct DeckInput<'a> {
    pub number: u8,
    /// Kennung des geladenen Tracks in der Show.
    pub track: Option<&'a TrackKey>,
    pub position_ms: Option<f64>,
    pub moving: bool,
    /// Abspielrate (1,0 = Originaltempo, negativ = rückwärts).
    pub rate: f64,
    pub grid: Option<&'a BeatGrid>,
    /// Player meldet die genaue Position (CDJ-3000) statt nur Beats.
    pub precise: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FiringSource {
    Cue {
        track: TrackKey,
        id: u32,
        name: String,
    },
    Global {
        id: u32,
        name: String,
        on: GlobalEvent,
    },
    /// Von Hand ausgelöst (Test-Button, Cue in der Oberfläche).
    Manual { name: String },
    /// Auto-Pilot (Abschnitt, Hit, Pause …).
    Auto { name: String },
}

/// Ein Auslösen mit den fertigen Aktionen (Platzhalter ersetzt).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Firing {
    pub deck: Option<u8>,
    pub source: FiringSource,
    pub actions: Vec<Action>,
    /// Wie viel Echtzeit seit dem exakten Cue-Zeitpunkt vergangen ist.
    pub late_ms: f64,
}

impl Firing {
    pub fn name(&self) -> &str {
        match &self.source {
            FiringSource::Cue { name, .. }
            | FiringSource::Global { name, .. }
            | FiringSource::Manual { name }
            | FiringSource::Auto { name } => name,
        }
    }

    pub fn id(&self) -> u32 {
        match &self.source {
            FiringSource::Cue { id, .. } | FiringSource::Global { id, .. } => *id,
            FiringSource::Manual { .. } | FiringSource::Auto { .. } => 0,
        }
    }
}

#[derive(Debug, Default)]
struct DeckTrack {
    track: Option<TrackKey>,
    /// Letzte Auswertung: (Zeit, Position).
    last: Option<(f64, f64)>,
    /// Cues, die schon ausgelöst haben und noch nicht wieder scharf sind.
    fired: HashSet<u32>,
}

/// Merkt sich je Deck, wo der Playhead zuletzt stand und was schon ausgelöst hat.
#[derive(Debug, Default)]
pub struct TriggerEngine {
    decks: BTreeMap<u8, DeckTrack>,
}

fn allowed(table: &TriggerTable, cue: &Cue, deck: u8, master: Option<u8>) -> bool {
    table.armed
        && cue.armed
        && !cue.actions.is_empty()
        && match cue.scope.unwrap_or(table.scope) {
            FireScope::Master => master == Some(deck),
            FireScope::AnyDeck => true,
        }
}

impl TriggerEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cue-Punkte prüfen. Regelmäßig aufrufen (alle 1–5 ms), mit allen Decks.
    pub fn update(
        &mut self,
        t: f64,
        decks: &[DeckInput<'_>],
        master: Option<u8>,
        table: &TriggerTable,
    ) -> Vec<Firing> {
        let mut out = Vec::new();
        for d in decks {
            let st = self.decks.entry(d.number).or_default();
            if st.track.as_ref() != d.track {
                st.track = d.track.cloned();
                st.last = None;
                st.fired.clear();
            }
            let (Some(key), Some(pos)) = (d.track, d.position_ms) else {
                st.last = None;
                continue;
            };
            let cues: Vec<(&Cue, f64)> = table
                .cues(key)
                .iter()
                .filter_map(|c| cue_position_ms(&c.position, d.grid).map(|p| (c, p - c.lead_ms)))
                .collect();
            st.fired.retain(|id| {
                cues.iter()
                    .any(|(c, point)| c.id == *id && pos >= point - REARM_MS)
            });
            let prev = st.last.replace((t, pos));
            if !d.moving {
                continue;
            }
            let Some((t0, p0)) = prev else {
                continue;
            };
            let expected = (t - t0).max(0.0) * 1000.0 * d.rate.max(0.0);
            let delta = pos - p0;
            let jumped = delta < -1.0 || delta > expected * 1.5 + JUMP_SLACK_MS;
            let landing = if d.precise {
                JUMP_LANDING_MS
            } else {
                JUMP_LANDING_LEGACY_MS
            };

            let mut hits: Vec<(&Cue, f64)> = cues
                .iter()
                .copied()
                .filter(|(c, point)| {
                    !st.fired.contains(&c.id)
                        && allowed(table, c, d.number, master)
                        && if jumped {
                            c.fire_on_landing && *point <= pos && pos - point <= landing
                        } else {
                            p0 <= *point && *point < pos
                        }
                })
                .collect();
            hits.sort_by(|a, b| a.1.total_cmp(&b.1));
            for (c, point) in hits {
                st.fired.insert(c.id);
                out.push(Firing {
                    deck: Some(d.number),
                    source: FiringSource::Cue {
                        track: key.clone(),
                        id: c.id,
                        name: c.name.clone(),
                    },
                    actions: c.actions.iter().map(|a| a.render(Some(d.number))).collect(),
                    late_ms: (pos - point) / d.rate.max(0.01),
                });
            }
        }
        out
    }

    /// Globale Trigger zu einem Engine-Ereignis.
    pub fn on_event(
        &mut self,
        ev: &EngineEvent,
        master: Option<u8>,
        table: &TriggerTable,
    ) -> Vec<Firing> {
        let (on, deck) = match *ev {
            EngineEvent::TrackLoaded { deck, .. } => (GlobalEvent::TrackLoaded, deck),
            EngineEvent::PlayStarted { deck, .. } => (GlobalEvent::Play, deck),
            EngineEvent::PlayStopped { deck, .. } => (GlobalEvent::Stop, deck),
            EngineEvent::MasterChanged { to: Some(deck), .. } => (GlobalEvent::MasterChanged, deck),
            EngineEvent::OnAirChanged { deck, on_air } => (
                if on_air {
                    GlobalEvent::OnAir
                } else {
                    GlobalEvent::OffAir
                },
                deck,
            ),
            _ => return Vec::new(),
        };
        if !table.armed {
            return Vec::new();
        }
        table
            .globals
            .iter()
            .filter(|g| g.armed && g.on == on && !g.actions.is_empty())
            .filter(|g| match g.decks {
                DeckFilter::Any => true,
                DeckFilter::Master => master == Some(deck),
                DeckFilter::Deck(n) => n == deck,
            })
            .map(|g| Firing {
                deck: Some(deck),
                source: FiringSource::Global {
                    id: g.id,
                    name: g.name.clone(),
                    on,
                },
                actions: g.actions.iter().map(|a| a.render(Some(deck))).collect(),
                late_ms: 0.0,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use decklight_show::{CuePosition, GlobalTrigger};

    fn osc(cmd: &str) -> Vec<Action> {
        vec![Action::Osc {
            command: cmd.into(),
        }]
    }

    fn key() -> TrackKey {
        TrackKey("Song".into())
    }

    fn table_with(cues: &[(f64, &str)]) -> TriggerTable {
        let mut t = TriggerTable::default();
        for (ms, name) in cues {
            t.add_cue(&key(), Cue::at_ms(name, *ms, osc(name)));
        }
        t
    }

    /// Spielt eine Folge von (Zeit s, Position ms, läuft) auf Deck 1 ab.
    fn run(
        table: &TriggerTable,
        master: Option<u8>,
        steps: impl IntoIterator<Item = (f64, f64, bool)>,
    ) -> Vec<Firing> {
        let mut e = TriggerEngine::new();
        let k = key();
        let mut out = Vec::new();
        for (t, pos, moving) in steps {
            let d = DeckInput {
                number: 1,
                track: Some(&k),
                position_ms: Some(pos),
                moving,
                rate: 1.0,
                grid: None,
                precise: true,
            };
            out.extend(e.update(t, &[d], master, table));
        }
        out
    }

    /// Normaler Lauf von `from` bis `to` (ms) ab Zeit `t0`, 2-ms-Schritte.
    fn play(t0: f64, from: f64, to: f64) -> Vec<(f64, f64, bool)> {
        let n = ((to - from) / 2.0) as usize;
        (0..=n)
            .map(|i| (t0 + i as f64 * 0.002, from + i as f64 * 2.0, true))
            .collect()
    }

    fn names(f: &[Firing]) -> Vec<&str> {
        f.iter().map(Firing::name).collect()
    }

    #[test]
    fn fires_once_when_playhead_crosses() {
        let t = table_with(&[(1000.0, "A"), (1500.0, "B")]);
        let f = run(&t, Some(1), play(0.0, 0.0, 3000.0));
        assert_eq!(names(&f), ["A", "B"]);
        assert!(f.iter().all(|f| f.late_ms < 2.1));
        assert_eq!(f[0].deck, Some(1));
    }

    #[test]
    fn cue_at_start_fires_on_play() {
        let t = table_with(&[(0.0, "Start")]);
        let mut steps = vec![(0.0, 0.0, false), (0.5, 0.0, false)];
        steps.extend(play(1.0, 0.0, 100.0));
        assert_eq!(names(&run(&t, Some(1), steps)), ["Start"]);
    }

    #[test]
    fn pause_and_clock_correction_do_not_fire_twice() {
        let t = table_with(&[(1000.0, "A")]);
        let mut steps = play(0.0, 0.0, 1050.0);
        // Stopp: die Uhr korrigiert um 100 ms zurück, dann Pause, dann weiter.
        steps.push((1.06, 950.0, false));
        steps.push((2.0, 950.0, false));
        steps.extend(play(3.0, 950.0, 2000.0));
        assert_eq!(names(&run(&t, Some(1), steps)), ["A"]);
    }

    #[test]
    fn scrubbing_while_paused_does_not_fire() {
        let t = table_with(&[(1000.0, "A")]);
        let mut steps: Vec<_> = (0..=100)
            .map(|i| (f64::from(i) * 0.01, 900.0 + f64::from(i) * 2.0, false))
            .collect();
        steps.extend(play(2.0, 1100.0, 1500.0));
        assert!(run(&t, Some(1), steps).is_empty());
    }

    #[test]
    fn jump_back_rearms_and_fires_again() {
        // Loop 600–1100 ms: Cue bei 1000 löst in jeder Runde einmal aus.
        let t = table_with(&[(1000.0, "A")]);
        let mut steps = Vec::new();
        let mut t0 = 0.0;
        steps.extend(play(t0, 0.0, 1100.0));
        for _ in 0..3 {
            t0 = steps.last().unwrap().0 + 0.002;
            steps.extend(play(t0, 600.0, 1100.0));
        }
        assert_eq!(run(&t, Some(1), steps).len(), 4);
    }

    #[test]
    fn short_jump_back_inside_rearm_window_does_not_fire_again() {
        let t = table_with(&[(1000.0, "A")]);
        let mut steps = play(0.0, 0.0, 1100.0);
        steps.extend(play(1.2, 950.0, 1200.0));
        assert_eq!(run(&t, Some(1), steps).len(), 1);
    }

    #[test]
    fn jump_over_cue_does_not_fire_but_landing_on_it_does() {
        let t = table_with(&[(5000.0, "Über"), (20_000.0, "Ziel")]);
        let mut steps = play(0.0, 0.0, 1000.0);
        steps.extend(play(1.002, 10_000.0, 10_100.0));
        steps.extend(play(2.0, 20_010.0, 20_100.0));
        assert_eq!(names(&run(&t, Some(1), steps)), ["Ziel"]);

        let mut t2 = t.clone();
        for c in t2.tracks.values_mut().flatten() {
            c.fire_on_landing = false;
        }
        let mut steps = play(0.0, 0.0, 1000.0);
        steps.extend(play(2.0, 20_010.0, 20_100.0));
        assert!(run(&t2, Some(1), steps).is_empty());
    }

    #[test]
    fn lead_time_fires_early() {
        let mut t = table_with(&[(1000.0, "A")]);
        t.tracks
            .values_mut()
            .flatten()
            .for_each(|c| c.lead_ms = 200.0);
        let f = run(&t, Some(1), play(0.0, 0.0, 850.0));
        assert_eq!(names(&f), ["A"]);
    }

    #[test]
    fn master_scope_and_any_deck() {
        let mut t = table_with(&[(1000.0, "A")]);
        assert!(run(&t, Some(2), play(0.0, 0.0, 1100.0)).is_empty());
        assert!(run(&t, None, play(0.0, 0.0, 1100.0)).is_empty());
        t.scope = FireScope::AnyDeck;
        assert_eq!(run(&t, Some(2), play(0.0, 0.0, 1100.0)).len(), 1);
        // Eigene Einstellung schlägt die globale.
        t.tracks
            .values_mut()
            .flatten()
            .for_each(|c| c.scope = Some(FireScope::Master));
        assert!(run(&t, Some(2), play(0.0, 0.0, 1100.0)).is_empty());
    }

    #[test]
    fn disarmed_and_panic_block_and_rearming_is_not_retroactive() {
        let mut t = table_with(&[(1000.0, "A"), (2000.0, "B")]);
        let id_a = t.cues(&key())[0].id;
        t.set_armed(id_a, false);
        assert_eq!(names(&run(&t, Some(1), play(0.0, 0.0, 2100.0))), ["B"]);

        // Not-Aus mitten im Lauf, nach Cue A wieder scharf: A kommt nicht nach.
        t.set_armed(id_a, true);
        let mut e = TriggerEngine::new();
        let k = key();
        let mut fired = Vec::new();
        for (time, pos, moving) in play(0.0, 0.0, 3000.0) {
            t.armed = !(900.0..1500.0).contains(&pos);
            let d = DeckInput {
                number: 1,
                track: Some(&k),
                position_ms: Some(pos),
                moving,
                rate: 1.0,
                grid: None,
                precise: true,
            };
            fired.extend(e.update(time, &[d], Some(1), &t));
        }
        assert_eq!(names(&fired), ["B"]);
    }

    #[test]
    fn track_change_resets_state() {
        let t = table_with(&[(1000.0, "A")]);
        let mut e = TriggerEngine::new();
        let (a, other) = (key(), TrackKey("Other".into()));
        let mut n = 0;
        for (round, k) in [&a, &other, &a].into_iter().enumerate() {
            for (time, pos, moving) in play(round as f64 * 10.0, 0.0, 1100.0) {
                let d = DeckInput {
                    number: 1,
                    track: Some(k),
                    position_ms: Some(pos),
                    moving,
                    rate: 1.0,
                    grid: None,
                    precise: true,
                };
                n += e.update(time, &[d], Some(1), &t).len();
            }
        }
        assert_eq!(n, 2);
    }

    #[test]
    fn bar_cues_use_the_grid() {
        let mut t = TriggerTable::default();
        t.add_cue(
            &key(),
            Cue {
                position: CuePosition::Bar { bar: 2, beat: 1 },
                ..Cue::at_ms("Takt 2", 0.0, osc("x"))
            },
        );
        let grid = BeatGrid::uniform(120.0, 100.0, 10_000.0);
        let mut e = TriggerEngine::new();
        let k = key();
        let mut hits = Vec::new();
        for (time, pos, moving) in play(0.0, 0.0, 3000.0) {
            let d = DeckInput {
                number: 1,
                track: Some(&k),
                position_ms: Some(pos),
                moving,
                rate: 1.0,
                grid: Some(&grid),
                precise: true,
            };
            for f in e.update(time, &[d], Some(1), &t) {
                hits.push(pos - f.late_ms);
            }
        }
        assert_eq!(hits.len(), 1);
        assert!((hits[0] - 2100.0).abs() < 2.1, "{hits:?}");
    }

    #[test]
    fn global_triggers_filter_and_render() {
        let mut t = TriggerTable::default();
        let g = |name: &str, on, decks| GlobalTrigger {
            id: 0,
            name: name.into(),
            on,
            decks,
            actions: osc("Go+ Exec 10{deck}"),
            armed: true,
        };
        t.add_global(g("Laden", GlobalEvent::TrackLoaded, DeckFilter::Any));
        t.add_global(g("Master", GlobalEvent::MasterChanged, DeckFilter::Any));
        t.add_global(g("Play 2", GlobalEvent::Play, DeckFilter::Deck(2)));
        t.add_global(g("Stop M", GlobalEvent::Stop, DeckFilter::Master));
        let mut e = TriggerEngine::new();
        let play = |deck| EngineEvent::PlayStarted {
            deck,
            position_ms: 0.0,
        };
        assert!(e.on_event(&play(1), Some(1), &t).is_empty());
        let f = e.on_event(&play(2), Some(1), &t);
        assert_eq!(names(&f), ["Play 2"]);
        assert_eq!(
            f[0].actions,
            osc("Go+ Exec 102"),
            "Platzhalter {{deck}} ersetzt"
        );
        let stop = |deck| EngineEvent::PlayStopped {
            deck,
            position_ms: 0.0,
        };
        assert!(e.on_event(&stop(2), Some(1), &t).is_empty());
        assert_eq!(names(&e.on_event(&stop(1), Some(1), &t)), ["Stop M"]);
        let m = EngineEvent::MasterChanged {
            from: Some(1),
            to: Some(2),
        };
        assert_eq!(names(&e.on_event(&m, Some(2), &t)), ["Master"]);
        let none = EngineEvent::MasterChanged {
            from: Some(2),
            to: None,
        };
        assert!(e.on_event(&none, None, &t).is_empty());
        t.panic();
        assert!(e.on_event(&m, Some(2), &t).is_empty());
    }
}
