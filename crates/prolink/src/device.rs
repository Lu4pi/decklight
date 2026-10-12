//! Liste der Geräte im Netz, gepflegt aus den Keep-Alive-Paketen.

use crate::packet::{DeviceKind, KeepAlive};
use serde::Serialize;
use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

/// Geräte melden sich etwa alle 1,5 s. Nach dieser Zeit ohne Keep-Alive gilt
/// ein Gerät als verschwunden.
pub const DEFAULT_DEVICE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Device {
    pub number: u8,
    pub name: String,
    pub kind: DeviceKind,
    pub ip: Ipv4Addr,
    pub mac: [u8; 6],
    #[serde(skip)]
    pub first_seen: Instant,
    #[serde(skip)]
    pub last_seen: Instant,
}

impl Device {
    pub fn mac_string(&self) -> String {
        self.mac
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":")
    }
}

/// Was beim Eintragen eines Keep-Alives passiert ist.
#[derive(Debug, Clone, PartialEq)]
pub enum Observed {
    /// Gerät war bisher unbekannt.
    New(Device),
    /// Unter derselben Nummer meldet sich ein anderes Gerät (IP oder Name neu).
    Replaced { old: Device, new: Device },
    /// Bekanntes Gerät, nur Zeitstempel aktualisiert.
    Refreshed,
}

#[derive(Debug, Default, Clone)]
pub struct DeviceTable {
    devices: BTreeMap<u8, Device>,
}

impl DeviceTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, k: &KeepAlive, now: Instant) -> Observed {
        let fresh = Device {
            number: k.number,
            name: k.name.clone(),
            kind: k.kind,
            ip: k.ip,
            mac: k.mac,
            first_seen: now,
            last_seen: now,
        };
        match self.devices.get_mut(&k.number) {
            Some(known) if known.ip == k.ip && known.name == k.name => {
                known.last_seen = now;
                Observed::Refreshed
            }
            Some(known) => {
                let old = std::mem::replace(known, fresh.clone());
                Observed::Replaced { old, new: fresh }
            }
            None => {
                self.devices.insert(k.number, fresh.clone());
                Observed::New(fresh)
            }
        }
    }

    /// Entfernt Geräte, die sich länger als `timeout` nicht gemeldet haben.
    pub fn expire(&mut self, now: Instant, timeout: Duration) -> Vec<Device> {
        let gone: Vec<u8> = self
            .devices
            .values()
            .filter(|d| now.saturating_duration_since(d.last_seen) > timeout)
            .map(|d| d.number)
            .collect();
        gone.into_iter()
            .filter_map(|n| self.devices.remove(&n))
            .collect()
    }

    pub fn get(&self, number: u8) -> Option<&Device> {
        self.devices.get(&number)
    }

    pub fn list(&self) -> Vec<Device> {
        self.devices.values().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ka(number: u8, ip: [u8; 4]) -> KeepAlive {
        KeepAlive {
            name: "CDJ-3000".into(),
            number,
            kind: DeviceKind::Player,
            mac: [0; 6],
            ip: Ipv4Addr::from(ip),
            peers_seen: 1,
        }
    }

    #[test]
    fn new_refresh_replace_expire() {
        let mut t = DeviceTable::new();
        let t0 = Instant::now();
        assert!(matches!(
            t.observe(&ka(1, [10, 0, 0, 1]), t0),
            Observed::New(_)
        ));
        assert_eq!(t.observe(&ka(1, [10, 0, 0, 1]), t0), Observed::Refreshed);
        assert!(matches!(
            t.observe(&ka(1, [10, 0, 0, 9]), t0),
            Observed::Replaced { .. }
        ));
        assert!(matches!(
            t.observe(&ka(2, [10, 0, 0, 2]), t0),
            Observed::New(_)
        ));

        let later = t0 + Duration::from_secs(3);
        t.observe(&ka(2, [10, 0, 0, 2]), later);
        let gone = t.expire(t0 + Duration::from_secs(6), DEFAULT_DEVICE_TIMEOUT);
        assert_eq!(gone.iter().map(|d| d.number).collect::<Vec<_>>(), vec![1]);
        assert_eq!(t.len(), 1);
    }
}
