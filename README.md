# Streaming Data Loader

This repository is now organized as a Tauri-first desktop app with a minimal vanilla TypeScript frontend.

## Layout

- `src/`: Rust source for the Tauri application
- `frontend/`: vanilla TypeScript and CSS for the webview UI
- `index.html`: Vite entry HTML
- `icons/`, `capabilities/`, `tauri.conf.json`, `Cargo.toml`: root Tauri project files
- `legacy-reference/`: archived SDL implementation kept as reference only

## Frontend

The active frontend uses:

- Vite
- TypeScript
- plain HTML
- plain CSS

No Vue runtime or Vue build plugins are used by the active Tauri app.
