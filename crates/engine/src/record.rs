//! Eingänge aufzeichnen und wieder abspielen (JSON Lines). Damit lassen sich
//! Abläufe vom echten Setup im Test exakt reproduzieren.

use crate::engine::Engine;
use crate::events::Stamped;
use decklight_prolink::Packet;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputRecord {
    pub t: f64,
    pub packet: Packet,
}

pub fn write_record(w: &mut impl Write, rec: &InputRecord) -> io::Result<()> {
    serde_json::to_writer(&mut *w, rec)?;
    w.write_all(b"\n")
}

pub fn read_records(r: impl BufRead) -> io::Result<Vec<InputRecord>> {
    let lines: Vec<String> = r.lines().collect::<io::Result<_>>()?;
    let lines: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        match serde_json::from_str(line) {
            Ok(rec) => out.push(rec),
            // Eine abgeschnittene letzte Zeile (Mitschnitt hart beendet) wird übersprungen.
            Err(_) if i + 1 == lines.len() => break,
            Err(e) => return Err(io::Error::other(format!("Zeile {}: {e}", i + 1))),
        }
    }
    Ok(out)
}

/// Spielt Aufzeichnungen durch eine Engine und sammelt die Ereignisse.
pub fn replay(engine: &mut Engine, records: &[InputRecord]) -> Vec<Stamped> {
    records
        .iter()
        .flat_map(|r| {
            engine
                .handle_packet(r.t, &r.packet)
                .into_iter()
                .map(|event| Stamped { t: r.t, event })
        })
        .collect()
}
