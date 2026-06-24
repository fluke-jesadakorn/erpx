#!/bin/bash
# Start n8n with env loaded from .env
# Used by launchd plist (which can't source .env directly)

set -a
. /Users/fluke/Desktop/Work/Contents/infra/.env
set +a

# Use node 22 (nvm) — node 26 system breaks isolated-vm native build
export PATH="/Users/fluke/.nvm/versions/node/v22.23.0/bin:$PATH"

exec n8n start
