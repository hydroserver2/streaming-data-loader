# Streaming Data Loader

The Streaming Data Loader (SDL) is a desktop app for getting time-series data
from CSV files into HydroServer.

It is built for workflows where a logger, instrument, or upstream process keeps
writing rows to a local CSV file. SDL lets a user connect
to a HydroServer workspace, inspect the CSV structure, configure how timestamps
should be interpreted, and map value columns to HydroServer datastreams. Once a
data source is enabled, SDL watches the file for changes and pushes new
observations into the selected datastreams.

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

## Typical workflow

1. Connect SDL to a HydroServer workspace.
2. Choose a CSV file that is being updated over time.
3. Review the preview and configure file parsing and timestamp rules.
4. Map CSV value columns to HydroServer datastreams.
5. Enable the data source and let SDL keep the datastreams current as new rows appear.

## Local development

- `npm run dev` runs the frontend with Vite.
- `npm run tauri dev` runs the desktop app with the frontend dev server.
- `npm test` runs the frontend test suite.
