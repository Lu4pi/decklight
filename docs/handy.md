# DeckLight auf Handy und Tablet

Läuft DeckLight auf dem Rechner, zeigt jedes Handy im selben Netz die Live-Ansicht.
Es braucht keine eigene App: die Desktop-App liefert die Oberfläche selbst aus.

1. Rechner und Handy ins gleiche WLAN bringen. Das CDJ-Netz bleibt am Kabel; DeckLight
   hört für PRO DJ LINK nur auf die unter **Netzwerk** gewählte Schnittstelle.
2. In DeckLight oben rechts **Handy** antippen. Der QR-Code enthält die Adresse
   (`http://<Rechner-IP>:8750/monitor`) und, falls gesetzt, die PIN.
3. Im Handy-Browser **Zum Home-Bildschirm** wählen. DeckLight startet dann im Vollbild
   wie eine App (iPhone: Teilen → Zum Home-Bildschirm; Android: Menü → App installieren
   bzw. Zum Startbildschirm).

Ohne PIN kann jeder im selben Netz zusehen; mit PIN nur, wer den QR-Code hat.
Bedienen (Master, Not-Aus, Cues) geht vom Handy nur, wenn **Vom Handy aus bedienen
erlauben** an ist.

Hinweis: Über `http://` erlauben Browser keine Bildschirmsperre-Sperre. Für lange Shows
die automatische Sperre am Handy abschalten oder das Handy ans Ladekabel hängen.
