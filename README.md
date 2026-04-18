# Streaming Data Loader

The Streaming Data Loader (SDL) is a desktop app for getting time-series data
from CSV files into HydroServer.

It is built for workflows where a logger, instrument, or upstream process keeps
writing rows to a local CSV file. SDL lets a user connect
to a HydroServer workspace, inspect the CSV structure, configure how timestamps
should be interpreted, and map value columns to HydroServer datastreams. Once a
data source is enabled, SDL watches the file for changes and pushes new
observations into the selected datastreams.

The desktop UI and the watcher/uploader runtime are now split. The UI talks to
the headless daemon over localhost HTTP, and the daemon is the only process
that reads and writes persisted config/workspace/log state.

## What SDL does

- Connects to a HydroServer instance with either an API key or username and
  password.
- Loads datastreams from the selected workspace so CSV columns can be mapped to
  the right targets.
- Previews CSV files before setup so the user can confirm headers, delimiter,
  and where data begins.
- Parses timestamps from ISO 8601 or custom formats, with timezone
  handling when needed.
- Tracks upload progress so only new rows are sent after the initial load.
- Batches uploads, retries transient failures, and records recent job logs and
  status.
- Supports filesystem-triggered updates with a lightweight, always on operating system task so you can set up the job orchestration once and not worry about it again.

## Daemon layout

- `streaming-data-loader` is the Tauri desktop UI.
- `streaming-data-loader-daemon` is the headless Rust service.
- On macOS, both default to `/Users/Shared/Streaming Data Loader` for shared
  config, workspace, and log files unless `SDL_CONFIG_DIR` is set.
- The daemon publishes its active localhost API endpoint and bearer token to
  `<config_dir>/daemon.endpoint.json`.

## Daemon API

The runtime boundary is intentionally small:

- Commands go in with `POST /api/commands/...`.
- Status comes out through `GET /api/status` as Server-Sent Events.
- The Tauri app only uses direct commands for OS-specific concerns like service
  install/restart/uninstall and local file-manager actions.

Key command routes:

- `POST /api/commands/bootstrap`
- `POST /api/commands/update-server-config`
- `POST /api/commands/create-job`
- `POST /api/commands/update-job`
- `POST /api/commands/delete-job`
- `POST /api/commands/run-job-now`

The status stream sends full app snapshots containing health, config, and job
runtime summaries. The frontend treats the daemon as its backend and no longer
mutates shared state files directly.

## macOS launchd

The repository includes a launchd template at
`deploy/macos/com.hydroserver.sdl.plist`.

Example install flow:

```sh
sudo cp deploy/macos/com.hydroserver.sdl.plist /Library/LaunchDaemons/com.hydroserver.sdl.plist
sudo launchctl bootstrap system /Library/LaunchDaemons/com.hydroserver.sdl.plist
```

## Typical workflow

1. Connect SDL to a HydroServer workspace.
2. Choose a CSV file that is being updated over time.
3. Review the preview and configure file parsing and timestamp rules.
4. Map CSV value columns to HydroServer datastreams.
5. Enable the data source and let SDL keep the datastreams current as new rows appear.

## Local development

- `npm run dev` runs the frontend with Vite.
- `npm run tauri dev` runs the desktop app with the frontend dev server.
- `npm run tauri dev` also sets `SDL_CONFIG_DIR=.sdl-dev-data` so the desktop
  app and daemon use a repo-local config directory during development instead of
  the shared system directory.
- `cargo run --bin streaming-data-loader-daemon` runs the headless daemon.
- `npm test` runs the frontend test suite.
