# Auto-Pilot: Licht folgt dem Track

Wenn der Lichtoperator kurz weg ist, übernimmt DeckLight: Es erkennt in jedem
Track Intro, Groove, Breakdown, Build-up, Drop, Pausen und Outro und schickt
passende Befehle an die grandMA3, **genau auf die Eins** und nicht hinterher wie
klassisches Sound-to-Light. Kommt der Operator zurück, gibt eine Taste das Licht
wieder ab.

## Woher DeckLight die Struktur kennt

DeckLight kennt den ganzen Track schon beim Laden:

1. **rekordbox-Phrasenanalyse** (in rekordbox unter *Voreinstellungen → Analyse →
   Phrase* einschalten, dann Tracks neu analysieren und exportieren). Aus *Up*
   wird Build-up, aus *Down* und *Bridge* Breakdown, aus *Chorus* nach einem
   Breakdown der Drop.
2. **Drei-Band-Wellenform** (CDJ-3000, sonst die farbige Wellenform): Wo der Bass
   für mindestens vier Takte fehlt, ist ein Breakdown; ziehen danach Mitten und
   Höhen an, ist das der Build-up; wo der Bass zurückkommt, der Drop. Fast stille
   Beats direkt davor sind eine Pause.

Stimmen beide überein, hat ein Abschnitt 92 % Sicherheit. Widersprechen sie
sich, sinkt sie auf 55 %; unter der eingestellten Mindest-Sicherheit (Standard
50 %) behandelt Auto den Abschnitt wie Groove. Ohne Phrasen arbeitet Auto nur
mit der Wellenform.

Die erkannten Abschnitte zeigt DeckLight als farbiges Band unter jeder
Wellenform: grau Intro, grün Groove, blau Breakdown, gelb Build-up, rot Drop,
lila Outro. Pausen sind hell hinterlegt.

## Was Auto sendet

| Ereignis | Standard-Befehl |
|---|---|
| Intro | `Goto Sequence "DeckLight Auto" Cue 5` |
| Groove (wechselt alle 16 Takte) | Cue 1, 2, 3 |
| Breakdown | Cue 10 |
| Build-up | Cue 20, dazu stufenlos `Master 3.2 At 0…100` |
| Drop (wechselt alle 16 Takte und bei jedem neuen Drop) | Cue 30, 31, 32 |
| Drop-Hit, genau auf der Eins | `Go+ Sequence "DeckLight Hit"` |
| Pause im Track, DJ stoppt | Cue 50 (Blackout) |
| Outro | Cue 60 |
| Auto aus | `Off Sequence "DeckLight Auto"`, `Off Sequence "DeckLight Hit"` |

Alles ist unter **Auto** in DeckLight änderbar. Mehrere Zeilen bei einem
Abschnitt wechseln sich ab, bei Ereignissen gehen alle Zeilen raus.
Platzhalter: `{seq}` Sequenz, `{deck}` Deck, `{bpm}` Tempo, `{pct}` Build-up 0–100.

Tempo und Phase laufen wie gewohnt über Speed-Master 3.1 weiter.

### Welches Deck zählt

Das Deck, das **On Air** ist und läuft; sind mehrere On Air, der Tempo-Master.
Wird im Mix ein anderes Deck führend, übernimmt Auto auf dessen nächster Eins
und ohne Drop-Hit. Ein Deck, das nur im Kopfhörer läuft, zählt nicht.
Springt der DJ (Hot Cue, Suchen), sendet Auto den Abschnitt an der neuen Stelle,
aber keinen Hit.

### Sicherheit

- **Schattenmodus:** Auto läuft mit und schreibt ins Protokoll, was es senden
  würde, sendet aber nichts. Damit beim ersten Abend prüfen, ob die Erkennung
  zu den eigenen Tracks passt.
- **Not-Aus** (Esc) stoppt auch Auto; nach dem Entsperren sendet Auto den
  aktuellen Abschnitt neu.
- Tracks mit eigenen DeckLight-Cues: Auto läuft wahlweise mit oder hält sich
  raus (*Tracks mit eigenen Cues*).
- Auto startet nach jedem Programmstart **aus**.

## grandMA3 vorbereiten (einmal)

1. **Sequenz „DeckLight Auto“** anlegen und auf einen freien Executor legen.
   Cues mit diesen Nummern speichern (Inhalt frei):
   1–3 Groove-Looks, 5 Intro, 10 Breakdown, 20 Build-up (Strobe/FX), 30–32
   Drop-Looks, 50 Blackout, 60 Outro.
2. **Sequenz „DeckLight Hit“**: ein kurzer Blinder- oder Strobe-Hit, der sich
   selbst ausblendet (Cue 1 mit Follow auf einen leeren Cue oder Off-Zeit).
3. **Speed-Master 2** (`Master 3.2`) den Build-up-Effekten zuweisen. Auto zieht
   ihn im Build-up von 0 auf 100 und am Ende zurück. Wer das nicht will, leert das
   Feld *Build-up stufenlos*.
4. **OSC-Ziel** wie in [grandMA3 einrichten](grandma3.md) (OSC an die MA3).

### Auto per Taste an der MA3

1. In der MA3 unter *Menu → In & Out → OSC* eine zweite Zeile anlegen: IP des
   DeckLight-Rechners, Port **8100**, *Send* an.
2. Ein Makro mit `SendOSC 2 "/decklight/auto,i,1"` (Auto an) und eins mit
   `SendOSC 2 "/decklight/auto,i,0"` (Auto aus) anlegen, auf Tasten legen. Die
   `2` ist die Nummer der OSC-Zeile. `SendOSC 2 "/decklight/auto"` ohne Wert
   schaltet um.
3. Die Windows-Firewall muss UDP 8100 erlauben (die DeckLight-Firewall-Regel
   unter *Utility* deckt das ab).

Auto lässt sich außerdem mit der **AUTO**-Taste oben in DeckLight und auf dem
Handy (`/monitor`, Bedienung aus dem Netz erlauben) schalten.

## Probe ohne DJ

`decklight simulate` spielt Tracks mit Breakdown, Build-up, Pause und Drop. Unter
*Auto* den Schattenmodus einschalten, Auto an, und im Protokoll unter *Utility*
mitlesen.
