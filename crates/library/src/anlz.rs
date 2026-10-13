//! Formate der Analyse-Daten: Beatgrid aus dem dbserver, Tags aus Rekordbox'
//! ANLZ-Dateien (PWV5 farbige Wellenform, PCO2 Cues). ANLZ ist Big Endian,
//! das dbserver-Beatgrid Little Endian.
//!
//! Zu jedem Parser gibt es einen Encoder; Simulator und Tests erzeugen damit
//! dieselben Bytes, die echte Player liefern.

use crate::track::{CueKind, CuePoint, GridBeat, WaveColumn, Waveform};

fn be32(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}
fn le32(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
fn le16(d: &[u8], at: usize) -> Option<u16> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

/// Beatgrid aus der dbserver-Antwort 0x4602: 20 Bytes Kopf, je Beat 16 Bytes.
pub fn parse_dbserver_beat_grid(d: &[u8]) -> Vec<GridBeat> {
    d.get(20..)
        .unwrap_or_default()
        .chunks_exact(16)
        .filter_map(|e| {
            Some(GridBeat {
                beat_in_bar: le16(e, 0)? as u8,
                bpm: f64::from(le16(e, 2)?) / 100.0,
                time_ms: le32(e, 4)?,
            })
        })
        .collect()
}

pub fn encode_dbserver_beat_grid(beats: &[GridBeat]) -> Vec<u8> {
    let mut d = vec![0u8; 20];
    for b in beats {
        let mut e = [0u8; 16];
        e[0..2].copy_from_slice(&u16::from(b.beat_in_bar).to_le_bytes());
        e[2..4].copy_from_slice(&((b.bpm * 100.0).round() as u16).to_le_bytes());
        e[4..8].copy_from_slice(&b.time_ms.to_le_bytes());
        d.extend_from_slice(&e);
    }
    d
}

/// Sucht ein ANLZ-Tag in einer Antwort (manche Antworten haben ein Präfix).
pub fn find_tag<'a>(d: &'a [u8], fourcc: &[u8; 4]) -> Option<&'a [u8]> {
    let start =
        (0..=d.len().saturating_sub(12).min(16)).find(|&i| d.get(i..i + 4) == Some(fourcc))?;
    let tag = &d[start..];
    let len = be32(tag, 8)? as usize;
    tag.get(..len.min(tag.len()))
}

fn tag_header(fourcc: &[u8; 4], len_header: u32, len_tag: u32) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(fourcc);
    d.extend_from_slice(&len_header.to_be_bytes());
    d.extend_from_slice(&len_tag.to_be_bytes());
    d
}

/// Alle Tags einer ANLZ-Datei (`ANLZ0000.DAT`/`.EXT`): Kopf `PMAI`, dann Tags
/// mit Kennung, Kopf- und Gesamtlänge.
pub fn file_tags(d: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    if d.get(..4) != Some(b"PMAI") {
        return out;
    }
    let Some(mut at) = be32(d, 4).map(|l| l as usize) else {
        return out;
    };
    while let (Some(fourcc), Some(len)) = (d.get(at..at + 4), be32(d, at + 8)) {
        let len = len as usize;
        if len < 12 {
            break;
        }
        let Some(tag) = d.get(at..at + len) else {
            break;
        };
        out.push(([fourcc[0], fourcc[1], fourcc[2], fourcc[3]], tag));
        at += len;
    }
    out
}

/// Baut eine ANLZ-Datei aus Tags (für Tests und den Simulator).
pub fn encode_file(tags: &[Vec<u8>]) -> Vec<u8> {
    let body: usize = tags.iter().map(Vec::len).sum();
    let mut d = tag_header(b"PMAI", 0x1c, (0x1c + body) as u32);
    d.resize(0x1c, 0);
    for t in tags {
        d.extend_from_slice(t);
    }
    d
}

/// PQTZ: Beatgrid in `ANLZ0000.DAT`, je Beat 8 Bytes (Schlag im Takt, BPM × 100, Zeit in ms).
pub fn parse_pqtz(tag: &[u8]) -> Option<Vec<GridBeat>> {
    if tag.get(..4)? != b"PQTZ" {
        return None;
    }
    let len_header = be32(tag, 4)? as usize;
    let count = be32(tag, 0x14)? as usize;
    Some(
        tag.get(len_header..)?
            .chunks_exact(8)
            .take(count)
            .map(|e| GridBeat {
                beat_in_bar: u16::from_be_bytes([e[0], e[1]]) as u8,
                bpm: f64::from(u16::from_be_bytes([e[2], e[3]])) / 100.0,
                time_ms: u32::from_be_bytes([e[4], e[5], e[6], e[7]]),
            })
            .collect(),
    )
}

pub fn encode_pqtz(beats: &[GridBeat]) -> Vec<u8> {
    let mut d = tag_header(b"PQTZ", 0x18, 0x18 + 8 * beats.len() as u32);
    d.extend_from_slice(&0u32.to_be_bytes());
    d.extend_from_slice(&0x0008_0000u32.to_be_bytes());
    d.extend_from_slice(&(beats.len() as u32).to_be_bytes());
    for b in beats {
        d.extend_from_slice(&u16::from(b.beat_in_bar).to_be_bytes());
        d.extend_from_slice(&((b.bpm * 100.0).round() as u16).to_be_bytes());
        d.extend_from_slice(&b.time_ms.to_be_bytes());
    }
    d
}

/// PWV5: farbige Detail-Wellenform, 150 Spalten pro Sekunde, je 2 Bytes:
/// Rot 3 Bit, Grün 3 Bit, Blau 3 Bit, Höhe 5 Bit, 2 Bit ungenutzt.
pub fn parse_pwv5(tag: &[u8]) -> Option<Waveform> {
    if tag.get(..4)? != b"PWV5" {
        return None;
    }
    let len_header = be32(tag, 4)? as usize;
    let count = be32(tag, 16)? as usize;
    let data = tag.get(len_header..)?;
    let scale = |v: u16| ((v & 7) * 255 / 7) as u8;
    let columns = data
        .chunks_exact(2)
        .take(count)
        .map(|c| {
            let v = u16::from_be_bytes([c[0], c[1]]);
            WaveColumn {
                height: ((v >> 2) & 0x1f) as u8,
                rgb: [scale(v >> 13), scale(v >> 10), scale(v >> 7)],
            }
        })
        .collect();
    Some(Waveform {
        per_second: 150.0,
        columns,
    })
}

pub fn encode_pwv5(w: &Waveform) -> Vec<u8> {
    let count = w.columns.len() as u32;
    let mut d = tag_header(b"PWV5", 0x18, 0x18 + 2 * count);
    d.extend_from_slice(&2u32.to_be_bytes());
    d.extend_from_slice(&count.to_be_bytes());
    d.extend_from_slice(&0x0096_0305u32.to_be_bytes());
    let q = |c: u8| (u16::from(c) * 7 + 127) / 255;
    for c in &w.columns {
        let v = (q(c.rgb[0]) << 13)
            | (q(c.rgb[1]) << 10)
            | (q(c.rgb[2]) << 7)
            | (u16::from(c.height & 0x1f) << 2);
        d.extend_from_slice(&v.to_be_bytes());
    }
    d
}

/// PCO2: erweiterte Cue-Liste (Memory Cues oder Hot Cues) mit Farben und Kommentaren.
pub fn parse_pco2(tag: &[u8]) -> Option<Vec<CuePoint>> {
    if tag.get(..4)? != b"PCO2" {
        return None;
    }
    let len_header = be32(tag, 4)? as usize;
    let mut at = len_header;
    let mut cues = Vec::new();
    while let Some(entry) = tag.get(at..) {
        if entry.len() < 0x1c || &entry[..4] != b"PCP2" {
            break;
        }
        let len_entry = be32(entry, 8)? as usize;
        if len_entry < 0x1c {
            break;
        }
        let e = &entry[..len_entry.min(entry.len())];
        let hot = be32(e, 0x0c)?;
        let is_loop = e[0x10] == 2;
        let time = be32(e, 0x14)?;
        let loop_time = be32(e, 0x18)?;
        let (comment, color) = parse_pcp2_tail(e);
        cues.push(CuePoint {
            kind: if hot == 0 {
                CueKind::Memory
            } else {
                CueKind::Hot(hot as u8)
            },
            position_ms: time,
            loop_end_ms: (is_loop && loop_time != u32::MAX).then_some(loop_time),
            color,
            comment,
        });
        at += len_entry;
    }
    Some(cues)
}

fn parse_pcp2_tail(e: &[u8]) -> (String, Option<[u8; 3]>) {
    let Some(len_comment) = be32(e, 0x28).map(|l| l as usize) else {
        return (String::new(), None);
    };
    let Some(raw) = e.get(0x2c..0x2c + len_comment) else {
        return (String::new(), None);
    };
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    let rgb_at = 0x2c + len_comment + 1;
    let color = e
        .get(rgb_at..rgb_at + 3)
        .map(|c| [c[0], c[1], c[2]])
        .filter(|c| c != &[0, 0, 0]);
    (String::from_utf16_lossy(&units), color)
}

pub fn encode_pco2(cues: &[CuePoint], hot: bool) -> Vec<u8> {
    let mut entries = Vec::new();
    for c in cues
        .iter()
        .filter(|c| matches!(c.kind, CueKind::Hot(_)) == hot)
    {
        let comment: Vec<u8> = c
            .comment
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_be_bytes)
            .collect();
        let len_entry = 0x2c + comment.len() + 4 + 0x0c;
        let mut e = tag_header(b"PCP2", 0x10, len_entry as u32);
        e.extend_from_slice(
            &(match c.kind {
                CueKind::Hot(n) => u32::from(n),
                CueKind::Memory => 0,
            })
            .to_be_bytes(),
        );
        e.push(if c.loop_end_ms.is_some() { 2 } else { 1 });
        e.extend_from_slice(&[0, 0, 0]);
        e.extend_from_slice(&c.position_ms.to_be_bytes());
        e.extend_from_slice(&c.loop_end_ms.unwrap_or(u32::MAX).to_be_bytes());
        e.push(0); // Farb-ID
        e.extend_from_slice(&[0; 7]);
        e.extend_from_slice(&[0, 0, 0, 0]); // Loop-Zähler/Nenner
        e.extend_from_slice(&(comment.len() as u32).to_be_bytes());
        e.extend_from_slice(&comment);
        let rgb = c.color.unwrap_or_default();
        e.extend_from_slice(&[0, rgb[0], rgb[1], rgb[2]]);
        e.resize(len_entry, 0);
        entries.extend_from_slice(&e);
    }
    let count = cues
        .iter()
        .filter(|c| matches!(c.kind, CueKind::Hot(_)) == hot)
        .count() as u16;
    let mut d = tag_header(b"PCO2", 0x14, (0x14 + entries.len()) as u32);
    d.extend_from_slice(&u32::from(hot).to_be_bytes());
    d.extend_from_slice(&count.to_be_bytes());
    d.extend_from_slice(&[0, 0]);
    d.extend_from_slice(&entries);
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_grid_roundtrip() {
        let beats = vec![
            GridBeat {
                time_ms: 120,
                beat_in_bar: 1,
                bpm: 126.0,
            },
            GridBeat {
                time_ms: 596,
                beat_in_bar: 2,
                bpm: 126.0,
            },
        ];
        assert_eq!(
            parse_dbserver_beat_grid(&encode_dbserver_beat_grid(&beats)),
            beats
        );
    }

    #[test]
    fn pwv5_roundtrip_and_bits() {
        let w = Waveform {
            per_second: 150.0,
            columns: vec![
                WaveColumn {
                    height: 31,
                    rgb: [255, 0, 0],
                },
                WaveColumn {
                    height: 3,
                    rgb: [0, 255, 255],
                },
            ],
        };
        let tag = encode_pwv5(&w);
        assert_eq!(&tag[24..26], &[0xe0, 0x7c]); // rot 7, Höhe 31
        let mut with_prefix = vec![0, 0, 0, 0];
        with_prefix.extend_from_slice(&tag);
        assert_eq!(
            parse_pwv5(find_tag(&with_prefix, b"PWV5").unwrap()),
            Some(w.clone())
        );
        let o = w.overview(1);
        assert_eq!(
            o.columns,
            vec![WaveColumn {
                height: 31,
                rgb: [255, 0, 0]
            }]
        );
    }

    #[test]
    fn pco2_roundtrip() {
        let cues = vec![
            CuePoint {
                kind: CueKind::Hot(1),
                position_ms: 30_000,
                loop_end_ms: None,
                color: Some([255, 0, 64]),
                comment: "Drop".into(),
            },
            CuePoint {
                kind: CueKind::Hot(2),
                position_ms: 60_000,
                loop_end_ms: Some(67_500),
                color: None,
                comment: String::new(),
            },
        ];
        let tag = encode_pco2(&cues, true);
        assert_eq!(parse_pco2(&tag), Some(cues));
        assert_eq!(parse_pco2(&encode_pco2(&[], false)), Some(vec![]));
    }
}
