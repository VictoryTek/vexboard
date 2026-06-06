#!/usr/bin/env bash
# Installs VexBoard git hooks by symlinking scripts/hooks/ into .git/hooks/.
# Run once after cloning: bash scripts/install-hooks.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOOKS_SRC="$ROOT/scripts/hooks"
HOOKS_DST="$ROOT/.git/hooks"

if [ ! -d "$HOOKS_DST" ]; then
    echo "ERROR: $HOOKS_DST does not exist — are you in a git repository?"
    exit 1
fi

for hook in "$HOOKS_SRC"/*; do
    name="$(basename "$hook")"
    target="$HOOKS_DST/$name"

    if [ -L "$target" ]; then
        rm "$target"
    elif [ -f "$target" ]; then
        echo "WARNING: $target already exists as a regular file — skipping."
        continue
    fi

    ln -s "$hook" "$target"
    chmod +x "$hook"
    echo "Installed: .git/hooks/$name -> $hook"
done

echo "Done. Git hooks installed."
