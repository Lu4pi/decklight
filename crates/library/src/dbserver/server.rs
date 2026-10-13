//! Kleiner dbserver für Simulator und Tests: beantwortet dieselben Anfragen wie
//! ein Player (Metadaten, Cover, Beatgrid, Wellenform, Cues).

use super::field::Field;
use super::message::{item, kind, Message};
use crate::anlz;
use crate::track::TrackData;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// Liefert die Tracks eines simulierten Players.
pub trait TrackSource: Send + Sync + 'static {
    fn track(&self, slot: u8, rekordbox_id: u32) -> Option<TrackData>;
}

pub struct DbServer {
    pub query_port: u16,
    pub db_port: u16,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for DbServer {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

impl DbServer {
    /// `query_port` 0 = beliebig (Tests), 12523 = wie ein echter Player.
    pub async fn start(
        ip: IpAddr,
        query_port: u16,
        source: Arc<dyn TrackSource>,
    ) -> std::io::Result<Self> {
        let query = TcpListener::bind(SocketAddr::new(ip, query_port)).await?;
        let db = TcpListener::bind(SocketAddr::new(ip, 0)).await?;
        let (query_port, db_port) = (query.local_addr()?.port(), db.local_addr()?.port());
        let q = tokio::spawn(async move {
            while let Ok((mut s, _)) = query.accept().await {
                tokio::spawn(async move {
                    let mut req = [0u8; 19];
                    if s.read_exact(&mut req).await.is_ok() && &req[4..18] == b"RemoteDBServer" {
                        let _ = s.write_all(&db_port.to_be_bytes()).await;
                    }
                });
            }
        });
        let d = tokio::spawn(async move {
            while let Ok((s, _)) = db.accept().await {
                let source = source.clone();
                tokio::spawn(async move {
                    if let Err(e) = serve(s, source).await {
                        tracing::debug!(error = %e, "dbserver-Sitzung beendet");
                    }
                });
            }
        });
        Ok(Self {
            query_port,
            db_port,
            tasks: vec![q, d],
        })
    }
}

async fn serve(mut s: TcpStream, source: Arc<dyn TrackSource>) -> std::io::Result<()> {
    let mut hello = [0u8; 5];
    s.read_exact(&mut hello).await?;
    s.write_all(&hello).await?;
    let mut buf = Vec::new();
    let mut pending: Vec<Message> = Vec::new();
    loop {
        let m = loop {
            match Message::decode(&buf).map_err(std::io::Error::other)? {
                Some((m, n)) => {
                    buf.drain(..n);
                    break m;
                }
                None => {
                    let mut chunk = [0u8; 4096];
                    let n = s.read(&mut chunk).await?;
                    if n == 0 {
                        return Ok(());
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
            }
        };
        let tx = m.transaction;
        let slot = m.arg_u32(0).map_or(0, |d| (d >> 8) as u8);
        let id = m.arg_u32(1).unwrap_or(0);
        let reply = |kind: u16, args: Vec<Field>| Message::new(tx, kind, args);
        let blob_reply = |kind_req: u16, kind_resp: u16, blob: Option<Vec<u8>>| match blob {
            Some(b) => reply(
                kind_resp,
                vec![
                    Field::n4(u32::from(kind_req)),
                    Field::n4(0),
                    Field::n4(b.len() as u32),
                    Field::Blob(b),
                ],
            ),
            None => reply(
                kind::MENU_AVAILABLE,
                vec![Field::n4(u32::from(kind_req)), Field::n4(0)],
            ),
        };
        let out: Vec<Message> = match m.kind {
            kind::SETUP => vec![reply(
                kind::MENU_AVAILABLE,
                vec![Field::n4(0), Field::n4(1)],
            )],
            kind::TEARDOWN => return Ok(()),
            kind::REKORDBOX_METADATA | kind::UNANALYZED_METADATA => {
                pending = source
                    .track(slot, id)
                    .map(|t| menu_items(&t))
                    .unwrap_or_default();
                let count = if pending.is_empty() {
                    u32::MAX
                } else {
                    pending.len() as u32
                };
                vec![reply(
                    kind::MENU_AVAILABLE,
                    vec![Field::n4(u32::from(m.kind)), Field::n4(count)],
                )]
            }
            kind::RENDER_MENU => {
                let mut v = vec![reply(kind::MENU_HEADER, vec![])];
                v.extend(std::mem::take(&mut pending).into_iter().map(|mut i| {
                    i.transaction = tx;
                    i
                }));
                v.push(reply(kind::MENU_FOOTER, vec![]));
                v
            }
            kind::ARTWORK => {
                // Cover-ID = Rekordbox-ID im Simulator.
                let art = source.track(slot, id).and_then(|t| t.artwork);
                vec![blob_reply(kind::ARTWORK, kind::ARTWORK_RESPONSE, art)]
            }
            kind::BEAT_GRID => {
                let grid = source
                    .track(slot, id)
                    .map(|t| anlz::encode_dbserver_beat_grid(&t.beat_grid));
                vec![blob_reply(kind::BEAT_GRID, kind::BEAT_GRID_RESPONSE, grid)]
            }
            kind::ANLZ_TAG => {
                let fourcc = m.arg_u32(2).unwrap_or(0).to_le_bytes();
                let blob = source.track(slot, id).and_then(|t| match &fourcc {
                    b"PWV5" => t.waveform.as_ref().map(anlz::encode_pwv5),
                    b"PWV7" => t
                        .waveform
                        .as_ref()
                        .filter(|w| !w.bands.is_empty())
                        .map(anlz::encode_pwv7),
                    b"PCO2" => {
                        let mut b = anlz::encode_pco2(&t.cues, false);
                        b.extend(anlz::encode_pco2(&t.cues, true));
                        Some(b)
                    }
                    _ => None,
                });
                vec![blob_reply(kind::ANLZ_TAG, kind::ANLZ_TAG_RESPONSE, blob)]
            }
            kind::WAVE_DETAIL => {
                let blob = source.track(slot, id).and_then(|t| {
                    t.waveform.map(|w| {
                        let mut b = vec![0u8; 19];
                        b.extend(w.columns.iter().map(|c| {
                            let light =
                                (c.rgb.iter().map(|&v| u16::from(v)).sum::<u16>() * 7 / 765) as u8;
                            (c.height & 0x1f) | (light.min(7) << 5)
                        }));
                        b
                    })
                });
                vec![blob_reply(
                    kind::WAVE_DETAIL,
                    kind::WAVE_DETAIL_RESPONSE,
                    blob,
                )]
            }
            other => vec![reply(
                kind::MENU_AVAILABLE,
                vec![Field::n4(u32::from(other)), Field::n4(0)],
            )],
        };
        let mut bytes = Vec::new();
        for o in out {
            bytes.extend(o.encode());
        }
        s.write_all(&bytes).await?;
    }
}

fn menu_items(t: &TrackData) -> Vec<Message> {
    let md = &t.metadata;
    let row = |number: u32, label: &str, kind: u32, artwork: u32| {
        Message::new(
            0,
            super::message::kind::MENU_ITEM,
            vec![
                Field::n4(0),
                Field::n4(number),
                Field::n4(((label.encode_utf16().count() + 1) * 2) as u32),
                Field::String(label.to_string()),
                Field::n4(2),
                Field::String(String::new()),
                Field::n4(kind),
                Field::n4(0),
                Field::n4(artwork),
                Field::n4(0),
                Field::n4(0),
                Field::n4(0),
            ],
        )
    };
    vec![
        row(md.rekordbox_id, &md.title, item::TITLE, md.artwork_id),
        row(1, &md.artist, item::ARTIST, 0),
        row(2, &md.album, item::ALBUM, 0),
        row(md.duration_s, "", item::DURATION, 0),
        row(
            md.bpm.map_or(0, |b| (b * 100.0).round() as u32),
            "",
            item::TEMPO,
            0,
        ),
        row(3, &md.genre, item::GENRE, 0),
        row(4, &md.key, item::KEY, 0),
        row(u32::from(md.rating), "", item::RATING, 0),
        row(5, &md.comment, item::COMMENT, 0),
    ]
}
