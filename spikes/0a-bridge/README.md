# Spike 0a — the bridge

**Question (BUILD-PLAN §0a):** can a Rust process and a Flutter app talk JSON
over a local socket — button press → JSON → reply displayed — with a clean
error when the Rust side dies?

**Approach:** std-only Rust TCP server on `127.0.0.1:42321`, newline-delimited
JSON envelopes `{"type":...,"text":...}`. Flutter Linux app, one text field +
Send button. Deliberately no async runtime, no TLS, no reconnect logic —
Phase 0 spike, not architecture.

## Layout

| Path | What |
|---|---|
| `bridge-rust/` | Rust server (cargo project, std + serde only) |
| `bridge-flutter/` | Flutter Linux app (button + text field + status line) |
| `bridge-flutter/dart_client_test.dart` | Headless replica of the app's send logic |
| `test-bridge.py` | Raw-socket probe client (4 probes) |
| `start-server.sh` | Detached server launcher (setsid nohup, logs to `bridge-rust/server.log`) |

## Observed results (this session, run by the agent)

- `cargo build` — finished, 5.39s.
- `flutter build linux --debug` — built.
- `python3 test-bridge.py` — **all 4 probes PASS**: ping→pong; echo round-trip
  verbatim; malformed JSON → `{"type":"app_error","text":"malformed JSON: …"}`
  with reason, connection kept; unknown type → `app_error` with reason.
- `dart run dart_client_test.dart` — same logic as the app's `_send()`:
  round-trip `hello from dart` in 22 ms; then `pkill -x bridge-rust`, send
  again → `Connection error: Connection refused` immediately. Clean error,
  no hang, no crash.

## Not observed

- The GUI itself: no button was pressed by anyone. Widget wiring
  (handler → socket → status repaint) is inferred from the identical headless
  logic, not watched in the real app.

## Run instructions

```bash
# server (idempotent-ish: kills nothing, fails loudly if port taken)
bash "spikes/0a-bridge/start-server.sh"

# app — either run from source:
cd "spikes/0a-bridge/bridge-flutter" && /home/muffin/flutter/bin/flutter run -d linux
# or run the built bundle:
"spikes/0a-bridge/bridge-flutter/build/linux/x64/debug/bundle/bridge_flutter"

# kill test
pkill -x bridge-rust   # then press Send again in the app
```

## Verdict: PARTIAL — transport VALIDATED, GUI pending hands-on

- **Validated (observed):** the socket bridge works end-to-end from Dart code
  identical to the app's, including the kill-the-server failure mode.
- **Pending:** the user pressing the button in the real app — BUILD-PLAN's
  verification gate for 0a. If the GUI shows the reply and a clean error
  after `pkill`, 0a is done and 0b (MCP client) is next.

## Recommendation for the real build

Newline-delimited JSON over TCP was sufficient and trivial on both sides; keep
the envelope shape (`type` + optional `text`) as the seed of the real protocol.
Decide later (Phase 4 area): length-prefixed framing vs newline framing, and
whether the server should take its port from config instead of hardcoding.