//! Erzeugt PRO-DJ-LINK-Pakete.
//!
//! Gebraucht für den eigenen Keep-Alive (virtueller Player), für den Simulator
//! und für Tests. Das Layout ist das Spiegelbild von [`crate::packet`].

use crate::packet::{
    flag, kind, percent_to_pitch, Beat, CdjStatus, KeepAlive, MixerStatus, OnAirChannels,
    PrecisePosition, MAGIC,
};

/// Länge eines CDJ-Status-Pakets mit aktueller NXS2-Firmware.
pub const CDJ_STATUS_LEN: usize = 0x11c;
pub const KEEP_ALIVE_LEN: usize = 0x36;
pub const BEAT_LEN: usize = 0x60;
pub const PRECISE_POSITION_LEN: usize = 0x3c;
pub const MIXER_STATUS_LEN: usize = 0x38;

fn header(len: usize, kind: u8) -> Vec<u8> {
    let mut d = vec![0u8; len];
    d[..10].copy_from_slice(MAGIC);
    d[0x0a] = kind;
    d
}

fn put_name(d: &mut [u8], at: usize, name: &str) {
    for (slot, b) in d[at..at + 20]
        .iter_mut()
        .zip(name.bytes().filter(u8::is_ascii))
    {
        *slot = b;
    }
}

fn put_u16(d: &mut [u8], at: usize, v: u16) {
    d[at..at + 2].copy_from_slice(&v.to_be_bytes());
}

fn put_u32(d: &mut [u8], at: usize, v: u32) {
    d[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

/// Kopf der Pakete auf Port 50001/50002: Name ab 0x0b, dann 0x01, Subtyp,
/// Gerätenummer und Länge des Rests.
fn put_short_header(d: &mut [u8], name: &str, subtype: u8, number: u8) {
    put_name(d, 0x0b, name);
    d[0x1f] = 0x01;
    d[0x20] = subtype;
    d[0x21] = number;
    let rest = (d.len() - 0x24) as u16;
    put_u16(d, 0x22, rest);
}

fn bpm_raw(bpm: Option<f64>) -> u16 {
    bpm.map_or(0xffff, |b| (b * 100.0).round().clamp(0.0, 65534.0) as u16)
}

pub fn keep_alive(k: &KeepAlive) -> Vec<u8> {
    let mut d = header(KEEP_ALIVE_LEN, kind::KEEP_ALIVE);
    put_name(&mut d, 0x0c, &k.name);
    d[0x20] = 0x01;
    d[0x21] = 0x02;
    put_u16(&mut d, 0x22, KEEP_ALIVE_LEN as u16);
    d[0x24] = k.number;
    d[0x25] = 0x01;
    d[0x26..0x2c].copy_from_slice(&k.mac);
    d[0x2c..0x30].copy_from_slice(&k.ip.octets());
    d[0x30] = k.peers_seen;
    d[0x34] = match k.kind {
        crate::packet::DeviceKind::Mixer => 0x02,
        crate::packet::DeviceKind::Rekordbox => 0x03,
        _ => 0x01,
    };
    d
}

pub fn cdj_status(s: &CdjStatus) -> Vec<u8> {
    let mut d = header(CDJ_STATUS_LEN, kind::CDJ_STATUS);
    put_short_header(&mut d, &s.name, 0x03, s.number);
    d[0x24] = s.number;
    d[0x27] = u8::from(s.rekordbox_id != 0);
    d[0x28] = s.track_source_player;
    d[0x29] = s.track_slot.to_byte();
    d[0x2a] = s.track_type;
    put_u32(&mut d, 0x2c, s.rekordbox_id);
    put_u16(&mut d, 0x32, s.track_number);
    d[0x7b] = s.play_state.to_byte();
    let mut f = 0x84; // feste Bits, die echte Player immer setzen
    if s.playing {
        f |= flag::PLAYING;
    }
    if s.master {
        f |= flag::MASTER;
    }
    if s.sync {
        f |= flag::SYNC;
    }
    if s.on_air {
        f |= flag::ON_AIR;
    }
    d[0x89] = f;
    d[0x8b] = if s.playing { 0x7a } else { 0x7e };
    let pitch = percent_to_pitch(s.pitch_percent);
    put_u32(&mut d, 0x8c, pitch);
    put_u16(&mut d, 0x90, 0x8000);
    put_u16(&mut d, 0x92, bpm_raw(s.track_bpm));
    put_u32(&mut d, 0x98, pitch);
    d[0x9d] = if s.playing { 0x01 } else { 0x00 };
    d[0x9f] = 0xff;
    put_u32(&mut d, 0xa0, s.beat_number.unwrap_or(0xffff_ffff));
    put_u16(&mut d, 0xa4, 0x01ff);
    d[0xa6] = s.beat_in_bar;
    put_u32(&mut d, 0xc0, pitch);
    put_u32(&mut d, 0xc4, pitch);
    put_u32(&mut d, 0xc8, s.packet_counter);
    d[0xcc] = 0x0f;
    d
}

pub fn mixer_status(m: &MixerStatus) -> Vec<u8> {
    let mut d = header(MIXER_STATUS_LEN, kind::MIXER_STATUS);
    put_short_header(&mut d, &m.name, 0x00, m.number);
    d[0x24] = m.number;
    d[0x27] = if m.master { 0xf0 } else { 0xd0 };
    put_u32(&mut d, 0x28, crate::packet::PITCH_NEUTRAL);
    put_u16(&mut d, 0x2e, bpm_raw(m.bpm));
    put_u32(&mut d, 0x30, crate::packet::PITCH_NEUTRAL);
    d[0x36] = m.beat_in_bar;
    d
}

pub fn beat(b: &Beat) -> Vec<u8> {
    let mut d = header(BEAT_LEN, kind::BEAT);
    put_short_header(&mut d, &b.name, 0x00, b.number);
    let beat_ms = b.next_beat_ms;
    put_u32(&mut d, 0x24, beat_ms);
    put_u32(&mut d, 0x28, beat_ms.saturating_mul(2));
    put_u32(&mut d, 0x2c, b.next_bar_ms);
    put_u32(&mut d, 0x30, beat_ms.saturating_mul(4));
    put_u32(
        &mut d,
        0x34,
        b.next_bar_ms.saturating_add(beat_ms.saturating_mul(4)),
    );
    put_u32(&mut d, 0x38, beat_ms.saturating_mul(8));
    for at in (0x3c..0x54).step_by(4) {
        put_u32(&mut d, at, 0xffff_ffff);
    }
    put_u32(&mut d, 0x54, percent_to_pitch(b.pitch_percent));
    put_u16(&mut d, 0x5a, bpm_raw(b.track_bpm));
    d[0x5c] = b.beat_in_bar;
    d[0x5f] = b.number;
    d
}

pub fn precise_position(p: &PrecisePosition) -> Vec<u8> {
    let mut d = header(PRECISE_POSITION_LEN, kind::PRECISE_POSITION);
    put_short_header(&mut d, &p.name, 0x02, p.number);
    put_u32(&mut d, 0x24, p.track_length_s);
    put_u32(&mut d, 0x28, p.position_ms);
    put_u32(
        &mut d,
        0x2c,
        (p.pitch_percent * 100.0).round() as i32 as u32,
    );
    let bpm = p.effective_bpm.map_or(0, |b| (b * 10.0).round() as u32);
    put_u32(&mut d, 0x38, bpm);
    d
}

pub fn on_air(o: &OnAirChannels) -> Vec<u8> {
    let len = if o.channels.len() > 4 { 0x35 } else { 0x2d };
    let mut d = header(len, kind::ON_AIR);
    put_short_header(&mut d, &o.name, 0x00, o.number);
    for (i, &on) in o.channels.iter().take(6).enumerate() {
        let at = if i < 4 { 0x24 + i } else { 0x2e + (i - 4) };
        d[at] = u8::from(on);
    }
    d
}

/// Kanal und Bytes eines zerlegten Pakets (für das Abspielen von Mitschnitten).
/// Unbekannte Pakete lassen sich nicht wiederherstellen.
pub fn packet(p: &crate::packet::Packet) -> Option<(crate::packet::Channel, Vec<u8>)> {
    use crate::packet::{Channel, Packet};
    Some(match p {
        Packet::KeepAlive(k) => (Channel::Announce, keep_alive(k)),
        Packet::CdjStatus(s) => (Channel::Status, cdj_status(s)),
        Packet::MixerStatus(m) => (Channel::Status, mixer_status(m)),
        Packet::Beat(b) => (Channel::Beat, beat(b)),
        Packet::PrecisePosition(p) => (Channel::Beat, precise_position(p)),
        Packet::OnAir(o) => (Channel::Beat, on_air(o)),
        Packet::Unknown { .. } => return None,
    })
}
