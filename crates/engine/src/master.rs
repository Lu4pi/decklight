//! Welches Deck gibt den Timecode vor?

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "mode", content = "deck", rename_all = "snake_case")]
pub enum MasterMode {
    /// Fest gewähltes Deck.
    Manual(u8),
    /// Das Deck mit Tempo-Master-Flag.
    TempoMaster,
    /// Das laufende Deck, das am Mixer on air ist. Bei mehreren bleibt das
    /// bisherige, bis sein Fader unten ist (klassischer Übergang).
    #[default]
    OnAir,
}

/// Was der Selector über ein Deck wissen muss.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeckFacts {
    pub number: u8,
    pub loaded: bool,
    pub playing: bool,
    pub on_air: bool,
    pub tempo_master: bool,
    /// Seit wann das Deck on air ist (für die Wahl unter mehreren).
    pub on_air_since: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MasterSelector {
    pub mode: MasterMode,
    /// Ein Wechsel muss so lange stabil sein, bevor er gilt (Hysterese).
    pub hold_s: f64,
    current: Option<u8>,
    pending: Option<(Option<u8>, f64)>,
}

impl Default for MasterSelector {
    fn default() -> Self {
        Self::new(MasterMode::default())
    }
}

impl MasterSelector {
    pub fn new(mode: MasterMode) -> Self {
        Self {
            mode,
            hold_s: 0.5,
            current: None,
            pending: None,
        }
    }

    pub fn current(&self) -> Option<u8> {
        self.current
    }

    pub fn set_mode(&mut self, mode: MasterMode) {
        self.mode = mode;
        self.pending = None;
    }

    fn desired(&self, decks: &[DeckFacts]) -> Option<u8> {
        let find = |n: u8| decks.iter().find(|d| d.number == n);
        match self.mode {
            MasterMode::Manual(n) => find(n).map(|d| d.number),
            MasterMode::TempoMaster => decks
                .iter()
                .find(|d| d.tempo_master && d.loaded)
                .map(|d| d.number)
                .or_else(|| self.current.filter(|&n| find(n).is_some_and(|d| d.loaded))),
            MasterMode::OnAir => {
                let live = |d: &&DeckFacts| d.loaded && d.playing && d.on_air;
                if self.current.and_then(find).as_ref().is_some_and(live) {
                    return self.current;
                }
                decks
                    .iter()
                    .filter(live)
                    .max_by(|a, b| {
                        let a_t = a.on_air_since.unwrap_or(f64::MIN);
                        let b_t = b.on_air_since.unwrap_or(f64::MIN);
                        a_t.total_cmp(&b_t).then(b.number.cmp(&a.number))
                    })
                    .map(|d| d.number)
                    // Niemand live: letztes Deck halten, solange es geladen ist.
                    .or_else(|| self.current.filter(|&n| find(n).is_some_and(|d| d.loaded)))
            }
        }
    }

    /// Neu bewerten. Liefert `(alt, neu)`, wenn sich der Master geändert hat.
    pub fn update(&mut self, t: f64, decks: &[DeckFacts]) -> Option<(Option<u8>, Option<u8>)> {
        let want = self.desired(decks);
        if want == self.current {
            self.pending = None;
            return None;
        }
        // Erster Master und manuelle Wahl gelten sofort.
        let immediate = self.current.is_none() || matches!(self.mode, MasterMode::Manual(_));
        let since = match self.pending {
            Some((p, since)) if p == want => since,
            _ => {
                self.pending = Some((want, t));
                t
            }
        };
        if immediate || t - since >= self.hold_s {
            let old = self.current;
            self.current = want;
            self.pending = None;
            return Some((old, want));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(number: u8, playing: bool, on_air: bool, since: f64) -> DeckFacts {
        DeckFacts {
            number,
            loaded: true,
            playing,
            on_air,
            tempo_master: false,
            on_air_since: on_air.then_some(since),
        }
    }

    #[test]
    fn on_air_transition_without_flicker() {
        let mut m = MasterSelector::default();
        assert_eq!(
            m.update(0.0, &[deck(1, true, true, 0.0), deck(2, false, false, 0.0)]),
            Some((None, Some(1)))
        );
        // Deck 2 startet und kommt dazu: Deck 1 bleibt Master.
        let both = [deck(1, true, true, 0.0), deck(2, true, true, 10.0)];
        assert_eq!(m.update(10.0, &both), None);
        assert_eq!(m.update(20.0, &both), None);
        // Fader 1 runter: erst nach der Haltezeit wechselt der Master.
        let only2 = [deck(1, true, false, 0.0), deck(2, true, true, 10.0)];
        assert_eq!(m.update(30.0, &only2), None);
        // Kurzes Zucken des Faders setzt die Haltezeit zurück.
        assert_eq!(m.update(30.2, &both), None);
        assert_eq!(m.update(30.3, &only2), None);
        assert_eq!(m.update(30.6, &only2), None);
        assert_eq!(m.update(30.8, &only2), Some((Some(1), Some(2))));
        // Alles aus: letzter Master bleibt.
        let none = [deck(1, false, false, 0.0), deck(2, false, false, 0.0)];
        assert_eq!(m.update(40.0, &none), None);
        assert_eq!(m.current(), Some(2));
    }

    #[test]
    fn tempo_master_and_manual() {
        let mut m = MasterSelector::new(MasterMode::TempoMaster);
        let mut d = [deck(1, true, true, 0.0), deck(2, true, true, 0.0)];
        d[1].tempo_master = true;
        assert_eq!(m.update(0.0, &d), Some((None, Some(2))));
        d[1].tempo_master = false; // Übergabe läuft: Master halten
        assert_eq!(m.update(1.0, &d), None);
        m.set_mode(MasterMode::Manual(1));
        assert_eq!(m.update(1.1, &d), Some((Some(2), Some(1))));
    }
}
