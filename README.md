# NotchUsage

A vertical bar pinned to the edge of the screen, one progress ring per AI CLI
provider, showing how much of your rate limit is gone. Hover a ring for the
detail bubble.

macOS 13+ only. Tauri 2 + React 18 + TypeScript.

> Layout inspired by a concept posted by **@hivinz_** on X. This is a personal
> build, not a product.

---

## Quick start

```bash
make install && make dev
```

The CI uses Node.js 24 (`.node-version`) and Rust 1.92.0
(`rust-toolchain.toml`). Keep both lockfiles committed.

`make dev` starts a recorded process group for this project. A second launch is
refused; use `make stop` before restarting. It never kills another project to free
a port, and the native app also holds a single-instance lock.
Other targets:

| Target | Does |
|---|---|
| `make dev` | start Vite + Tauri for this project |
| `make stop` | stop only this project’s verified development process group |
| `make ps` | list what is currently running |
| `make build` | release `.app` + `.dmg` in `src-tauri/target/release/bundle` |
| `make test` | Rust, frontend logic, SVG and statusline tests |
| `make check` | `tsc --noEmit` + `cargo clippy -D warnings` |

There is no Dock icon and no app menu, so **right-click the bar → Quit** is the
only way to exit from the UI. `make stop` works from a terminal.

The menu is drawn in the webview, not as an `NSMenu` — see *The context menu is
not native* below.

---

## GitHub Actions and releases

The single workflow `.github/workflows/ci-release.yml` validates pull requests
and pushes, tests the native macOS code, and builds a universal `.dmg` plus a
zipped `.app` for Intel and Apple Silicon. A new app version on the repository's
default branch creates its tag and GitHub Release after all checks pass.
The initial version is **0.0.1** (`v0.0.1`).

Release notes combine an optional editorial summary, GitHub pull requests and
categorized commits. Downloads include SHA-256 checksums and build metadata.
Apple signing/notarization is optional; without credentials the workflow
clearly labels its packages as ad-hoc test builds.

For the next release, run `npm run release:version -- patch`, review and commit
the five synchronized version files, then merge to the default branch.
See the [complete setup, signing and recovery guide](docs/releases.md) before
the first push to that branch, which can publish `v0.0.1` automatically.

---

## What it reads, and from where

NotchUsage detects installed CLIs and local usage files. Each provider has an
independent worker, so a stalled query cannot delay another provider. Claude may
need the statusline hookup described below, depending on its version.

### Claude Code — statusline feed and version-dependent CLI support

The official statusline payload can expose `rate_limits.five_hour` and
`rate_limits.seven_day` with **0–100 percentages** and reset timestamps. The hook
captures only this metadata; the app checks for a new file once per second.
It does not create a fresh server measurement: Claude decides when to update its
payload. After Claude stops publishing, the reading ages and becomes stale.

Some CLI builds expose a stream-json `get_usage` control request, which the app
can try without a prompt. **Claude Code 2.1.92 on the tested machine returned
“Unsupported control request subtype: get_usage”.** The same machine also has
**2.1.224 in the login shell's nvm PATH**, selected by the native app; that version
returned a direct usage reading. No statusline settings needed to change. An
unsupported control request is surfaced rather than interpreted as zero usage. An unsupported binary is not launched again on
every poll; it is retried after its binary changes or the configuration reloads.
Use the statusline hook with a Claude version that publishes rate-limit fields.

The app never reads OAuth tokens, Keychain credentials, or calls private usage
HTTP endpoints. It does not modify your Claude settings automatically.

See [Claude statusline documentation](https://code.claude.com/docs/en/statusline).

### Codex — live, via the app server

```bash
codex app-server --stdio
```
```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{…}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read","params":null}
```

The process stays open between reads. The default reconciliation interval is
10 seconds, with `account/rateLimits/updated` notifications consumed between
reads. An event from a separate CLI session is not guaranteed to reach this
connection: periodic reads remain necessary. Network or provider caching can
add latency; this is not a guaranteed real-time billing meter.

See [Codex app-server documentation](https://learn.chatgpt.com/docs/app-server).

Two protocol requirements:

* **stdin must stay open.** Close it and the server exits in ~150ms, before it
  answers the rate-limit read.
* **Responses arrive out of order**, interleaved with notifications. Match on the
  JSON-RPC `id`, never on position.

There may be no `codex` on your `PATH` at all — with only the desktop app
installed, the binary lives at
`/Applications/ChatGPT.app/Contents/Resources/codex`. NotchUsage looks there.

### Fallbacks

If a CLI cannot be reached, NotchUsage falls back to files the provider left
behind, and the bubble says so:

| Provider | Fallback |
|---|---|
| Claude | `~/.notchusage/claude.json`, written by the optional statusline hook |
| Codex | `$CODEX_HOME/sessions/**/rollout-*.jsonl` (default `~/.codex`), newest 8 files, last 512 KB each |

The bubble shows the source and age. **Age comes from the measurement**, not
from the latest scan. For Codex logs this is the usage event's timestamp, even
when later chat messages have changed the file's modification time. Logs cannot
prove which account is currently signed in, so their readings are always marked
unverified and excluded from the main numeric display.

At `staleAfterSeconds` (default 120), an unknown/future timestamp, or a passed
primary reset deadline, the ring dims and displays `—`. The last valid number
stays in the bubble with a warning. No reset is guessed and failures never turn
into a confident zero. Freshness and countdowns advance on a one-second UI clock,
including when the backend is disconnected. A recent cached reading can survive
a temporary error until its original expiry; its timestamp is never renewed by
that error.

### Finding the CLIs at all

A GUI app launched from Finder does **not** inherit your shell `PATH` — it gets a
bare `/usr/bin:/bin:/usr/sbin:/sbin`, where neither CLI lives. NotchUsage runs
your login shell (with a 4s timeout, so a `.zshrc` that blocks on an SSH
passphrase cannot hang startup) to recover the real `PATH`, then adds the usual
install prefixes and app bundles.

Lookups cache **hits only**. Caching a miss would mean a CLI you install later
stays invisible until you restart the app.

---

## Moving the bar

Press and drag it. After 4px of movement the rail contracts around the grab point
and becomes a small liquid orb. Moving within 64pt of an edge turns on magnetic
attraction; release there and the orb springs into the mirrored rail. Release in
the middle and it remains reachable as a floating orb. Right-click it and choose
**Attach to nearest edge** to recover without another drag.

While the orb is detached, provider queries pause and the last trustworthy
reading keeps its original timestamp. Any result already in flight is discarded,
so it cannot refresh the display during motion. Queries resume only after the
attachment animation completes. This pauses NotchUsage collection; it does not
change consumption at the provider.

The liquid moves using transforms on the small orb surface. Its level follows
the last reading without drawing a percentage, provider mark, caption, or
progress ring. **Reduce Motion** in macOS removes the continuous liquid movement
and shortens attachment. A press that moves less than 4px remains a click.

Only the final attached edge and vertical position are written back to
`~/.notchusage/config.json`, on a dedicated ordered writer, so disk sync never
runs on AppKit's motion loop.

---

## Configuration

`~/.notchusage/config.json`. Every key is optional; missing keys fall back to
defaults. A malformed reload retains the last working configuration and reports
the error in the context menu. Dragging refuses to overwrite malformed JSON.

```json
{
  "edge": "right",
  "barWidth": 62, "concaveRadius": 31, "topOffset": null, "edgeInset": 0,
  "ringDiameter": 38, "ringLineWidth": 3, "itemGap": 28,
  "refreshSeconds": 10, "staleAfterSeconds": 120,
  "showClaude": true, "showCodex": true,
  "customCommand": null, "customTitle": "Custom",
  "claudeColor": "#FF5F2E", "codexColor": "#00E07A", "customColor": "#E8E80A"
}
```

Right-click → **Reload config** re-reads the file and re-lays out without a
restart, including colors and icon cache versions. **Refresh usage** requests a
coalesced read without restarting workers.

### `topOffset`

`null` (the default) means *just below the menu bar*, resolved at runtime from
live screen geometry so it adapts to notched and un-notched displays.

An explicit `0` really does pin the bar to the top of the screen — but **the menu
bar will draw over it**. macOS composites the menu bar above every ordinary
window level; this was verified empirically all the way up to
`NSScreenSaverWindowLevel` (1000), where the clock still drew straight through
the bar. That is why the default is not 0.

### `refreshSeconds`

Defaults to **10 seconds**, clamped to 5–3600. Failures back off to 20, 40, 80,
then 120 seconds at the default interval. Success resets the delay. A larger
configured interval remains the minimum. Manual refreshes are coalesced and
rate-limited to one per two seconds per provider. Sleep pauses collection; wake
signals background workers without blocking AppKit. Reload discards results
from the prior configuration generation.

An explicit older `refreshSeconds` setting still takes precedence over the new
default. `staleAfterSeconds` is independent; choose a value longer than your
normal refresh interval if you raise it.

### Custom provider

Set `customCommand` to any shell command that prints JSON on stdout:

```json
{ "percent": 52, "resets_at": 1756400000, "label": "Session",
  "secondary_percent": 11 }
```

`percent` is required; everything else is optional. Also accepts
`secondary_label` and `secondary_resets_at`. Must exit successfully. The entire
owned process group is terminated after 3s, even when a descendant holds a pipe
open. Subprocess output is bounded to 4 MiB per stream.

Provider CLIs can contact their services to retrieve limits. A custom command
can also use the network. NotchUsage adds no separate HTTP client or telemetry.

### Icons

No logo is bundled with NotchUsage or downloaded. Two sources instead:

* **The vendor's own icon, if its app is installed.** On first run the Codex mark
  is copied out of `ChatGPT.app` into `~/.notchusage/icons/codex.vendor.png`. The
  file was already on the machine — the vendor's installer put it there — and
  copying it into the config directory keeps the asset-protocol scope pinned to
  `~/.notchusage/icons/**`. App icons are a dark glyph on a light rounded square,
  so they are inverted at render time to the flat light-on-dark mark.
* **The Claude mark is drawn** from primitives: twelve spokes, alternating full
  and 72% length.

To use your own, drop a PNG at `~/.notchusage/icons/<id>.png` where `<id>` is
`claude`, `codex`, or `custom`. A file you supply always wins over a seeded one
and is rendered untouched — no inversion, no recolouring.

---

## Optional: the statusline hook

Needed when your installed Claude does not support the usage control request.
The Claude version must also publish `rate_limits` in its statusline payload;
the hook cannot manufacture a missing field.

`scripts/claude-statusline.sh` writes only whitelisted usage metadata atomically to
`~/.notchusage/claude.json` and prints a compact line (model, context %, 5h %,
7d %). Files are private (0600), JSON is validated before replacement, input is
bounded to 1 MiB, and zero percentages are preserved. It needs `/bin/sh` and
`python3` — no `jq`. Invalid payloads preserve the last good snapshot.

```json
{
  "statusLine": {
    "type": "command",
    "command": "/absolute/path/to/notchusage/scripts/claude-statusline.sh"
  }
}
```

in `~/.claude/settings.json`.

**If you already have a statusline**, do not replace it — Claude Code allows only
one, and overwriting it silently breaks whatever was there. Chain instead, with a
wrapper that tees stdin to both:

```sh
#!/bin/sh
payload=$(cat)
printf '%s' "$payload" | /path/to/notchusage/scripts/claude-statusline.sh --capture-only >/dev/null 2>&1
printf '%s' "$payload" | /path/to/your/existing-statusline.sh
```

Point `statusLine.command` at the wrapper.

---

## How the window works

The parts that are easy to get wrong, and why they are the way they are.

**Non-activating panel.** The window is converted to an `NSPanel` with
`NSWindowStyleMask::NonactivatingPanel`, and the subclass hard-codes
`canBecomeKeyWindow` and `canBecomeMainWindow` to `false`. These are two
independent AppKit mechanisms: the style mask stops the *app* activating, the
overrides stop the *window* taking key status. The style mask alone is not
enough — the webview asks for first responder and steals focus from your
terminal.

**Covering the menu bar area.** `setFrame:display:` runs its argument through
`constrainFrameRect:toScreen:`, which clamps a window's top edge to
`visibleFrame`. NotchUsage overrides that method on its own panel subclass at
runtime so the requested frame is honoured exactly.

**Which screen.** `NSScreen::mainScreen` is *the screen holding the window with
keyboard focus* — it follows your clicks between displays and has nothing to do
with where the menu bar is. NotchUsage uses `NSScreen::screens[0]`, the primary.

**Click-through.** The window is far wider than the visible bar, because the
bubble opens beside it in the same window. macOS hit-tests per *window*, not per
pixel, so that transparent area would swallow every click meant for the desktop
behind it — CSS `pointer-events: none` does not help, it acts long after AppKit
has assigned the event. Instead a 30 Hz loop polls `NSEvent::mouseLocation` and
toggles `setIgnoresMouseEvents:`, so the window is invisible to the pointer
except over the bar itself. `mouseLocation` and `pressedMouseButtons` are plain
class properties, so **no Accessibility permission is required**.

**Hover** is pushed from Rust rather than left to CSS `:hover`, because toggling
`ignoresMouseEvents` can swallow the `mouseenter` that would start one.

**Coordinates.** AppKit's screen space has a bottom-left origin; everything else
uses top-left. The conversion happens in exactly one place, `screen.rs`.

**The context menu is not native.** `Menu::popup` deadlocks this app.
`-[NSMenu popUpMenuPositioningItem:...]` starts an `NSMenuTrackingSession` event
loop that runs until it sees the events it is waiting for; with
`ActivationPolicy::Accessory` and a panel that refuses key status, those events
never arrive, so the main thread parks in `nextEventMatchingMask:` forever and
the entire UI freezes. This was confirmed with a thread sample, not inferred.
Activating the app first would feed the loop, but stealing focus is the one thing
this app must not do — so the menu is HTML. While it is open the panel stops
being click-through, otherwise its buttons would sit in dead space.

---

## Known limitations

* **Main screen only.** The bar lives on the display carrying the menu bar. There
  is no per-display instance and no follow-the-cursor mode.
* **The menu bar wins.** You cannot draw over it (see `topOffset` above).
* **Claude data freezes when Claude Code is closed** *if* you are on the
  statusline feed. Updates depend on Claude publishing rate-limit data. The
  optional CLI path is version dependent and uses a bounded subprocess.
* **Codex JSONL format drifts.** The fallback parser accepts several field
  spellings and both absolute (`resets_at`) and relative (`resets_in_seconds`)
  reset encodings, but a genuinely new shape will read as "no data" rather than
  guess.
* **No `codex exec` support.** Those runs emit `rate_limits: null`; the scanner
  skips them and keeps walking back.
* **Not sandboxable**, so no App Store. It spawns arbitrary CLIs, reads outside
  its container, and overrides an AppKit method at runtime.
* **Ad-hoc builds** may be blocked by Gatekeeper. Developer ID signing and
  notarization are available in the release workflow; see [the guide](docs/releases.md).

---

## Layout

```
node width = barWidth + 9 gap
window     = barWidth + 2*(9 gap + 200 bubble + 25 tail + 10 margin)
pad        = max(15, 2*concaveRadius - 4)
bar height = 2*pad + n*(ring + 8 + 16 label) + (n-1)*itemGap
window h   = 130 slack + bar height + 130 slack
```

The rail stays at one local x-coordinate with equal transparent space on both
sides. That prevents a webview rebase when the silhouette changes edges; the
unused half extends harmlessly past the display and stays click-through.

The window keeps 130pt of transparent slack **above and below** the bar. The
bubble is centred on whichever ring is hovered, and without that slack a bubble
anchored to the first ring would need a negative offset and get clamped to the
window edge — which reads as a bubble pinned in place with only its tail moving.
The slack is click-through, so it costs nothing.

Computed once in Rust (`layout.rs`) and handed to React, so the window size and
the CSS cannot drift apart and clip the last ring.

The pure-black rail starts at zero width on the display edge. Two tangent
quarter ellipses open it to its full width; the lower pair closes it back into
the edge. `concaveRadius` controls the vertical radius, and half `barWidth` is
the horizontal radius. The 9pt gutter remains transparent between the rail and
the bubble tip. Mouse hit-testing follows this same outline, including its
hollow caps.

The bubble uses the same pure black, 16pt corners, and a 25pt curved tip rather
than a small CSS triangle. Its shadow follows the combined silhouette. Bubble
sizes are sent with the Rust layout so CSS, positioning, and the window agree.
The end rings retain their 38pt diameter; the default spacing matches the
reference proportions. Existing explicit config values still take precedence.

Run `npm run test:design` for SVG silhouette checks and `make test` for the Rust
layout, hit-testing, parsing, process deadlines, connection, freshness, and hook
tests. The live Codex integration test is opt-in (`cargo test
live_codex_connection_is_reused -- --ignored`) because it contacts the signed-in
account. It sends no prompts.
