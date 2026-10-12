//! SMPTE-Timecode: Bildraten, Umrechnung von Millisekunden, Drop-Frame.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FrameRate {
    #[serde(rename = "24")]
    Fps24,
    #[default]
    #[serde(rename = "25")]
    Fps25,
    /// 29,97 fps Drop-Frame.
    #[serde(rename = "29.97df")]
    Fps2997Df,
    #[serde(rename = "30")]
    Fps30,
}

impl FrameRate {
    pub const ALL: [FrameRate; 4] = [Self::Fps24, Self::Fps25, Self::Fps2997Df, Self::Fps30];

    /// Bilder pro Sekunde in Echtzeit.
    pub fn fps(self) -> f64 {
        match self {
            Self::Fps24 => 24.0,
            Self::Fps25 => 25.0,
            Self::Fps2997Df => 30_000.0 / 1001.0,
            Self::Fps30 => 30.0,
        }
    }

    /// Nennwert der Frame-Zählung (30 bei Drop-Frame).
    pub fn nominal(self) -> u32 {
        match self {
            Self::Fps24 => 24,
            Self::Fps25 => 25,
            Self::Fps2997Df | Self::Fps30 => 30,
        }
    }

    /// Typ-Code wie in Art-Net (OpTimeCode) und MTC: 0=24, 1=25, 2=29,97DF, 3=30.
    pub fn code(self) -> u8 {
        match self {
            Self::Fps24 => 0,
            Self::Fps25 => 1,
            Self::Fps2997Df => 2,
            Self::Fps30 => 3,
        }
    }

    pub fn frame_ms(self) -> f64 {
        1000.0 / self.fps()
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().replace(',', ".").as_str() {
            "24" => Some(Self::Fps24),
            "25" => Some(Self::Fps25),
            "29.97" | "29.97df" | "2997" | "df" => Some(Self::Fps2997Df),
            "30" => Some(Self::Fps30),
            _ => None,
        }
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Fps24 => "24",
            Self::Fps25 => "25",
            Self::Fps2997Df => "29.97df",
            Self::Fps30 => "30",
        })
    }
}

/// Ein Timecode-Wert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Timecode {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
    pub rate: FrameRate,
}

const DAY_MS: f64 = 24.0 * 3600.0 * 1000.0;

impl Timecode {
    /// Fortlaufende Bildnummer (in Echtzeit-Frames) zu einer Zeit in Millisekunden.
    pub fn frame_index(ms: f64, rate: FrameRate) -> i64 {
        (ms.rem_euclid(DAY_MS) * rate.fps() / 1000.0 + 1e-9).floor() as i64
    }

    /// Timecode zu einer Zeit in Millisekunden (Wrap nach 24 h).
    pub fn from_ms(ms: f64, rate: FrameRate) -> Self {
        Self::from_frame_index(Self::frame_index(ms, rate), rate)
    }

    /// Timecode zu einer fortlaufenden Bildnummer. Bei Drop-Frame werden die
    /// Labels 00 und 01 zu Beginn jeder Minute ausgelassen, außer jede zehnte.
    pub fn from_frame_index(index: i64, rate: FrameRate) -> Self {
        let nominal = i64::from(rate.nominal());
        let mut n = index.rem_euclid(Self::frames_per_day(rate));
        if rate == FrameRate::Fps2997Df {
            let per_10min = 17_982; // 10 * 60 * 30 - 9 * 2
            let d = n / per_10min;
            let m = n % per_10min;
            n += 18 * d + if m > 1 { 2 * ((m - 2) / 1798) } else { 0 };
        }
        let frames = n % nominal;
        let total_s = n / nominal;
        Self {
            hours: (total_s / 3600 % 24) as u8,
            minutes: (total_s / 60 % 60) as u8,
            seconds: (total_s % 60) as u8,
            frames: frames as u8,
            rate,
        }
    }

    fn frames_per_day(rate: FrameRate) -> i64 {
        match rate {
            FrameRate::Fps2997Df => 17_982 * 6 * 24,
            other => i64::from(other.nominal()) * 86_400,
        }
    }

    /// Zeit in Millisekunden zum Anfang dieses Frames.
    pub fn to_ms(self) -> f64 {
        let nominal = i64::from(self.rate.nominal());
        let mut n = ((i64::from(self.hours) * 60 + i64::from(self.minutes)) * 60
            + i64::from(self.seconds))
            * nominal
            + i64::from(self.frames);
        if self.rate == FrameRate::Fps2997Df {
            let total_minutes = i64::from(self.hours) * 60 + i64::from(self.minutes);
            n -= 2 * (total_minutes - total_minutes / 10);
        }
        n as f64 * 1000.0 / self.rate.fps()
    }

    /// Liest "HH:MM:SS:FF" (bei Drop-Frame auch "HH:MM:SS;FF").
    pub fn parse(s: &str, rate: FrameRate) -> Option<Self> {
        let parts: Vec<u8> = s
            .trim()
            .split([':', ';', '.'])
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        let [hours, minutes, seconds, frames] = parts[..] else {
            return None;
        };
        (hours < 24 && minutes < 60 && seconds < 60 && u32::from(frames) < rate.nominal())
            .then_some(Self {
                hours,
                minutes,
                seconds,
                frames,
                rate,
            })
    }
}

impl fmt::Display for Timecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sep = if self.rate == FrameRate::Fps2997Df {
            ';'
        } else {
            ':'
        };
        write!(
            f,
            "{:02}:{:02}:{:02}{sep}{:02}",
            self.hours, self.minutes, self.seconds, self.frames
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_drop_rates() {
        let tc = Timecode::from_ms(3_723_480.0, FrameRate::Fps25);
        assert_eq!(tc.to_string(), "01:02:03:12");
        assert_eq!(tc.to_ms(), 3_723_480.0);
        assert_eq!(Timecode::from_ms(999.0, FrameRate::Fps24).frames, 23);
        assert_eq!(
            Timecode::from_ms(1000.0, FrameRate::Fps30).to_string(),
            "00:00:01:00"
        );
        // Wrap nach 24 Stunden.
        assert_eq!(
            Timecode::from_ms(DAY_MS + 40.0, FrameRate::Fps25).to_string(),
            "00:00:00:01"
        );
    }

    #[test]
    fn drop_frame_skips_labels() {
        let r = FrameRate::Fps2997Df;
        // Bild 1800 ist das erste der Minute 1: Labels ;00 und ;01 fehlen.
        assert_eq!(
            Timecode::from_frame_index(1799, r).to_string(),
            "00:00:59;29"
        );
        assert_eq!(
            Timecode::from_frame_index(1800, r).to_string(),
            "00:01:00;02"
        );
        // Jede zehnte Minute wird nicht ausgelassen.
        assert_eq!(
            Timecode::from_frame_index(17_982, r).to_string(),
            "00:10:00;00"
        );
        // Eine Stunde Echtzeit ergibt bei Drop-Frame wieder genau 01:00:00;00.
        assert_eq!(Timecode::from_ms(3_600_000.0, r).to_string(), "01:00:00;00");
        for idx in [0i64, 1799, 1800, 17_981, 17_982, 107_892, 2_000_000] {
            let tc = Timecode::from_frame_index(idx, r);
            assert_eq!(Timecode::frame_index(tc.to_ms(), r), idx, "{tc}");
        }
    }

    #[test]
    fn parse_and_rates() {
        let tc = Timecode::parse("02:00:00:00", FrameRate::Fps25).unwrap();
        assert_eq!(tc.to_ms(), 7_200_000.0);
        assert!(Timecode::parse("02:00:00:25", FrameRate::Fps25).is_none());
        assert_eq!(FrameRate::parse("29,97"), Some(FrameRate::Fps2997Df));
        assert_eq!(FrameRate::Fps25.code(), 1);
    }
}
