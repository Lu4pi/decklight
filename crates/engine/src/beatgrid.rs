//! Beatgrid eines Tracks: Zeitpunkt jedes Beats.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridBeat {
    /// Position im Track in Millisekunden.
    pub time_ms: f64,
    /// Beat im Takt (1–4).
    pub beat_in_bar: u8,
    /// Tempo ab diesem Beat.
    pub bpm: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BeatGrid {
    pub beats: Vec<GridBeat>,
}

impl BeatGrid {
    /// Gleichmäßiges Raster, wenn (noch) kein echtes Beatgrid vorliegt.
    pub fn uniform(bpm: f64, first_beat_ms: f64, length_ms: f64) -> Self {
        let step = 60_000.0 / bpm;
        let mut beats = Vec::new();
        let mut t = first_beat_ms;
        let mut n = 0u32;
        while t <= length_ms {
            beats.push(GridBeat {
                time_ms: t,
                beat_in_bar: (n % 4) as u8 + 1,
                bpm,
            });
            n += 1;
            t = first_beat_ms + f64::from(n) * step;
        }
        Self { beats }
    }

    /// Zeitpunkt von Beat `number` (ab 1).
    pub fn time_of(&self, number: u32) -> Option<f64> {
        let idx = usize::try_from(number.checked_sub(1)?).ok()?;
        self.beats.get(idx).map(|b| b.time_ms)
    }

    /// Beat-Nummer (ab 1) an einer Position; vor dem ersten Beat 1.
    pub fn beat_at(&self, pos_ms: f64) -> u32 {
        let idx = self.beats.partition_point(|b| b.time_ms <= pos_ms);
        u32::try_from(idx.max(1)).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_grid_lookup() {
        let g = BeatGrid::uniform(120.0, 100.0, 10_000.0);
        assert_eq!(g.time_of(1), Some(100.0));
        assert_eq!(g.time_of(3), Some(1100.0));
        assert_eq!(g.time_of(0), None);
        assert_eq!(g.beat_at(0.0), 1);
        assert_eq!(g.beat_at(1099.0), 2);
        assert_eq!(g.beat_at(1100.0), 3);
        assert_eq!(g.beats[4].beat_in_bar, 1);
    }
}
