# Streaming Data Loader

This repository is now organized as a Tauri-first project.

## Layout

- `src/`: root Rust application source for the Tauri app
- `icons/`, `capabilities/`, `tauri.conf.json`, `Cargo.toml`: root Tauri project files
- `legacy-reference/`: archived SDL implementation, including the previous Vue frontend, Python sidecar, packaging files, and workflow definitions

## Transitional State

The current Tauri configuration still points at the archived frontend in `legacy-reference/` for development and build-time web assets while the UI is being migrated.
