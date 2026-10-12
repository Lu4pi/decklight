# Änderungen

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
