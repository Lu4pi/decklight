//! Nachrichten des dbserver-Protokolls (TCP, meist Port 1051).
//!
//! Aufbau: Magic (Zahl 0x872349ae), Transaktions-ID, Typ (2 Byte),
//! Anzahl Argumente (1 Byte), Liste der Argumenttypen (Blob mit 12 Bytes),
//! dann die Argumente selbst.

use super::field::Field;

pub const MAGIC: u32 = 0x8723_49ae;
/// Transaktions-ID der Begrüßung/Einrichtung.
pub const SETUP_TRANSACTION: u32 = 0xffff_fffe;

pub mod kind {
    pub const SETUP: u16 = 0x0000;
    pub const TEARDOWN: u16 = 0x0100;
    pub const REKORDBOX_METADATA: u16 = 0x2002;
    /// Metadaten nicht analysierter Dateien (nur Tags, kein Grid, keine Wellenform).
    pub const UNANALYZED_METADATA: u16 = 0x2202;
    pub const ARTWORK: u16 = 0x2003;
    pub const BEAT_GRID: u16 = 0x2204;
    pub const ANLZ_TAG: u16 = 0x2c04;
    /// Blaue Detail-Wellenform; der CDJ-3000 liefert sie auch für Dateien, die
    /// er selbst analysiert hat (ohne rekordbox).
    pub const WAVE_DETAIL: u16 = 0x2904;
    pub const WAVE_DETAIL_RESPONSE: u16 = 0x4a02;
    pub const RENDER_MENU: u16 = 0x3000;

    pub const MENU_AVAILABLE: u16 = 0x4000;
    pub const MENU_HEADER: u16 = 0x4001;
    pub const ARTWORK_RESPONSE: u16 = 0x4002;
    pub const MENU_ITEM: u16 = 0x4101;
    pub const MENU_FOOTER: u16 = 0x4201;
    pub const BEAT_GRID_RESPONSE: u16 = 0x4602;
    pub const ANLZ_TAG_RESPONSE: u16 = 0x4f02;
}

/// Menü-Kennung im DMST-Argument.
pub mod menu {
    pub const MAIN: u8 = 0x01;
    pub const DATA: u8 = 0x08;
}

/// Art der Menüzeilen (Argument 6 einer 0x4101-Nachricht).
pub mod item {
    pub const ALBUM: u32 = 0x0002;
    pub const TITLE: u32 = 0x0004;
    pub const GENRE: u32 = 0x0006;
    pub const ARTIST: u32 = 0x0007;
    pub const RATING: u32 = 0x000a;
    pub const DURATION: u32 = 0x000b;
    pub const TEMPO: u32 = 0x000d;
    pub const KEY: u32 = 0x000f;
    pub const COMMENT: u32 = 0x0023;
}

/// Packt anfragenden Player, Menü, Slot und Track-Typ in eine Zahl.
pub fn dmst(player: u8, menu: u8, slot: u8, track_type: u8) -> Field {
    Field::n4(u32::from_be_bytes([player, menu, slot, track_type]))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub transaction: u32,
    pub kind: u16,
    pub args: Vec<Field>,
}

impl Message {
    pub fn new(transaction: u32, kind: u16, args: Vec<Field>) -> Self {
        Self {
            transaction,
            kind,
            args,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        Field::n4(MAGIC).encode(&mut out);
        Field::n4(self.transaction).encode(&mut out);
        Field::n2(self.kind).encode(&mut out);
        Field::n1(self.args.len() as u8).encode(&mut out);
        let mut tags = [0u8; 12];
        for (slot, a) in tags.iter_mut().zip(&self.args) {
            *slot = a.arg_tag();
        }
        Field::Blob(tags.to_vec()).encode(&mut out);
        for a in &self.args {
            a.encode(&mut out);
        }
        out
    }

    /// Liest eine Nachricht. `Ok(None)`: noch nicht vollständig im Puffer.
    pub fn decode(d: &[u8]) -> Result<Option<(Message, usize)>, String> {
        let mut at = 0;
        let mut fields = Vec::new();
        // Kopf: Magic, Transaktion, Typ, Anzahl, Typenliste.
        for _ in 0..5 {
            match Field::decode(&d[at..])? {
                Some((f, n)) => {
                    fields.push(f);
                    at += n;
                }
                None => return Ok(None),
            }
        }
        if fields[0].as_u32() != Some(MAGIC) {
            return Err("falsches Magic".into());
        }
        let transaction = fields[1].as_u32().ok_or("Transaktion fehlt")?;
        let kind = fields[2].as_u32().ok_or("Typ fehlt")? as u16;
        let count = fields[3].as_u32().ok_or("Anzahl fehlt")? as usize;
        let mut args = Vec::with_capacity(count);
        for _ in 0..count {
            match Field::decode(&d[at..])? {
                Some((f, n)) => {
                    args.push(f);
                    at += n;
                }
                None => return Ok(None),
            }
        }
        Ok(Some((
            Message {
                transaction,
                kind,
                args,
            },
            at,
        )))
    }

    pub fn arg_u32(&self, i: usize) -> Option<u32> {
        self.args.get(i).and_then(Field::as_u32)
    }

    pub fn arg_str(&self, i: usize) -> Option<&str> {
        self.args.get(i).and_then(Field::as_str)
    }

    pub fn arg_blob(&self, i: usize) -> Option<&[u8]> {
        self.args.get(i).and_then(Field::as_blob)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_request_bytes() {
        let m = Message::new(
            1,
            kind::REKORDBOX_METADATA,
            vec![dmst(1, menu::MAIN, 3, 1), Field::n4(42)],
        );
        let bytes = m.encode();
        assert_eq!(
            bytes,
            [
                0x11, 0x87, 0x23, 0x49, 0xae, // Magic
                0x11, 0, 0, 0, 1, // Transaktion
                0x10, 0x20, 0x02, // Typ
                0x0f, 0x02, // zwei Argumente
                0x14, 0, 0, 0, 12, 6, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // Typenliste
                0x11, 1, 1, 3, 1, // DMST
                0x11, 0, 0, 0, 42, // Rekordbox-ID
            ]
        );
        assert_eq!(Message::decode(&bytes).unwrap(), Some((m, bytes.len())));
        assert_eq!(Message::decode(&bytes[..bytes.len() - 1]).unwrap(), None);
    }

    #[test]
    fn string_and_blob_roundtrip() {
        let m = Message::new(
            7,
            kind::MENU_ITEM,
            vec![
                Field::n4(0),
                Field::String("Bürobeat ♪".into()),
                Field::Blob(vec![1, 2, 3]),
            ],
        );
        let bytes = m.encode();
        let (back, n) = Message::decode(&bytes).unwrap().unwrap();
        assert_eq!(n, bytes.len());
        assert_eq!(back.arg_str(1), Some("Bürobeat ♪"));
        assert_eq!(back.arg_blob(2), Some(&[1u8, 2, 3][..]));
    }
}
