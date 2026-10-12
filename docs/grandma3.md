# DeckLight mit grandMA3

DeckLight schickt dem Pult (oder grandMA3 onPC) zwei Dinge:

- **Timecode** (Art-Net oder MTC) vom Master-Deck, damit eine Timecode-Show
  synchron zum Track läuft,
- **Befehle** per OSC (`/cmd`) an Cue-Punkten und bei Deck-Ereignissen.

Beides geht einzeln oder zusammen.

## Netz

DeckLight-Rechner und Pult brauchen eine Verbindung im Licht-Netz (z. B. 2.x.x.x
oder 192.168.1.x). Der Rechner kann dafür eine zweite Netzwerkkarte nutzen; das
DJ-Netz bleibt getrennt. Für onPC auf demselben Rechner reicht `127.0.0.1`.

## 1. Art-Net-Timecode

**In DeckLight** (*Ausgänge → Timecode*):

1. „Timecode senden“ einschalten.
2. Bildrate wie im Pult wählen (Standard 25 fps).
3. *Art-Net an*: IP des Pults (am sichersten) oder Broadcast des Licht-Netzes,
   z. B. `2.255.255.255`. Port 6454 lassen.
4. *Bei Stopp*: „letzten Wert halten“ (das Pult bleibt stehen, wenn der DJ pausiert)
   oder „verstummen“ (das Pult merkt den Ausfall).

**In der grandMA3:**

1. *Menu → DMX Protocols → Art-Net*: Art-Net aktivieren und die Schnittstelle im
   Licht-Netz wählen, damit das Pult Art-Net-Pakete annimmt.
2. *Menu → In & Out → Timecode*: einen Timecode-Slot auf die Quelle **Art-Net**
   stellen und die Bildrate wie in DeckLight setzen. Läuft ein Track, zählt der
   Slot sichtbar mit.
3. Einen Timecode-Pool-Eintrag anlegen, ihm diesen Slot zuweisen und die Events
   der Show aufnehmen oder von Hand setzen.

**Zeiten:** Jeder Track hat seine eigene Stunde. Der erste Track startet bei
01:00:00:00, der zweite bei 02:00:00:00 usw.; die Liste steht in *Setlist &
Offsets*. Eine Stelle 1:23 im zweiten Track sendet also 02:01:23:00. Pro Track
eine Timecode-Show (oder ein Abschnitt in einer langen) mit dieser Startzeit
anlegen. Offsets lassen sich per Doppelklick ändern oder als CSV mit dem
Show-Planer austauschen (`key;label;offset`).

Läuft der DJ rückwärts, springt per Hot Cue oder loopt, springt der Timecode mit.
Ein Pitch ändert die Laufgeschwindigkeit, der Timecode bleibt zur Musik synchron
(Abweichung im Simulator unter 5 ms, also unter einem Frame).

## 2. MTC (Alternative)

Wenn Art-Net nicht geht, MIDI-Timecode über einen virtuellen MIDI-Port: unter
macOS den IAC-Treiber in *Audio-MIDI-Setup* aktivieren, unter Windows z. B.
loopMIDI installieren. In DeckLight *MTC an MIDI-Ausgang* wählen, im Pult den
Timecode-Slot auf **MIDI** stellen. Für ein Hardware-Pult braucht es ein
MIDI-Interface.

## 3. OSC-Befehle

**In der grandMA3** (*Menu → In & Out → OSC*):

1. Eine Zeile anlegen, Schnittstelle im Licht-Netz wählen.
2. **Port** (z. B. `8000`) und optional ein **Präfix** (z. B. `gma3`) eintragen.
3. **Receive** und **Receive Command** einschalten, *Enable Input* aktivieren.

**In DeckLight** (*Ausgänge → Befehle an die grandMA3*):

1. *OSC-Ziel*: `IP-des-Pults:8000`, *Präfix*: `gma3`, speichern.
2. **OSC testen** schickt den Befehl aus dem Testfeld; im Pult steht er in der
   Kommandozeile bzw. im System Monitor.

**Cues anlegen** (*Cues & Trigger*): Track wählen, in die Wellenform klicken,
**+ Cue hier**. Ein Cue hat

- eine Position als Zeit oder als **Takt/Schlag** (bleibt beim Neu-Analysieren
  des Grids auf dem Beat),
- optional einen **Vorlauf** in ms, um die Reaktionszeit von Moving Lights
  auszugleichen,
- eine oder mehrere Aktionen: *MA3-Befehl (OSC)* wie `Go+ Exec 201` oder
  `Off Sequence 12`, freie *OSC-Adresse* mit Werten, oder MIDI Note/CC/Program.

`{deck}` im Befehl wird durch die Decknummer ersetzt (`Go+ Exec 10{deck}`).
**Testen** schickt die Aktionen sofort ans Pult.

Rekordbox-Hot-Cues erscheinen als blasse Vorlagen: Aktion eintragen, schon ist
es ein Cue. **Globale Trigger** reagieren auf *Track geladen*, *Master-Wechsel*,
*Play*, *Stop*, *On Air* und *Off Air*, für jedes Deck, nur den Master oder ein
bestimmtes Deck.

### Wann ein Cue auslöst

- Wenn der Playhead im Lauf über den Cue läuft (Standard: nur auf dem
  Master-Deck, umstellbar auf *jedes Deck* global oder je Cue).
- Erneut erst, wenn der Playhead mindestens 250 ms davor stand (Loop, Hot Cue,
  Zurückspulen).
- Pause und Scrubben im Stillstand lösen nichts aus. Ein Sprung nach vorn
  überspringt Cues, ein Hot Cue genau auf einen Cue löst ihn aus.
- **Not-Aus** (Esc oder roter Knopf) sperrt alle Trigger sofort, Timecode läuft
  weiter. Wieder scharf mit Strg/Cmd + Umschalt + A.

Das Protokoll unter *Monitoring* zeigt jede ausgelöste Aktion mit Verspätung in
ms und lässt sich als CSV speichern.

DeckLight nutzt bewusst `/cmd` mit Befehlstext statt Objekt-Adressen, weil sich
diese zwischen MA3-Versionen ändern. Mit Präfix `gma3` sendet DeckLight
`/gma3/cmd "Go+ Exec 201"`.

## Probe ohne DJ

1. `decklight simulate` (Kommandozeilen-Werkzeug, liegt jedem Release bei) spielt
   zwei virtuelle CDJ-3000 und einen DJM in Schleife, nur auf diesem Rechner.
2. DeckLight mit *nur zuhören* starten, Timecode und OSC wie oben einrichten.
3. Im Pult müssen Timecode-Slot und Kommandozeile mitlaufen.

## Kommandozeile

Ohne Oberfläche (z. B. auf einem Server neben dem Pult):

```sh
decklight run --iface en5 --artnet 2.255.255.255 --fps 25 --offsets show-offsets.csv \
              --osc 192.168.1.50:8000 --osc-prefix gma3 --triggers show-cues.json
```

`decklight triggers > show-cues.json` erzeugt eine Vorlage für die Cue-Datei.
Not-Aus: `n` + Enter, wieder scharf: `s` + Enter.
