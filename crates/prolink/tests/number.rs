//! Virtueller Player: belegte Nummern werden erkannt und DeckLight weicht aus,
//! ohne je 1–4 zu nehmen.

use decklight_prolink::packet::KeepAlive;
use decklight_prolink::{
    encode, DeviceKind, Link, LinkConfig, LinkEvent, PortConfig, VirtualPlayer,
};
use std::net::Ipv4Addr;
use std::time::Duration;

fn keep_alive(name: &str, number: u8, ip: [u8; 4]) -> Vec<u8> {
    encode::keep_alive(&KeepAlive {
        name: name.into(),
        number,
        kind: DeviceKind::Player,
        mac: [0, 1, 2, 3, 4, number],
        ip: Ipv4Addr::from(ip),
        peers_seen: 2,
    })
}

async fn start(number: u8) -> Link {
    Link::start(LinkConfig {
        bind_ip: Ipv4Addr::LOCALHOST,
        ports: PortConfig {
            announce: 0,
            beat: 0,
            status: 0,
        },
        virtual_player: Some(VirtualPlayer {
            name: "DeckLight".into(),
            number,
            ip: Ipv4Addr::LOCALHOST,
            mac: [2, 0, 0, 0, 0, 1],
            broadcast: Ipv4Addr::LOCALHOST,
        }),
        ..Default::default()
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn takes_a_free_number_if_the_wanted_one_is_in_use() {
    let link = start(7).await;
    let port = link.local_ports().announce;
    let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    // Vor dem ersten eigenen Keep-Alive: ein anderer Rechner ist schon Nummer 7.
    sock.send_to(
        &keep_alive("rekordbox", 7, [10, 0, 0, 9]),
        ("127.0.0.1", port),
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(link.own_number(), Some(8));
    // Auch nach dem Zuhören vor der Anmeldung bleibt es bei der freien Nummer.
    tokio::time::sleep(decklight_prolink::net::LISTEN_BEFORE_ANNOUNCE).await;
    assert_eq!(link.own_number(), Some(8));
}

#[tokio::test(flavor = "multi_thread")]
async fn moves_away_when_another_device_appears_with_our_number() {
    let link = start(9).await;
    let mut events = link.subscribe();
    let port = link.local_ports().announce;
    let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for (n, ip) in [(1u8, 11u8), (10, 12), (9, 13)] {
        sock.send_to(
            &keep_alive("CDJ-3000", n, [10, 0, 0, ip]),
            ("127.0.0.1", port),
        )
        .await
        .unwrap();
    }
    let switched = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(LinkEvent::NumberConflict { switched_to, .. }) = events.recv().await {
                return switched_to;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(switched, Some(11), "9 und 10 belegt");
    assert_eq!(link.own_number(), Some(11));
}
