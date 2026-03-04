#!/usr/bin/env bash
set -euo pipefail

export XDG_RUNTIME_DIR=/run/pipewire
mkdir -p "$XDG_RUNTIME_DIR"

# Start WirePlumber (session manager) in the background
wireplumber &
WP_PID=$!

# Give WirePlumber a moment to initialize
sleep 1

echo "PipeWire + WirePlumber running (socket at $XDG_RUNTIME_DIR/pipewire-0)"

# Run PipeWire in the foreground
exec pipewire
