#!/bin/sh
# Install Zeabur toolkit skills (github.com/zeabur/toolkit-skills) for
# Claude Code at container start, so the agent can rent/manage servers
# and bind subdomains. Requires ZEABUR_API_KEY at runtime to actually
# call the Zeabur API; install itself needs no auth.
if command -v claude >/dev/null 2>&1; then
  claude plugin marketplace add zeabur/toolkit-skills 2>/dev/null || true
  claude plugin install toolkit@zeabur-toolkit 2>/dev/null || true
fi

exec "$@"
