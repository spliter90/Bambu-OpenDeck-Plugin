# Changelog

Alle wesentlichen Änderungen an diesem Projekt werden hier dokumentiert.

## 0.2.0 - 2026-10-07

### Hinzugefügt

- dynamische Statusfarben auf der OpenDeck-Taste
  - Grün: bereit / IDLE
  - Gelb: druckt, vorbereitet, slicet oder pausiert
  - Blau: Druck fertig
  - Rot: Fehler
  - Dunkelgrau: offline / unbekannt
- kurzer Tastendruck pausiert einen laufenden Druck
- mindestens 2 Sekunden halten und loslassen setzt einen pausierten Druck fort
- Statusprüfung vor Pause-/Resume-Kommandos
- visuelle OpenDeck-Bestätigung bei gültigem Befehl
- erweiterte Tests für Statusfarben und Pause-/Resume-Zustände
- ausführliche deutschsprachige Dokumentation

### Geändert

- Plugin-Beschreibung und Tooltip erweitert
- Projektversion auf 0.2.0 angehoben
- Dokumentation an die nun vorhandene Drucksteuerung angepasst

## 0.1.0 - 2026-10-07

### Hinzugefügt

- erste OpenAction-Version
- lokale Bambu-MQTT-Verbindung über TLS/8883
- Anzeige von Status, Fortschritt und Restzeit
- optionale Endzeitanzeige
- automatische Wiederverbindung
- periodische `pushall`-Abfrage
- gemeinsame MQTT-Verbindung für mehrere Action-Instanzen
- Builds für Linux, Windows und macOS
