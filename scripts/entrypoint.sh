#!/bin/sh
# Install Zeabur plugin for Claude Code if not already present.
# Runs at container start so the auth token from inherit_env is available.
if command -v claude >/dev/null 2>&1; then
  claude plugin add zeabur 2>/dev/null || true
fi

exec "$@"
