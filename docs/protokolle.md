# Protokoll-Referenzen

| Thema | Quelle | Wofür |
|---|---|---|
| PRO DJ LINK | [DJ Link Ecosystem Analysis](https://djl-analysis.deepsymmetry.org/) | Keep-Alive, Status, Beats, präzise Position, dbserver, ANLZ |
| PRO DJ LINK (Code) | [beat-link](https://github.com/Deep-Symmetry/beat-link) | Referenzimplementierung (Java), Paket-Vorlagen |
| Wireshark-Dissector | [dysentery](https://github.com/Deep-Symmetry/dysentery) | Mitschnitte lesen |
| Rekordbox-Export | [rekordcrate](https://github.com/Holzhaus/rekordcrate) | `export.pdb` und ANLZ-Dateien in Rust |
| Art-Net 4 | [Art-Net-Spezifikation](https://art-net.org.uk/) | OpTimeCode (0x9700) |
| MIDI Timecode | MIDI 1.0, MTC-Anhang | Quarter-Frame und Full-Frame |
| grandMA3 OSC | grandMA3-Handbuch, Kapitel "OSC" | `/cmd` mit Befehlstext |
| TCNet (v2) | TCNet-Spezifikation V3.5.1B (tc-supply.com) | späterer TCNet-Server |

## Ports und Gerätenummern

- UDP 50000: Keep-Alive/Anmeldung, UDP 50001: Beats, präzise Position, On-Air;
  UDP 50002: Status von Playern und Mixer. TCP 12523: dbserver (Metadaten).
- Player 1–6, Mixer 33 (0x21), rekordbox ab 17. DeckLight nutzt standardmäßig 7.
- CDJs schicken Status-Pakete nur an angemeldete Geräte, deshalb meldet sich
  DeckLight als virtueller Player an. Beats und Keep-Alives kommen per Broadcast.

## Offene Punkte für echte Hardware

Die Offsets in `crates/prolink/src/packet.rs` folgen der Deep-Symmetry-Analyse und
beat-link. Sie werden gegen eigene Wireshark-Mitschnitte (CDJ-3000, CDJ-2000NXS2,
DJM-900NXS2, DJM-V10) geprüft, sobald diese vorliegen (Checkliste Phase 0).
Besonders zu prüfen: Mixer-Status (Typ 0x29) und On-Air-Paket des DJM-V10.
