# Bambu OpenDeck Plugin

Ein natives **OpenAction-Plugin für OpenDeck**, das einen Bambu-Lab-3D-Drucker direkt im lokalen Netzwerk überwacht und über eine einzelne Taste bedient.

Das Plugin zeigt den aktuellen Druckerstatus, Fortschritt und die verbleibende Druckzeit beziehungsweise die voraussichtliche Endzeit an. Während eines laufenden Drucks kann derselbe OpenDeck-Knopf den Druck pausieren und wieder fortsetzen.

**Aktuelle Version: 0.2.0**

## Funktionen

- Live-Status des Bambu-Lab-Druckers über lokales MQTT
- Fortschritt in Prozent
- verbleibende Druckzeit
- alternativ berechnete Endzeit
- Layer-Anzeige als Fallback
- farbiger Tastenhintergrund je Druckerstatus
- Pause per kurzem Tastendruck
- Fortsetzen durch mindestens 2 Sekunden langes Halten
- automatische Wiederverbindung bei Verbindungsabbruch
- periodischer `pushall`-Statusabgleich
- gemeinsame MQTT-Verbindung für mehrere identische OpenDeck-Tasten
- keine Bambu-Cloud erforderlich
- OpenAction-kompatibel
- Builds für Linux, Windows und macOS

## Anzeige auf der Taste

Während eines Drucks kann die Taste beispielsweise so aussehen:

```text
DRUCKT
67 %
1h 24m
```

Mit aktivierter Endzeitanzeige:

```text
DRUCKT
67 %
Fertig 17:42
```

### Statusfarben

| Farbe | Bedeutung |
| --- | --- |
| 🟢 Grün | Drucker bereit / `IDLE` |
| 🟡 Gelb | Druckt, bereitet vor, slicet oder ist pausiert |
| 🔵 Blau | Druck abgeschlossen / `FINISH` |
| 🔴 Rot | Fehlerzustand oder gemeldeter Druckerfehler |
| ⚫ Dunkelgrau | Offline, unbekannter Status oder noch nicht verbunden |

Die Farbe wird zusammen mit dem Statustext und den aktuellen Druckdaten automatisch aktualisiert.

## Tastensteuerung

Die Status-Taste ist gleichzeitig die Drucksteuerung:

| Bedienung | Aktion | Voraussetzung |
| --- | --- | --- |
| Kurz drücken und loslassen | Druck pausieren | Drucker meldet `RUNNING` |
| Mindestens 2 Sekunden halten und loslassen | Druck fortsetzen | Drucker meldet `PAUSE` oder `PAUSED` |

Das Plugin prüft den aktuellen Druckerstatus, bevor ein Steuerbefehl gesendet wird. Ein Pause-Befehl wird beispielsweise nicht gesendet, wenn der Drucker bereits pausiert oder bereit ist.

Bei einem gültigen Befehl zeigt OpenDeck eine Bestätigung an. Bei einem nicht zulässigen Befehl erscheint ein Warnhinweis.

## Verbindung zum Bambu-Drucker

Die Verbindung erfolgt ausschließlich im lokalen Netzwerk über MQTT/TLS:

- Port: `8883`
- Benutzername: `bblp`
- Passwort: LAN Access Code des Druckers
- Status-Topic: `device/<SERIAL>/report`
- Befehls-Topic: `device/<SERIAL>/request`

Das Plugin verwendet für Statusabfragen außerdem regelmäßig:

```json
{"pushing":{"sequence_id":"0","command":"pushall","version":1,"push_target":1}}
```

Für Pause und Fortsetzen werden die lokalen Bambu-Print-Kommandos verwendet.

## Voraussetzungen

Du benötigst:

1. OpenDeck mit OpenAction-Unterstützung
2. einen Bambu-Lab-Drucker, der über das lokale Netzwerk erreichbar ist
3. IP-Adresse oder Hostname des Druckers
4. Seriennummer des Druckers
5. LAN Access Code

Der Rechner mit OpenDeck und der Drucker müssen sich gegenseitig im Netzwerk erreichen können.

## Einrichtung in OpenDeck

Nach der Installation findest du die Action unter:

**Bambu Lab → Druckerstatus**

In den Einstellungen der Taste werden folgende Werte eingetragen:

### IP-Adresse / Hostname

Beispiel:

```text
192.168.178.50
```

### Seriennummer

Die Seriennummer wird für die MQTT-Topics des Druckers benötigt.

### LAN Access Code

Der lokale Zugangscode des Bambu-Druckers. Im Property Inspector wird er als Passwortfeld dargestellt.

Der Code wird nicht in das Plugin-Log geschrieben. OpenDeck speichert Action-Einstellungen jedoch lokal; die lokale OpenDeck-Konfiguration sollte deshalb entsprechend geschützt werden.

### Zeitanzeige

Zur Auswahl stehen:

- **Restzeit** – z. B. `1h 24m`
- **Endzeit** – z. B. `Fertig 17:42`

### Status-Snapshot

Standardmäßig fordert das Plugin alle **30 Sekunden** einen vollständigen Status über `pushall` an. MQTT-Liveupdates werden unabhängig davon sofort verarbeitet.

Der Wert kann zwischen 10 und 300 Sekunden eingestellt werden.

## Installation über GitHub Actions

Jeder erfolgreiche CI-Lauf erzeugt fertige ZIP-Artefakte für:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-pc-windows-msvc`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

Für einen normalen 64-Bit-Linux-PC, beispielsweise CachyOS, ist das Artefakt

```text
opendeck-bambu-x86_64-unknown-linux-gnu
```

das passende Paket.

## Unter Linux selbst bauen

Benötigt werden Rust stable sowie die OpenSSL-Entwicklungsdateien.

### Arch Linux / CachyOS

```bash
sudo pacman -S --needed rust openssl pkgconf
```

### Debian / Ubuntu

```bash
sudo apt install cargo rustc libssl-dev pkg-config
```

Danach:

```bash
./scripts/build-linux.sh
```

Das fertige Plugin-Bundle befindet sich anschließend unter:

```text
dist/de.spliter90.bambu.sdPlugin
```

### Direkt installieren

```bash
./scripts/install-linux.sh
```

Danach OpenDeck neu starten und **Bambu Lab → Druckerstatus** auf eine Taste legen.

## Technischer Aufbau

Das Plugin ist in Rust implementiert und verwendet die native OpenAction-API.

Pro eindeutigem Drucker-Setup wird nur eine MQTT-Verbindung aufgebaut. Mehrere sichtbare OpenDeck-Tasten mit identischen Verbindungsdaten abonnieren denselben internen Status-Stream.

Bambu-Statusmeldungen können nur Teilinformationen enthalten. Das Plugin führt diese Delta-Updates deshalb in einem lokalen Snapshot zusammen und fordert zusätzlich regelmäßig einen vollständigen `pushall`-Status an.

Die Tastenfarbe wird als dynamisches SVG erzeugt und über OpenAction als Bild auf die Taste übertragen.

## Sicherheit

Bambu-Drucker verwenden für die lokale MQTT-Verbindung ein lokales beziehungsweise selbstsigniertes Zertifikat. Das Plugin akzeptiert dieses Zertifikat für die konfigurierte LAN-Verbindung.

Daher gilt:

- nur eine vertrauenswürdige lokale Drucker-IP beziehungsweise einen vertrauenswürdigen Hostnamen konfigurieren
- den MQTT-Port des Druckers nicht unnötig aus dem Internet erreichbar machen
- den LAN Access Code wie ein Passwort behandeln

Das Plugin benötigt keine Bambu-Cloud-Zugangsdaten.

## Fehlerbehebung

### Taste zeigt OFFLINE

Prüfe:

- Drucker eingeschaltet?
- IP-Adresse korrekt?
- Rechner und Drucker im selben erreichbaren Netzwerk?
- LAN Access Code korrekt?
- Seriennummer korrekt?
- TCP-Port 8883 erreichbar?

### Taste zeigt SETUP FEHLT

Mindestens IP/Hostname, Seriennummer oder LAN Access Code fehlt in den Action-Einstellungen.

### Pause funktioniert nicht

Ein kurzer Tastendruck sendet Pause nur, wenn der zuletzt bekannte Druckerstatus `RUNNING` ist.

### Fortsetzen funktioniert nicht

Die Taste mindestens 2 Sekunden gedrückt halten und anschließend loslassen. Fortsetzen wird nur bei `PAUSE` oder `PAUSED` gesendet.

### Farbe passt nicht zum erwarteten Zustand

Die Farblogik orientiert sich am Status, den der Drucker über `gcode_state` meldet. Nach einem abgeschlossenen Druck kann der Drucker später von `FINISH` wieder auf `IDLE` wechseln; die Taste wechselt dann entsprechend von Blau auf Grün.

## Build und CI

GitHub Actions baut und testet das Plugin automatisch bei Änderungen auf `main`.

Build-Matrix:

| Plattform | Target |
| --- | --- |
| Windows | x86_64-pc-windows-msvc |
| macOS Intel | x86_64-apple-darwin |
| macOS Apple Silicon | aarch64-apple-darwin |
| Linux x86_64 | x86_64-unknown-linux-gnu |
| Linux ARM64 | aarch64-unknown-linux-gnu |

Der Linux-x86_64-Job führt zusätzlich die Rust-Tests aus.

## Geplante Erweiterungen

Mögliche nächste Actions beziehungsweise Anzeigen:

- Düsen- und Betttemperatur
- aktueller Layer / Gesamt-Layer
- AMS-Filamentinformationen
- Lüfterstatus
- separate Pause-/Resume-Action
- Druck abbrechen mit Sicherheitsbestätigung
- weitere Bambu-Status- und Fehlerdetails

## Lizenz

Dieses Projekt steht unter der **MIT License**. Siehe [LICENSE](LICENSE).

## Hinweis

Dieses Projekt ist ein unabhängiges Community-Plugin und kein offizielles Produkt von Bambu Lab oder OpenDeck.
