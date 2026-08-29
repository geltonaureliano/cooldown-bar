#!/usr/bin/env python3
"""Capture only rate-limit metadata; never persist prompts or session paths."""
import json
import math
import os
from pathlib import Path
import sys
import tempfile

MAX_BYTES = 1024 * 1024

def percentage(window):
    if not isinstance(window, dict):
        return None
    for key in ("used_percentage", "used_percent", "usedPercent", "utilization"):
        value = window.get(key)
        if isinstance(value, (float, int)) and not isinstance(value, bool) and math.isfinite(value):
            return round(max(0, min(100, value)))
    return None

def capture(data, destination):
    limits = data.get("rate_limits")
    if not isinstance(limits, dict):
        return
    # Whitelist only usage metadata; the raw hook payload contains workspace data.
    fields = ("used_percentage", "used_percent", "usedPercent", "utilization", "resets_at", "resetsAt")
    limits = {name: {key: window[key] for key in fields if key in window}
              for name, window in limits.items()
              if isinstance(window, dict) and (name == "five_hour" or name.startswith("seven_day"))}
    if not limits:
        return
    destination.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    temp = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf8", dir=destination.parent, prefix=".claude-", delete=False) as stream:
            temp = stream.name
            json.dump({"rate_limits": limits}, stream, allow_nan=False)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp, destination)
    finally:
        if temp and os.path.exists(temp):
            os.unlink(temp)

def status(data):
    parts = []
    model = data.get("model", {})
    if isinstance(model, dict):
        name = model.get("display_name") or model.get("id")
        if name:
            parts.append(str(name))
    context = percentage(data.get("context_window"))
    if context is not None:
        parts.append(f"ctx {context}%")
    limits = data.get("rate_limits") or {}
    if isinstance(limits, dict):
        for name, label in (("five_hour", "5h"), ("seven_day", "7d")):
            value = percentage(limits.get(name))
            if value is not None:
                parts.append(f"{label} {value}%")
    return "  ".join(parts)

def main():
    raw = sys.stdin.buffer.read(MAX_BYTES + 1)
    if len(raw) > MAX_BYTES:
        return
    try:
        data = json.loads(raw)
        if not isinstance(data, dict):
            return
    except (ValueError, UnicodeError):
        return
    try:
        capture(data, Path.home() / ".notchusage" / "claude.json")
    except (OSError, ValueError):
        pass  # Capture failure must not break the terminal's statusline.
    if "--capture-only" not in sys.argv:
        sys.stdout.write(status(data))

if __name__ == "__main__":
    main()
