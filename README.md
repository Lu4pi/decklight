# DeckLight

Open-Source-Show-Steuerung für Pioneer/AlphaTheta-CDJs: Decks live sehen, das Netz
überwachen und eine grandMA3 per Timecode und OSC synchron zur Musik fahren.

> Status: frühe Entwicklung (v0.1). Geräteerkennung, Engine, Timecode-Ausgänge,
> Track-Metadaten, Cues mit OSC/MIDI und Simulator laufen; Oberfläche und Webserver
> folgen Phase für Phase
> (siehe [docs/checkliste.md](docs/checkliste.md)).

## Was DeckLight v1 können soll

- **Sehen:** alle Decks mit Wellenform, Playhead, Titel, Artist, Cover, BPM, Pitch,
  Key, Restzeit und On-Air-Anzeige.
- **Überwachen:** Geräte, Netzwerk, Track-History, Warnungen.
- **Triggern:** Art-Net-Timecode und MTC je Track mit eigenem Offset, Cue-Punkte mit
  OSC-Befehlen (`/cmd`) an die grandMA3, MIDI Note/CC.
- **Overlay:** `/monitor` fürs Tablet, `/overlay` für OBS und Streams.

## Schnellstart ohne Hardware

```sh
# Terminal 1: zwei virtuelle CDJ-3000 und ein DJM, nur auf diesem Rechner
cargo run -p decklight -- simulate

# Terminal 2: Decks live anzeigen, mit Titel aus dem simulierten dbserver
cargo run -p decklight -- watch --passive
```

Mit echten Playern: `decklight interfaces` zeigt die Netzwerkschnittstellen,
`decklight watch --iface en5` meldet DeckLight als virtuellen Player 7 an (nie 1–4)
und zeigt alle Decks. Ohne Anmeldung (`--passive`) schicken echte CDJs keinen Status.

Timecode fürs Pult: `decklight run --artnet 2.255.255.255 --fps 25 --offsets offsets.csv`
sendet Art-Net-Timecode vom Master-Deck, jeder Track bekommt eine eigene Stunde.
`decklight osc --to 192.168.1.50:8000 --prefix gma3 "Go+ Exec 201"` testet die
OSC-Verbindung. Einrichtung am Pult: [docs/grandma3.md](docs/grandma3.md).

Cues fürs Pult: `decklight triggers > show-cues.json` erzeugt eine Vorlage mit
Cues (Position in ms oder Takt/Schlag, Vorlauf, OSC-/MIDI-Aktionen) und globalen
Triggern (Track geladen, Master-Wechsel, Play, Stop). Dann
`decklight run --osc 192.168.1.50:8000 --osc-prefix gma3 --triggers show-cues.json`.
Rekordbox-Hot-Cues neuer Tracks landen automatisch als Vorlagen in der Datei.
Not-Aus während der Show: `n` + Enter, wieder scharf mit `s` + Enter.

Mitschnitte vom echten Setup: `decklight record show.jsonl` speichert alle
Pakete, `decklight replay show.jsonl` spielt sie durch die Engine und gibt die
Ereignisse (Track geladen, Play, Sprünge, Master-Wechsel) als JSON aus.

Eigene Abläufe für den Simulator: `decklight scenario > meine-show.json`, Datei
anpassen, dann `decklight simulate --scenario meine-show.json`.

## Aufbau

| Pfad | Inhalt |
|---|---|
| `crates/prolink` | PRO DJ LINK: Pakete lesen/schreiben, Geräteerkennung, virtueller Player |
| `crates/engine` | Deck-Zustand, Timecode-Uhr mit Jitterfilter, Master-Auswahl, Ereignisse |
| `crates/outputs` | Timecode-Rechnung, Art-Net-Timecode, MTC, OSC, MIDI |
| `crates/library` | dbserver-Client und -Server, ANLZ (Wellenform, Cues), Track-Cache |
| `crates/show` | Show-Daten: Track-Offsets mit CSV-Import/Export, Cues und Trigger |
| `crates/triggers` | Auslöse-Logik für Cues und globale Trigger, Aktions-Log |
| `crates/core` | Laufzeit: verbindet Netz, Engine, Show und Ausgänge |
| `crates/simulator` | Virtuelle CDJs und DJM mit echten Paketen, Szenarien als JSON |
| `crates/app` | Kommandozeile `decklight` |
| `ui/` | Svelte-5-Oberfläche (Desktop, Monitor, Overlay) |
| `src-tauri/` | Tauri-2-Hülle für macOS, Windows, Linux |
| `docs/` | Checkliste, Protokoll-Referenzen |

## Entwickeln

Voraussetzungen: Rust (stable), Node 22. Unter Linux für die Desktop-App zusätzlich
`libwebkit2gtk-4.1-dev librsvg2-dev libasound2-dev`.

```sh
cargo test --all                         # Kern
cargo clippy --all-targets -- -D warnings
cd ui && npm ci && npm run check && npm run build
cargo build --manifest-path src-tauri/Cargo.toml   # Desktop-Hülle
```

## Lizenz und Dank

GPL-3.0-or-later. Das Protokollwissen stammt aus der
[DJ Link Ecosystem Analysis](https://djl-analysis.deepsymmetry.org/) von Deep
Symmetry (James Elliott) und den Projekten beat-link und beat-link-trigger.
DeckLight ist nicht mit Pioneer DJ, AlphaTheta oder TC Supply verbunden.
