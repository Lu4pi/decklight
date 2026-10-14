//! Auto-Pilot: verfolgt das führende Deck und meldet, wann welcher Look dran
//! ist. Reine Logik ohne Ein-/Ausgabe; der Trigger-Thread ruft [`Pilot::update`]
//! alle paar Millisekunden auf und macht aus den Meldungen MA3-Befehle.
//!
//! Führend ist das Deck, das On Air ist und läuft (bei mehreren der
//! Tempo-Master). Wird im Mix ein anderes Deck führend, übernimmt Auto erst auf
//! dessen nächster „1“, ohne Drop-Hit.

use crate::config::{AutoConfig, ManualCues};
use crate::structure::{Section, Source, Structure};
use serde::Serialize;
use std::collections::BTreeMap;

/// Was der Auto-Pilot auslöst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum AutoCue {
    /// Neuer Abschnitt oder nächster Look im selben Abschnitt.
    Section(Section),
    /// Eins des Drops.
    Hit,
    /// Stille im Track.
    Pause,
    /// Nichts läuft mehr.
    Stop,
    /// Auto aus: Licht zurück an den Operator.
    Release,
    /// Build-up-Fortschritt 0–100.
    BuildLevel(u8),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AutoFiring {
    pub deck: Option<u8>,
    pub cue: AutoCue,
    /// Zeile bei Abschnitten mit mehreren Looks.
    pub rotation: usize,
    pub bpm: Option<f64>,
    /// Für das Log, z. B. „Drop (Phrasen + Wellenform, 92 %)“.
    pub reason: String,
}

/// Ein Deck zum Auswertezeitpunkt.
#[derive(Debug, Clone, Copy)]
pub struct DeckView<'a> {
    pub number: u8,
    pub moving: bool,
    pub on_air: bool,
    pub position_ms: Option<f64>,
    pub rate: f64,
    pub bpm: Option<f64>,
    pub structure: Option<&'a Structure>,
    /// Track hat eigene Cues in DeckLight.
    pub has_manual_cues: bool,
}

/// Nächster Abschnitt, z. B. „Drop in 4 Takten“.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Upcoming {
    pub section: Section,
    pub bars: u32,
}

/// Für die Anzeige.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct AutoStatus {
    pub active: bool,
    /// Schattenmodus: nur anzeigen, nichts senden.
    pub shadow: bool,
    pub deck: Option<u8>,
    pub section: Option<Section>,
    pub confidence: u8,
    pub source: Option<Source>,
    pub next: Option<Upcoming>,
    pub build_pct: Option<u8>,
    /// Warum Auto gerade nichts tut (leer, wenn alles läuft).
    pub note: String,
}

/// Mindestabstand zwischen zwei Stopp-Erkennungen, in Sekunden.
const STOP_AFTER_S: f64 = 0.5;
/// Ab so viel Abweichung gilt eine Positionsänderung als Sprung.
const JUMP_MS: f64 = 400.0;

#[derive(Debug, Default)]
pub struct Pilot {
    active: bool,
    leader: Option<u8>,
    /// Neues führendes Deck, wartet auf seine nächste „1“.
    pending: Option<u8>,
    /// Zuletzt ausgelöster Abschnitt (`None`: beim nächsten Mal neu senden).
    state: Option<Section>,
    /// Kein Hit beim nächsten Abschnitt (nach Einschalten, Deckwechsel, Sprung).
    quiet: bool,
    stopped: bool,
    idle_since: Option<f64>,
    last_change: f64,
    last_hit: Option<f64>,
    /// Letzte Position je Deck: (Zeit, Position).
    last: BTreeMap<u8, (f64, f64)>,
    seg_start: f64,
    rot_block: u32,
    rotation: [usize; 6],
    build_sent: Option<u8>,
    build_sent_at: f64,
    pause_fired: Option<f64>,
    status: AutoStatus,
}

fn index(s: Section) -> usize {
    Section::ALL.iter().position(|x| *x == s).unwrap_or(0)
}

fn source_label(s: Source) -> &'static str {
    match s {
        Source::Phrases => "Phrasen",
        Source::Waveform => "Wellenform",
        Source::Both => "Phrasen + Wellenform",
    }
}

impl Pilot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn status(&self) -> &AutoStatus {
        &self.status
    }

    /// Ein- oder ausschalten. Beim Ausschalten kommt die Übergabe an den
    /// Operator zurück (Auto-Sequenz aus, Build-Master auf 0).
    pub fn set_active(&mut self, on: bool) -> Vec<AutoFiring> {
        if on == self.active {
            return Vec::new();
        }
        self.active = on;
        self.status.active = on;
        self.reset();
        if on {
            return Vec::new();
        }
        self.leader = None;
        self.pending = None;
        let mut out = vec![AutoFiring {
            deck: None,
            cue: AutoCue::Release,
            rotation: 0,
            bpm: None,
            reason: "Auto aus".into(),
        }];
        if self.build_sent.is_some_and(|p| p > 0) {
            out.push(AutoFiring {
                deck: None,
                cue: AutoCue::BuildLevel(0),
                rotation: 0,
                bpm: None,
                reason: "Auto aus".into(),
            });
        }
        self.build_sent = None;
        self.status = AutoStatus::default();
        out
    }

    /// Nach Not-Aus oder geänderten Einstellungen: den aktuellen Abschnitt beim
    /// nächsten Mal neu senden, ohne Hit.
    pub fn reset(&mut self) {
        self.state = None;
        self.quiet = true;
        self.stopped = false;
        self.idle_since = None;
        self.pause_fired = None;
    }

    pub fn update(
        &mut self,
        t: f64,
        decks: &[DeckView<'_>],
        master: Option<u8>,
        cfg: &AutoConfig,
    ) -> Vec<AutoFiring> {
        let mut out = Vec::new();
        if !self.active {
            self.remember(t, decks);
            return out;
        }
        let any_on_air = decks.iter().any(|d| d.on_air);
        let candidates: Vec<&DeckView<'_>> = decks
            .iter()
            .filter(|d| d.moving && d.position_ms.is_some() && (d.on_air || !any_on_air))
            .collect();
        let is_candidate =
            |n: Option<u8>| n.is_some_and(|n| candidates.iter().any(|d| d.number == n));

        if candidates.is_empty() {
            let since = *self.idle_since.get_or_insert(t);
            if !self.stopped && self.leader.is_some() && t - since >= STOP_AFTER_S {
                self.stopped = true;
                self.state = None;
                self.quiet = true;
                out.push(self.firing(
                    self.leader,
                    AutoCue::Stop,
                    None,
                    "Stopp: kein Deck läuft".into(),
                ));
                self.release_build(self.leader, &mut out);
            }
            self.status.deck = None;
            self.status.section = None;
            self.status.next = None;
            self.status.build_pct = None;
            self.status.note = "Kein Deck läuft On Air".into();
            self.remember(t, decks);
            return out;
        }
        self.idle_since = None;
        if self.stopped {
            self.stopped = false;
            self.leader = None;
        }

        let wanted = if is_candidate(master) {
            master
        } else if is_candidate(self.leader) {
            self.leader
        } else {
            candidates.first().map(|d| d.number)
        };
        if wanted != self.leader {
            if self.leader.is_none() || !is_candidate(self.leader) {
                self.switch(wanted);
            } else {
                self.pending = wanted;
            }
        } else {
            self.pending = None;
        }
        // Übergabe im Mix auf der nächsten „1“ des neuen Decks.
        if let Some(p) = self.pending {
            if let Some(d) = decks.iter().find(|d| d.number == p) {
                let pos = d.position_ms.unwrap_or(0.0);
                let crossed = match (d.structure, self.last.get(&p)) {
                    (Some(s), Some(&(_, before))) => {
                        pos >= before && s.bars_between(before, pos) > 0
                    }
                    (None, _) => true,
                    _ => false,
                };
                if crossed {
                    self.switch(Some(p));
                }
            }
        }
        let Some(d) = decks
            .iter()
            .find(|d| Some(d.number) == self.leader)
            .copied()
        else {
            self.remember(t, decks);
            return out;
        };
        self.evaluate(t, &d, cfg, &mut out);
        self.remember(t, decks);
        out
    }

    fn switch(&mut self, to: Option<u8>) {
        self.leader = to;
        self.pending = None;
        self.quiet = true;
        self.rot_block = 0;
        self.pause_fired = None;
    }

    fn remember(&mut self, t: f64, decks: &[DeckView<'_>]) {
        for d in decks {
            match d.position_ms {
                Some(p) => {
                    self.last.insert(d.number, (t, p));
                }
                None => {
                    self.last.remove(&d.number);
                }
            }
        }
    }

    fn firing(
        &self,
        deck: Option<u8>,
        cue: AutoCue,
        bpm: Option<f64>,
        reason: String,
    ) -> AutoFiring {
        let rotation = match cue {
            AutoCue::Section(s) => self.rotation[index(s)],
            _ => 0,
        };
        AutoFiring {
            deck,
            cue,
            rotation,
            bpm,
            reason,
        }
    }

    fn release_build(&mut self, deck: Option<u8>, out: &mut Vec<AutoFiring>) {
        if self.build_sent.is_some_and(|p| p > 0) {
            out.push(self.firing(deck, AutoCue::BuildLevel(0), None, "Build-up vorbei".into()));
            self.build_sent = Some(0);
        }
    }

    fn evaluate(&mut self, t: f64, d: &DeckView<'_>, cfg: &AutoConfig, out: &mut Vec<AutoFiring>) {
        let deck = Some(d.number);
        let raw = d.position_ms.unwrap_or(0.0);
        let rate = if d.rate.abs() < 0.01 {
            1.0
        } else {
            d.rate.abs()
        };
        let pos = raw + cfg.lead_ms * rate;
        let jumped = self
            .last
            .get(&d.number)
            .is_some_and(|&(t0, p0)| (raw - (p0 + (t - t0) * 1000.0 * d.rate)).abs() > JUMP_MS);
        if jumped {
            self.quiet = true;
            self.pause_fired = None;
        }
        let bpm = d.bpm.filter(|b| *b > 20.0).unwrap_or(120.0);
        let bar_s = 240.0 / bpm;
        self.status.deck = deck;

        if cfg.manual_cues == ManualCues::Yield && d.has_manual_cues {
            self.state = None;
            self.status.section = None;
            self.status.next = None;
            self.status.note = format!("Deck {} hat eigene Cues", d.number);
            self.release_build(deck, out);
            return;
        }
        let structure = d.structure.filter(|s| !s.segments.is_empty());
        let seg = structure.and_then(|s| s.segment_at(pos));
        let (section, confidence, source, seg_start) = match seg {
            Some((_, s)) if s.confidence >= cfg.min_confidence => {
                (s.section, s.confidence, Some(s.source), s.start_ms)
            }
            Some((_, s)) => (Section::Groove, s.confidence, Some(s.source), s.start_ms),
            None => (Section::Groove, 0, None, self.seg_start),
        };
        self.status.note = if structure.is_none() {
            "Keine Struktur: Groove mit Look-Wechsel nach Takten".into()
        } else {
            String::new()
        };
        self.status.section = Some(section);
        self.status.confidence = confidence;
        self.status.source = source;
        self.status.next = structure.and_then(|s| {
            let (i, _) = seg?;
            let next = s.segments.get(i + 1)?;
            Some(Upcoming {
                section: next.section,
                bars: s.bars_between(pos, next.start_ms),
            })
        });

        // Pause im Track: Blackout, danach den Abschnitt neu senden.
        if let Some(p) = structure.and_then(|s| s.pause_at(pos)) {
            if self.pause_fired != Some(p.start_ms) && !jumped {
                self.pause_fired = Some(p.start_ms);
                self.state = None;
                out.push(self.firing(deck, AutoCue::Pause, Some(bpm), "Pause im Track".into()));
                self.release_build(deck, out);
            }
            self.status.build_pct = None;
            return;
        }

        if self.state != Some(section) {
            let early = self.state.is_some()
                && t - self.last_change < bar_s * 0.9
                && section != Section::Drop
                && !self.quiet;
            if !early {
                let why = match source {
                    Some(src) => format!(
                        "{} ({}, {confidence} %)",
                        section.label(),
                        source_label(src)
                    ),
                    None => section.label().to_string(),
                };
                if self.state == Some(Section::Build) {
                    self.release_build(deck, out);
                }
                out.push(self.firing(deck, AutoCue::Section(section), Some(bpm), why));
                self.rotation[index(section)] += 1;
                let on_the_one = seg.is_some_and(|(_, s)| pos - s.start_ms < 60_000.0 / bpm);
                let hit_due = self
                    .last_hit
                    .is_none_or(|h| t - h >= f64::from(cfg.hit_every_bars) * bar_s * 0.95);
                if section == Section::Drop && !self.quiet && on_the_one && hit_due {
                    out.push(self.firing(deck, AutoCue::Hit, Some(bpm), "Drop-Hit".into()));
                    self.last_hit = Some(t);
                }
                self.state = Some(section);
                self.last_change = t;
                self.seg_start = if seg.is_some() { seg_start } else { pos };
                self.rot_block = 0;
                self.quiet = false;
            }
        } else if cfg.rotate_bars > 0 && matches!(section, Section::Groove | Section::Drop) {
            let bars = match structure {
                Some(s) => s.bars_since(self.seg_start, pos),
                None => ((pos - self.seg_start).max(0.0) / (bar_s * 1000.0 * rate)) as u32,
            };
            let block = bars / cfg.rotate_bars;
            if block > self.rot_block && !jumped {
                out.push(self.firing(
                    deck,
                    AutoCue::Section(section),
                    Some(bpm),
                    format!(
                        "{}: nächster Look nach {} Takten",
                        section.label(),
                        cfg.rotate_bars
                    ),
                ));
                self.rotation[index(section)] += 1;
            }
            self.rot_block = block;
            self.quiet = false;
        } else {
            self.quiet = false;
        }

        // Build-up stufenlos.
        match (section, seg) {
            (Section::Build, Some((_, s))) if self.state == Some(Section::Build) => {
                let span = (s.end_ms - s.start_ms).max(1.0);
                let pct = (((pos - s.start_ms) / span) * 100.0)
                    .clamp(0.0, 100.0)
                    .round() as u8;
                self.status.build_pct = Some(pct);
                let changed = self
                    .build_sent
                    .is_none_or(|p| p.abs_diff(pct) >= 2 || (pct == 100 && p != 100));
                if changed && t - self.build_sent_at >= 0.05 {
                    out.push(self.firing(
                        deck,
                        AutoCue::BuildLevel(pct),
                        Some(bpm),
                        format!("Build-up {pct} %"),
                    ));
                    self.build_sent = Some(pct);
                    self.build_sent_at = t;
                }
            }
            _ => {
                self.status.build_pct = None;
                if self.state != Some(Section::Build) {
                    self.release_build(deck, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::analyze;
    use crate::structure::tests::{edm, track};

    const BAR_MS: f64 = 4.0 * 60_000.0 / 128.0;

    fn deck<'a>(n: u8, pos: f64, s: &'a Structure) -> DeckView<'a> {
        DeckView {
            number: n,
            moving: true,
            on_air: true,
            position_ms: Some(pos),
            rate: 1.0,
            bpm: Some(128.0),
            structure: Some(s),
            has_manual_cues: false,
        }
    }

    fn cfg() -> AutoConfig {
        AutoConfig {
            lead_ms: 0.0,
            ..AutoConfig::default()
        }
    }

    /// Spielt `from..to` (Takte) mit 10-ms-Schritten ab.
    fn play(
        p: &mut Pilot,
        s: &Structure,
        from: f64,
        to: f64,
        t0: f64,
    ) -> (Vec<(f64, AutoCue)>, f64) {
        let mut out = Vec::new();
        let mut t = t0;
        let mut pos = from * BAR_MS;
        while pos < to * BAR_MS {
            for f in p.update(t, &[deck(1, pos, s)], Some(1), &cfg()) {
                out.push((pos / BAR_MS, f.cue));
            }
            t += 0.01;
            pos += 10.0;
        }
        (out, t)
    }

    fn sections(f: &[(f64, AutoCue)]) -> Vec<(u32, AutoCue)> {
        f.iter()
            .filter(|(_, c)| !matches!(c, AutoCue::BuildLevel(_)))
            .map(|(bar, c)| (bar.round() as u32, *c))
            .collect()
    }

    #[test]
    fn full_track_fires_on_time() {
        let s = analyze(&track(120, edm)).unwrap();
        let mut p = Pilot::new();
        p.set_active(true);
        let (f, t) = play(&mut p, &s, 0.0, 119.0, 0.0);
        use AutoCue::*;
        assert_eq!(
            sections(&f),
            vec![
                (0, Section(crate::Section::Intro)),
                (16, Section(crate::Section::Groove)),
                (32, Section(crate::Section::Groove)),
                (48, Section(crate::Section::Breakdown)),
                (64, Section(crate::Section::Build)),
                (72, Pause),
                (72, Section(crate::Section::Drop)),
                (72, Hit),
                (88, Section(crate::Section::Drop)),
                (104, Section(crate::Section::Outro)),
            ]
        );
        // Drop genau auf der Eins (10-ms-Raster).
        let drop = f.iter().find(|(_, c)| *c == Hit).unwrap().0;
        assert!((drop - 72.0).abs() * BAR_MS <= 10.0, "{drop}");
        // Build-up steigt und geht vor dem Drop zurück auf 0.
        let levels: Vec<u8> = f
            .iter()
            .filter_map(|(_, c)| match c {
                BuildLevel(v) => Some(*v),
                _ => None,
            })
            .collect();
        assert!(levels.len() > 20);
        assert!(levels.iter().any(|v| *v >= 95));
        assert_eq!(levels.last(), Some(&0));
        // Stopp: nach einer halben Sekunde ohne laufendes Deck.
        let mut stopped = Vec::new();
        for i in 0..100 {
            let mut d = deck(1, 119.0 * BAR_MS, &s);
            d.moving = false;
            stopped.extend(p.update(t + f64::from(i) * 0.01, &[d], Some(1), &cfg()));
        }
        assert_eq!(stopped.iter().filter(|f| f.cue == Stop).count(), 1);
        // Ausschalten gibt das Licht zurück.
        assert_eq!(p.set_active(false)[0].cue, Release);
    }

    #[test]
    fn rotation_picks_next_look() {
        let s = analyze(&track(120, edm)).unwrap();
        let mut p = Pilot::new();
        p.set_active(true);
        let mut rot = Vec::new();
        let mut t = 0.0;
        let mut pos = 16.0 * BAR_MS;
        while pos < 48.0 * BAR_MS {
            for f in p.update(t, &[deck(1, pos, &s)], Some(1), &cfg()) {
                rot.push(f.rotation);
            }
            t += 0.01;
            pos += 10.0;
        }
        assert_eq!(rot, vec![0, 1]);
    }

    #[test]
    fn jump_into_drop_has_no_hit() {
        let s = analyze(&track(120, edm)).unwrap();
        let mut p = Pilot::new();
        p.set_active(true);
        let (_, t) = play(&mut p, &s, 50.0, 52.0, 0.0);
        let f = p.update(t, &[deck(1, 72.0 * BAR_MS + 5.0, &s)], Some(1), &cfg());
        assert!(f.iter().any(|f| f.cue == AutoCue::Section(Section::Drop)));
        assert!(!f.iter().any(|f| f.cue == AutoCue::Hit));
    }

    #[test]
    fn mix_hands_over_on_the_next_one() {
        let s = analyze(&track(120, edm)).unwrap();
        let mut p = Pilot::new();
        p.set_active(true);
        let c = cfg();
        // Deck 1 im Drop, Deck 2 im Intro; Master wechselt zu Deck 2.
        let mut t = 0.0;
        let (mut a, mut b) = (80.0 * BAR_MS, 0.5 * BAR_MS);
        let mut log = Vec::new();
        for i in 0..400 {
            let master = if i < 50 { 1 } else { 2 };
            for f in p.update(t, &[deck(1, a, &s), deck(2, b, &s)], Some(master), &c) {
                log.push((f.deck, f.cue, b / BAR_MS));
            }
            t += 0.01;
            a += 10.0;
            b += 10.0;
        }
        let intro = log
            .iter()
            .find(|(d, c, _)| *d == Some(2) && *c == AutoCue::Section(Section::Intro))
            .unwrap();
        // Übergabe auf der nächsten „1“ von Deck 2 (Takt 1), nicht sofort.
        assert!((intro.2 - 1.0).abs() < 0.01, "{intro:?}");
        assert!(!log.iter().any(|(_, c, _)| *c == AutoCue::Hit));
    }

    #[test]
    fn off_air_deck_is_not_followed() {
        let s = analyze(&track(120, edm)).unwrap();
        let mut p = Pilot::new();
        p.set_active(true);
        let mut cue = deck(2, 80.0 * BAR_MS, &s);
        cue.on_air = false;
        let live = deck(1, 20.0 * BAR_MS, &s);
        let f = p.update(0.0, &[live, cue], Some(2), &cfg());
        assert_eq!(f[0].deck, Some(1));
        assert_eq!(f[0].cue, AutoCue::Section(Section::Groove));
    }

    #[test]
    fn yields_to_manual_cues() {
        let s = analyze(&track(120, edm)).unwrap();
        let mut p = Pilot::new();
        p.set_active(true);
        let mut d = deck(1, 80.0 * BAR_MS, &s);
        d.has_manual_cues = true;
        let c = AutoConfig {
            manual_cues: ManualCues::Yield,
            ..cfg()
        };
        assert!(p.update(0.0, &[d], Some(1), &c).is_empty());
        assert!(!p.status().note.is_empty());
    }

    #[test]
    fn inactive_pilot_is_silent() {
        let s = analyze(&track(120, edm)).unwrap();
        let mut p = Pilot::new();
        assert!(p
            .update(0.0, &[deck(1, 0.0, &s)], Some(1), &cfg())
            .is_empty());
        assert!(p.set_active(false).is_empty());
    }
}
