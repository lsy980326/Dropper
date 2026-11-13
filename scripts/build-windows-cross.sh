#!/bin/sh
# macOS/Linux cross build. Requires cargo-xwin, LLVM, lld-link, NSIS, and the Windows Rust target.
set -eu
cd "$(dirname "$0")/.."
python3 scripts/fetch-windows-media.py
python3 scripts/bundle-media.py --windows --source-dir artifacts/windows-media --output src-tauri/resources/media-windows
node node_modules/@tauri-apps/cli/tauri.js build --runner cargo-xwin --target x86_64-pc-windows-msvc --no-bundle -- --locked --target-dir target-windows --xwin-sdk-version 10.0.22621
python3 scripts/fetch-webview-runtime.py
DROPPER_VERSION=$(node -p "JSON.parse(require('fs').readFileSync('package.json','utf8')).version")
makensis -INPUTCHARSET UTF8 "-DVERSION=$DROPPER_VERSION" scripts/windows-installer.nsi
