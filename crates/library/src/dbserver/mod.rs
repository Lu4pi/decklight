//! dbserver: Metadaten-Dienst der Player (TCP).

pub mod client;
pub mod field;
pub mod message;
pub mod server;

pub use client::{DbClient, DbError};
pub use field::Field;
pub use message::Message;

/// Port, über den man den eigentlichen dbserver-Port erfragt.
pub const QUERY_PORT: u16 = 12523;
