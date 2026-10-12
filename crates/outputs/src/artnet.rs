//! Art-Net Timecode (OpTimeCode, 0x9700), wie ihn grandMA3 und andere Pulte empfangen.

use crate::timecode::Timecode;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

pub const ARTNET_PORT: u16 = 6454;
pub const OP_TIMECODE: u16 = 0x9700;
pub const PACKET_LEN: usize = 19;

pub fn encode(tc: &Timecode) -> [u8; PACKET_LEN] {
    let mut d = [0u8; PACKET_LEN];
    d[..8].copy_from_slice(b"Art-Net\0");
    d[8..10].copy_from_slice(&OP_TIMECODE.to_le_bytes());
    d[10] = 0; // Protokollversion, High-Byte
    d[11] = 14; // Protokollversion 14
    d[12] = 0; // Filler
    d[13] = 0; // Stream-ID (0 = Master)
    d[14] = tc.frames;
    d[15] = tc.seconds;
    d[16] = tc.minutes;
    d[17] = tc.hours;
    d[18] = tc.rate.code();
    d
}

/// Liest ein OpTimeCode-Paket (für Tests und Monitoring).
pub fn decode(d: &[u8]) -> Option<(u8, u8, u8, u8, u8)> {
    (d.len() >= PACKET_LEN && &d[..8] == b"Art-Net\0" && d[8..10] == OP_TIMECODE.to_le_bytes())
        .then(|| (d[17], d[16], d[15], d[14], d[18]))
}

/// Sendet Art-Net-Timecode per Broadcast oder Unicast.
pub struct ArtNetSender {
    socket: UdpSocket,
    target: SocketAddr,
}

impl ArtNetSender {
    /// `target` z. B. 2.255.255.255, 10.255.255.255 oder die IP des Pults.
    pub fn new(target: Ipv4Addr, port: u16) -> io::Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_broadcast(true)?;
        Ok(Self {
            socket,
            target: SocketAddr::from((target, port)),
        })
    }

    pub fn send(&self, tc: &Timecode) -> io::Result<()> {
        self.socket.send_to(&encode(tc), self.target).map(|_| ())
    }

    pub fn target(&self) -> SocketAddr {
        self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timecode::FrameRate;

    #[test]
    fn layout() {
        let tc = Timecode::parse("01:02:03:04", FrameRate::Fps25).unwrap();
        let d = encode(&tc);
        assert_eq!(&d[..12], b"Art-Net\0\x00\x97\x00\x0e");
        assert_eq!(&d[14..], &[4, 3, 2, 1, 1]);
        assert_eq!(decode(&d), Some((1, 2, 3, 4, 1)));
    }
}
