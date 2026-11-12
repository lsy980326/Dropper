#!/bin/sh
# macOS/Linux cross build. Requires cargo-xwin, LLVM, lld-link, NSIS, and the Windows Rust target.
set -eu
cd "$(dirname "$0")/.."
python3 scripts/fetch-windows-media.py
python3 scripts/bundle-media.py --windows --source-dir artifacts/windows-media --output src-tauri/resources/media-windows
npm run build
cargo xwin build --manifest-path src-tauri/Cargo.toml --release --locked --target x86_64-pc-windows-msvc --target-dir src-tauri/target-windows --xwin-sdk-version 10.0.22621
python3 scripts/fetch-webview-runtime.py
makensis scripts/windows-installer.nsi
