use decklight_prolink::encode;
use decklight_prolink::packet::*;
use std::net::Ipv4Addr;

/// Keep-Alive-Vorlage aus beat-link (VirtualCdj), mit Nummer 3 und IP 169.254.1.3.
#[test]
fn parses_reference_keep_alive_bytes() {
    let mut d: Vec<u8> = vec![
        0x51, 0x73, 0x70, 0x74, 0x31, 0x57, 0x6d, 0x4a, 0x4f, 0x4c, 0x06, 0x00, 0x62, 0x65, 0x61,
        0x74, 0x2d, 0x6c, 0x69, 0x6e, 0x6b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x01, 0x02, 0x00, 0x36, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
    ];
    d[0x24] = 3;
    d[0x26..0x2c].copy_from_slice(&[0x00, 0xe0, 0x36, 0x12, 0x34, 0x56]);
    d[0x2c..0x30].copy_from_slice(&[169, 254, 1, 3]);
    let Some(Packet::KeepAlive(k)) = parse(Channel::Announce, &d) else {
        panic!("kein Keep-Alive")
    };
    assert_eq!(k.name, "beat-link");
    assert_eq!(k.number, 3);
    assert_eq!(k.ip, Ipv4Addr::new(169, 254, 1, 3));
    assert_eq!(k.mac, [0x00, 0xe0, 0x36, 0x12, 0x34, 0x56]);
    assert_eq!(k.kind, DeviceKind::Player);
    // Unser Encoder erzeugt dieselben Bytes.
    assert_eq!(encode::keep_alive(&k), d);
}

#[test]
fn keep_alive_roundtrip_mixer() {
    let k = KeepAlive {
        name: "DJM-900NXS2".into(),
        number: MIXER_DEVICE_NUMBER,
        kind: DeviceKind::Mixer,
        mac: [1, 2, 3, 4, 5, 6],
        ip: Ipv4Addr::new(192, 168, 0, 33),
        peers_seen: 3,
    };
    assert_eq!(
        parse(Channel::Announce, &encode::keep_alive(&k)),
        Some(Packet::KeepAlive(k))
    );
}

fn status() -> CdjStatus {
    CdjStatus {
        name: "CDJ-3000".into(),
        number: 2,
        track_source_player: 2,
        track_slot: TrackSlot::Usb,
        track_type: 1,
        rekordbox_id: 4711,
        track_number: 12,
        play_state: PlayState::Playing,
        playing: true,
        master: true,
        sync: false,
        on_air: true,
        pitch_percent: 2.5,
        track_bpm: Some(126.0),
        beat_number: Some(65),
        beat_in_bar: 1,
        packet_counter: 99,
    }
}

#[test]
fn cdj_status_roundtrip() {
    let s = status();
    let bytes = encode::cdj_status(&s);
    assert_eq!(bytes.len(), encode::CDJ_STATUS_LEN);
    let Some(Packet::CdjStatus(p)) = parse(Channel::Status, &bytes) else {
        panic!()
    };
    assert_eq!(p.name, s.name);
    assert_eq!(p.rekordbox_id, 4711);
    assert_eq!(p.play_state, PlayState::Playing);
    assert!(p.playing && p.master && p.on_air && !p.sync);
    assert!((p.pitch_percent - 2.5).abs() < 0.001);
    assert_eq!(p.track_bpm, Some(126.0));
    assert!((p.effective_bpm().unwrap() - 129.15).abs() < 0.01);
    assert_eq!(p.beat_number, Some(65));
}

#[test]
fn cdj_status_without_track() {
    let s = CdjStatus {
        rekordbox_id: 0,
        play_state: PlayState::NoTrack,
        playing: false,
        track_bpm: None,
        beat_number: None,
        ..status()
    };
    let Some(Packet::CdjStatus(p)) = parse(Channel::Status, &encode::cdj_status(&s)) else {
        panic!()
    };
    assert_eq!(p.track_bpm, None);
    assert_eq!(p.beat_number, None);
    assert_eq!(p.effective_bpm(), None);
}

#[test]
fn beat_precise_position_on_air_mixer_roundtrip() {
    let b = Beat {
        name: "CDJ-3000".into(),
        number: 1,
        next_beat_ms: 476,
        next_bar_ms: 1428,
        pitch_percent: -1.5625, // exakt darstellbar
        track_bpm: Some(128.0),
        beat_in_bar: 2,
    };
    assert_eq!(
        parse(Channel::Beat, &encode::beat(&b)),
        Some(Packet::Beat(b))
    );

    let p = PrecisePosition {
        name: "CDJ-3000".into(),
        number: 3,
        track_length_s: 372,
        position_ms: 61_234,
        pitch_percent: -3.25,
        effective_bpm: Some(123.8),
    };
    assert_eq!(
        parse(Channel::Beat, &encode::precise_position(&p)),
        Some(Packet::PrecisePosition(p))
    );

    for channels in [
        vec![true, false, false, true],
        vec![false, true, false, false, true, true],
    ] {
        let o = OnAirChannels {
            name: "DJM-V10".into(),
            number: 0x21,
            channels,
        };
        assert_eq!(
            parse(Channel::Beat, &encode::on_air(&o)),
            Some(Packet::OnAir(o))
        );
    }

    let m = MixerStatus {
        name: "DJM-900NXS2".into(),
        number: 0x21,
        master: false,
        bpm: Some(128.0),
        beat_in_bar: 3,
    };
    assert_eq!(
        parse(Channel::Status, &encode::mixer_status(&m)),
        Some(Packet::MixerStatus(m))
    );
}

#[test]
fn rejects_foreign_and_truncated_packets() {
    assert_eq!(parse(Channel::Status, b"hello world, not dj link"), None);
    assert_eq!(parse(Channel::Status, &[]), None);
    let short = &encode::cdj_status(&status())[..0x40];
    assert!(matches!(
        parse(Channel::Status, short),
        Some(Packet::Unknown { kind: 0x0a, .. })
    ));
    // Ein Beat-Paket auf dem Status-Port ist kein Status.
    let beat = encode::beat(&Beat {
        name: "x".into(),
        number: 1,
        next_beat_ms: 1,
        next_bar_ms: 1,
        pitch_percent: 0.0,
        track_bpm: None,
        beat_in_bar: 1,
    });
    assert!(matches!(
        parse(Channel::Status, &beat),
        Some(Packet::Unknown { .. })
    ));
}

#[test]
fn pitch_conversion() {
    assert_eq!(percent_to_pitch(0.0), PITCH_NEUTRAL);
    assert_eq!(pitch_to_percent(0x20_0000), 100.0);
    assert_eq!(pitch_to_percent(0), -100.0);
    assert!((pitch_to_percent(percent_to_pitch(-7.93)) + 7.93).abs() < 0.0001);
}

#[test]
fn classify() {
    assert_eq!(classify_device(1, 1, "CDJ-2000NXS2"), DeviceKind::Player);
    assert_eq!(classify_device(0x21, 2, "DJM-V10"), DeviceKind::Mixer);
    assert_eq!(classify_device(0x11, 3, "rekordbox"), DeviceKind::Rekordbox);
    assert_eq!(classify_device(7, 1, "DeckLight"), DeviceKind::Other);
}
