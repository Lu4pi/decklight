//! Parser für PRO-DJ-LINK-Pakete.
//!
//! Offsets nach der "DJ Link Ecosystem Analysis" von Deep Symmetry
//! (<https://djl-analysis.deepsymmetry.org/>). Alle Mehrbyte-Werte sind Big Endian.
//! Pakete, die wir nicht kennen oder die zu kurz sind, werden als
//! [`Packet::Unknown`] gemeldet und nie als Fehler behandelt: fremde Geräte im
//! Netz dürfen den Listener nicht aus dem Tritt bringen.

use serde::Serialize;
use std::net::Ipv4Addr;

/// Jedes PRO-DJ-LINK-Paket beginnt mit diesen zehn Bytes.
pub const MAGIC: &[u8; 10] = b"Qspt1WmJOL";

/// Der Mixer meldet sich immer mit dieser Gerätenummer.
pub const MIXER_DEVICE_NUMBER: u8 = 0x21;

/// Rohwert für 0 % Pitch (entspricht Faktor 1,0).
pub const PITCH_NEUTRAL: u32 = 0x10_0000;

/// UDP-Port, auf dem ein Paket ankam. Davon hängt die Bedeutung des Typ-Bytes ab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Channel {
    /// Port 50000: Keep-Alive, Anmeldung, Nummernvergabe.
    Announce,
    /// Port 50001: Beats, präzise Position, On-Air-Kanäle.
    Beat,
    /// Port 50002: Status von Playern und Mixer.
    Status,
}

pub mod kind {
    //! Typ-Bytes (Offset 0x0a) je Kanal.
    pub const KEEP_ALIVE: u8 = 0x06;
    pub const BEAT: u8 = 0x28;
    pub const PRECISE_POSITION: u8 = 0x0b;
    pub const ON_AIR: u8 = 0x03;
    pub const CDJ_STATUS: u8 = 0x0a;
    pub const MIXER_STATUS: u8 = 0x29;
}

/// Grobe Geräteklasse, abgeleitet aus Nummer, Typ-Byte und Name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Player,
    Mixer,
    Rekordbox,
    Other,
}

/// Keep-Alive auf Port 50000, etwa alle 1,5 s von jedem Gerät.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeepAlive {
    pub name: String,
    pub number: u8,
    pub kind: DeviceKind,
    pub mac: [u8; 6],
    pub ip: Ipv4Addr,
    pub peers_seen: u8,
}

/// Wiedergabezustand (Byte P1 im CDJ-Status).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayState {
    NoTrack,
    Loading,
    Playing,
    Looping,
    Paused,
    Cued,
    CuePlaying,
    CueScratch,
    Searching,
    SpunDown,
    Ended,
    Unknown(u8),
}

impl PlayState {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x00 => Self::NoTrack,
            0x02 => Self::Loading,
            0x03 => Self::Playing,
            0x04 => Self::Looping,
            0x05 => Self::Paused,
            0x06 => Self::Cued,
            0x07 => Self::CuePlaying,
            0x08 => Self::CueScratch,
            0x09 => Self::Searching,
            0x0e => Self::SpunDown,
            0x11 => Self::Ended,
            other => Self::Unknown(other),
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Self::NoTrack => 0x00,
            Self::Loading => 0x02,
            Self::Playing => 0x03,
            Self::Looping => 0x04,
            Self::Paused => 0x05,
            Self::Cued => 0x06,
            Self::CuePlaying => 0x07,
            Self::CueScratch => 0x08,
            Self::Searching => 0x09,
            Self::SpunDown => 0x0e,
            Self::Ended => 0x11,
            Self::Unknown(b) => b,
        }
    }

    /// Läuft der Playhead vorwärts?
    pub fn is_moving(self) -> bool {
        matches!(self, Self::Playing | Self::Looping | Self::CuePlaying)
    }
}

/// Medium, aus dem der Track geladen wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackSlot {
    None,
    Cd,
    Sd,
    Usb,
    Rekordbox,
    Unknown(u8),
}

impl TrackSlot {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::None,
            1 => Self::Cd,
            2 => Self::Sd,
            3 => Self::Usb,
            4 => Self::Rekordbox,
            other => Self::Unknown(other),
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Cd => 1,
            Self::Sd => 2,
            Self::Usb => 3,
            Self::Rekordbox => 4,
            Self::Unknown(b) => b,
        }
    }
}

/// Status-Flags (Byte F im CDJ-Status).
pub mod flag {
    pub const PLAYING: u8 = 0x40;
    pub const MASTER: u8 = 0x20;
    pub const SYNC: u8 = 0x10;
    pub const ON_AIR: u8 = 0x08;
}

/// Status eines Players (Port 50002, Typ 0x0a), etwa alle 200 ms.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CdjStatus {
    pub name: String,
    pub number: u8,
    /// Player, von dessen Medium der Track stammt (bei Link-Export ein anderer).
    pub track_source_player: u8,
    pub track_slot: TrackSlot,
    pub track_type: u8,
    /// Rekordbox-ID des Tracks, 0 wenn nichts geladen.
    pub rekordbox_id: u32,
    pub track_number: u16,
    pub play_state: PlayState,
    pub playing: bool,
    pub master: bool,
    pub sync: bool,
    pub on_air: bool,
    /// Pitch in Prozent (Fader-Stellung, z. B. +3,2).
    pub pitch_percent: f64,
    /// Track-BPM ohne Pitch, `None` wenn kein Track oder keine Analyse.
    pub track_bpm: Option<f64>,
    /// Beat-Nummer im Track (ab 1), `None` wenn unbekannt.
    pub beat_number: Option<u32>,
    /// Beat im Takt (1–4), 0 wenn unbekannt.
    pub beat_in_bar: u8,
    pub packet_counter: u32,
}

impl CdjStatus {
    /// BPM mit Pitch, so wie sie gerade zu hören sind.
    pub fn effective_bpm(&self) -> Option<f64> {
        self.track_bpm
            .map(|bpm| bpm * (1.0 + self.pitch_percent / 100.0))
    }
}

/// Status des Mixers (Port 50002, Typ 0x29).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MixerStatus {
    pub name: String,
    pub number: u8,
    pub master: bool,
    pub bpm: Option<f64>,
    pub beat_in_bar: u8,
}

/// Beat-Paket (Port 50001, Typ 0x28), bei jedem Beat eines laufenden Players.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Beat {
    pub name: String,
    pub number: u8,
    /// Millisekunden bis zum nächsten Beat (bei aktuellem Tempo).
    pub next_beat_ms: u32,
    /// Millisekunden bis zum nächsten Taktanfang.
    pub next_bar_ms: u32,
    pub pitch_percent: f64,
    pub track_bpm: Option<f64>,
    pub beat_in_bar: u8,
}

impl Beat {
    pub fn effective_bpm(&self) -> Option<f64> {
        self.track_bpm
            .map(|bpm| bpm * (1.0 + self.pitch_percent / 100.0))
    }
}

/// Präzise Position (Port 50001, Typ 0x0b), nur CDJ-3000, etwa alle 30 ms.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PrecisePosition {
    pub name: String,
    pub number: u8,
    pub track_length_s: u32,
    pub position_ms: u32,
    pub pitch_percent: f64,
    pub effective_bpm: Option<f64>,
}

/// On-Air-Zustand der Mixer-Kanäle (Port 50001, Typ 0x03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OnAirChannels {
    pub name: String,
    pub number: u8,
    /// Index 0 = Kanal 1. Vier Einträge beim DJM-900NXS2, sechs beim V10.
    pub channels: Vec<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Packet {
    KeepAlive(KeepAlive),
    CdjStatus(CdjStatus),
    MixerStatus(MixerStatus),
    Beat(Beat),
    PrecisePosition(PrecisePosition),
    OnAir(OnAirChannels),
    /// Gültiger Header, aber Typ unbekannt oder Paket zu kurz.
    Unknown {
        channel: Channel,
        kind: u8,
        len: usize,
    },
}

/// Prüft den Header und liefert das Typ-Byte.
pub fn packet_kind(data: &[u8]) -> Option<u8> {
    (data.len() > 0x0a && &data[..10] == MAGIC).then(|| data[0x0a])
}

/// Zerlegt ein Paket. `None` heißt: kein PRO-DJ-LINK-Paket.
pub fn parse(channel: Channel, data: &[u8]) -> Option<Packet> {
    let kind = packet_kind(data)?;
    let unknown = || Packet::Unknown {
        channel,
        kind,
        len: data.len(),
    };
    let parsed = match (channel, kind) {
        (Channel::Announce, kind::KEEP_ALIVE) => parse_keep_alive(data).map(Packet::KeepAlive),
        (Channel::Status, kind::CDJ_STATUS) => parse_cdj_status(data).map(Packet::CdjStatus),
        (Channel::Status, kind::MIXER_STATUS) => parse_mixer_status(data).map(Packet::MixerStatus),
        (Channel::Beat, kind::BEAT) => parse_beat(data).map(Packet::Beat),
        (Channel::Beat, kind::PRECISE_POSITION) => {
            parse_precise_position(data).map(Packet::PrecisePosition)
        }
        (Channel::Beat, kind::ON_AIR) => parse_on_air(data).map(Packet::OnAir),
        _ => None,
    };
    Some(parsed.unwrap_or_else(unknown))
}

/// Rechnet den Pitch-Rohwert (0x100000 = 0 %) in Prozent um.
pub fn pitch_to_percent(raw: u32) -> f64 {
    (f64::from(raw) - f64::from(PITCH_NEUTRAL)) / f64::from(PITCH_NEUTRAL) * 100.0
}

pub fn percent_to_pitch(percent: f64) -> u32 {
    let raw = f64::from(PITCH_NEUTRAL) * (1.0 + percent / 100.0);
    raw.round().clamp(0.0, f64::from(u32::MAX)) as u32
}

fn bpm_from_raw(raw: u16) -> Option<f64> {
    (raw != 0xffff && raw != 0).then(|| f64::from(raw) / 100.0)
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([d[at], d[at + 1]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

/// Gerätename: nullterminiertes ASCII. Auf Port 50000 ab 0x0c, sonst ab 0x0b.
fn name_at(d: &[u8], at: usize) -> String {
    let raw = &d[at..at + 20];
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).trim().to_string()
}

pub fn classify_device(number: u8, type_byte: u8, name: &str) -> DeviceKind {
    let upper = name.to_ascii_uppercase();
    if upper.starts_with("DJM") || number == MIXER_DEVICE_NUMBER {
        DeviceKind::Mixer
    } else if upper.contains("REKORDBOX") || type_byte == 0x03 {
        DeviceKind::Rekordbox
    } else if (1..=6).contains(&number) || upper.starts_with("CDJ") || upper.starts_with("XDJ") {
        DeviceKind::Player
    } else {
        DeviceKind::Other
    }
}

fn parse_keep_alive(d: &[u8]) -> Option<KeepAlive> {
    if d.len() < 0x36 {
        return None;
    }
    let name = name_at(d, 0x0c);
    let number = d[0x24];
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&d[0x26..0x2c]);
    Some(KeepAlive {
        kind: classify_device(number, d[0x34], &name),
        name,
        number,
        mac,
        ip: Ipv4Addr::new(d[0x2c], d[0x2d], d[0x2e], d[0x2f]),
        peers_seen: d[0x30],
    })
}

fn parse_cdj_status(d: &[u8]) -> Option<CdjStatus> {
    if d.len() < 0xcc {
        return None;
    }
    let flags = d[0x89];
    let beat = u32_at(d, 0xa0);
    Some(CdjStatus {
        name: name_at(d, 0x0b),
        number: d[0x21],
        track_source_player: d[0x28],
        track_slot: TrackSlot::from_byte(d[0x29]),
        track_type: d[0x2a],
        rekordbox_id: u32_at(d, 0x2c),
        track_number: u16_at(d, 0x32),
        play_state: PlayState::from_byte(d[0x7b]),
        playing: flags & flag::PLAYING != 0,
        master: flags & flag::MASTER != 0,
        sync: flags & flag::SYNC != 0,
        on_air: flags & flag::ON_AIR != 0,
        pitch_percent: pitch_to_percent(u32_at(d, 0x8c)),
        track_bpm: bpm_from_raw(u16_at(d, 0x92)),
        beat_number: (beat != 0xffff_ffff && beat != 0).then_some(beat),
        beat_in_bar: d[0xa6],
        packet_counter: u32_at(d, 0xc8),
    })
}

fn parse_mixer_status(d: &[u8]) -> Option<MixerStatus> {
    if d.len() < 0x37 {
        return None;
    }
    Some(MixerStatus {
        name: name_at(d, 0x0b),
        number: d[0x21],
        master: d[0x27] & flag::MASTER != 0,
        bpm: bpm_from_raw(u16_at(d, 0x2e)),
        beat_in_bar: d[0x36],
    })
}

fn parse_beat(d: &[u8]) -> Option<Beat> {
    if d.len() < 0x5d {
        return None;
    }
    Some(Beat {
        name: name_at(d, 0x0b),
        number: d[0x21],
        next_beat_ms: u32_at(d, 0x24),
        next_bar_ms: u32_at(d, 0x2c),
        pitch_percent: pitch_to_percent(u32_at(d, 0x54)),
        track_bpm: bpm_from_raw(u16_at(d, 0x5a)),
        beat_in_bar: d[0x5c],
    })
}

fn parse_precise_position(d: &[u8]) -> Option<PrecisePosition> {
    if d.len() < 0x3c {
        return None;
    }
    let bpm_x10 = u32_at(d, 0x38);
    Some(PrecisePosition {
        name: name_at(d, 0x0b),
        number: d[0x21],
        track_length_s: u32_at(d, 0x24),
        position_ms: u32_at(d, 0x28),
        pitch_percent: f64::from(u32_at(d, 0x2c) as i32) / 100.0,
        effective_bpm: (bpm_x10 != 0 && bpm_x10 != u32::MAX).then(|| f64::from(bpm_x10) / 10.0),
    })
}

fn parse_on_air(d: &[u8]) -> Option<OnAirChannels> {
    if d.len() < 0x28 {
        return None;
    }
    let mut channels: Vec<bool> = d[0x24..0x28].iter().map(|&b| b != 0).collect();
    // Sechs-Kanal-Mixer (DJM-V10) hängen Kanal 5 und 6 hinten an.
    if d.len() >= 0x30 {
        channels.extend(d[0x2e..0x30].iter().map(|&b| b != 0));
    }
    Some(OnAirChannels {
        name: name_at(d, 0x0b),
        number: d[0x21],
        channels,
    })
}
