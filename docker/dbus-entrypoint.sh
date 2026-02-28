#!/usr/bin/env bash
set -euo pipefail

mkdir -p /bus
rm -f /bus/session-bus

# Start a *session* bus on a known unix socket path.
# --nofork keeps it in the foreground (docker-friendly)
exec dbus-daemon \
  --session \
  --nofork \
  --nopidfile \
  --address=unix:path=/bus/session-bus
