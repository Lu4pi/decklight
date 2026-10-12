//! MIDI Timecode: Quarter-Frames im Lauf, Full-Frame bei Sprüngen und Stillstand.

use crate::timecode::Timecode;

/// Quarter-Frame-Nachricht `piece` (0–7) für den Timecode `tc`.
/// Ein kompletter Durchlauf beschreibt `tc` und dauert zwei Frames.
pub fn quarter_frame(tc: &Timecode, piece: u8) -> [u8; 2] {
    let nibble = match piece & 7 {
        0 => tc.frames & 0x0f,
        1 => tc.frames >> 4,
        2 => tc.seconds & 0x0f,
        3 => tc.seconds >> 4,
        4 => tc.minutes & 0x0f,
        5 => tc.minutes >> 4,
        6 => tc.hours & 0x0f,
        _ => (tc.rate.code() << 1) | ((tc.hours >> 4) & 1),
    };
    [0xf1, ((piece & 7) << 4) | (nibble & 0x0f)]
}

/// Full-Frame-SysEx: setzt den Empfänger hart auf `tc`.
pub fn full_frame(tc: &Timecode) -> [u8; 10] {
    [
        0xf0,
        0x7f,
        0x7f,
        0x01,
        0x01,
        (tc.rate.code() << 5) | tc.hours,
        tc.minutes,
        tc.seconds,
        tc.frames,
        0xf7,
    ]
}

/// Setzt Quarter-Frames wieder zu einem Timecode zusammen (für Tests).
#[derive(Debug, Default)]
pub struct QuarterFrameDecoder {
    nibbles: [u8; 8],
    seen: u8,
}

impl QuarterFrameDecoder {
    /// Liefert nach Stück 7 den übertragenen Wert (h, m, s, f, Raten-Code).
    pub fn push(&mut self, msg: [u8; 2]) -> Option<(u8, u8, u8, u8, u8)> {
        if msg[0] != 0xf1 {
            return None;
        }
        let piece = msg[1] >> 4;
        self.nibbles[usize::from(piece)] = msg[1] & 0x0f;
        self.seen |= 1 << piece;
        if piece != 7 || self.seen != 0xff {
            return None;
        }
        self.seen = 0;
        let n = &self.nibbles;
        Some((
            n[6] | ((n[7] & 1) << 4),
            n[4] | (n[5] << 4),
            n[2] | (n[3] << 4),
            n[0] | (n[1] << 4),
            (n[7] >> 1) & 3,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timecode::FrameRate;

    #[test]
    fn quarter_frames_roundtrip() {
        let tc = Timecode::parse("23:59:58:29", FrameRate::Fps30).unwrap();
        let mut dec = QuarterFrameDecoder::default();
        let mut got = None;
        for piece in 0..8 {
            got = dec.push(quarter_frame(&tc, piece));
        }
        assert_eq!(got, Some((23, 59, 58, 29, 3)));
    }

    #[test]
    fn full_frame_layout() {
        let tc = Timecode::parse("01:00:00:00", FrameRate::Fps25).unwrap();
        assert_eq!(
            full_frame(&tc),
            [0xf0, 0x7f, 0x7f, 0x01, 0x01, 0x21, 0, 0, 0, 0xf7]
        );
    }
}
