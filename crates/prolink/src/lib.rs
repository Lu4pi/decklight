//! PRO DJ LINK für DeckLight: Pakete lesen und schreiben, Geräte im Netz finden,
//! als virtueller Player mitlaufen.
//!
//! Protokollwissen stammt aus der "DJ Link Ecosystem Analysis" von Deep Symmetry.
//! DeckLight liest nur mit und tritt nie mit einer Player-Nummer 1–4 auf.

pub mod device;
pub mod encode;
pub mod net;
pub mod packet;
pub mod snapshot;

pub use device::{Device, DeviceTable};
pub use net::{
    interface_for, interfaces, pick_interface, Interface, Link, LinkConfig, LinkEvent, PortConfig,
    VirtualPlayer,
};
pub use packet::{parse, Channel, DeviceKind, Packet, PlayState};
pub use snapshot::{LinkSnapshot, PlayerSnapshot};
