# Streaming Data Loader

This repository is now organized as a Tauri-first app with a browser-first local
development loop.

## Active layout

- `src/`: Rust source for the Tauri shell
- `frontend/`: plain TypeScript, HTML, and Tailwind-authored CSS
- `sidecar/`: active FastAPI sidecar for SDL v2
- `.vscode/`: committed VS Code tasks, launch configs, and workspace settings
- `legacy-reference/`: archived SDL implementation kept as a reference only

## Local development

- `Run Task -> SDL: Dev` starts the sidecar, Tailwind watcher, and Vite preview
- `Run and Debug -> SDL: Dev` starts Tailwind + Vite, then launches the FastAPI
  sidecar under the Python debugger
- `npm run tauri dev` now starts the desktop window plus the Tailwind watcher,
  Vite dev server, and, in dev mode, auto-starts the root sidecar if the
  configured sidecar port is not already in use

The browser opens automatically from the Vite dev server, and the root frontend
uses `/api` plus Vite proxying instead of hardcoding the sidecar port.
