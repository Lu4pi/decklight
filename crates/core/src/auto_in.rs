//! OSC-Eingang für den Auto-Piloten: Eine Taste an der grandMA3 schaltet ihn
//! ein und aus, z. B. per `SendOSC` mit `/decklight/auto,i,1`.
//!
//! Angenommen werden (jeweils auch mit beliebigem Präfix davor):
//! `/decklight/auto` mit 1/0, `on`/`off`/`toggle` oder ohne Wert (umschalten),
//! sowie `/decklight/auto/on`, `/decklight/auto/off`, `/decklight/auto/toggle`.

use crate::monitor::Level;
use crate::runtime::Shared;
use decklight_outputs::osc::{decode, OscArg};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Was eine OSC-Nachricht mit dem Auto-Piloten tun soll: `Some(None)` =
/// umschalten, `Some(Some(an))` = setzen, `None` = nicht für uns.
pub fn command(address: &str, args: &[OscArg]) -> Option<Option<bool>> {
    let a = address.trim_end_matches('/').to_ascii_lowercase();
    if a.ends_with("/decklight/auto/on") {
        return Some(Some(true));
    }
    if a.ends_with("/decklight/auto/off") {
        return Some(Some(false));
    }
    if a.ends_with("/decklight/auto/toggle") {
        return Some(None);
    }
    if !a.ends_with("/decklight/auto") {
        return None;
    }
    Some(match args.first() {
        None => None,
        Some(OscArg::Int(i)) => Some(*i != 0),
        Some(OscArg::Float(f)) => Some(*f >= 0.5),
        Some(OscArg::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "on" | "an" | "1" | "true" => Some(true),
            "off" | "aus" | "0" | "false" => Some(false),
            _ => None,
        },
    })
}

pub(crate) struct AutoInThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for AutoInThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

pub(crate) fn spawn(port: u16, shared: Arc<Mutex<Shared>>) -> Option<AutoInThread> {
    let socket = match UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(port, error = %e, "OSC-Eingang für Auto nicht verfügbar");
            if let Ok(mut s) = shared.lock() {
                s.monitor.warn(
                    Level::Warning,
                    format!("OSC-Eingang (Auto ein/aus) auf Port {port} belegt: {e}"),
                );
            }
            return None;
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(200)));
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handle = std::thread::Builder::new()
        .name("decklight-auto-osc".into())
        .spawn(move || {
            tracing::info!(port, "OSC-Eingang für Auto bereit");
            let mut buf = [0u8; 2048];
            while !flag.load(Ordering::Relaxed) {
                let Ok((n, from)) = socket.recv_from(&mut buf) else {
                    continue;
                };
                let Some((address, args)) = decode(&buf[..n]) else {
                    continue;
                };
                if let Some(on) = command(&address, &args) {
                    let now = crate::runtime::set_auto(&shared, on);
                    tracing::info!(%from, %address, on = now, "Auto per OSC");
                }
            }
        })
        .ok()?;
    Some(AutoInThread {
        stop,
        handle: Some(handle),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn understands_ma3_messages() {
        assert_eq!(
            command("/decklight/auto", &[OscArg::Int(1)]),
            Some(Some(true))
        );
        assert_eq!(
            command("/gma3/decklight/auto", &[OscArg::Int(0)]),
            Some(Some(false))
        );
        assert_eq!(command("/decklight/auto", &[]), Some(None));
        assert_eq!(
            command("/decklight/auto", &[OscArg::Float(1.0)]),
            Some(Some(true))
        );
        assert_eq!(
            command("/decklight/auto", &[OscArg::String("off".into())]),
            Some(Some(false))
        );
        assert_eq!(command("/DeckLight/Auto/On", &[]), Some(Some(true)));
        assert_eq!(command("/decklight/auto/toggle", &[]), Some(None));
        assert_eq!(command("/gma3/cmd", &[]), None);
    }
}
