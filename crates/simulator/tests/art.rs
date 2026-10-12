#[test]
fn cover_is_a_png() {
    let png = decklight_simulator::art::cover(1001);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    if let Ok(dir) = std::env::var("DECKLIGHT_PNG_OUT") {
        std::fs::write(format!("{dir}/cover.png"), &png).unwrap();
    }
}
