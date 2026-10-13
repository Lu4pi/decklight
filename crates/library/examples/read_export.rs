//! Rekordbox-Export lesen und zusammenfassen: `cargo run -p decklight-library --example read_export -- /Volumes/STICK`
fn main() {
    let root = std::env::args().nth(1).expect("Pfad zum Stick");
    let ex =
        decklight_library::export::Export::open(std::path::Path::new(&root)).expect("Export lesen");
    let mut ids: Vec<_> = ex.db.tracks.keys().copied().collect();
    ids.sort();
    println!(
        "{} Tracks, {} Artists, {} Alben",
        ids.len(),
        ex.db.artists.len(),
        ex.db.albums.len()
    );
    for id in ids {
        let t = ex.track(id).unwrap();
        let m = &t.metadata;
        println!(
            "{id:>5} {} | {} | {} | {} | {}s | {:?} BPM | Grid {} | Welle {} | Cues {} | Cover {} B",
            m.title, m.artist, m.album, m.key, m.duration_s, m.bpm, t.beat_grid.len(),
            t.waveform.as_ref().map_or(0, |w| w.columns.len()), t.cues.len(),
            t.artwork.as_ref().map_or(0, Vec::len)
        );
    }
}
