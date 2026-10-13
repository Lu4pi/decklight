//! Rekordbox-Export auf einem USB-Stick oder einer SD-Karte lesen
//! (`PIONEER/rekordbox/export.pdb` plus Analyse-Dateien unter `PIONEER/USBANLZ`).
//!
//! Damit gibt es Titel, Beatgrid, Wellenform, Cues und Cover auch ohne den
//! dbserver der Player: als Rückfall, wenn ein Player keine Antwort gibt, und für
//! den Simulator mit echten Sticks. Das Format ist das der Deep-Symmetry-Analyse
//! ("Database Exports"): Little-Endian-Seiten mit Zeilengruppen am Seitenende,
//! Texte als DeviceSQL-Strings.

use crate::anlz;
use crate::track::{CuePoint, TrackData, TrackMetadata};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    #[error("export.pdb ist beschädigt oder kein Rekordbox-Export")]
    Format,
}

const HEAP: usize = 0x28;

fn le16(d: &[u8], at: usize) -> Option<u16> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}
fn le32(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// DeviceSQL-String an `at`: kurzes ASCII (Bit 0 gesetzt, Länge im Kopfbyte),
/// langes ASCII (0x40) oder UTF-16LE (0x90).
fn dstring(d: &[u8], at: usize) -> Option<String> {
    let kind = *d.get(at)?;
    if kind & 1 == 1 {
        let len = usize::from(kind >> 1);
        let raw = d.get(at + 1..at + len.max(1))?;
        return Some(String::from_utf8_lossy(raw).into_owned());
    }
    let len = usize::from(le16(d, at + 1)?);
    let raw = d.get(at + 4..at + len.max(4))?;
    match kind {
        0x40 => Some(String::from_utf8_lossy(raw).into_owned()),
        0x90 => {
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            Some(String::from_utf16_lossy(&units))
        }
        _ => None,
    }
}

/// Seitentypen, die DeckLight liest.
mod page {
    pub const TRACKS: u32 = 0;
    pub const GENRES: u32 = 1;
    pub const ARTISTS: u32 = 2;
    pub const ALBUMS: u32 = 3;
    pub const KEYS: u32 = 5;
    pub const ARTWORK: u32 = 13;
}

/// String-Felder einer Track-Zeile (Index in der Offset-Tabelle ab 0x5e).
mod field {
    pub const ANALYZE_PATH: usize = 14;
    pub const COMMENT: usize = 16;
    pub const TITLE: usize = 17;
    pub const FILE_PATH: usize = 20;
    pub const COUNT: usize = 21;
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportTrack {
    pub id: u32,
    pub title: String,
    pub artist_id: u32,
    pub album_id: u32,
    pub genre_id: u32,
    pub key_id: u32,
    pub artwork_id: u32,
    pub duration_s: u32,
    /// BPM × 100
    pub tempo: u32,
    pub rating: u8,
    pub comment: String,
    pub file_path: String,
    pub analyze_path: String,
}

/// Inhalt der `export.pdb` (nur die Tabellen, die DeckLight braucht).
#[derive(Debug, Default, PartialEq)]
pub struct Database {
    pub tracks: HashMap<u32, ExportTrack>,
    pub artists: HashMap<u32, String>,
    pub albums: HashMap<u32, String>,
    pub genres: HashMap<u32, String>,
    pub keys: HashMap<u32, String>,
    pub artwork: HashMap<u32, String>,
}

impl Database {
    pub fn parse(d: &[u8]) -> Result<Self, ExportError> {
        let page_size = le32(d, 4).ok_or(ExportError::Format)? as usize;
        let num_tables = le32(d, 8).ok_or(ExportError::Format)? as usize;
        if le32(d, 0) != Some(0) || !(512..=65_536).contains(&page_size) || num_tables > 64 {
            return Err(ExportError::Format);
        }
        let mut db = Database::default();
        for t in 0..num_tables {
            let at = 0x1c + t * 16;
            let (Some(ty), Some(first), Some(last)) =
                (le32(d, at), le32(d, at + 8), le32(d, at + 12))
            else {
                return Err(ExportError::Format);
            };
            let mut index = first as usize;
            // Schutz gegen Schleifen in kaputten Dateien.
            for _ in 0..d.len() / page_size + 1 {
                let Some(p) = d.get(index * page_size..(index + 1) * page_size) else {
                    break;
                };
                for row in rows(p) {
                    db.add(ty, p, row);
                }
                if index == last as usize {
                    break;
                }
                index = le32(p, 0x0c).ok_or(ExportError::Format)? as usize;
            }
        }
        Ok(db)
    }

    fn add(&mut self, ty: u32, p: &[u8], r: usize) {
        let s = |at: usize| dstring(p, at).unwrap_or_default();
        match ty {
            page::TRACKS => {
                let Some(id) = le32(p, r + 0x48) else { return };
                let text = |i: usize| {
                    le16(p, r + 0x5e + 2 * i)
                        .map(|o| s(r + usize::from(o)))
                        .unwrap_or_default()
                };
                let u = |o: usize| le32(p, r + o).unwrap_or(0);
                self.tracks.insert(
                    id,
                    ExportTrack {
                        id,
                        title: text(field::TITLE),
                        artist_id: u(0x44),
                        album_id: u(0x40),
                        genre_id: u(0x3c),
                        key_id: u(0x20),
                        artwork_id: u(0x1c),
                        duration_s: u32::from(le16(p, r + 0x54).unwrap_or(0)),
                        tempo: u(0x38),
                        rating: p.get(r + 0x59).copied().unwrap_or(0),
                        comment: text(field::COMMENT),
                        file_path: text(field::FILE_PATH),
                        analyze_path: text(field::ANALYZE_PATH),
                    },
                );
            }
            page::ARTISTS => {
                let (Some(sub), Some(id), Some(near)) = (le16(p, r), le32(p, r + 4), p.get(r + 9))
                else {
                    return;
                };
                let ofs = if sub == 0x64 {
                    le16(p, r + 0x0a).map_or(0, usize::from)
                } else {
                    usize::from(*near)
                };
                self.artists.insert(id, s(r + ofs));
            }
            page::ALBUMS => {
                let (Some(sub), Some(id), Some(near)) =
                    (le16(p, r), le32(p, r + 0x0c), p.get(r + 0x15))
                else {
                    return;
                };
                let ofs = if sub == 0x84 {
                    le16(p, r + 0x16).map_or(0, usize::from)
                } else {
                    usize::from(*near)
                };
                self.albums.insert(id, s(r + ofs));
            }
            page::GENRES => {
                if let Some(id) = le32(p, r) {
                    self.genres.insert(id, s(r + 4));
                }
            }
            page::KEYS => {
                if let Some(id) = le32(p, r) {
                    self.keys.insert(id, s(r + 8));
                }
            }
            page::ARTWORK => {
                if let Some(id) = le32(p, r) {
                    self.artwork.insert(id, s(r + 4));
                }
            }
            _ => {}
        }
    }
}

/// Anfang (im Seitenpuffer) aller vorhandenen Zeilen einer Datenseite.
fn rows(p: &[u8]) -> Vec<usize> {
    let flags = p.get(0x1b).copied().unwrap_or(0x40);
    if flags & 0x40 != 0 {
        return Vec::new();
    }
    let small = usize::from(p.get(0x18).copied().unwrap_or(0));
    let large = usize::from(le16(p, 0x22).unwrap_or(0));
    let n = if large > small && large != 0x1fff {
        large
    } else {
        small
    };
    let mut out = Vec::new();
    for g in 0..n.div_ceil(16) {
        let end = p.len() - g * 0x24;
        let Some(present) = le16(p, end - 4) else {
            break;
        };
        for j in 0..16 {
            if g * 16 + j >= n {
                break;
            }
            if present & (1 << j) != 0 {
                if let Some(ofs) = le16(p, end - 6 - 2 * j) {
                    out.push(HEAP + usize::from(ofs));
                }
            }
        }
    }
    out
}

/// Ein Rekordbox-Export: Datenbank plus Zugriff auf Analyse-Dateien und Cover.
pub struct Export {
    root: PathBuf,
    pub db: Database,
}

impl Export {
    /// `root` ist das Wurzelverzeichnis des Sticks (enthält `PIONEER`).
    pub fn open(root: &Path) -> Result<Self, ExportError> {
        let pdb = root.join("PIONEER").join("rekordbox").join("export.pdb");
        let data =
            std::fs::read(&pdb).map_err(|e| ExportError::Io(pdb.display().to_string(), e))?;
        Ok(Self {
            root: root.to_path_buf(),
            db: Database::parse(&data)?,
        })
    }

    /// Prüft, ob unter `root` ein Rekordbox-Export liegt.
    pub fn exists(root: &Path) -> bool {
        root.join("PIONEER")
            .join("rekordbox")
            .join("export.pdb")
            .is_file()
    }

    /// Lokaler Pfad zu einem Pfad aus der Datenbank (`/PIONEER/...`).
    pub fn file(&self, export_path: &str) -> PathBuf {
        let mut p = self.root.clone();
        for part in export_path
            .split(['/', '\\'])
            .filter(|s| !s.is_empty() && *s != "..")
        {
            p.push(part);
        }
        p
    }

    pub fn track(&self, id: u32) -> Option<TrackData> {
        let t = self.db.tracks.get(&id)?;
        let name = |m: &HashMap<u32, String>, k: u32| m.get(&k).cloned().unwrap_or_default();
        let metadata = TrackMetadata {
            rekordbox_id: id,
            title: t.title.clone(),
            artist: name(&self.db.artists, t.artist_id),
            album: name(&self.db.albums, t.album_id),
            genre: name(&self.db.genres, t.genre_id),
            key: name(&self.db.keys, t.key_id),
            comment: t.comment.clone(),
            duration_s: t.duration_s,
            bpm: (t.tempo > 0).then(|| f64::from(t.tempo) / 100.0),
            rating: if t.rating > 5 {
                t.rating / 20
            } else {
                t.rating
            },
            artwork_id: t.artwork_id,
        };
        let mut data = TrackData {
            metadata,
            ..Default::default()
        };
        if !t.analyze_path.is_empty() {
            let dat = self.file(&t.analyze_path);
            if let Ok(d) = std::fs::read(&dat) {
                for (cc, tag) in anlz::file_tags(&d) {
                    if &cc == b"PQTZ" {
                        data.beat_grid = anlz::parse_pqtz(tag).unwrap_or_default();
                    }
                }
            }
            if let Ok(d) = std::fs::read(dat.with_extension("EXT")) {
                let mut cues: Vec<CuePoint> = Vec::new();
                for (cc, tag) in anlz::file_tags(&d) {
                    match &cc {
                        b"PWV5" => data.waveform = anlz::parse_pwv5(tag),
                        b"PCO2" => cues.extend(anlz::parse_pco2(tag).unwrap_or_default()),
                        _ => {}
                    }
                }
                cues.sort_by_key(|c| c.position_ms);
                data.cues = cues;
            }
            // CDJ-3000-Analyse: Drei-Band-Wellenform ersetzt die RGB-Variante.
            if let Ok(d) = std::fs::read(dat.with_extension("2EX")) {
                if let Some(w) = anlz::file_tags(&d)
                    .into_iter()
                    .find(|(cc, _)| cc == b"PWV7")
                    .and_then(|(_, tag)| anlz::parse_pwv7(tag))
                {
                    data.waveform = Some(w);
                }
            }
        }
        if let Some(path) = self.db.artwork.get(&t.artwork_id) {
            data.artwork = std::fs::read(self.file(path)).ok();
        }
        Some(data)
    }
}

impl crate::dbserver::server::TrackSource for Export {
    fn track(&self, _slot: u8, rekordbox_id: u32) -> Option<TrackData> {
        Export::track(self, rekordbox_id)
    }
}

/// Erzeugt `export.pdb`-Bytes (für Tests und den Simulator). Jede Tabelle passt
/// auf eine Seite.
#[derive(Default)]
pub struct PdbBuilder {
    pub tracks: Vec<ExportTrack>,
    pub artists: Vec<(u32, String)>,
    pub albums: Vec<(u32, String)>,
    pub genres: Vec<(u32, String)>,
    pub keys: Vec<(u32, String)>,
    pub artwork: Vec<(u32, String)>,
}

fn encode_dstring(s: &str) -> Vec<u8> {
    if s.is_ascii() && s.len() < 126 {
        let mut d = vec![(((s.len() + 1) << 1) | 1) as u8];
        d.extend_from_slice(s.as_bytes());
        d
    } else {
        let units: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut d = vec![0x90];
        d.extend_from_slice(&((units.len() + 4) as u16).to_le_bytes());
        d.push(0);
        d.extend_from_slice(&units);
        d
    }
}

impl PdbBuilder {
    const PAGE: usize = 4096;

    pub fn build(&self) -> Vec<u8> {
        let tables: Vec<(u32, Vec<Vec<u8>>)> = vec![
            (page::TRACKS, self.tracks.iter().map(track_row).collect()),
            (
                page::GENRES,
                self.genres
                    .iter()
                    .map(|(id, n)| id_row(*id, 4, n))
                    .collect(),
            ),
            (
                page::ARTISTS,
                self.artists
                    .iter()
                    .map(|(id, n)| artist_row(*id, n))
                    .collect(),
            ),
            (
                page::ALBUMS,
                self.albums
                    .iter()
                    .map(|(id, n)| album_row(*id, n))
                    .collect(),
            ),
            (
                page::KEYS,
                self.keys.iter().map(|(id, n)| id_row(*id, 8, n)).collect(),
            ),
            (
                page::ARTWORK,
                self.artwork
                    .iter()
                    .map(|(id, n)| id_row(*id, 4, n))
                    .collect(),
            ),
        ];
        let mut d = vec![0u8; Self::PAGE * (tables.len() + 1)];
        d[4..8].copy_from_slice(&(Self::PAGE as u32).to_le_bytes());
        d[8..12].copy_from_slice(&(tables.len() as u32).to_le_bytes());
        for (i, (ty, rows)) in tables.iter().enumerate() {
            let index = i + 1;
            let at = 0x1c + i * 16;
            d[at..at + 4].copy_from_slice(&ty.to_le_bytes());
            d[at + 8..at + 12].copy_from_slice(&(index as u32).to_le_bytes());
            d[at + 12..at + 16].copy_from_slice(&(index as u32).to_le_bytes());
            let p = &mut d[index * Self::PAGE..(index + 1) * Self::PAGE];
            p[4..8].copy_from_slice(&(index as u32).to_le_bytes());
            p[8..12].copy_from_slice(&ty.to_le_bytes());
            p[0x18] = rows.len() as u8;
            p[0x1b] = 0x34;
            let mut heap = 0usize;
            for (j, row) in rows.iter().enumerate() {
                let (g, k) = (j / 16, j % 16);
                let end = Self::PAGE - g * 0x24;
                p[HEAP + heap..HEAP + heap + row.len()].copy_from_slice(row);
                p[end - 6 - 2 * k..end - 4 - 2 * k].copy_from_slice(&(heap as u16).to_le_bytes());
                let flags = le16(p, end - 4).unwrap_or(0) | (1 << k);
                p[end - 4..end - 2].copy_from_slice(&flags.to_le_bytes());
                heap += row.len().div_ceil(4) * 4;
            }
        }
        d
    }
}

fn id_row(id: u32, text_at: usize, name: &str) -> Vec<u8> {
    let mut r = vec![0u8; text_at];
    r[0..4].copy_from_slice(&id.to_le_bytes());
    r.extend(encode_dstring(name));
    r
}

fn artist_row(id: u32, name: &str) -> Vec<u8> {
    let mut r = vec![0u8; 10];
    r[0..2].copy_from_slice(&0x60u16.to_le_bytes());
    r[4..8].copy_from_slice(&id.to_le_bytes());
    r[8] = 3;
    r[9] = 10;
    r.extend(encode_dstring(name));
    r
}

fn album_row(id: u32, name: &str) -> Vec<u8> {
    let mut r = vec![0u8; 0x16];
    r[0..2].copy_from_slice(&0x80u16.to_le_bytes());
    r[0x0c..0x10].copy_from_slice(&id.to_le_bytes());
    r[0x14] = 3;
    r[0x15] = 0x16;
    r.extend(encode_dstring(name));
    r
}

fn track_row(t: &ExportTrack) -> Vec<u8> {
    let fixed = 0x5e + 2 * field::COUNT;
    let mut r = vec![0u8; fixed];
    let put = |r: &mut Vec<u8>, at: usize, v: u32| r[at..at + 4].copy_from_slice(&v.to_le_bytes());
    r[0..2].copy_from_slice(&0x24u16.to_le_bytes());
    put(&mut r, 0x1c, t.artwork_id);
    put(&mut r, 0x20, t.key_id);
    put(&mut r, 0x38, t.tempo);
    put(&mut r, 0x3c, t.genre_id);
    put(&mut r, 0x40, t.album_id);
    put(&mut r, 0x44, t.artist_id);
    put(&mut r, 0x48, t.id);
    r[0x54..0x56].copy_from_slice(&(t.duration_s as u16).to_le_bytes());
    r[0x59] = t.rating;
    let mut strings: Vec<(usize, &str)> = vec![
        (field::ANALYZE_PATH, &t.analyze_path),
        (field::COMMENT, &t.comment),
        (field::TITLE, &t.title),
        (field::FILE_PATH, &t.file_path),
    ];
    strings.sort_by_key(|(i, _)| *i);
    // Leere Felder zeigen auf einen gemeinsamen leeren String.
    let empty = r.len();
    r.extend(encode_dstring(""));
    for i in 0..field::COUNT {
        let at = 0x5e + 2 * i;
        r[at..at + 2].copy_from_slice(&(empty as u16).to_le_bytes());
    }
    for (i, s) in strings {
        let ofs = r.len();
        r.extend(encode_dstring(s));
        let at = 0x5e + 2 * i;
        r[at..at + 2].copy_from_slice(&(ofs as u16).to_le_bytes());
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{CueKind, GridBeat, WaveColumn, Waveform};

    fn sample() -> PdbBuilder {
        PdbBuilder {
            tracks: vec![
                ExportTrack {
                    id: 41,
                    title: "Strobe".into(),
                    artist_id: 3,
                    album_id: 5,
                    genre_id: 2,
                    key_id: 7,
                    artwork_id: 9,
                    duration_s: 637,
                    tempo: 12800,
                    rating: 80,
                    comment: "Peak".into(),
                    file_path: "/Contents/deadmau5/Strobe.mp3".into(),
                    analyze_path: "/PIONEER/USBANLZ/P016/0000875E/ANLZ0000.DAT".into(),
                },
                ExportTrack {
                    id: 42,
                    title: "Ünïcödé Tïtle".into(),
                    artist_id: 4,
                    ..Default::default()
                },
            ],
            artists: vec![(3, "deadmau5".into()), (4, "Ünïcödé Ärtist".into())],
            albums: vec![(5, "For Lack of a Better Name".into())],
            genres: vec![(2, "Progressive House".into())],
            keys: vec![(7, "10A".into())],
            artwork: vec![(9, "/PIONEER/Artwork/00001/a9.jpg".into())],
        }
    }

    #[test]
    fn database_roundtrip_with_unicode() {
        let db = Database::parse(&sample().build()).unwrap();
        let t = &db.tracks[&41];
        assert_eq!(t.title, "Strobe");
        assert_eq!(t.tempo, 12800);
        assert_eq!(t.duration_s, 637);
        assert_eq!(
            t.analyze_path,
            "/PIONEER/USBANLZ/P016/0000875E/ANLZ0000.DAT"
        );
        assert_eq!(db.artists[&3], "deadmau5");
        assert_eq!(db.artists[&4], "Ünïcödé Ärtist");
        assert_eq!(db.tracks[&42].title, "Ünïcödé Tïtle");
        assert_eq!(db.albums[&5], "For Lack of a Better Name");
        assert_eq!(db.keys[&7], "10A");
        assert_eq!(db.genres[&2], "Progressive House");
        assert_eq!(db.artwork[&9], "/PIONEER/Artwork/00001/a9.jpg");
        assert!(Database::parse(b"kein pdb").is_err());
    }

    #[test]
    fn many_rows_span_several_row_groups() {
        let mut b = PdbBuilder::default();
        for i in 0..40u32 {
            b.genres.push((i + 1, format!("Genre {i}")));
        }
        let db = Database::parse(&b.build()).unwrap();
        assert_eq!(db.genres.len(), 40);
        assert_eq!(db.genres[&40], "Genre 39");
    }

    #[test]
    fn reads_a_stick_with_analysis_files_and_artwork() {
        let root = std::env::temp_dir().join(format!("decklight-usb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let rb = root.join("PIONEER/rekordbox");
        std::fs::create_dir_all(&rb).unwrap();
        std::fs::write(rb.join("export.pdb"), sample().build()).unwrap();
        let anlz_dir = root.join("PIONEER/USBANLZ/P016/0000875E");
        std::fs::create_dir_all(&anlz_dir).unwrap();
        let grid: Vec<GridBeat> = (0..8)
            .map(|i| GridBeat {
                time_ms: 100 + i * 469,
                beat_in_bar: (i % 4) as u8 + 1,
                bpm: 128.0,
            })
            .collect();
        std::fs::write(
            anlz_dir.join("ANLZ0000.DAT"),
            anlz::encode_file(&[anlz::encode_pqtz(&grid)]),
        )
        .unwrap();
        let wave = Waveform {
            per_second: 150.0,
            columns: vec![
                WaveColumn {
                    height: 20,
                    rgb: [255, 0, 0]
                };
                300
            ],
            bands: Vec::new(),
        };
        let cues = vec![
            CuePoint {
                kind: CueKind::Hot(1),
                position_ms: 30_100,
                loop_end_ms: None,
                color: Some([255, 0, 0]),
                comment: "Drop".into(),
            },
            CuePoint {
                kind: CueKind::Memory,
                position_ms: 100,
                loop_end_ms: None,
                color: None,
                comment: String::new(),
            },
        ];
        std::fs::write(
            anlz_dir.join("ANLZ0000.EXT"),
            anlz::encode_file(&[
                anlz::encode_pco2(&cues, false),
                anlz::encode_pwv5(&wave),
                anlz::encode_pco2(&cues, true),
            ]),
        )
        .unwrap();
        let art = root.join("PIONEER/Artwork/00001");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("a9.jpg"), b"\xff\xd8jpeg").unwrap();

        assert!(Export::exists(&root));
        let ex = Export::open(&root).unwrap();
        let t = ex.track(41).unwrap();
        assert_eq!(t.metadata.label(), "deadmau5 – Strobe");
        assert_eq!(t.metadata.bpm, Some(128.0));
        assert_eq!(t.metadata.rating, 4);
        assert_eq!(t.metadata.key, "10A");
        assert_eq!(t.beat_grid, grid);
        assert_eq!(t.waveform.unwrap().columns.len(), 300);
        assert_eq!(t.cues.len(), 2);
        assert_eq!(t.cues[0].kind, CueKind::Memory);
        assert_eq!(t.cues[1].comment, "Drop");
        assert_eq!(t.artwork.as_deref(), Some(&b"\xff\xd8jpeg"[..]));
        // Track ohne Analyse: nur Metadaten.
        let u = ex.track(42).unwrap();
        assert!(u.beat_grid.is_empty() && u.waveform.is_none());
        assert!(ex.track(99).is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}
