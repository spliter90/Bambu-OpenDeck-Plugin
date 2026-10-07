# OpenDeck Bambu Status

Native OpenAction plugin for OpenDeck that displays the live status of a Bambu Lab 3D printer directly on a key.

## Version 0.1.0

The first action is **Druckerstatus**. It displays, depending on printer state:

```text
DRUCKT
67 %
1h 24m
```

or, when `Endzeit` is selected:

```text
DRUCKT
67 %
Fertig 12:18
```

Other states are shown as `BEREIT`, `PAUSE`, `FERTIG`, `FEHLER`, `STARTET` or `OFFLINE`.

### Status colors and key control

- Green: ready / idle
- Yellow: printing, preparing, slicing or paused
- Blue: print finished
- Red: printer error
- Dark gray: offline / unknown

The same key can control the active print:
- Short press (release before 2 seconds): pause, only while the printer is `RUNNING`
- Long press (hold for 2 seconds or more, then release): resume, only while the printer is `PAUSE`/`PAUSED`

Invalid controls are not sent to the printer.

## How it works

The plugin talks directly to the printer on the local network via MQTT over TLS on port 8883.

- MQTT username: `bblp`
- MQTT password: the printer's LAN Access Code
- report topic: `device/<SERIAL>/report`
- request topic: `device/<SERIAL>/request`
- periodic full snapshot: `pushing.pushall`

No Bambu Cloud account or token is used.

Bambu printers use a locally issued/self-signed certificate chain. For LAN mode this plugin accepts the printer certificate without normal hostname/CA verification. Do not point the plugin at an untrusted host outside your LAN.

## OpenDeck settings

After adding **Bambu Lab -> Druckerstatus** to a key, enter:

1. Printer IP address, e.g. `192.168.1.50`
2. Printer serial number
3. LAN Access Code from the printer
4. Time display: remaining time or calculated finish time
5. Optional `pushall` interval (10-300 seconds, default 30)

The access code is masked in the property inspector and is never written to the plugin log. OpenDeck still persists action settings locally, so treat the local OpenDeck configuration as sensitive.

## Linux build

Requirements:

- Rust stable
- OpenSSL development files (`openssl` on Arch/CachyOS; `libssl-dev` + `pkg-config` on Debian/Ubuntu)

Build the plugin bundle:

```bash
./scripts/build-linux.sh
```

Install it into OpenDeck:

```bash
./scripts/install-linux.sh
```

Then restart OpenDeck and select **Bambu Lab -> Druckerstatus**.

For a Linux ARM64 host:

```bash
./scripts/build-linux.sh aarch64-unknown-linux-gnu
./scripts/install-linux.sh aarch64-unknown-linux-gnu
```

## GitHub Actions

`.github/workflows/build.yml` builds bundles for:

- Windows x86_64
- macOS x86_64
- macOS ARM64
- Linux x86_64
- Linux ARM64

The workflow runs on pushes to `main`, manually, or when a `v*` tag is pushed.

## Development notes

Status reports from the printer can be partial deltas. The plugin merges incoming values into a cached snapshot, and periodically sends a `pushall` request to refresh all values. Multiple OpenDeck keys with identical printer connection settings share one MQTT connection.

The plugin currently only reads status data. It does not send print-control commands such as stop, pause, resume, temperature changes or movement commands.
