#!/usr/bin/env python3
"""Phase 0a spike: raw test client for the Rust bridge server.

Sends four probes over one TCP connection, prints send/recv verbatim:
  1. {"type":"ping"}            -> expect {"type":"pong",...}
  2. {"type":"echo","text":...} -> expect same text back
  3. malformed JSON             -> expect app_error with reason
  4. unknown type               -> expect app_error unknown message type

Exit 0 only if all four replies match expectations.
"""
import json
import socket
import sys

HOST, PORT = "127.0.0.1", 42321

def recv_line(sock):
    buf = b""
    while not buf.endswith(b"\n"):
        chunk = sock.recv(4096)
        if not chunk:
            raise ConnectionError("server closed connection while we awaited a reply")
        buf += chunk
    return buf.decode().strip()

def probe(sock, label, raw, expect_type, expect_text=None):
    sock.sendall((raw + "\n").encode())
    reply_raw = recv_line(sock)
    print(f"[{label}] sent: {raw}")
    print(f"[{label}] recv: {reply_raw}")
    try:
        reply = json.loads(reply_raw)
    except json.JSONDecodeError:
        print(f"[{label}] FAIL: reply is not JSON")
        return False
    if reply.get("type") != expect_type:
        print(f"[{label}] FAIL: expected type {expect_type!r}, got {reply.get('type')!r}")
        return False
    if expect_text is not None and reply.get("text") != expect_text:
        print(f"[{label}] FAIL: expected text {expect_text!r}, got {reply.get('text')!r}")
        return False
    print(f"[{label}] PASS")
    return True

def main():
    ok = True
    with socket.create_connection((HOST, PORT), timeout=5) as sock:
        sock.settimeout(5)
        ok &= probe(sock, "ping", '{"type":"ping"}', "pong")
        ok &= probe(sock, "echo", '{"type":"echo","text":"hello atrium"}', "echo", "hello atrium")
        ok &= probe(sock, "malformed", '{"type":"echo", nope', "app_error")
        ok &= probe(sock, "unknown", '{"type":"shatter","text":"x"}', "app_error")
    print("RESULT:", "ALL PASS" if ok else "FAILURES PRESENT")
    sys.exit(0 if ok else 1)

if __name__ == "__main__":
    main()