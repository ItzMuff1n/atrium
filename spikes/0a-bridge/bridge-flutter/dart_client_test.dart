// Phase 0a spike — headless replica of the Flutter app's bridge logic.
//
// This mirrors bridge-flutter/lib/main.dart's _send() exactly: connect to
// 127.0.0.1:42321 with 5s timeout, write one newline-terminated JSON envelope,
// read one reply chunk, decode, classify. No Flutter imports — so it runs on
// the plain Dart VM against the same socket protocol the real app uses.
//
// Purpose: prove the transport + framing + reply-classification works from
// Dart code (same socket APIs, same JSON codec, same framing) without a GUI.
// The one thing it does NOT prove: Flutter widget wiring (the button handler
// actually calling this logic, setState repaint). That's what the user's
// hands-on verification covers.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

Future<String> sendOnce(String text) async {
  try {
    final socket = await Socket.connect(
      '127.0.0.1',
      42321,
      timeout: const Duration(seconds: 5),
    );
    try {
      final request = jsonEncode({'type': 'echo', 'text': text});
      socket.write(request);
      socket.write('\n');
      final reply = await socket.first
          .timeout(const Duration(seconds: 5), onTimeout: () {
        throw TimeoutException('no reply within 5s');
      });
      final decoded = jsonDecode(utf8.decode(reply)) as Map<String, dynamic>;
      final type = decoded['type'] as String?;
      final replyText = decoded['text'] as String?;
      if (type == 'echo') {
        return replyText ?? '<echo reply without text>';
      } else if (type == 'app_error') {
        return 'Server error: ${replyText ?? '<no reason>'}';
      } else {
        return 'Unexpected reply: type=$type text=$replyText';
      }
    } finally {
      socket.destroy();
    }
  } on SocketException catch (e) {
    // Server dead: this is the kill-test path.
    return 'Connection error: ${e.message}';
  } on TimeoutException {
    return 'Connection error: server did not reply within 5s';
  } on FormatException catch (e) {
    return 'Bad reply from server: ${e.message}';
  }
}

Future<void> main() async {
  final sw = Stopwatch()..start();
  final r1 = await sendOnce('hello from dart');
  sw.stop();
  print('reply: $r1');
  print('elapsed: ${sw.elapsedMilliseconds}ms');
  print('');
  print('--- kill test: is the Rust server dead? ---');
  final ping = await Process.run('pgrep', ['-x', 'bridge-rust']);
  print('pgrep bridge-rust: rc=${ping.exitCode} out=${ping.stdout.trim()}');
  if (ping.exitCode == 0) {
    print('server still alive — killing it now to test the failure path');
    final kill = await Process.run('pkill', ['-x', 'bridge-rust']);
    print('pkill rc=${kill.exitCode}');
    await Future<void>.delayed(const Duration(milliseconds: 300));
    final check = await Process.run('pgrep', ['-x', 'bridge-rust']);
    print('pgrep after kill: rc=${check.exitCode} out=${check.stdout.trim()}');
    final r2 = await sendOnce('sentinel-after-kill');
    print('reply after kill: $r2');
    if (r2.startsWith('Connection error')) {
      print('KILL-TEST PASS: clean error, no hang, no crash');
    } else {
      print('KILL-TEST FAIL: expected a connection error, got a reply');
    }
  } else {
    print('server already dead — connection-error path only');
    final r2 = await sendOnce('sentinel-after-kill');
    print('reply: $r2');
    if (r2.startsWith('Connection error')) {
      print('KILL-TEST PASS: clean error, no hang, no Phase 0a crash');
    }
  }
}