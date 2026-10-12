//! Zwischenspeicher je Track, damit jeder Track nur einmal geladen wird.

use crate::track::TrackData;
use std::collections::HashMap;
use std::sync::Arc;

/// Track auf einem bestimmten Medium (Rekordbox-IDs gelten nur je Medium).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackId {
    pub source_player: u8,
    pub slot: u8,
    pub rekordbox_id: u32,
}

#[derive(Debug, Default)]
pub struct TrackCache {
    tracks: HashMap<TrackId, Arc<TrackData>>,
}

impl TrackCache {
    pub fn get(&self, id: &TrackId) -> Option<Arc<TrackData>> {
        self.tracks.get(id).cloned()
    }

    pub fn insert(&mut self, id: TrackId, data: TrackData) -> Arc<TrackData> {
        let data = Arc::new(data);
        self.tracks.insert(id, data.clone());
        data
    }

    /// Medium entfernt: alle Tracks dieses Players/Slots verwerfen.
    pub fn forget_media(&mut self, source_player: u8, slot: u8) {
        self.tracks
            .retain(|k, _| !(k.source_player == source_player && k.slot == slot));
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}
