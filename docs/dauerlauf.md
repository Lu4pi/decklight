# Dauerlauf 4 h

Lauf vom 12./13.10.2026, Release-Build unter Linux, `decklight soak --minutes 240 --sample-secs 10`: Simulator-Demo → Kern → Art-Net-Timecode, OSC-Trigger und WebSocket-Client im selben Prozess.

```
 14380 s  CPU   8.5 %  RAM   13.0 MB  WS 40.1/s  TC 24.6/s  Trigger 322
 14390 s  CPU   7.7 %  RAM   13.0 MB  WS 40.0/s  TC 24.5/s  Trigger 322
 14400 s  CPU   8.1 %  RAM   13.0 MB  WS 40.0/s  TC 24.5/s  Trigger 322
✓ 1440 Messungen
✓ RAM 12.2 → 13.0 MB (max 13.0 MB)
  CPU Mittel 8.8 %, Spitze 20.5 % (100 % = ein Kern)
✓ Oberfläche mindestens 39.8 Zustände/s
✓ Timecode lief in 1440 von 1440 Messfenstern
✓ 322 Trigger, 322 OSC-Nachrichten, 0 stille Fenster
✓ 0 Trigger-Fehler
✓ größte Trigger-Verspätung 3.1 ms
BESTANDEN
```

Messwerte alle 10 s (Auszug: die ersten und letzten drei Messungen):

```
t_s;cpu_pct;rss_mb;ws_fps;osc_msgs;tc_pps;triggers
10;9.5;12.0;40.1;3;23.6;3
20;9.3;12.0;40.0;1;25.0;1
30;9.4;12.0;39.9;0;25.4;0
14380;8.5;13.0;40.1;0;24.6;0
14390;7.7;13.0;40.0;0;24.5;0
14400;8.1;13.0;40.0;0;24.5;0
```
