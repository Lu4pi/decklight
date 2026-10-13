//! Echter Rekordbox-Export (Demo-Tracks, siehe `fixtures/rekordbox-demo/NOTICE.md`).

use decklight_library::export::Export;
use std::path::Path;

#[test]
fn reads_the_rekordbox_demo_export() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rekordbox-demo");
    let ex = Export::open(&root).unwrap();
    assert_eq!(ex.db.tracks.len(), 2);
    let t = ex.track(1).unwrap();
    assert_eq!(t.metadata.title, "Demo Track 1");
    assert_eq!(t.metadata.artist, "Loopmasters");
    assert_eq!(t.metadata.key, "Fm");
    assert_eq!(t.metadata.duration_s, 172);
    assert_eq!(t.metadata.bpm, Some(128.0));
    // Beatgrid: 128 BPM, Schläge 1–4 der Reihe nach, Abstand ~469 ms.
    assert_eq!(t.beat_grid.len(), 368);
    for w in t.beat_grid.windows(2) {
        assert_eq!(w[1].beat_in_bar, w[0].beat_in_bar % 4 + 1);
        let d = w[1].time_ms - w[0].time_ms;
        assert!((468..=470).contains(&d), "{d}");
    }
    // Farbige Wellenform: 150 Spalten/s über die ganze Länge.
    let wave = t.waveform.unwrap();
    assert!((wave.columns.len() as f64 / 150.0 - 172.0).abs() < 1.5);
    assert!(wave.columns.iter().any(|c| c.height > 20));
    let t2 = ex.track(2).unwrap();
    assert_eq!(t2.metadata.bpm, Some(120.0));
    assert_eq!(t2.beat_grid.len(), 257);
}
