# DeckLight mit grandMA3

DeckLight schickt dem Pult zwei Dinge: **Timecode** (Art-Net oder MTC) für
Timecode-Shows und **Befehle** per OSC für einzelne Cues.

## 1. Art-Net-Timecode

1. Rechner mit DeckLight und Pult (oder grandMA3 onPC) ins selbe Netz, z. B. 2.x.x.x
   oder 192.168.1.x.
2. DeckLight starten:
   ```sh
   decklight run --iface en5 --artnet 2.255.255.255 --fps 25 --offsets show-offsets.csv
   ```
   Statt Broadcast geht auch die IP des Pults.
3. grandMA3: *Menu → In & Out → Timecode*: als Quelle **Art-Net** wählen, Bildrate
   passend einstellen (25 fps).
4. Jeder Track bekommt automatisch eine eigene Stunde: erster Track 01:00:00:00,
   zweiter 02:00:00:00 usw. Die Zuordnung steht in `show-offsets.csv` und kann dort
   geändert werden (`key;label;offset`).
5. Timecode-Show im Pult aufnehmen oder Events auf die Stunde des Tracks legen.

`--stop-mode freeze` (Standard) sendet im Stillstand den letzten Wert weiter,
`--stop-mode stop` verstummt, damit das Pult den Ausfall erkennt.

## 2. MTC (Alternative)

`decklight midi-ports` listet die MIDI-Ausgänge. Unter macOS den IAC-Treiber
aktivieren, unter Windows z. B. loopMIDI installieren. Dann
`decklight run --mtc "IAC"` und im Pult MIDI-Timecode als Quelle wählen.

## 3. OSC-Befehle

1. grandMA3: *Menu → In & Out → OSC*: eine Zeile anlegen, **Receive** und
   **Receive Command** aktivieren, Port (z. B. 8000) und optional ein Präfix
   (z. B. `gma3`) eintragen.
2. Verbindung testen:
   ```sh
   decklight osc --to 192.168.1.50:8000 --prefix gma3 "Go+ Exec 201"
   ```
3. Cue-Punkte und globale Trigger senden dieselben Befehle automatisch (Phase 4).

DeckLight nutzt bewusst `/cmd` mit Befehlstext statt Objekt-Adressen, weil sich
diese zwischen MA3-Versionen ändern.
