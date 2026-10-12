//! Lädt alles zu einem Track in einer dbserver-Sitzung.

use crate::dbserver::client::{DbClient, DbError, TrackLocation};
use crate::track::TrackData;
use std::net::IpAddr;

/// Metadaten sind Pflicht; Cover, Beatgrid, Wellenform und Cues werden geladen,
/// soweit der Player sie hat (ältere Exporte haben z. B. keine farbige Wellenform).
pub async fn load_track(
    ip: IpAddr,
    query_port: u16,
    asking_as: u8,
    loc: TrackLocation,
) -> Result<TrackData, DbError> {
    let mut c = DbClient::connect_via(ip, query_port, asking_as).await?;
    let metadata = c.metadata(loc).await?;
    let artwork = match metadata.artwork_id {
        0 => None,
        id => c.artwork(loc, id).await.ok(),
    };
    let beat_grid = c.beat_grid(loc).await.unwrap_or_default();
    let waveform = c.waveform(loc).await.ok();
    let cues = c.cues(loc).await.unwrap_or_default();
    c.close().await;
    Ok(TrackData {
        metadata,
        artwork,
        beat_grid,
        cues,
        waveform,
    })
}
