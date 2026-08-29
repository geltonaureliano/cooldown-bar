# Configuration

Cooldown Bar reads `~/.cooldown-bar/config.json`. An upgrade keeps using `~/.notchusage/config.json` when the new file does not exist and the legacy file does.

All properties are optional. Invalid values are replaced with safe defaults. A malformed file is not overwritten by drag persistence.

## Example

```json
{
  "edge": "right",
  "barWidth": 62,
  "concaveRadius": 31,
  "topOffset": null,
  "edgeInset": 0,
  "ringDiameter": 38,
  "ringLineWidth": 3,
  "itemGap": 28,
  "refreshSeconds": 10,
  "staleAfterSeconds": 120,
  "showClaude": true,
  "showCodex": true,
  "customCommand": null,
  "customTitle": "Custom",
  "claudeColor": "#FF5F2E",
  "codexColor": "#00E07A",
  "customColor": "#E8E80A"
}
```

## Layout properties

`edge` accepts `left` or `right`.

`topOffset` accepts a number of display points or `null`. The default `null` places the panel below the live menu bar area. An explicit zero can place content underneath the macOS menu bar.

`barWidth`, `concaveRadius`, `edgeInset`, `ringDiameter`, `ringLineWidth`, and `itemGap` control panel geometry. Unsafe values are clamped.

## Collection properties

`refreshSeconds` defaults to 10 and is clamped from 5 to 3600 seconds. Provider failures use exponential backoff.

`staleAfterSeconds` defaults to 120 and is clamped from 30 to 86400 seconds. A reading becomes stale according to its measurement time, not its most recent scan time.

`showClaude` and `showCodex` control built in providers.

## Custom provider

`customCommand` can contain a shell command that prints one JSON object to standard output and exits successfully within three seconds.

```json
{
  "percent": 52,
  "resets_at": 1756400000,
  "label": "Session",
  "secondary_percent": 11,
  "secondary_label": "Weekly"
}
```

`percent` is required. The other properties are optional. Cooldown Bar limits captured output and terminates the owned process group after the timeout.

## Icons

Place custom PNG files in `~/.cooldown-bar/icons` with the names `claude.png`, `codex.png`, or `custom.png`.

Legacy files in `~/.notchusage/icons` remain available during upgrades. New files take priority.
