# DeckLight v1: Checkliste

Erledigte Punkte werden hier abgehakt. Grundlage: Bauplan v1 vom 12.10.2026.

## Feature-Liste v1 (Abnahmekriterien)

| # | Feature | Fertig, wenn … |
|---|---|---|
| F1 | Geräteerkennung PRO DJ LINK | CDJ-3000/2000NXS2 und DJM-900NXS2/V10 erscheinen innerhalb von 3 s mit Name, Nummer, IP |
| F2 | Deck-Status live | Play/Pause/Cue/Loop, Position, BPM, Pitch, Beat im Takt, Tempo-Master, On-Air je Deck, Aktualisierung ≥ 30×/s in der UI |
| F3 | Track-Metadaten | Titel, Artist, Album, Key, Dauer, Cover werden beim Laden in ≤ 2 s angezeigt |
| F4 | Wellenform + Beatgrid | Farbige Übersichts- und scrollende Detail-Wellenform (CDJ-3000-Stil), Beatgrid, Hot Cues und Memory Cues als Marker |
| F5 | Master-Deck-Logik | Modi: Manuell, Tempo-Master, On-Air/höchster Fader. Kein Flackern beim Übergang (Hysterese) |
| F6 | Track-Bibliothek + Offsets | Jeder erkannte Track bekommt automatisch einen Offset (Standard: neue Stunde je Track), manuell editierbar, gespeichert in der Show-Datei |
| F7 | Timecode-Ausgang | Art-Net Timecode (25/30/29,97/24 fps) und MTC; folgt Pitch, Pause, Cue-Sprüngen, Loops; Abweichung < 1 Frame |
| F8 | Cue-Punkte und Trigger | Cue auf der Wellenform setzen (oder Rekordbox-Hot-Cue übernehmen) → Aktion OSC an grandMA3 (`/cmd` + Befehlstext) oder MIDI Note/CC; Arm/Disarm, Not-Aus, Log |
| F9 | Globale Trigger | "Track geladen", "Deck wird Master", "Play", "Stop" → OSC/MIDI-Aktion |
| F10 | Monitoring | Geräte-Panel, Warnungen, Track-History mit Uhrzeit, CSV-Export |
| F11 | Webserver + Overlay | `/monitor` (Vollbild, Handy/Tablet) und `/overlay` (transparent, aktueller Track + Cover + BPM) im lokalen Netz, live per WebSocket |
| F12 | Simulator | Virtuelle CDJs aus Audiodateien bzw. aufgezeichneten Mitschnitten, damit alles ohne Hardware entwickelt und getestet werden kann |
| F13 | Show-Datei | Projekt speichern/laden (Tracks, Offsets, Cues, Ausgänge), Auto-Save, Backup |
| F14 | Installer | Signierte Builds für macOS (Apple Silicon + Intel) und Windows |

---

## Checkliste

### Phase 0: Vorbereitung
- [x] GitHub-Repo anlegen (z. B. `Lu4pi/decklight`), Lizenz GPL-3.0, README
- [x] Rust-Workspace + Svelte + Tauri Grundgerüst, Lint, Formatter
- [x] CI (GitHub Actions) für macOS, Windows, Linux: Build + Tests
- [x] Protokoll-Referenzen in `docs/` verlinken (DJ Link Analysis, Art-Net-Spec, grandMA3 OSC)
- [ ] Testaufbau festlegen: welche CDJs/DJM vorhanden, Switch, grandMA3 onPC
- [ ] Wireshark-Mitschnitte vom echten Setup aufnehmen (Laden, Play, Pitch, Loop, Hot Cue, Fader) und als Testdaten einchecken  
  _Werkzeug fertig: `decklight record mitschnitt.jsonl` zeichnet alle Pakete auf, `decklight replay` spielt sie durch die Engine._

### Phase 1: PRO DJ LINK lesen
- [x] Netzwerk-Interface wählbar, UDP 50000/50001/50002 öffnen
- [x] Geräteerkennung (Keep-Alive), Geräteliste mit Typ, Nummer, IP
- [x] Status-Pakete CDJ parsen (Zustand, Position, Pitch, BPM, Beat, Master, On-Air)
- [x] Mixer-Status parsen (On-Air je Kanal, Fader soweit verfügbar)
- [x] Beat-Pakete parsen
- [ ] Als virtueller Player anmelden (Nummer ohne Konflikt), sauber abmelden  
  _Stand: Keep-Alive als Player 7 und Konflikterkennung fertig; Nummern-Claim-Ablauf folgt mit Hardware-Test._
- [x] dbserver-Client: Metadaten, Cover, Beatgrid, Wellenformen, Cues  
  _Stand: Client und simulierter dbserver (`decklight simulate` liefert Metadaten, Cover, Wellen, Cues). Byte-Layout nach Doku, Abgleich mit echtem CDJ steht aus._
- [ ] Fallback: Rekordbox-Export vom USB-Stick lesen (rekordcrate)
- [ ] Cache pro Track; Unit-Tests gegen Mitschnitte  
  _Stand: Cache und Tests gegen den simulierten dbserver fertig; Mitschnitte vom echten CDJ fehlen noch._
- [x] **Meilenstein:** CLI zeigt live alle Decks mit Position, BPM, Titel  
  _`decklight watch` zeigt den Titel unter jedem Deck, geprüft mit `decklight simulate`._

### Phase 2: Engine
- [x] DeckState pro Player inkl. Track-Wechsel-Erkennung
- [x] TimecodeClock: Interpolation zwischen Paketen, Jitterfilter, harte Sprünge nur bei Cue/Loop/Hot Cue
- [x] Rückwärtslauf, Pause, Loop-Ende korrekt
- [x] MasterSelector mit drei Modi + Hysterese
- [x] Event-Bus (Events typisiert, aufzeichen- und abspielbar)
- [ ] Simulator: virtuelle CDJs aus Audiodatei + PCAP-Replay  
  _Stand: virtuelle CDJs/DJM mit JSON-Szenarien fertig; Audiodatei und PCAP-Replay offen._
- [x] **Meilenstein:** Timecode-Abweichung im Simulator < 1 Frame über 10 min  
  _Stand: CDJ-3000 max. 3,4 ms, CDJ-2000NXS2 (nur Beat-Pakete) max. 5,2 ms im Lauf; NXS2 kennt nach Start/Stopp/Sprung die Position erst mit dem nächsten Beat genau (`crates/engine/tests/timecode_accuracy.rs`)._

### Phase 3: Ausgänge für grandMA3
- [x] Art-Net Timecode (OpTimeCode) senden, Broadcast/Unicast, fps wählbar
- [x] MTC (Quarter-Frame + Full-Frame bei Sprung) über MIDI-Port
- [x] OSC-Client (UDP), Ziel-IP/Port/Präfix konfigurierbar, `/cmd` mit Befehlstext
- [x] MIDI Note/CC als Aktion  
  _Stand: Nachrichten und Ausgang fertig; Auslösen über Trigger folgt in Phase 4._
- [ ] Track-Offsets: automatische Vergabe, Editor, CSV-Import/Export  
  _Stand: Automatik und CSV fertig (`decklight run --offsets`); Editor folgt mit der Oberfläche._
- [ ] Test mit grandMA3 onPC: Timecode-Show auf zwei Tracks programmieren, Deck-Wechsel, Pitch ±8 %, Loop  
  _Automatischer Ersatz: `crates/core/tests/timecode_e2e.rs` (simulierter CDJ → Art-Net-Empfänger, jeder Frame zur richtigen Zeit). Test am echten Pult steht aus._
- [ ] **Meilenstein:** MA3 fährt eine Timecode-Show synchron zu einem echten oder simulierten CDJ

### Phase 4: Trigger
- [x] Datenmodell Cue (Track, Position ms oder Takt/Beat, Vorlauf, Aktionen, Farbe, Name)  
  _`crates/show/src/cues.rs`; Aktionen: grandMA3-Befehl über `/cmd`, beliebige OSC-Adresse, MIDI Note/CC/Program; Platzhalter `{deck}`._
- [x] Cue auslösen nur bei Master-Deck oder bei jedem Deck (einstellbar)  
  _Global und je Cue überschreibbar._
- [x] Rekordbox-Hot-Cues als Cue-Vorlage übernehmen  
  _Hot und Memory Cues neuer Tracks werden als Cues ohne Aktion angelegt, solange der Track noch keine eigenen hat._
- [x] Globale Trigger (Track geladen, Master-Wechsel, Play, Stop)  
  _Dazu On Air/Off Air; Filter: jedes Deck, Master-Deck oder ein bestimmtes Deck._
- [x] Arm/Disarm pro Cue und global, Not-Aus-Taste (Tastenkürzel)  
  _`Core::panic()`/`set_armed()`; in `decklight run`: n + Enter = Not-Aus, s + Enter = scharf. Tastenkürzel in der Oberfläche folgt in Phase 5._
- [x] Aktions-Log mit Zeitstempel  
  _Ringpuffer mit Uhrzeit, Deck, Quelle, Aktionen, Verspätung und Fehler; CSV-Export._
- [x] Tests: kein doppeltes Auslösen bei Pause/Scrubben, korrektes Verhalten bei Rücksprung  
  _Unit-Tests in `crates/triggers`; `crates/triggers/tests/simulated_show.rs` vergleicht 5 min Simulator mit Jitter gegen den echten Playhead (CDJ-3000 max. 3 ms, NXS2 max. 5 ms im Lauf, identische Cue-Folge); `crates/core/tests/triggers_e2e.rs` misst OSC über UDP (± 3 ms)._

### Phase 5: Oberfläche
- [ ] Design-System (Farben, Schrift, Deck-Farben, Dark Mode)
- [ ] Deck-Kachel: Cover, Titel, Artist, BPM, Pitch, Key, Zeit, Restzeit, Zustand
- [ ] Übersichts-Wellenform + scrollende Detail-Wellenform mit Beatgrid und Cue-Markern (60 fps)
- [ ] On-Air/Master-Hervorhebung, großer Timecode-Zähler
- [ ] Setlist-/Offset-Ansicht
- [ ] Cue-Editor direkt auf der Wellenform (klicken = Cue setzen, Aktion wählen)
- [ ] Ausgänge-Seite (Art-Net, MTC, OSC, MIDI) mit Test-Button
- [ ] Monitoring-Panel: Geräte, Warnungen, Netzwerk, Log
- [ ] Track-History mit CSV-Export

### Phase 6: Webserver und Overlay
- [ ] axum-Server, WebSocket-Stream des Zustands
- [ ] `/monitor`: Vollbild-Ansicht für Handy/Tablet
- [ ] `/overlay`: transparentes Overlay (aktueller Track, Cover, BPM, Fortschritt), Themes per URL-Parameter
- [ ] Optional PIN-Schutz, nur lesend

### Phase 7: Show-Datei, Stabilität, Release
- [ ] Show-Datei speichern/laden, Auto-Save, Backup-Kopien
- [ ] Track-Wiedererkennung über USB-Sticks hinweg (Titel + Artist + Dauer + Hash)  
  _Stand: Offsets hängen an Artist + Titel + Dauer und wandern beim Laden der Metadaten mit; Hash folgt._
- [ ] Dauerlauftest 4 h mit Simulator, CPU/RAM-Messung
- [ ] Test am echten Setup (CDJ-3000 + DJM), Fehlerliste abarbeiten
- [ ] Doku: Setup in 5 Schritten, grandMA3-Anleitung (Art-Net TC + OSC einrichten), Rider-Vorlage
- [ ] Signierte Installer macOS + Windows, GitHub Release v1.0.0

---

