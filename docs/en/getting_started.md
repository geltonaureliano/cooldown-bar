# Getting started

## Requirements

Cooldown Bar requires macOS 13 or newer. The application bundle is universal and supports Apple Silicon and Intel Macs.

Claude Code and Codex readings require the relevant command line tool or desktop application to be installed and authenticated. A provider that is not available stays out of the main display.

## Install a release

1. Open the [GitHub Releases](https://github.com/geltonaureliano/cooldown-bar/releases) page.

2. Download `Cooldown_Bar_<version>_universal.dmg`.

3. Open the image and move Cooldown Bar to Applications.

4. Start Cooldown Bar from Applications.

An ad hoc signed build can trigger Gatekeeper. Open macOS System Settings, then Privacy and Security, only when you trust the downloaded release and its repository.

## Build from source

Install Node.js 24, Rust 1.92, Xcode Command Line Tools, and the Apple targets used by the universal build.

```bash
npm ci
npm test
npm run build
npm run tauri dev
```

To build and verify local distribution artifacts:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
SIGNING_ENABLED=false node scripts/ci/macos.mjs build /tmp/cooldown-bar-distribution
```

## Move the bar

Press and drag the panel. After the movement threshold, it contracts into a liquid orb. Release near an edge to attach it. Release away from both edges to keep it floating.

Usage collection pauses while detached. The providers continue to account for usage normally. Only Cooldown Bar polling is paused.

## Use the context menu

Right click the panel to refresh usage, reload configuration, attach to the nearest edge, or quit.
