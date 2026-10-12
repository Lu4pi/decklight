//! Rekordbox-Cues als Vorlagen und Auflösung von Takt/Schlag in Millisekunden.

use decklight_engine::BeatGrid;
use decklight_library::{CueKind, CuePoint};
use decklight_show::{Cue, CuePosition};

/// Position eines Cues in ms. Takt/Schlag braucht ein Beatgrid.
pub fn cue_position_ms(pos: &CuePosition, grid: Option<&BeatGrid>) -> Option<f64> {
    match *pos {
        CuePosition::Ms { ms } => Some(ms),
        CuePosition::Bar { bar, beat } => {
            let beats = &grid?.beats;
            let first_down = beats.iter().position(|b| b.beat_in_bar == 1)?;
            let idx = usize::try_from(bar.checked_sub(1)?)
                .ok()?
                .checked_mul(4)?
                .checked_add(usize::from(beat.checked_sub(1)?))?
                .checked_add(first_down)?;
            beats.get(idx).map(|b| b.time_ms)
        }
    }
}

/// Hot Cues und Memory Cues aus Rekordbox als Cues ohne Aktionen.
/// Die Aktionen trägt der Lichtoperator später im Cue-Editor ein.
pub fn hot_cue_templates(points: &[CuePoint]) -> Vec<Cue> {
    let mut memory = 0;
    let mut out: Vec<Cue> = points
        .iter()
        .map(|p| {
            let base = match p.kind {
                CueKind::Hot(n) => {
                    let letter = char::from(b'A' + n.saturating_sub(1).min(25));
                    format!("Hot Cue {letter}")
                }
                CueKind::Memory => {
                    memory += 1;
                    format!("Memory {memory}")
                }
            };
            let name = if p.comment.is_empty() {
                base
            } else {
                format!("{base}: {}", p.comment)
            };
            let mut cue = Cue::at_ms(&name, f64::from(p.position_ms), Vec::new());
            cue.color = p.color;
            cue
        })
        .collect();
    out.sort_by(|a, b| {
        let pos = |c: &Cue| match c.position {
            CuePosition::Ms { ms } => ms,
            CuePosition::Bar { .. } => 0.0,
        };
        pos(a).total_cmp(&pos(b))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_and_beat_from_first_downbeat() {
        // Grid beginnt auf Schlag 3: Takt 1 startet beim dritten Eintrag.
        let mut g = BeatGrid::uniform(120.0, 0.0, 60_000.0);
        for (i, b) in g.beats.iter_mut().enumerate() {
            b.beat_in_bar = ((i + 2) % 4) as u8 + 1;
        }
        let at = |bar, beat| cue_position_ms(&CuePosition::Bar { bar, beat }, Some(&g));
        assert_eq!(at(1, 1), Some(1000.0));
        assert_eq!(at(1, 2), Some(1500.0));
        assert_eq!(at(17, 1), Some(1000.0 + 64.0 * 500.0));
        assert_eq!(at(0, 1), None);
        assert_eq!(at(1000, 1), None);
        assert_eq!(
            cue_position_ms(&CuePosition::Bar { bar: 1, beat: 1 }, None),
            None
        );
        assert_eq!(
            cue_position_ms(&CuePosition::Ms { ms: 5.0 }, None),
            Some(5.0)
        );
    }

    #[test]
    fn templates_are_named_sorted_and_coloured() {
        let p = |kind, ms, comment: &str| CuePoint {
            kind,
            position_ms: ms,
            loop_end_ms: None,
            color: Some([255, 0, 0]),
            comment: comment.into(),
        };
        let t = hot_cue_templates(&[
            p(CueKind::Hot(2), 30_000, "Drop"),
            p(CueKind::Memory, 1_000, ""),
            p(CueKind::Hot(1), 15_000, ""),
        ]);
        let names: Vec<_> = t.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Memory 1", "Hot Cue A", "Hot Cue B: Drop"]);
        assert!(t.iter().all(|c| c.actions.is_empty() && c.color.is_some()));
    }
}
