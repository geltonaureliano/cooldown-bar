<div align="center">

# Cooldown Bar

A quiet macOS rate limit monitor for Claude Code, Codex, and custom command providers.

[English documentation](docs/en/README.md) | [Documentação em português](docs/pt_BR/README.md)

[![CI](https://github.com/geltonaureliano/cooldown-bar/actions/workflows/ci-release.yml/badge.svg)](https://github.com/geltonaureliano/cooldown-bar/actions/workflows/ci-release.yml)
[![Release](https://img.shields.io/github/v/release/geltonaureliano/cooldown-bar?display_name=tag)](https://github.com/geltonaureliano/cooldown-bar/releases)
[![Platform](https://img.shields.io/badge/platform-macOS%2013%2B-black)](https://github.com/geltonaureliano/cooldown-bar)

</div>

Cooldown Bar keeps rate limit information visible without opening a dashboard. It lives on either side of the display, updates each provider independently, and becomes a liquid orb while you move it.

## Highlights

1. Native macOS panel with a compact edge attached layout.

2. Claude Code, Codex, and JSON command providers.

3. Freshness, reset time, source, and verification details for every reading.

4. Fluid drag motion with magnetic edge attachment and Reduce Motion support.

5. Bounded provider processes, stale data protection, atomic configuration writes, and a single instance lock.

6. Universal release artifacts for Apple Silicon and Intel Macs.

## Requirements

Cooldown Bar supports macOS 13 or newer. Claude Code and Codex readings require the corresponding tools or desktop applications to be installed and signed in.

Provider information can be delayed by the provider, its network, or its local cache. Cooldown Bar is a status display, not an official billing meter.

## Installation

Download the latest DMG from [GitHub Releases](https://github.com/geltonaureliano/cooldown-bar/releases), open it, and move Cooldown Bar to Applications.

Unsigned development releases use ad hoc signing and may require approval in macOS Privacy and Security. Production releases can use Developer ID signing and Apple notarization when the repository secrets are configured.

## Interaction

Drag the bar away from the edge to turn it into a liquid orb. Move close to either side of the display and release to attach it. Release in the middle to leave it floating. Provider collection pauses while the orb is detached and resumes after attachment.

Right click the bar to refresh usage, reload configuration, attach the orb to the nearest edge, or quit.

## Configuration

Cooldown Bar reads `~/.cooldown-bar/config.json`. Existing installations that still have `~/.notchusage/config.json` continue to use that file until the user moves the configuration.

Every property is optional. See the complete [configuration reference](docs/en/configuration.md) or [referência de configuração](docs/pt_BR/configuration.md).

## Privacy

Cooldown Bar has no telemetry and no separate analytics service. It invokes provider tools already installed on the Mac and reads their local responses. A custom provider can perform any action allowed by the command configured by the user.

See [providers and data sources](docs/en/providers.md) for the trust and freshness model.

## Development

```bash
npm ci
npm test
npm run build
npm run tauri dev
```

Rust checks used by CI:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

The project uses React, TypeScript, Tauri, Rust, and native AppKit integration. Read the [architecture overview](docs/en/architecture.md) before changing panel behavior, process management, or polling.

## Documentation

1. [English documentation](docs/en/README.md)

2. [Documentação em português](docs/pt_BR/README.md)

3. [Contributing](CONTRIBUTING.md)

4. [Security policy](SECURITY.md)

5. [Release process](docs/en/releases.md)

## Project status

Version 0.0.1 is the first public release line. The interface is macOS specific because it depends on AppKit panel behavior and screen safe area geometry.

The repository currently has no open source license. Public visibility does not grant permission to copy, modify, or redistribute the code until a license is added by the copyright holder.

Claude, Claude Code, Codex, ChatGPT, macOS, and Apple are trademarks of their respective owners. Cooldown Bar is an independent project and is not endorsed by those companies.
