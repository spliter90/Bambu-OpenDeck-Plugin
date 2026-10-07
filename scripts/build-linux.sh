#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-x86_64-unknown-linux-gnu}"
PLUGIN_ID="de.spliter90.bambu.sdPlugin"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$ROOT"

cargo build --release --target "$TARGET"

rm -rf "dist/$PLUGIN_ID"
mkdir -p "dist/$PLUGIN_ID"
cp -R assets/* "dist/$PLUGIN_ID/"
cp "target/$TARGET/release/opendeck-bambu" "dist/$PLUGIN_ID/opendeck-bambu-$TARGET"
chmod +x "dist/$PLUGIN_ID/opendeck-bambu-$TARGET"

echo "Built: $ROOT/dist/$PLUGIN_ID"
