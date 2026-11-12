#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
node scripts/fetch-mac-intel-media.mjs
python3 scripts/bundle-media.py --source-dir artifacts/mac-intel-media --output src-tauri/resources/media-intel --architecture x86_64 --skip-execution
npm run tauri build -- --target x86_64-apple-darwin --config src-tauri/tauri.intel.conf.json --bundles app,dmg
