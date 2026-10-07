#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-x86_64-unknown-linux-gnu}"
PLUGIN_ID="de.spliter90.bambu.sdPlugin"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$ROOT/scripts/build-linux.sh" "$TARGET"

if [[ -d "$HOME/.var/app/me.amankhanna.opendeck/config/opendeck" ]]; then
  PLUGIN_DIR="$HOME/.var/app/me.amankhanna.opendeck/config/opendeck/plugins"
else
  PLUGIN_DIR="$HOME/.config/opendeck/plugins"
fi

mkdir -p "$PLUGIN_DIR"
rm -rf "$PLUGIN_DIR/$PLUGIN_ID"
cp -R "$ROOT/dist/$PLUGIN_ID" "$PLUGIN_DIR/$PLUGIN_ID"

echo "Installed to: $PLUGIN_DIR/$PLUGIN_ID"
echo "Restart OpenDeck, then add: Bambu Lab -> Druckerstatus"
