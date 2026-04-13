# Streaming Data Loader

This repository is now organized as a Tauri-first app with a browser-first local
development loop.

## Active layout

- `src/`: Rust source for the Tauri shell
- `frontend/`: plain TypeScript, HTML, and Tailwind-authored CSS
- `.vscode/`: committed VS Code tasks, launch configs, and workspace settings
- `legacy-reference/`: archived SDL implementation kept as a reference only

## Local development

- `Run Task -> SDL: Dev` starts the Tailwind watcher and Vite preview
- `npm run tauri dev` starts the desktop window plus the Tailwind watcher and
  Vite dev server

The desktop frontend talks directly to native Tauri commands. The browser-only
frontend test harness still uses the shared API layer with a non-Tauri fetch
fallback.
