//! Client für den dbserver eines Players.

use super::field::Field;
use super::message::{dmst, item, kind, menu, Message, SETUP_TRANSACTION};
use crate::anlz;
use crate::track::{CuePoint, GridBeat, TrackMetadata, Waveform};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Verbindung zum dbserver: {0}")]
    Io(#[from] std::io::Error),
    #[error("dbserver antwortet nicht rechtzeitig")]
    Timeout,
    #[error("dbserver-Protokoll: {0}")]
    Protocol(String),
    #[error("Track nicht gefunden")]
    NotFound,
}

/// Rekordbox-Track auf einem Medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackLocation {
    /// Slot-Nummer wie im CDJ-Status (2 = SD, 3 = USB, 4 = rekordbox).
    pub slot: u8,
    /// Track-Typ wie im CDJ-Status (1 = rekordbox).
    pub track_type: u8,
    pub rekordbox_id: u32,
}

pub struct DbClient {
    stream: TcpStream,
    buf: Vec<u8>,
    next_tx: u32,
    /// Spielernummer, als die wir anfragen (1–4, siehe [`asking_player`]).
    player: u8,
}

/// Der dbserver beantwortet nur Anfragen "echter" Player-Nummern. Ist unsere eigene
/// Nummer größer als 4, geben wir uns als ein anderer vorhandener Player aus
/// (wie beat-link), nie aber als der Player, den wir gerade fragen.
pub fn asking_player(own: u8, target: u8, present: &[u8]) -> u8 {
    if (1..=4).contains(&own) && own != target {
        return own;
    }
    present
        .iter()
        .copied()
        .filter(|&n| (1..=4).contains(&n) && n != target)
        .min()
        .unwrap_or(if target == 4 { 3 } else { 4 })
}

impl DbClient {
    pub async fn connect(ip: IpAddr, asking_as: u8) -> Result<Self, DbError> {
        Self::connect_via(ip, super::QUERY_PORT, asking_as).await
    }

    /// Wie [`connect`], mit abweichendem Abfrage-Port (Tests, Simulator).
    pub async fn connect_via(ip: IpAddr, query_port: u16, asking_as: u8) -> Result<Self, DbError> {
        let port = timeout(query_db_port(SocketAddr::new(ip, query_port))).await??;
        let mut stream = timeout(TcpStream::connect(SocketAddr::new(ip, port))).await??;
        stream.set_nodelay(true)?;
        // Begrüßung: Zahl 1 hin, Zahl 1 zurück.
        let mut hello = Vec::new();
        Field::n4(1).encode(&mut hello);
        stream.write_all(&hello).await?;
        let mut answer = [0u8; 5];
        timeout(stream.read_exact(&mut answer)).await??;
        if answer[..] != hello[..] {
            return Err(DbError::Protocol("unerwartete Begrüßung".into()));
        }
        let mut client = Self {
            stream,
            buf: Vec::new(),
            next_tx: 1,
            player: asking_as,
        };
        let setup = Message::new(
            SETUP_TRANSACTION,
            kind::SETUP,
            vec![Field::n4(u32::from(asking_as))],
        );
        client.send(&setup).await?;
        let reply = client.read().await?;
        if reply.kind != kind::MENU_AVAILABLE {
            return Err(DbError::Protocol(format!(
                "Einrichtung abgelehnt ({:#06x})",
                reply.kind
            )));
        }
        Ok(client)
    }

    async fn send(&mut self, m: &Message) -> Result<(), DbError> {
        self.stream.write_all(&m.encode()).await?;
        Ok(())
    }

    async fn read(&mut self) -> Result<Message, DbError> {
        loop {
            if let Some((m, n)) = Message::decode(&self.buf).map_err(DbError::Protocol)? {
                self.buf.drain(..n);
                return Ok(m);
            }
            let mut chunk = [0u8; 8192];
            let n = timeout(self.stream.read(&mut chunk)).await??;
            if n == 0 {
                return Err(DbError::Protocol("Verbindung geschlossen".into()));
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    async fn request(&mut self, kind: u16, args: Vec<Field>) -> Result<Message, DbError> {
        let tx = self.next_tx;
        self.next_tx += 1;
        self.send(&Message::new(tx, kind, args)).await?;
        loop {
            let m = self.read().await?;
            if m.transaction == tx {
                return Ok(m);
            }
        }
    }

    fn dmst(&self, menu: u8, loc: &TrackLocation) -> Field {
        dmst(self.player, menu, loc.slot, loc.track_type)
    }

    pub async fn metadata(&mut self, loc: TrackLocation) -> Result<TrackMetadata, DbError> {
        let d = self.dmst(menu::MAIN, &loc);
        let avail = self
            .request(
                kind::REKORDBOX_METADATA,
                vec![d.clone(), Field::n4(loc.rekordbox_id)],
            )
            .await?;
        let count = avail.arg_u32(1).unwrap_or(0);
        if avail.kind != kind::MENU_AVAILABLE || count == 0 || count == u32::MAX {
            return Err(DbError::NotFound);
        }
        let tx = self.next_tx;
        self.next_tx += 1;
        let render = Message::new(
            tx,
            kind::RENDER_MENU,
            vec![
                d,
                Field::n4(0),
                Field::n4(count),
                Field::n4(0),
                Field::n4(count),
                Field::n4(0),
            ],
        );
        self.send(&render).await?;
        let mut md = TrackMetadata {
            rekordbox_id: loc.rekordbox_id,
            ..Default::default()
        };
        loop {
            let m = self.read().await?;
            match m.kind {
                kind::MENU_FOOTER => break,
                kind::MENU_ITEM => apply_item(&mut md, &m),
                _ => {}
            }
        }
        Ok(md)
    }

    async fn blob(&mut self, kind: u16, expect: u16, args: Vec<Field>) -> Result<Vec<u8>, DbError> {
        let m = self.request(kind, args).await?;
        if m.kind != expect {
            return Err(DbError::NotFound);
        }
        m.arg_blob(3)
            .map(<[u8]>::to_vec)
            .filter(|b| !b.is_empty())
            .ok_or(DbError::NotFound)
    }

    pub async fn artwork(
        &mut self,
        loc: TrackLocation,
        artwork_id: u32,
    ) -> Result<Vec<u8>, DbError> {
        let d = self.dmst(menu::DATA, &loc);
        self.blob(
            kind::ARTWORK,
            kind::ARTWORK_RESPONSE,
            vec![d, Field::n4(artwork_id)],
        )
        .await
    }

    pub async fn beat_grid(&mut self, loc: TrackLocation) -> Result<Vec<GridBeat>, DbError> {
        let d = self.dmst(menu::DATA, &loc);
        let blob = self
            .blob(
                kind::BEAT_GRID,
                kind::BEAT_GRID_RESPONSE,
                vec![d, Field::n4(loc.rekordbox_id)],
            )
            .await?;
        Ok(anlz::parse_dbserver_beat_grid(&blob))
    }

    /// Ein Tag aus den ANLZ-Dateien, z. B. `PWV5` aus `EXT`.
    pub async fn anlz_tag(
        &mut self,
        loc: TrackLocation,
        fourcc: &[u8; 4],
        ext: &[u8; 3],
    ) -> Result<Vec<u8>, DbError> {
        let d = self.dmst(menu::DATA, &loc);
        let tag = Field::n4(u32::from_le_bytes(*fourcc));
        let ext = Field::n4(u32::from_le_bytes([ext[0], ext[1], ext[2], 0]));
        self.blob(
            kind::ANLZ_TAG,
            kind::ANLZ_TAG_RESPONSE,
            vec![d, Field::n4(loc.rekordbox_id), tag, ext],
        )
        .await
    }

    /// Detail-Wellenform: die Drei-Band-Variante des CDJ-3000 (`PWV7` aus
    /// `2EX`), sonst die RGB-Variante (`PWV5` aus `EXT`).
    pub async fn waveform(&mut self, loc: TrackLocation) -> Result<Waveform, DbError> {
        if let Ok(blob) = self.anlz_tag(loc, b"PWV7", b"2EX").await {
            if let Some(w) = anlz::find_tag(&blob, b"PWV7").and_then(anlz::parse_pwv7) {
                if !w.bands.is_empty() {
                    return Ok(w);
                }
            }
        }
        let blob = self.anlz_tag(loc, b"PWV5", b"EXT").await?;
        anlz::find_tag(&blob, b"PWV5")
            .and_then(anlz::parse_pwv5)
            .ok_or(DbError::NotFound)
    }

    /// Hot Cues und Memory Cues.
    pub async fn cues(&mut self, loc: TrackLocation) -> Result<Vec<CuePoint>, DbError> {
        let blob = self.anlz_tag(loc, b"PCO2", b"EXT").await?;
        let mut cues = Vec::new();
        // Die EXT-Datei enthält zwei PCO2-Tags (Memory und Hot Cues) hintereinander.
        let mut i = 0;
        while i + 12 <= blob.len() {
            if &blob[i..i + 4] == b"PCO2" {
                let len = u32::from_be_bytes([blob[i + 8], blob[i + 9], blob[i + 10], blob[i + 11]])
                    as usize;
                let end = (i + len.max(12)).min(blob.len());
                cues.extend(anlz::parse_pco2(&blob[i..end]).unwrap_or_default());
                i = end;
            } else {
                i += 1;
            }
        }
        cues.sort_by_key(|c| c.position_ms);
        Ok(cues)
    }

    pub async fn close(mut self) {
        let _ = self
            .send(&Message::new(SETUP_TRANSACTION, kind::TEARDOWN, vec![]))
            .await;
    }
}

fn apply_item(md: &mut TrackMetadata, m: &Message) {
    let label = m.arg_str(3).unwrap_or_default().to_string();
    let number = m.arg_u32(1).unwrap_or(0);
    match m.arg_u32(6).unwrap_or(0) & 0xffff {
        item::TITLE => {
            md.title = label;
            md.artwork_id = m.arg_u32(8).unwrap_or(0);
        }
        item::ARTIST => md.artist = label,
        item::ALBUM => md.album = label,
        item::GENRE => md.genre = label,
        item::KEY => md.key = label,
        item::COMMENT => md.comment = label,
        item::DURATION => md.duration_s = number,
        item::TEMPO => md.bpm = (number > 0).then(|| f64::from(number) / 100.0),
        item::RATING => md.rating = number as u8,
        _ => {}
    }
}

async fn query_db_port(addr: SocketAddr) -> Result<u16, DbError> {
    let mut s = TcpStream::connect(addr).await?;
    let mut q = vec![0, 0, 0, 0x0f];
    q.extend_from_slice(b"RemoteDBServer\0");
    s.write_all(&q).await?;
    let mut port = [0u8; 2];
    s.read_exact(&mut port).await?;
    Ok(u16::from_be_bytes(port))
}

async fn timeout<T>(f: impl std::future::Future<Output = T>) -> Result<T, DbError> {
    tokio::time::timeout(TIMEOUT, f)
        .await
        .map_err(|_| DbError::Timeout)
}
