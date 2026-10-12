//! UDP-Anbindung: Ports 50000–50002 öffnen, Pakete verteilen, Geräte pflegen
//! und optional als virtueller Player Keep-Alives senden.

use crate::device::{Device, DeviceTable, Observed, DEFAULT_DEVICE_TIMEOUT};
use crate::encode;
use crate::packet::{self, Channel, DeviceKind, KeepAlive, Packet};
use serde::Serialize;
use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Abstand der eigenen Keep-Alives (wie bei echten Playern).
pub const KEEP_ALIVE_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PortConfig {
    pub announce: u16,
    pub beat: u16,
    pub status: u16,
}

impl PortConfig {
    /// Die echten Ports 50000, 50001, 50002.
    pub const STANDARD: Self = Self::from_base(50000);

    /// Drei aufeinanderfolgende Ports ab `base`. Für Tests und Simulator auf einem Rechner.
    pub const fn from_base(base: u16) -> Self {
        Self {
            announce: base,
            beat: base + 1,
            status: base + 2,
        }
    }

    pub fn port(&self, channel: Channel) -> u16 {
        match channel {
            Channel::Announce => self.announce,
            Channel::Beat => self.beat,
            Channel::Status => self.status,
        }
    }
}

impl Default for PortConfig {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Eine IPv4-Netzwerkschnittstelle des Rechners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Interface {
    pub name: String,
    pub ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub broadcast: Ipv4Addr,
    pub mac: Option<[u8; 6]>,
    pub loopback: bool,
}

/// Alle IPv4-Schnittstellen, Loopback zuletzt.
pub fn interfaces() -> io::Result<Vec<Interface>> {
    let mut out: Vec<Interface> = if_addrs::get_if_addrs()?
        .into_iter()
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => {
                let broadcast = v4
                    .broadcast
                    .unwrap_or_else(|| Ipv4Addr::from(u32::from(v4.ip) | !u32::from(v4.netmask)));
                let mac = mac_address::mac_address_by_name(&i.name)
                    .ok()
                    .flatten()
                    .map(|m| m.bytes());
                Some(Interface {
                    loopback: v4.ip.is_loopback(),
                    name: i.name,
                    ip: v4.ip,
                    netmask: v4.netmask,
                    broadcast,
                    mac,
                })
            }
            _ => None,
        })
        .collect();
    out.sort_by_key(|i| i.loopback);
    Ok(out)
}

/// Wählt eine Schnittstelle: nach Name oder IP, sonst die erste mit privater
/// Adresse (CDJ-Netze laufen fast immer in 169.254/16, 192.168/16 oder 10/8).
pub fn pick_interface(wanted: Option<&str>) -> io::Result<Interface> {
    let all = interfaces()?;
    let found = match wanted {
        Some(w) => all
            .into_iter()
            .find(|i| i.name == w || i.ip.to_string() == w),
        None => all
            .into_iter()
            .find(|i| !i.loopback && (i.ip.is_private() || i.ip.is_link_local())),
    };
    found.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            match wanted {
                Some(w) => format!("Netzwerkschnittstelle {w} nicht gefunden"),
                None => "keine passende Netzwerkschnittstelle gefunden".into(),
            },
        )
    })
}

/// Einstellungen für den eigenen Auftritt als virtueller Player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualPlayer {
    pub name: String,
    /// Nie 1–4 verwenden, sonst stören wir echte Player. Standard: 7.
    pub number: u8,
    pub ip: Ipv4Addr,
    pub mac: [u8; 6],
    /// Ziel der Keep-Alives (Broadcast-Adresse des CDJ-Netzes).
    pub broadcast: Ipv4Addr,
}

impl VirtualPlayer {
    pub const DEFAULT_NUMBER: u8 = 7;

    pub fn on_interface(iface: &Interface, number: u8) -> Self {
        Self {
            name: "DeckLight".into(),
            number,
            ip: iface.ip,
            mac: iface.mac.unwrap_or_default(),
            broadcast: iface.broadcast,
        }
    }

    fn keep_alive(&self, peers_seen: u8) -> KeepAlive {
        KeepAlive {
            name: self.name.clone(),
            number: self.number,
            kind: DeviceKind::Player,
            mac: self.mac,
            ip: self.ip,
            peers_seen,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinkConfig {
    pub bind_ip: Ipv4Addr,
    pub ports: PortConfig,
    pub device_timeout: Duration,
    /// Ohne virtuellen Player kommen nur Broadcasts an (Keep-Alive, Beats).
    /// CDJ-Status schicken die Player gezielt an angemeldete Geräte.
    pub virtual_player: Option<VirtualPlayer>,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            bind_ip: Ipv4Addr::UNSPECIFIED,
            ports: PortConfig::STANDARD,
            device_timeout: DEFAULT_DEVICE_TIMEOUT,
            virtual_player: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LinkEvent {
    DeviceFound(Device),
    DeviceLost(Device),
    /// Ein anderes Gerät nutzt unsere Player-Nummer.
    NumberConflict {
        number: u8,
        other: Device,
        /// Auf diese freie Nummer ist DeckLight ausgewichen.
        switched_to: Option<u8>,
    },
    Packet {
        from: SocketAddr,
        #[serde(skip)]
        at: Instant,
        packet: Packet,
    },
}

/// Laufende Verbindung zum PRO-DJ-LINK-Netz. Beim Drop werden alle Tasks beendet.
pub struct Link {
    events: broadcast::Sender<LinkEvent>,
    devices: Arc<Mutex<DeviceTable>>,
    tasks: Vec<JoinHandle<()>>,
    local_ports: PortConfig,
    own_number: Arc<AtomicU8>,
}

impl Link {
    pub async fn start(config: LinkConfig) -> io::Result<Self> {
        let (events, _) = broadcast::channel(4096);
        let devices = Arc::new(Mutex::new(DeviceTable::new()));
        let mut tasks = Vec::new();

        let announce = Arc::new(bind_udp(config.bind_ip, config.ports.announce)?);
        let beat = bind_udp(config.bind_ip, config.ports.beat)?;
        let status = bind_udp(config.bind_ip, config.ports.status)?;
        let local_ports = PortConfig {
            announce: announce.local_addr()?.port(),
            beat: beat.local_addr()?.port(),
            status: status.local_addr()?.port(),
        };

        let own_number = Arc::new(AtomicU8::new(
            config.virtual_player.as_ref().map_or(0, |v| v.number),
        ));
        let ctx = Ctx {
            events: events.clone(),
            devices: devices.clone(),
            own_ip: config.virtual_player.as_ref().map(|v| v.ip),
            own_number: own_number.clone(),
        };
        tasks.push(tokio::spawn(receive(
            ctx.clone(),
            announce.clone(),
            Channel::Announce,
        )));
        tasks.push(tokio::spawn(receive(
            ctx.clone(),
            Arc::new(beat),
            Channel::Beat,
        )));
        tasks.push(tokio::spawn(receive(
            ctx.clone(),
            Arc::new(status),
            Channel::Status,
        )));
        tasks.push(tokio::spawn(expire(ctx, config.device_timeout)));

        if let Some(vp) = config.virtual_player.clone() {
            if (1..=4).contains(&vp.number) {
                tracing::warn!(
                    number = vp.number,
                    "virtuelle Player-Nummer 1–4 kann echte Player stören"
                );
            }
            let target = SocketAddrV4::new(vp.broadcast, config.ports.announce);
            tasks.push(tokio::spawn(announce_loop(
                announce,
                vp,
                target,
                devices.clone(),
                own_number.clone(),
            )));
        }

        Ok(Self {
            events,
            devices,
            tasks,
            local_ports,
            own_number,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LinkEvent> {
        self.events.subscribe()
    }

    pub fn devices(&self) -> Vec<Device> {
        self.devices.lock().map(|t| t.list()).unwrap_or_default()
    }

    /// Tatsächlich gebundene Ports (relevant, wenn mit Port 0 gestartet wurde).
    /// Aktuelle eigene Player-Nummer (kann nach einem Konflikt wechseln), ohne
    /// virtuellen Player `None`.
    pub fn own_number(&self) -> Option<u8> {
        match self.own_number.load(Ordering::Relaxed) {
            0 => None,
            n => Some(n),
        }
    }

    pub fn local_ports(&self) -> PortConfig {
        self.local_ports
    }

    pub fn stop(self) {}
}

impl Drop for Link {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

#[derive(Clone)]
struct Ctx {
    events: broadcast::Sender<LinkEvent>,
    devices: Arc<Mutex<DeviceTable>>,
    own_ip: Option<Ipv4Addr>,
    own_number: Arc<AtomicU8>,
}

fn bind_udp(ip: Ipv4Addr, port: u16) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::new(IpAddr::V4(ip), port).into())?;
    UdpSocket::from_std(socket.into())
}

async fn receive(ctx: Ctx, socket: Arc<UdpSocket>, channel: Channel) {
    let mut buf = vec![0u8; 2048];
    loop {
        let (len, from) = match socket.recv_from(&mut buf).await {
            Ok(r) => r,
            Err(e) => {
                // Unter Windows liefert recv_from z. B. ConnectionReset nach ICMP.
                tracing::debug!(?channel, error = %e, "Empfangsfehler");
                continue;
            }
        };
        let Some(packet) = packet::parse(channel, &buf[..len]) else {
            continue;
        };
        let at = Instant::now();
        if let Packet::KeepAlive(ka) = &packet {
            if ctx.own_ip == Some(ka.ip) && ctx.own_number.load(Ordering::Relaxed) == ka.number {
                continue; // unser eigenes Echo
            }
            note_keep_alive(&ctx, ka, at);
        }
        let _ = ctx.events.send(LinkEvent::Packet { from, at, packet });
    }
}

fn note_keep_alive(ctx: &Ctx, ka: &KeepAlive, at: Instant) {
    let observed = match ctx.devices.lock() {
        Ok(mut t) => t.observe(ka, at),
        Err(_) => return,
    };
    let device = match observed {
        Observed::Refreshed => return,
        Observed::New(d) => d,
        Observed::Replaced { old, new } => {
            let _ = ctx.events.send(LinkEvent::DeviceLost(old));
            new
        }
    };
    let own = ctx.own_number.load(Ordering::Relaxed);
    if ctx.own_ip.is_some() && own == device.number {
        // Das andere Gerät war zuerst da oder ist ein echter Player: ausweichen.
        let taken: Vec<u8> = ctx
            .devices
            .lock()
            .map(|t| t.list().iter().map(|d| d.number).collect())
            .unwrap_or_default();
        let switched_to = free_number(&taken, own);
        if let Some(n) = switched_to {
            ctx.own_number.store(n, Ordering::Relaxed);
            tracing::warn!(from = own, to = n, other = %device.name, "Player-Nummer belegt, weiche aus");
        }
        let _ = ctx.events.send(LinkEvent::NumberConflict {
            number: device.number,
            other: device.clone(),
            switched_to,
        });
    }
    tracing::info!(number = device.number, name = %device.name, ip = %device.ip, "Gerät erkannt");
    let _ = ctx.events.send(LinkEvent::DeviceFound(device));
}

async fn expire(ctx: Ctx, timeout: Duration) {
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        tick.tick().await;
        let gone = match ctx.devices.lock() {
            Ok(mut t) => t.expire(Instant::now(), timeout),
            Err(_) => return,
        };
        for d in gone {
            tracing::info!(number = d.number, name = %d.name, "Gerät verschwunden");
            let _ = ctx.events.send(LinkEvent::DeviceLost(d));
        }
    }
}

/// Erst zuhören, welche Nummern belegt sind, dann eine freie nehmen.
pub const LISTEN_BEFORE_ANNOUNCE: Duration = Duration::from_millis(1800);

/// Freie virtuelle Player-Nummer: die gewünschte, sonst die nächste aus 5–15.
/// Nummern 1–4 bleiben echten Playern vorbehalten.
pub fn free_number(taken: &[u8], wanted: u8) -> Option<u8> {
    let free = |n: u8| !taken.contains(&n);
    if !(1..=4).contains(&wanted) && free(wanted) {
        return Some(wanted);
    }
    let start = wanted.clamp(5, 15);
    (start..=15).chain(5..start).find(|&n| free(n))
}

async fn announce_loop(
    socket: Arc<UdpSocket>,
    mut vp: VirtualPlayer,
    target: SocketAddrV4,
    devices: Arc<Mutex<DeviceTable>>,
    own_number: Arc<AtomicU8>,
) {
    tokio::time::sleep(LISTEN_BEFORE_ANNOUNCE).await;
    let wanted = own_number.load(Ordering::Relaxed);
    let taken: Vec<u8> = devices
        .lock()
        .map(|t| t.list().iter().map(|d| d.number).collect())
        .unwrap_or_default();
    if taken.contains(&wanted) {
        if let Some(n) = free_number(&taken, wanted) {
            tracing::warn!(
                from = wanted,
                to = n,
                "Player-Nummer schon belegt, nehme eine freie"
            );
            own_number.store(n, Ordering::Relaxed);
        }
    }
    let mut tick = tokio::time::interval(KEEP_ALIVE_INTERVAL);
    loop {
        tick.tick().await;
        vp.number = own_number.load(Ordering::Relaxed);
        let peers = devices.lock().map(|t| t.len()).unwrap_or(0);
        let bytes = encode::keep_alive(&vp.keep_alive(u8::try_from(peers + 1).unwrap_or(u8::MAX)));
        if let Err(e) = socket.send_to(&bytes, target).await {
            tracing::warn!(error = %e, %target, "Keep-Alive konnte nicht gesendet werden");
        }
    }
}

#[cfg(test)]
mod number_tests {
    use super::free_number;

    #[test]
    fn picks_wanted_or_next_free_and_never_one_to_four() {
        assert_eq!(free_number(&[1, 2, 3, 4], 7), Some(7));
        assert_eq!(free_number(&[1, 2, 7], 7), Some(8));
        assert_eq!(free_number(&[7, 8, 9, 10, 11, 12, 13, 14, 15], 7), Some(5));
        assert_eq!(free_number(&[], 3), Some(5));
        assert_eq!(free_number(&(5..=15).collect::<Vec<_>>(), 7), None);
    }
}
