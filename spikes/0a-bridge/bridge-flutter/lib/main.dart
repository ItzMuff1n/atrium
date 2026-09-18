// Phase 0a spike — the Flutter half of the Atrium bridge.
//
// DESIGN.md §2 / DECISIONS.md §Bridge: local socket, JSON messages, not FFI.
// One button, one text field. Press button -> JSON to Rust -> reply displayed.
// Kill the Rust process -> next press must show a clean error, not a hang or
// crash (the spike's failure-mode test).
//
// Deliberately simple: connect/write/read/destroy per press, 5s timeouts.
// No streams, no reconnect logic, no state machines. A spike.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';

void main() {
  runApp(const BridgeApp());
}

class BridgeApp extends StatelessWidget {
  const BridgeApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Atrium bridge spike',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.deepPurple),
        useMaterial3: true,
      ),
      home: const BridgeHome(),
    );
  }
}

class BridgeHome extends StatefulWidget {
  const BridgeHome({super.key});

  @override
  State<BridgeHome> createState() => _BridgeHomeState();
}

class _BridgeHomeState extends State<BridgeHome> {
  final TextEditingController _controller = TextEditingController();
  String _status = 'Type something and press Send.';
  bool _sending = false;

  static const String host = '127.0.0.1';
  static const int port = 42321;

  Future<void> _send() async {
    if (_sending) return; // ignore double-taps
    setState(() {
      _sending = true;
      _status = 'Connecting…';
    });
    try {
      final socket = await Socket.connect(
        host,
        port,
        timeout: const Duration(seconds: 5),
      );
      try {
        // One JSON envelope per line, newline-terminated — same framing the
        // Rust server uses.
        final request = jsonEncode({'type': 'echo', 'text': _controller.text});
        socket.write(request);
        socket.write('\n');
        final reply = await socket.first
            .timeout(const Duration(seconds: 5), onTimeout: () {
          throw TimeoutException('no reply within 5s');
        });
        final decoded = jsonDecode(utf8.decode(reply)) as Map<String, dynamic>;
        final type = decoded['type'] as String?;
        final text = decoded['text'] as String?;
        if (type == 'echo') {
          _status = text ?? '<echo reply without text>';
        } else if (type == 'app_error') {
          _status = 'Server error: ${text ?? '<no reason>'}';
        } else {
          _status = 'Unexpected reply: type=$type text=$text';
        }
      } finally {
        socket.destroy();
      }
    } on SocketException catch (e) {
      // Rust process dead or not listening: must be a clean message, not a
      // hang or crash — the spike's kill-test requirement.
      _status = 'Connection error: ${e.message}';
    } on TimeoutException {
      _status = 'Connection error: server did not reply within 5s';
    } on FormatException catch (e) {
      _status = 'Bad reply from server: ${e.message}';
    } catch (e) {
      _status = 'Unexpected error: $e';
    }
    // setState in one place after the try/catch so success and every error
    // path land in the same code.
    setState(() => _sending = false);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Atrium 0a bridge spike')),
      body: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              controller: _controller,
              decoration: const InputDecoration(
                labelText: 'Message for the Rust process',
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 16),
            ElevatedButton(
              onPressed: _sending ? null : _send,
              child: Text(_sending ? 'Sending…' : 'Send'),
            ),
            const SizedBox(height: 24),
            Text(
              _status,
              style: const TextStyle(fontSize: 16),
              textAlign: TextAlign.center,
            ),
          ],
        ),
      ),
    );
  }
}