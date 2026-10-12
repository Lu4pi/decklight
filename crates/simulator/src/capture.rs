//! Mitschnitte abspielen: Wireshark-Dateien (pcap, pcapng) oder DeckLight-
//! Mitschnitte (`decklight record`, JSON Lines) werden mit ihrem Zeitverlauf an
//! die eigenen PRO-DJ-LINK-Ports geschickt, als stünden die Player im Netz.

use crate::Target;
use decklight_prolink::Channel;
use std::net::SocketAddr;
use std::time::Duration;

/// Ein Paket im Mitschnitt: Zeit seit Beginn in Sekunden, Kanal, Nutzdaten.
#[derive(Debug, Clone, PartialEq)]
pub struct Captured {
    pub t: f64,
    pub channel: Channel,
    pub bytes: Vec<u8>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CaptureError {
    #[error("keine pcap- oder pcapng-Datei")]
    Format,
    #[error("Datei endet mitten in einem Paket")]
    Truncated,
    #[error("Verbindungsart {0} wird nicht unterstützt (Ethernet, Linux SLL, Loopback, Raw IP)")]
    LinkType(u32),
}

fn channel_of(port: u16) -> Option<Channel> {
    match port {
        50000 => Some(Channel::Announce),
        50001 => Some(Channel::Beat),
        50002 => Some(Channel::Status),
        _ => None,
    }
}

/// UDP-Nutzdaten an Port 50000–50002 aus einem Rahmen der Verbindungsart `link`.
fn udp_payload(link: u32, frame: &[u8]) -> Option<(Channel, &[u8])> {
    let ip = match link {
        1 => {
            // Ethernet, ggf. mit VLAN-Tag.
            let mut off = 12;
            let mut ty = u16::from_be_bytes([*frame.get(off)?, *frame.get(off + 1)?]);
            while ty == 0x8100 {
                off += 4;
                ty = u16::from_be_bytes([*frame.get(off)?, *frame.get(off + 1)?]);
            }
            (ty == 0x0800).then_some(frame.get(off + 2..)?)?
        }
        113 => {
            let ty = u16::from_be_bytes([*frame.get(14)?, *frame.get(15)?]);
            (ty == 0x0800).then_some(frame.get(16..)?)?
        }
        276 => {
            let ty = u16::from_be_bytes([*frame.first()?, *frame.get(1)?]);
            (ty == 0x0800).then_some(frame.get(20..)?)?
        }
        0 | 108 => {
            let fam = u32::from_le_bytes(frame.get(..4)?.try_into().ok()?);
            let fam_be = u32::from_be_bytes(frame.get(..4)?.try_into().ok()?);
            (fam == 2 || fam_be == 2).then_some(frame.get(4..)?)?
        }
        101 | 228 => frame,
        _ => return None,
    };
    if ip.first()? >> 4 != 4 || *ip.get(9)? != 17 {
        return None;
    }
    let ihl = usize::from(ip[0] & 0x0f) * 4;
    let udp = ip.get(ihl..)?;
    let dst = u16::from_be_bytes([*udp.get(2)?, *udp.get(3)?]);
    let len = usize::from(u16::from_be_bytes([*udp.get(4)?, *udp.get(5)?]));
    let payload = udp.get(8..len.max(8).min(udp.len()))?;
    Some((channel_of(dst)?, payload))
}

fn supported(link: u32) -> bool {
    matches!(link, 0 | 1 | 101 | 108 | 113 | 228 | 276)
}

/// Liest pcap (µs oder ns, beide Byte-Reihenfolgen) und pcapng.
pub fn parse_pcap(data: &[u8]) -> Result<Vec<Captured>, CaptureError> {
    let magic = data.get(..4).ok_or(CaptureError::Format)?;
    let mut raw: Vec<(f64, Channel, Vec<u8>)> = Vec::new();
    match magic {
        [0x0a, 0x0d, 0x0d, 0x0a] => parse_ng(data, &mut raw)?,
        _ => {
            let (le, nanos) = match magic {
                [0xd4, 0xc3, 0xb2, 0xa1] => (true, false),
                [0xa1, 0xb2, 0xc3, 0xd4] => (false, false),
                [0x4d, 0x3c, 0xb2, 0xa1] => (true, true),
                [0xa1, 0xb2, 0x3c, 0x4d] => (false, true),
                _ => return Err(CaptureError::Format),
            };
            let u32_at = |at: usize| -> Result<u32, CaptureError> {
                let b: [u8; 4] = data
                    .get(at..at + 4)
                    .ok_or(CaptureError::Truncated)?
                    .try_into()
                    .map_err(|_| CaptureError::Truncated)?;
                Ok(if le {
                    u32::from_le_bytes(b)
                } else {
                    u32::from_be_bytes(b)
                })
            };
            let link = u32_at(20)? & 0x0fff_ffff;
            if !supported(link) {
                return Err(CaptureError::LinkType(link));
            }
            let mut off = 24;
            while off + 16 <= data.len() {
                let sec = f64::from(u32_at(off)?);
                let frac = f64::from(u32_at(off + 4)?);
                let incl = u32_at(off + 8)? as usize;
                let frame = data
                    .get(off + 16..off + 16 + incl)
                    .ok_or(CaptureError::Truncated)?;
                let t = sec + frac / if nanos { 1e9 } else { 1e6 };
                if let Some((ch, p)) = udp_payload(link, frame) {
                    raw.push((t, ch, p.to_vec()));
                }
                off += 16 + incl;
            }
        }
    }
    Ok(normalize(raw))
}

fn parse_ng(data: &[u8], raw: &mut Vec<(f64, Channel, Vec<u8>)>) -> Result<(), CaptureError> {
    let mut le = true;
    // Je Schnittstelle: Verbindungsart und Zeitauflösung (Ticks pro Sekunde).
    let mut ifaces: Vec<(u32, f64)> = Vec::new();
    let mut off = 0;
    while off + 12 <= data.len() {
        let rd = |at: usize, le: bool| -> Result<u32, CaptureError> {
            let b: [u8; 4] = data
                .get(at..at + 4)
                .ok_or(CaptureError::Truncated)?
                .try_into()
                .map_err(|_| CaptureError::Truncated)?;
            Ok(if le {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            })
        };
        let ty = rd(off, le)?;
        if ty == 0x0a0d_0d0a {
            le = data.get(off + 8..off + 12) == Some(&[0x4d, 0x3c, 0x2b, 0x1a]);
            ifaces.clear();
        }
        let len = rd(off + 4, le)? as usize;
        if len < 12 || off + len > data.len() {
            return Err(CaptureError::Truncated);
        }
        let body = &data[off + 8..off + len - 4];
        match ty {
            1 => {
                let rd16 = |b: &[u8]| {
                    if le {
                        u16::from_le_bytes([b[0], b[1]])
                    } else {
                        u16::from_be_bytes([b[0], b[1]])
                    }
                };
                let link = u32::from(rd16(body.get(..2).ok_or(CaptureError::Truncated)?));
                // Option if_tsresol (9): Standard 10^-6.
                let mut res = 1e6;
                let mut o = 8;
                while o + 4 <= body.len() {
                    let code = rd16(&body[o..o + 2]);
                    let olen = usize::from(rd16(&body[o + 2..o + 4]));
                    if code == 0 {
                        break;
                    }
                    if code == 9 && olen >= 1 {
                        let v = body.get(o + 4).copied().unwrap_or(6);
                        res = if v & 0x80 != 0 {
                            2f64.powi(i32::from(v & 0x7f))
                        } else {
                            10f64.powi(i32::from(v))
                        };
                    }
                    o += 4 + olen.div_ceil(4) * 4;
                }
                ifaces.push((link, res));
            }
            6 => {
                let r = |at: usize| rd(off + 8 + at, le);
                let iface = r(0)? as usize;
                let ts = (u64::from(r(4)?) << 32) | u64::from(r(8)?);
                let incl = r(12)? as usize;
                let frame = body.get(20..20 + incl).ok_or(CaptureError::Truncated)?;
                let &(link, res) = ifaces.get(iface).ok_or(CaptureError::Format)?;
                if !supported(link) {
                    return Err(CaptureError::LinkType(link));
                }
                if let Some((ch, p)) = udp_payload(link, frame) {
                    raw.push((ts as f64 / res, ch, p.to_vec()));
                }
            }
            _ => {}
        }
        off += len;
    }
    Ok(())
}

fn normalize(mut raw: Vec<(f64, Channel, Vec<u8>)>) -> Vec<Captured> {
    raw.sort_by(|a, b| a.0.total_cmp(&b.0));
    let t0 = raw.first().map_or(0.0, |r| r.0);
    raw.into_iter()
        .map(|(t, channel, bytes)| Captured {
            t: t - t0,
            channel,
            bytes,
        })
        .collect()
}

/// Spielt den Mitschnitt in Echtzeit (mal `speed`) ab, auf Wunsch in Schleife.
/// Pakete, die DeckLight selbst als Player 5–15 gesendet hat, sind enthalten,
/// stören aber nicht: DeckLight erkennt seine eigene Nummer.
pub async fn play(
    packets: Vec<Captured>,
    target: Target,
    speed: f64,
    repeat: bool,
) -> std::io::Result<()> {
    let socket = tokio::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).await?;
    socket.set_broadcast(true)?;
    let speed = if speed > 0.0 { speed } else { 1.0 };
    loop {
        let start = tokio::time::Instant::now();
        for p in &packets {
            let at = start + Duration::from_secs_f64(p.t / speed);
            tokio::time::sleep_until(at).await;
            let addr = SocketAddr::from((target.ip, target.ports.port(p.channel)));
            if let Err(e) = socket.send_to(&p.bytes, addr).await {
                tracing::warn!(error = %e, %addr, "Mitschnitt: Senden fehlgeschlagen");
            }
        }
        if !repeat || packets.is_empty() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn udp_frame(dst: u16, payload: &[u8]) -> Vec<u8> {
        let mut f = vec![0xff; 6];
        f.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x08, 0x00]);
        let total = 20 + 8 + payload.len();
        f.extend_from_slice(&[
            0x45,
            0,
            (total >> 8) as u8,
            total as u8,
            0,
            0,
            0,
            0,
            64,
            17,
            0,
            0,
        ]);
        f.extend_from_slice(&[192, 168, 1, 11, 192, 168, 1, 255]);
        let ulen = 8 + payload.len();
        f.extend_from_slice(&50002u16.to_be_bytes());
        f.extend_from_slice(&dst.to_be_bytes());
        f.extend_from_slice(&[(ulen >> 8) as u8, ulen as u8, 0, 0]);
        f.extend_from_slice(payload);
        f
    }

    fn pcap(frames: &[(f64, Vec<u8>)]) -> Vec<u8> {
        let mut d = vec![0xd4, 0xc3, 0xb2, 0xa1, 2, 0, 4, 0];
        d.extend_from_slice(&[0; 8]);
        d.extend_from_slice(&65535u32.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes());
        for (t, f) in frames {
            d.extend_from_slice(&(t.floor() as u32).to_le_bytes());
            d.extend_from_slice(&((t.fract() * 1e6).round() as u32).to_le_bytes());
            d.extend_from_slice(&(f.len() as u32).to_le_bytes());
            d.extend_from_slice(&(f.len() as u32).to_le_bytes());
            d.extend_from_slice(f);
        }
        d
    }

    fn pcapng(frames: &[(f64, Vec<u8>)]) -> Vec<u8> {
        let block = |ty: u32, body: &[u8]| {
            let pad = body.len().div_ceil(4) * 4;
            let len = (12 + pad) as u32;
            let mut b = ty.to_le_bytes().to_vec();
            b.extend_from_slice(&len.to_le_bytes());
            b.extend_from_slice(body);
            b.resize(8 + pad, 0);
            b.extend_from_slice(&len.to_le_bytes());
            b
        };
        let mut shb = 0x1a2b_3c4du32.to_le_bytes().to_vec();
        shb.extend_from_slice(&[1, 0, 0, 0]);
        shb.extend_from_slice(&u64::MAX.to_le_bytes());
        let mut d = block(0x0a0d_0d0a, &shb);
        // Schnittstelle: Ethernet, Auflösung Nanosekunden (Option 9 = 9).
        let mut idb = vec![1, 0, 0, 0];
        idb.extend_from_slice(&65535u32.to_le_bytes());
        idb.extend_from_slice(&[9, 0, 1, 0, 9, 0, 0, 0, 0, 0, 0, 0]);
        d.extend(block(1, &idb));
        for (t, f) in frames {
            let ts = (t * 1e9) as u64;
            let mut epb = 0u32.to_le_bytes().to_vec();
            epb.extend_from_slice(&((ts >> 32) as u32).to_le_bytes());
            epb.extend_from_slice(&(ts as u32).to_le_bytes());
            epb.extend_from_slice(&(f.len() as u32).to_le_bytes());
            epb.extend_from_slice(&(f.len() as u32).to_le_bytes());
            epb.extend_from_slice(f);
            d.extend(block(6, &epb));
        }
        d
    }

    #[test]
    fn reads_pcap_and_pcapng_and_keeps_only_prolink() {
        let frames = vec![
            (100.25, udp_frame(50002, b"status")),
            (100.5, udp_frame(5353, b"mdns")),
            (101.0, udp_frame(50001, b"beat")),
            (100.0, udp_frame(50000, b"hello")),
        ];
        for data in [pcap(&frames), pcapng(&frames)] {
            let c = parse_pcap(&data).unwrap();
            assert_eq!(c.len(), 3);
            assert_eq!(c[0].channel, Channel::Announce);
            assert_eq!(c[0].bytes, b"hello");
            assert!((c[1].t - 0.25).abs() < 1e-6);
            assert_eq!(c[2].channel, Channel::Beat);
            assert!((c[2].t - 1.0).abs() < 1e-6);
        }
        assert_eq!(parse_pcap(b"nope"), Err(CaptureError::Format));
        let mut cut = pcap(&frames);
        cut.truncate(cut.len() - 3);
        assert_eq!(parse_pcap(&cut), Err(CaptureError::Truncated));
    }

    #[tokio::test]
    async fn plays_with_original_timing() {
        let rx = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = rx.local_addr().unwrap().port();
        let target = Target {
            ip: std::net::Ipv4Addr::LOCALHOST,
            ports: decklight_prolink::PortConfig {
                announce: port,
                beat: port,
                status: port,
            },
        };
        let packets = vec![
            Captured {
                t: 0.0,
                channel: Channel::Status,
                bytes: b"a".to_vec(),
            },
            Captured {
                t: 0.2,
                channel: Channel::Beat,
                bytes: b"b".to_vec(),
            },
        ];
        let start = std::time::Instant::now();
        play(packets, target, 1.0, false).await.unwrap();
        let mut buf = [0u8; 8];
        let (n, _) = rx.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"a");
        let (n, _) = rx.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"b");
        assert!(start.elapsed() >= Duration::from_millis(195));
    }
}
