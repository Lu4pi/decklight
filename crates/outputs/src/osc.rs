//! OSC-Client für Pult-Befehle. grandMA3: `/cmd` (mit Präfix aus den OSC-Einstellungen
//! des Pults, z. B. `/gma3/cmd`) und der Befehlstext als String.

use rosc::{encoder, OscMessage, OscPacket, OscType};
use serde::{Deserialize, Serialize};
use std::io;
use std::net::{SocketAddr, UdpSocket};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum OscArg {
    Int(i32),
    Float(f32),
    String(String),
}

impl From<&OscArg> for OscType {
    fn from(a: &OscArg) -> Self {
        match a {
            OscArg::Int(i) => OscType::Int(*i),
            OscArg::Float(f) => OscType::Float(*f),
            OscArg::String(s) => OscType::String(s.clone()),
        }
    }
}

pub fn encode(address: &str, args: &[OscArg]) -> io::Result<Vec<u8>> {
    let msg = OscMessage {
        addr: address.to_string(),
        args: args.iter().map(OscType::from).collect(),
    };
    encoder::encode(&OscPacket::Message(msg)).map_err(|e| io::Error::other(e.to_string()))
}

/// Liest eine OSC-Nachricht (für Tests und den OSC-Monitor).
pub fn decode(d: &[u8]) -> Option<(String, Vec<OscArg>)> {
    let (_, packet) = rosc::decoder::decode_udp(d).ok()?;
    let OscPacket::Message(m) = packet else {
        return None;
    };
    let args = m
        .args
        .into_iter()
        .filter_map(|a| match a {
            OscType::Int(i) => Some(OscArg::Int(i)),
            OscType::Float(f) => Some(OscArg::Float(f)),
            OscType::String(s) => Some(OscArg::String(s)),
            _ => None,
        })
        .collect();
    Some((m.addr, args))
}

/// Ziel für OSC-Nachrichten.
pub struct OscSender {
    socket: UdpSocket,
    target: SocketAddr,
    prefix: String,
}

impl OscSender {
    /// `prefix` wird jeder Adresse vorangestellt (leer oder z. B. "/gma3").
    pub fn new(target: SocketAddr, prefix: &str) -> io::Result<Self> {
        let socket = UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_broadcast(true)?;
        Ok(Self {
            socket,
            target,
            prefix: normalize_prefix(prefix),
        })
    }

    pub fn send(&self, address: &str, args: &[OscArg]) -> io::Result<()> {
        let addr = format!("{}{}", self.prefix, address);
        self.socket
            .send_to(&encode(&addr, args)?, self.target)
            .map(|_| ())
    }

    /// grandMA3-Befehl, z. B. `Go+ Exec 201`.
    pub fn command(&self, text: &str) -> io::Result<()> {
        self.send("/cmd", &[OscArg::String(text.to_string())])
    }
}

fn normalize_prefix(p: &str) -> String {
    let p = p.trim().trim_end_matches('/');
    if p.is_empty() || p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grandma3_command_over_udp() {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        rx.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let tx = OscSender::new(rx.local_addr().unwrap(), "gma3/").unwrap();
        tx.command("Go+ Exec 201").unwrap();
        let mut buf = [0u8; 512];
        let n = rx.recv(&mut buf).unwrap();
        assert_eq!(
            decode(&buf[..n]),
            Some((
                "/gma3/cmd".into(),
                vec![OscArg::String("Go+ Exec 201".into())]
            ))
        );
    }
}
