# Providers and data sources

Cooldown Bar treats every reading as a measurement with a source, timestamp, verification state, and optional reset time.

## Claude Code

Cooldown Bar first asks the installed Claude CLI for supported usage information. The optional statusline capture can write a bounded snapshot to `~/.cooldown-bar/claude.json`.

The capture script reads JSON from standard input, writes atomically with private permissions, and does not interrupt the terminal statusline when capture fails.

To enable the capture, add a statusline command to `~/.claude/settings.json` and replace the repository path with the absolute path on your Mac.

```json
{
  "statusLine": {
    "type": "command",
    "command": "/absolute/path/to/cooldown-bar/scripts/claude-statusline.sh"
  }
}
```

Claude Code accepts one statusline command. If you already use one, call `scripts/claude-statusline.sh --capture-only` from your existing wrapper and keep your current renderer as the final command.

## Codex

Cooldown Bar maintains a bounded JSON RPC connection to the installed Codex app server and requests account rate limits. The connection remains open so notifications and repeated reads can be reconciled.

If live access is unavailable, recent local Codex session logs can provide an unverified fallback. A log cannot prove which account is currently active, so fallback readings do not become trusted primary values.

## Custom command

A custom provider runs the command configured by the user. The command is responsible for authentication, network access, output accuracy, and secret handling.

## Freshness and reliability

1. Each provider runs independently.

2. Queries have time and output limits.

3. Failed reads back off instead of creating a tight retry loop.

4. A cached value keeps its original measurement time.

5. Unknown or expired timestamps become stale.

6. A stale or unverified value is not presented as a confident current percentage.

7. Collection pauses during detached motion and results already in flight are discarded.

Provider services can cache or delay usage data. Cooldown Bar cannot guarantee instant billing accuracy.
