# Änderungen

## Unveröffentlicht

- **Startbildschirm:** Beim Öffnen der App läuft ein animierter Ladebildschirm
  (Licht schwenkt, Decks drehen, Beat-Anzeige im 120er-Takt), bis der Kern
  bereit ist. Monitor und OBS-Overlay starten ohne ihn.
- **Auto-Pilot:** Licht folgt dem Track, wenn der Operator kurz weg ist. DeckLight
  liest die rekordbox-Phrasenanalyse (PSSI) und verrechnet sie mit der
  Drei-Band-Wellenform zu Intro, Groove, Breakdown, Build-up, Drop, Pausen und
  Outro. Befehle je Abschnitt an die grandMA3 auf die Eins genau, Drop-Hit,
  Blackout bei Pausen und Stopp, Build-up stufenlos auf Speed-Master 2, Look-Wechsel
  alle 16 Takte, Übergabe im Mix auf der nächsten Eins. Ein/Aus per OSC von der MA3
  (`/decklight/auto`), mit der AUTO-Taste und am Handy; Schattenmodus zum Prüfen.
  Struktur-Band unter allen Wellenformen. Anleitung: [docs/auto.md](docs/auto.md).

## 1.0.0

Erste Version für den Einsatz am echten Setup.

- **PRO DJ LINK:** erkennt CDJ-3000, CDJ-2000NXS2, XDJ und DJM-Mixer, meldet sich
  als virtueller Player (5–15, nie 1–4), liest Status, Beats und Fader-Stellungen.
- **Decks:** farbige Wellenform (Übersicht und Detail), Beatgrid, Rekordbox-Cues,
  Titel, Artist, Cover, BPM, Pitch, Key, Restzeit, On-Air und Master.
- **Timecode:** Art-Net-Timecode und MTC vom Master-Deck, eine Stunde je Track
  (automatisch oder von Hand, CSV), folgt Pitch, Sprüngen, Loops und Rückwärtslauf.
- **Trigger:** Cues an Zeit oder Takt/Schlag mit Vorlauf, globale Trigger bei
  Deck-Ereignissen, OSC an die grandMA3 (`/cmd`), MIDI Note/CC/Program, Not-Aus,
  Protokoll mit Verspätung je Aktion.
- **Shows:** speichern, öffnen, Import/Export (`.decklight`), Sicherung alle fünf
  Minuten; Tracks werden über Inhalts-Fingerabdruck auch nach geänderten Tags
  wiedererkannt.
- **Web:** `/monitor` fürs Tablet und `/overlay` für OBS, optional mit PIN; Bedienung
  aus dem Netz nur nach Freigabe.
- **Werkzeuge:** Simulator, Mitschnitt und Wiedergabe, Dauerlauf (`decklight soak`).
