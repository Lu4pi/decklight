# DeckLight einrichten: in 5 Schritten

Du brauchst: einen Rechner (macOS 11+ oder Windows 10+), ein Netzwerkkabel zum
DJ-Netz, Pioneer/AlphaTheta-Player mit PRO DJ LINK (CDJ-3000, CDJ-2000NXS2,
XDJ-XZ …) und optional einen DJM-Mixer.

## 1. Installieren

Installer von der [Release-Seite](https://github.com/Lu4pi/decklight/releases)
laden: `.dmg` für macOS, `.msi` oder `-setup.exe` für Windows. Beim ersten Start
fragt das System, ob DeckLight im Netz kommunizieren darf: **zulassen** (unter
Windows für private *und* öffentliche Netze, sonst kommen keine Pakete der Player an).

## 2. Ans DJ-Netz anschließen

Rechner per Kabel in denselben Switch wie Player und Mixer stecken (WLAN geht,
ist aber unzuverlässig). Die Player bekommen ihre Adressen meist automatisch
(169.254.x.x); DeckLight findet sie in jedem Netz. In **Ausgänge → PRO DJ LINK**
die Netzwerkschnittstelle wählen, wenn der Rechner mehrere hat. DeckLight meldet
sich als Player 7 an (nie 1–4, änderbar) und stört die CDJs nicht.

Nach ein paar Sekunden erscheinen die Decks im Reiter **Decks**. Lädt ein Player
einen Track, kommen Titel, Cover, Wellenform und Beatgrid dazu.

## 3. Timecode einstellen

**Ausgänge → Timecode**: „Timecode senden“ an, Bildrate wie am Pult (meist 25 fps),
bei *Art-Net an* die IP des Pults oder die Broadcast-Adresse des Licht-Netzes
eintragen (DeckLight zeigt die Broadcast-Adressen deiner Netze an). Für MTC einen
MIDI-Ausgang wählen. Der Timecode folgt dem Master-Deck (Kopfzeile: *On Air /
Fader*, *Tempo-Master* oder ein festes Deck).

Jeder Track bekommt automatisch eine eigene Stunde (01:00:00:00, 02:00:00:00 …).
Im Reiter **Setlist & Offsets** per Doppelklick ändern, als CSV exportieren oder
importieren.

## 4. Cues fürs Pult anlegen

**Ausgänge → Befehle an die grandMA3**: OSC-Ziel (`IP:Port`) und Präfix eintragen,
speichern, **OSC testen**. Dann im Reiter **Cues & Trigger** einen Track wählen,
in der Wellenform auf die Stelle klicken, **+ Cue hier**, Aktion eintragen (z. B.
`Go+ Exec 201`). Rekordbox-Hot-Cues erscheinen als Vorlagen. **Globale Trigger**
lösen bei Deck-Ereignissen aus (Track geladen, Play, Stop, On Air …).

Not-Aus: **Esc** oder der rote Knopf oben rechts sperrt alle Trigger, der Timecode
läuft weiter. Wieder scharf: **Strg/Cmd + Umschalt + A**.

## 5. Show speichern

Oben links auf den Shownamen klicken: **Speichern**, **Öffnen**, **Neu**,
**Exportieren** (Datei `.decklight` für einen anderen Rechner) und **Importieren**.
DeckLight speichert die laufende Show ohnehin alle paar Sekunden und legt alle fünf
Minuten eine Sicherung an (die letzten 30 bleiben).

Für Tablet, Handy oder Stream: `http://<IP des Rechners>:8750/monitor` und
`/overlay` (siehe [Overlay-Parameter](../README.md#overlay)). Unter
**Ausgänge → Monitor & Overlay im Netz** lässt sich eine PIN setzen.

Weiter: [grandMA3 einrichten](grandma3.md) · [Rider-Vorlage](rider.md)

## Wenn etwas nicht geht

| Problem | Lösung |
| --- | --- |
| Keine Decks | Kabel, Schnittstelle in *Ausgänge*, Firewall-Freigabe; der Reiter *Monitoring* zeigt Warnungen |
| Decks da, aber keine Titel | Player braucht einen Moment nach dem Laden; USB-Stick mit rekordbox exportiert? Als Rückfall unter *Ausgänge → PRO DJ LINK* den Stick oder eine Kopie davon eintragen |
| Player-Nummer belegt | DeckLight weicht selbst auf eine freie Nummer aus 5–15 aus und meldet das unter *Monitoring* |
| Pult reagiert nicht auf OSC | *OSC testen*; im Pult Receive + Receive Command an, Port und Präfix gleich |
| Timecode springt | Master-Auswahl prüfen; bei *On Air / Fader* folgt der Timecode dem lautesten Kanal |
| Port belegt | Läuft rekordbox (Link/Performance) auf demselben Rechner? Dann beenden |
