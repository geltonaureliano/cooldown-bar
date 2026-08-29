# Architecture

Cooldown Bar combines a React interface with a Rust and AppKit host through Tauri.

## Frontend

The React layer renders the rail, provider detail bubble, context menu, and liquid drag state. Motion uses transforms on a small surface and respects the macOS Reduce Motion preference.

The frontend receives versioned snapshots from Tauri. One second presentation updates advance countdowns and freshness without pretending that a new provider measurement occurred.

## Rust host

The Rust layer owns configuration, screen geometry, the native panel, pointer tracking, provider workers, process limits, and lifecycle observers.

AppKit integration creates an accessory panel without a Dock icon. Screen changes, sleep, wake, and shutdown events are handled without blocking the main UI loop.

## Provider workers

Each provider has its own worker and retry state. Manual refresh requests are coalesced. Results belong to a configuration generation so a late response from an old setup cannot overwrite current state.

Child processes run with time and output bounds. Custom provider descendants are terminated as one process group when the deadline is reached.

## Persistence

Configuration position changes use a temporary file, file synchronization, and atomic rename. The application holds an instance lock for the current data directory and also locks the legacy directory when it exists, which avoids duplicate panels during an upgrade.

The repository ignores build output, dependency directories, local development state, generated Tauri schemas, signing material, and operating system metadata.
