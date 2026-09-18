#!/usr/bin/env bash
# Launch the Phase 0a spike Rust server fully detached from the Hermes TUI.
# setsid: own session (survives TUI death, can't feed EIO into TUI FDs).
# All FDs redirected: nothing points back at the terminal that spawned it.
set -euo pipefail
cd "/home/muffin/Desktop/Nexus project/spikes/0a-bridge/bridge-rust"
setsid nohup ./target/debug/bridge-rust > server.log 2>&1 < /dev/null &
sleep 0.3
echo "launcher done"