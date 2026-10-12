# DeckLight

Open-Source-Show-Steuerung für Pioneer/AlphaTheta-CDJs: Decks live sehen, das Netz
überwachen und eine grandMA3 per Timecode und OSC synchron zur Musik fahren.

**Installieren und loslegen:** [Einrichtung in 5 Schritten](docs/einrichtung.md) ·
[grandMA3 einrichten](docs/grandma3.md) · [Rider-Vorlage](docs/rider.md)

## Was DeckLight kann

- **Sehen:** alle Decks mit farbiger Wellenform, Beatgrid, Cues, Titel, Artist,
  Cover, BPM, Pitch, Key, Restzeit und On-Air-Anzeige.
- **Überwachen:** Geräte, Netzwerk, Track-History (CSV), Warnungen.
- **Timecode:** Art-Net-Timecode und MTC vom Master-Deck, jeder Track mit eigenem
  Offset (automatisch eine Stunde je Track, frei änderbar, CSV).
- **Triggern:** Cue-Punkte (Zeit oder Takt/Schlag, Vorlauf) und Deck-Ereignisse
  lösen OSC-Befehle (`/cmd`) an die grandMA3 und MIDI Note/CC/Program aus.
  Not-Aus mit Esc.
- **Shows:** speichern, öffnen, exportieren, automatische Sicherungen; Tracks werden
  auch auf anderen USB-Sticks und nach geänderten Tags wiedererkannt.
- **Web:** `/monitor` fürs Tablet, `/overlay` für OBS und Streams, optional mit PIN.

## Overlay

`http://<rechner>:8750/overlay` als Browser-Quelle in OBS einbinden (Hintergrund
transparent). Parameter:

| Parameter | Werte | Standard |
| --- | --- | --- |
| `theme` | `dark`, `light`, `glass` | `dark` |
| `layout` | `card` (Karte unten links), `bar` (Leiste über die ganze Breite) | `card` |
| `deck` | `master` oder `1`–`6` | `master` |
| `accent` | Hex-Farbe ohne `#`, z. B. `ff8a1f` | `ffb020` |
| `hide` | Komma-Liste aus `cover`, `artist`, `bpm`, `progress` | – |

Beispiel: `/overlay?theme=glass&layout=bar&accent=2fa8ff&hide=bpm`.

## Ohne Hardware ausprobieren

```sh
# Terminal 1: zwei virtuelle CDJ-3000 und ein DJM, nur auf diesem Rechner
cargo run -p decklight -- simulate

# Terminal 2: Oberfläche im Browser unter http://127.0.0.1:8750
(cd ui && npm ci && npm run build)
cargo run -p decklight -- serve --passive
```

Eigene Abläufe für den Simulator: `decklight scenario > meine-show.json`, Datei
anpassen, dann `decklight simulate --scenario meine-show.json`.

## Kommandozeile

Die Desktop-App (`src-tauri`) startet Kern und Server selbst. Für Server ohne
Bildschirm, Tests und Fehlersuche gibt es `decklight`:

| Befehl | Zweck |
| --- | --- |
| `decklight serve --data-dir show-daten` | Kern + Oberfläche im Browser (Port 8750) |
| `decklight interfaces` | Netzwerkschnittstellen |
| `decklight watch --iface en5` | Decks live im Terminal |
| `decklight run --artnet 2.255.255.255 --osc 192.168.1.50:8000 --triggers cues.json` | Timecode + Trigger ohne Oberfläche (`n` Not-Aus, `s` scharf) |
| `decklight osc --to 192.168.1.50:8000 --prefix gma3 "Go+ Exec 201"` | OSC-Verbindung testen |
| `decklight triggers > cues.json` | Vorlage für Cues und Trigger |
| `decklight record show.jsonl` / `decklight replay show.jsonl` | Mitschnitt vom echten Setup aufnehmen und durch die Engine spielen |
| `decklight soak --minutes 240` | Dauerlauf mit Simulator, misst CPU, RAM und Durchsatz |

Mit echten Playern meldet sich DeckLight als virtueller Player an (nie 1–4).
Ohne Anmeldung (`--passive`) schicken echte CDJs keinen Status.

## Aufbau

| Pfad | Inhalt |
|---|---|
| `crates/prolink` | PRO DJ LINK: Pakete lesen/schreiben, Geräteerkennung, virtueller Player |
| `crates/engine` | Deck-Zustand, Timecode-Uhr mit Jitterfilter, Master-Auswahl, Ereignisse |
| `crates/outputs` | Timecode-Rechnung, Art-Net-Timecode, MTC, OSC, MIDI |
| `crates/library` | dbserver-Client und -Server, ANLZ (Wellenform, Cues), Track-Cache |
| `crates/show` | Show-Daten: Track-Offsets mit CSV-Import/Export, Cues und Trigger |
| `crates/triggers` | Auslöse-Logik für Cues und globale Trigger, Aktions-Log |
| `crates/server` | Webserver: WebSocket-Zustand, Befehle, `/monitor`, `/overlay`, Einstellungen |
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
