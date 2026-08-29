#!/bin/sh
# Atomic usage capture plus a compact statusline. --capture-only is for chaining.
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 0
exec python3 "$SCRIPT_DIR/claude-statusline.py" "$@"
