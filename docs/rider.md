# Rider-Vorlage: DeckLight am DJ-Pult

Zum Kopieren in den technischen Rider. Platzhalter in `<…>` anpassen.

---

## Show-Steuerung DJ → Licht (DeckLight)

Das Licht läuft per Timecode und OSC synchron zu den DJ-Playern. Dafür bringt
die Produktion einen Rechner mit DeckLight mit (`<macOS/Windows-Laptop>`), der
sich ins DJ-Netz einklinkt. Er meldet sich als zusätzlicher Player (Nummer 5–15)
an und beeinflusst Wiedergabe, Sync und Link-Export der Player nicht.

### Vom Veranstalter benötigt

**DJ-Setup**

- `<2–4>` × Pioneer/AlphaTheta CDJ-3000 (alternativ CDJ-2000NXS2 / XDJ-XZ /
  XDJ-1000MK2), alle per PRO DJ LINK verbunden, aktuelle Firmware
- 1 × DJM-900NXS2, DJM-A9 oder DJM-V10 (für die On-Air-Erkennung) im selben Netz
- 1 × Netzwerk-Switch am DJ-Pult (mind. 8 Ports, Gigabit, **ohne** Managed-Funktionen
  wie IGMP-Snooping oder Port-Isolation), Player und Mixer per Kabel
- 1 × freier Switch-Port und 1 × Netzwerkkabel (Cat 5e, ≥ `<5>` m) für den
  DeckLight-Rechner
- 1 × Stromanschluss (230 V Schuko) am DJ-Pult für den Rechner
- Platz für einen Laptop (≈ 35 × 25 cm), nicht im Sichtfeld des DJs

**Licht**

- grandMA3 (light, full-size, compact, onPC mit Command Wing) Software ≥ `<2.0>`
- Netzverbindung DJ-Pult ↔ Licht-Netz: entweder
  - ein Kabel vom Licht-Netz zum DJ-Pult (zweite Netzwerkkarte am Rechner), oder
  - DJ- und Licht-Netz im selben IP-Bereich (nur nach Rücksprache)
- Am Pult freigeschaltet:
  - Art-Net-Eingang für Timecode (Bildrate `<25>` fps), Timecode-Slot `<1>`
  - OSC-Eingang mit *Receive Command*, Port `<8000>`, Präfix `<gma3>`
- IP-Adresse des Pults: `<2.0.0.10>`; für DeckLight wird eine freie Adresse im
  Licht-Netz benötigt: `<2.0.0.50>`

### Ablauf

- Aufbau und Test: `<30>` Minuten mit allen Playern eingeschaltet, vor dem
  Soundcheck.
- Der DJ benutzt seine rekordbox-USB-Sticks wie gewohnt. Hot Cues aus rekordbox
  können als Licht-Cues übernommen werden.
- Bei Problemen sperrt der Operator alle Licht-Befehle mit einem Tastendruck
  (Not-Aus); die Musik ist davon nie betroffen.

### Ansprechpartner

Licht/Show-Steuerung: `<Name>`, `<Telefon>`, `<E-Mail>`

---

*Erstellt mit DeckLight (GPL-3.0), https://github.com/Lu4pi/decklight*
