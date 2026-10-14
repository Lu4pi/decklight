//! Track-Struktur aus rekordbox-Phrasen und Drei-Band-Wellenform.
//!
//! Beide Quellen werden auf Takte des Beatgrids abgebildet. Die Phrasen geben
//! die Grenzen vor (sie liegen sauber auf Phrasenanfängen), die Wellenform
//! bestätigt sie, findet Build-ups am Ende von Breakdowns und Pausen vor dem
//! Drop. Ohne Phrasen entsteht die Struktur allein aus der Wellenform.

use decklight_library::{PhraseKind, PhraseMood, TrackData, Waveform};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Intro,
    Groove,
    Breakdown,
    Build,
    Drop,
    Outro,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Section::Intro,
        Section::Groove,
        Section::Breakdown,
        Section::Build,
        Section::Drop,
        Section::Outro,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Intro => "Intro",
            Section::Groove => "Groove",
            Section::Breakdown => "Breakdown",
            Section::Build => "Build-up",
            Section::Drop => "Drop",
            Section::Outro => "Outro",
        }
    }

    /// Volle Energie mit Bass.
    fn is_full(self) -> bool {
        matches!(self, Section::Groove | Section::Drop)
    }
}

/// Woher ein Abschnitt stammt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Phrases,
    Waveform,
    /// Phrasen und Wellenform stimmen überein.
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub section: Section,
    pub start_ms: f64,
    pub end_ms: f64,
    /// Länge in Takten.
    pub bars: u32,
    /// Sicherheit 0–100.
    pub confidence: u8,
    pub source: Source,
}

/// Fast stille Stelle (z. B. ein Beat Pause vor dem Drop).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pause {
    pub start_ms: f64,
    pub end_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Structure {
    pub segments: Vec<Segment>,
    pub pauses: Vec<Pause>,
    /// Beginn jedes Takts (Downbeats) in ms, für Phrasen-Rotation und
    /// „Drop in N Takten“.
    pub bars_ms: Vec<f64>,
    /// Phrasenanalyse von rekordbox war vorhanden.
    pub has_phrases: bool,
    /// Wellenform mit drei Bändern (CDJ-3000) statt nur Höhe.
    pub has_bands: bool,
}

impl Structure {
    /// Abschnitt an einer Position.
    pub fn segment_at(&self, pos_ms: f64) -> Option<(usize, &Segment)> {
        let i = self.segments.partition_point(|s| s.start_ms <= pos_ms);
        let i = i.checked_sub(1)?;
        let s = &self.segments[i];
        (pos_ms < s.end_ms).then_some((i, s))
    }

    /// Takte zwischen zwei Positionen (Downbeats in `(from, to]`).
    pub fn bars_between(&self, from_ms: f64, to_ms: f64) -> u32 {
        let a = self.bars_ms.partition_point(|&b| b <= from_ms);
        let b = self.bars_ms.partition_point(|&b| b <= to_ms);
        b.saturating_sub(a) as u32
    }

    /// Takte seit `start_ms` bis `pos_ms` (der Takt bei `start_ms` zählt als 0).
    pub fn bars_since(&self, start_ms: f64, pos_ms: f64) -> u32 {
        self.bars_between(start_ms + 1.0, pos_ms)
    }

    pub fn pause_at(&self, pos_ms: f64) -> Option<&Pause> {
        self.pauses
            .iter()
            .find(|p| p.start_ms <= pos_ms && pos_ms < p.end_ms)
    }
}

/// Energie je Takt: Bass, Mitten, Höhen (Mittelwert 0–31).
#[derive(Debug, Clone, Copy, Default)]
struct Energy {
    low: f64,
    mid: f64,
    high: f64,
}

impl Energy {
    fn total(self) -> f64 {
        self.low.max(self.mid).max(self.high)
    }
    fn lift(self) -> f64 {
        self.mid + 2.0 * self.high
    }
}

fn energy(w: &Waveform, from_ms: f64, to_ms: f64) -> Option<Energy> {
    let ps = w.per_second;
    let a = (from_ms.max(0.0) / 1000.0 * ps) as usize;
    let b = ((to_ms / 1000.0 * ps) as usize).min(w.columns.len());
    if b <= a {
        return None;
    }
    let n = (b - a) as f64;
    let e = if w.bands.len() == w.columns.len() {
        let s = w.bands[a..b].iter().fold([0.0; 3], |m, x| {
            [
                m[0] + f64::from(x[0]),
                m[1] + f64::from(x[1]),
                m[2] + f64::from(x[2]),
            ]
        });
        Energy {
            low: s[0] / n,
            mid: s[1] / n,
            high: s[2] / n,
        }
    } else {
        // Ohne Bänder: Höhe für alles, Helligkeit der Farbe als Höhen-Anteil.
        let (h, bright) = w.columns[a..b].iter().fold((0.0, 0.0), |m, c| {
            let br = c.rgb.iter().map(|&v| f64::from(v)).sum::<f64>() / 765.0;
            (m.0 + f64::from(c.height), m.1 + br * f64::from(c.height))
        });
        Energy {
            low: h / n,
            mid: h / n,
            high: bright / n,
        }
    };
    Some(e)
}

fn percentile(values: &[f64], p: f64) -> f64 {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| *x > 0.0).collect();
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).round() as usize]
}

/// Ab diesem Anteil am lauten Teil des Tracks gilt ein Takt als „voll“…
const FULL: f64 = 0.62;
/// …und darunter als „ohne Bass“. Dazwischen bleibt der vorige Zustand.
const WEAK: f64 = 0.38;
/// Kürzester Breakdown.
const MIN_BREAK_BARS: usize = 4;
/// Längster Build-up.
const MAX_BUILD_BARS: usize = 16;
const MIN_BUILD_BARS: usize = 4;

/// Struktur eines Tracks. `None` ohne brauchbares Beatgrid.
pub fn analyze(track: &TrackData) -> Option<Structure> {
    let grid = &track.beat_grid;
    if grid.len() < 16 {
        return None;
    }
    let beat_ms: Vec<f64> = grid.iter().map(|b| f64::from(b.time_ms)).collect();
    let last_beat = beat_ms[beat_ms.len() - 1];
    let beat_len = (last_beat - beat_ms[0]) / (beat_ms.len() - 1) as f64;
    let track_end = (f64::from(track.metadata.duration_s) * 1000.0).max(last_beat + beat_len);
    // Takte beginnen auf der „1“.
    let bar_idx: Vec<usize> = grid
        .iter()
        .enumerate()
        .filter(|(_, b)| b.beat_in_bar == 1)
        .map(|(i, _)| i)
        .collect();
    if bar_idx.len() < 4 {
        return None;
    }
    let bars_ms: Vec<f64> = bar_idx.iter().map(|&i| beat_ms[i]).collect();
    let bar_end = |k: usize| -> f64 {
        bars_ms
            .get(k + 1)
            .copied()
            .unwrap_or_else(|| (bars_ms[k] + 4.0 * beat_len).min(track_end))
    };

    let wave = track.waveform.as_ref().filter(|w| w.columns.len() > 100);
    let has_bands = wave.is_some_and(|w| w.bands.len() == w.columns.len());
    let bar_energy: Option<Vec<Energy>> = wave.map(|w| {
        (0..bars_ms.len())
            .map(|k| energy(w, bars_ms[k], bar_end(k)).unwrap_or_default())
            .collect()
    });
    let levels = bar_energy.as_ref().and_then(|e| bar_levels(e, has_bands));

    let mut segments = match &track.phrases {
        Some(p) if !p.phrases.is_empty() => {
            let segs = from_phrases(p, &beat_ms, track_end, &bars_ms);
            match (&levels, &bar_energy) {
                (Some(l), Some(e)) => verify(segs, l, e, &bars_ms, has_bands),
                _ => segs,
            }
        }
        _ => match (&levels, &bar_energy) {
            (Some(l), Some(e)) => from_waveform(l, e, &bars_ms, track_end, has_bands),
            _ => Vec::new(),
        },
    };
    for s in &mut segments {
        s.bars = (bars_ms.partition_point(|&b| b < s.end_ms - 1.0)
            - bars_ms.partition_point(|&b| b < s.start_ms - 1.0)) as u32;
    }
    let pauses = wave.map_or_else(Vec::new, |w| find_pauses(w, &beat_ms, &segments));
    Some(Structure {
        segments,
        pauses,
        bars_ms,
        has_phrases: track
            .phrases
            .as_ref()
            .is_some_and(|p| !p.phrases.is_empty()),
        has_bands,
    })
}

/// Pegel je Takt relativ zum lauten Teil des Tracks (1,0 = typisch laut).
fn bar_levels(e: &[Energy], has_bands: bool) -> Option<Vec<f64>> {
    let raw: Vec<f64> = e
        .iter()
        .map(|x| if has_bands { x.low } else { x.total() })
        .collect();
    let reference = percentile(&raw, 0.8);
    (reference >= 1.0).then(|| raw.iter().map(|v| v / reference).collect())
}

/// Takt voll (true) oder ohne Bass (false), mit Hysterese.
fn full_bars(levels: &[f64]) -> Vec<bool> {
    let mut state = levels.first().is_some_and(|&l| l >= (FULL + WEAK) / 2.0);
    levels
        .iter()
        .map(|&l| {
            if l >= FULL {
                state = true;
            } else if l < WEAK {
                state = false;
            }
            state
        })
        .collect()
}

/// Läufe gleichen Zustands: (voll, erster Takt, Takt nach dem Ende).
fn runs(full: &[bool]) -> Vec<(bool, usize, usize)> {
    let mut out: Vec<(bool, usize, usize)> = Vec::new();
    for (k, &f) in full.iter().enumerate() {
        match out.last_mut() {
            Some(r) if r.0 == f => r.2 = k + 1,
            _ => out.push((f, k, k + 1)),
        }
    }
    out
}

/// Kurze Unterbrechungen schließen: ein, zwei Takte ohne Bass mitten im Groove
/// sind ein Fill, kein Breakdown; ein einzelner voller Takt im Breakdown auch.
fn smooth(full: &mut [bool]) {
    for _ in 0..2 {
        let r = runs(full);
        for (i, &(f, a, b)) in r.iter().enumerate() {
            let inner = i > 0 && i + 1 < r.len();
            let short = if f { b - a < 2 } else { b - a < MIN_BREAK_BARS };
            if inner && short {
                full[a..b].iter_mut().for_each(|x| *x = !f);
            }
        }
    }
}

/// Takte am Ende eines Breakdowns, in denen Mitten und Höhen anziehen: ab
/// dem letzten Tiefpunkt der letzten 16 Takte, wenn es danach deutlich steigt.
fn build_tail(e: &[Energy], a: usize, b: usize) -> usize {
    if b - a < MIN_BUILD_BARS {
        return 0;
    }
    let lift: Vec<f64> = e[b - (b - a).min(MAX_BUILD_BARS)..b]
        .iter()
        .map(|x| x.lift())
        .collect();
    let n = lift.len();
    let min = lift.iter().copied().fold(f64::INFINITY, f64::min);
    let Some(start) = lift.iter().rposition(|&l| l <= min * 1.08 + 0.3) else {
        return 0;
    };
    let k = n - start;
    let peak = lift[n - 1].max(lift[n - 2]);
    if k >= MIN_BUILD_BARS && peak >= lift[start] * 1.3 + 1.0 {
        k
    } else {
        0
    }
}

/// Grenzen auf 8er-Phrasen ziehen, wenn sie höchstens einen Takt daneben liegen.
fn snap(k: usize, len: usize) -> usize {
    let r = k % 8;
    let s = if r == 1 {
        k - 1
    } else if r == 7 {
        k + 1
    } else {
        k
    };
    s.min(len)
}

fn from_waveform(
    levels: &[f64],
    e: &[Energy],
    bars_ms: &[f64],
    track_end: f64,
    has_bands: bool,
) -> Vec<Segment> {
    let mut full = full_bars(levels);
    smooth(&mut full);
    let r = runs(&full);
    let n = full.len();
    let at = |k: usize| if k >= n { track_end } else { bars_ms[k] };
    let penalty = if has_bands { 0 } else { 20 };
    let mut out = Vec::new();
    let mut prev_break = false;
    for (i, &(f, a, b)) in r.iter().enumerate() {
        let (a, b) = (
            if i == 0 { 0 } else { snap(a, n) },
            if i + 1 == r.len() { n } else { snap(b, n) },
        );
        if b <= a {
            continue;
        }
        let mean = levels[a..b].iter().sum::<f64>() / (b - a) as f64;
        let mut push = |section: Section, a: usize, b: usize, confidence: f64| {
            out.push(Segment {
                section,
                start_ms: if a == 0 { 0.0 } else { at(a) },
                end_ms: at(b),
                bars: 0,
                confidence: (confidence - f64::from(penalty)).clamp(0.0, 100.0) as u8,
                source: Source::Waveform,
            });
        };
        if f {
            let section = if prev_break {
                Section::Drop
            } else {
                Section::Groove
            };
            push(
                section,
                a,
                b,
                60.0 + 30.0 * (1.0 - 1.0 / mean.max(1.0)) + 10.0,
            );
            prev_break = false;
        } else if i == 0 {
            push(Section::Intro, a, b, 70.0);
        } else if i + 1 == r.len() {
            push(Section::Outro, a, b, 70.0);
        } else {
            let contrast = (1.0 - mean).clamp(0.0, 1.0);
            let conf = 50.0 + 45.0 * contrast;
            let tail = if has_bands { build_tail(e, a, b) } else { 0 };
            if tail > 0 && tail < b - a {
                push(Section::Breakdown, a, b - tail, conf);
                push(Section::Build, b - tail, b, conf - 5.0);
            } else if tail > 0 {
                push(Section::Build, a, b, conf - 5.0);
            } else {
                push(Section::Breakdown, a, b, conf);
            }
            prev_break = true;
        }
    }
    out
}

fn from_phrases(
    p: &decklight_library::PhraseAnalysis,
    beat_ms: &[f64],
    track_end: f64,
    bars_ms: &[f64],
) -> Vec<Segment> {
    let at_beat = |beat: u32| -> f64 {
        let i = (beat.max(1) - 1) as usize;
        beat_ms.get(i).copied().unwrap_or(track_end)
    };
    let mut out: Vec<Segment> = Vec::new();
    for (i, ph) in p.phrases.iter().enumerate() {
        let start = at_beat(ph.beat);
        let end = p
            .phrases
            .get(i + 1)
            .map_or_else(|| at_beat(p.end_beat).max(start), |n| at_beat(n.beat));
        let prev = out.last().map(|s| s.section);
        let section = match (p.mood, ph.kind) {
            (_, PhraseKind::Intro) => Section::Intro,
            (_, PhraseKind::Outro) => Section::Outro,
            (_, PhraseKind::Up) => Section::Build,
            (_, PhraseKind::Down | PhraseKind::Bridge) => Section::Breakdown,
            (_, PhraseKind::Chorus) => match prev {
                Some(Section::Groove) if p.mood == PhraseMood::High => Section::Groove,
                Some(Section::Intro) if p.mood == PhraseMood::High => Section::Groove,
                _ => Section::Drop,
            },
            (_, PhraseKind::Verse(_) | PhraseKind::Unknown(_)) => Section::Groove,
        };
        if end <= start {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.section == section => last.end_ms = end,
            _ => out.push(Segment {
                section,
                start_ms: if out.is_empty() { 0.0 } else { start },
                end_ms: end,
                bars: 0,
                confidence: 75,
                source: Source::Phrases,
            }),
        }
    }
    // Nach der letzten Phrase: Rest des Tracks als Outro.
    if let Some(last) = out.last_mut() {
        if last.section == Section::Outro || track_end - last.end_ms < 4.0 * 60_000.0 / 120.0 {
            last.end_ms = track_end;
        } else {
            let start = last.end_ms;
            out.push(Segment {
                section: Section::Outro,
                start_ms: start,
                end_ms: track_end,
                bars: 0,
                confidence: 60,
                source: Source::Phrases,
            });
        }
    }
    let _ = bars_ms;
    out
}

/// Phrasen-Abschnitte mit der Wellenform abgleichen und Build-ups abtrennen.
fn verify(
    segs: Vec<Segment>,
    levels: &[f64],
    e: &[Energy],
    bars_ms: &[f64],
    has_bands: bool,
) -> Vec<Segment> {
    let bar_of = |ms: f64| bars_ms.partition_point(|&b| b < ms - 1.0).min(levels.len());
    let mut full = full_bars(levels);
    smooth(&mut full);
    let mut out: Vec<Segment> = Vec::new();
    let count = segs.len();
    for (i, mut s) in segs.into_iter().enumerate() {
        let (a, b) = (bar_of(s.start_ms), bar_of(s.end_ms));
        if b <= a {
            out.push(s);
            continue;
        }
        let frac = full[a..b].iter().filter(|f| **f).count() as f64 / (b - a) as f64;
        let agrees = match s.section {
            Section::Intro | Section::Outro => true,
            sec if sec.is_full() => frac >= 0.6,
            _ => frac <= 0.4,
        };
        s.confidence = if agrees { 92 } else { 55 };
        if agrees {
            s.source = Source::Both;
        }
        // Breakdown direkt vor einem Drop: Build-up am Ende abtrennen, wenn
        // die Wellenform einen zeigt und rekordbox keinen eigenen hat.
        let next_is_drop = i + 1 < count;
        if s.section == Section::Breakdown && has_bands && next_is_drop && agrees {
            let tail = build_tail(e, a, b);
            if tail > 0 && tail < b - a {
                let split = bars_ms[b - tail];
                let mut build = s.clone();
                build.section = Section::Build;
                build.start_ms = split;
                build.confidence = 80;
                build.source = Source::Waveform;
                s.end_ms = split;
                out.push(s);
                out.push(build);
                continue;
            }
        }
        out.push(s);
    }
    // Ein Drop nach einem Build gilt immer als Drop, auch wenn rekordbox
    // „Chorus“ nach „Verse“ meinte.
    out
}

/// Fast stille Beats nach lauter Musik (Pause vor dem Drop, harter Stopp im Track).
fn find_pauses(w: &Waveform, beat_ms: &[f64], segments: &[Segment]) -> Vec<Pause> {
    let totals: Vec<f64> = beat_ms
        .windows(2)
        .map(|b| energy(w, b[0], b[1]).map_or(0.0, Energy::total))
        .collect();
    let reference = percentile(&totals, 0.8);
    if reference < 1.0 {
        return Vec::new();
    }
    let quiet = |i: usize| totals[i] < reference * 0.12;
    let in_music = |ms: f64| {
        segments.is_empty()
            || segments.iter().any(|s| {
                s.start_ms <= ms
                    && ms < s.end_ms
                    && !matches!(s.section, Section::Intro | Section::Outro)
            })
    };
    let mut out = Vec::new();
    let mut i = 1;
    while i < totals.len() {
        if quiet(i) && totals[i - 1] >= reference * 0.3 {
            let mut j = i;
            while j < totals.len() && quiet(j) {
                j += 1;
            }
            // Höchstens zwei Takte; länger ist ein Ende oder ein Break.
            if j - i <= 8 && j < totals.len() && in_music(beat_ms[i]) {
                out.push(Pause {
                    start_ms: beat_ms[i],
                    end_ms: beat_ms[j],
                });
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use decklight_library::{GridBeat, Phrase, PhraseAnalysis, WaveColumn};

    /// 128 BPM, `bars` Takte; `level(bar)` liefert (Bass, Mitten, Höhen) 0–31.
    pub(crate) fn track(bars: u32, level: impl Fn(u32, f64) -> [u8; 3]) -> TrackData {
        let beat = 60_000.0 / 128.0;
        let grid: Vec<GridBeat> = (0..bars * 4)
            .map(|i| GridBeat {
                time_ms: (f64::from(i) * beat).round() as u32,
                beat_in_bar: (i % 4) as u8 + 1,
                bpm: 128.0,
            })
            .collect();
        let len_ms = f64::from(bars * 4) * beat;
        let n = (len_ms / 1000.0 * 150.0) as usize;
        let bands: Vec<[u8; 3]> = (0..n)
            .map(|c| {
                let ms = c as f64 * 1000.0 / 150.0;
                let pos = ms / (beat * 4.0);
                level(pos.floor() as u32, pos.fract())
            })
            .collect();
        let mut t = TrackData::default();
        t.metadata.duration_s = (len_ms / 1000.0).ceil() as u32;
        t.beat_grid = grid;
        t.waveform = Some(Waveform {
            per_second: 150.0,
            columns: bands.iter().map(|b| WaveColumn::from_bands(*b)).collect(),
            bands,
        });
        t
    }

    /// Intro 16 · Groove 32 · Breakdown 16 + Build 8 (Pause im letzten Beat) · Drop 32 · Outro 16.
    pub(crate) fn edm(bar: u32, frac: f64) -> [u8; 3] {
        match bar {
            0..16 => [6, 10, 8],
            16..48 => [26, 14, 10],
            48..64 => [3, 14, 4],
            64..72 => {
                if bar == 71 && frac >= 0.75 {
                    [0, 0, 0]
                } else {
                    let p = f64::from(bar - 64) / 8.0;
                    [3, (12.0 + 10.0 * p) as u8, (4.0 + 22.0 * p) as u8]
                }
            }
            72..104 => [28, 16, 12],
            _ => [5, 8, 6],
        }
    }

    fn sections(s: &Structure) -> Vec<(Section, u32)> {
        s.segments.iter().map(|x| (x.section, x.bars)).collect()
    }

    #[test]
    fn waveform_only_finds_break_build_drop_and_pause() {
        let s = analyze(&track(120, edm)).unwrap();
        assert_eq!(
            sections(&s),
            vec![
                (Section::Intro, 16),
                (Section::Groove, 32),
                (Section::Breakdown, 16),
                (Section::Build, 8),
                (Section::Drop, 32),
                (Section::Outro, 16),
            ]
        );
        let drop = &s.segments[4];
        assert!((drop.start_ms - 72.0 * 4.0 * 60_000.0 / 128.0).abs() < 2.0);
        assert!(drop.confidence >= 70, "{}", drop.confidence);
        assert_eq!(s.pauses.len(), 1);
        let p = s.pauses[0];
        assert!((p.end_ms - drop.start_ms).abs() < 2.0, "{p:?}");
    }

    #[test]
    fn short_fill_is_not_a_breakdown() {
        let s = analyze(&track(64, |bar, _| match bar {
            0..8 => [6, 10, 8],
            31 => [4, 14, 10],
            _ => [26, 14, 10],
        }))
        .unwrap();
        assert!(!s.segments.iter().any(|x| x.section == Section::Breakdown));
        assert!(!s.segments.iter().any(|x| x.section == Section::Drop));
    }

    #[test]
    fn phrases_lead_and_waveform_confirms() {
        let mut t = track(120, edm);
        let ph = |bar: u32, kind| Phrase {
            beat: bar * 4 + 1,
            kind,
            fill_beat: None,
        };
        t.phrases = Some(PhraseAnalysis {
            mood: PhraseMood::High,
            end_beat: 480,
            phrases: vec![
                ph(0, PhraseKind::Intro),
                ph(16, PhraseKind::Chorus),
                ph(48, PhraseKind::Down),
                ph(72, PhraseKind::Chorus),
                ph(104, PhraseKind::Outro),
            ],
        });
        let s = analyze(&t).unwrap();
        assert_eq!(
            sections(&s),
            vec![
                (Section::Intro, 16),
                (Section::Groove, 32),
                (Section::Breakdown, 16),
                (Section::Build, 8),
                (Section::Drop, 32),
                (Section::Outro, 16),
            ]
        );
        assert_eq!(s.segments[4].source, Source::Both);
        assert!(s.segments[4].confidence >= 90);
        assert!(s.has_phrases && s.has_bands);
    }

    #[test]
    fn phrase_disagreeing_with_waveform_loses_confidence() {
        let mut t = track(64, |_, _| [26, 14, 10]);
        t.phrases = Some(PhraseAnalysis {
            mood: PhraseMood::High,
            end_beat: 256,
            phrases: vec![
                Phrase {
                    beat: 1,
                    kind: PhraseKind::Chorus,
                    fill_beat: None,
                },
                Phrase {
                    beat: 129,
                    kind: PhraseKind::Down,
                    fill_beat: None,
                },
            ],
        });
        let s = analyze(&t).unwrap();
        let down = s
            .segments
            .iter()
            .find(|x| x.section == Section::Breakdown)
            .unwrap();
        assert!(down.confidence < 60);
    }

    #[test]
    fn helpers() {
        let s = analyze(&track(120, edm)).unwrap();
        let bar = 4.0 * 60_000.0 / 128.0;
        assert_eq!(s.segment_at(70.5 * bar).unwrap().1.section, Section::Build);
        assert_eq!(s.bars_since(72.0 * bar, 80.2 * bar), 8);
        assert_eq!(s.bars_between(64.5 * bar, 72.0 * bar), 8);
        assert!(s.pause_at(71.8 * bar).is_some());
        assert!(analyze(&TrackData::default()).is_none());
    }
}
