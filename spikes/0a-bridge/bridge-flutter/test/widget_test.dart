// Phase 0a spike — widget test for the bridge app's wiring.
//
// Replaces the scaffolded counter test (MyApp is gone).
//
// What it proves: the Send button is wired to _send(), which talks JSON over
// the real socket to the Rust server (must be running on 127.0.0.1:42321),
// and the reply lands in the status Text via setState.
// What it does NOT prove: the app rendered by a human's hands — that remains
// the user's 0a verification gate.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:bridge_flutter/main.dart';

void main() {
  testWidgets('Send button wires to the socket and shows the echo reply',
      (WidgetTester tester) async {
    await tester.pumpWidget(const BridgeApp());

    // The spike UI: one text field, one Send button, idle status.
    expect(find.byType(TextField), findsOneWidget);
    expect(find.text('Send'), findsOneWidget);
    expect(find.text('Type something and press Send.'), findsOneWidget);

    await tester.enterText(find.byType(TextField), 'hello from widget test');

    // _send() does real socket I/O; runAsync lets real async complete.
    await tester.runAsync(() async {
      await tester.tap(find.text('Send'));
      await tester.pump();
      // Poll up to 5s for the reply to land in the status Text.
      for (var i = 0; i < 50; i++) {
        await Future<void>.delayed(const Duration(milliseconds: 100));
        await tester.pump();
        if (find.text('Type something and press Send.').evaluate().isEmpty) {
          break; // status line changed — reply (or error) arrived
        }
      }
    });

    // Idle status is gone; the echoed reply is displayed. The entered text
    // also appears once (inside the field), so the string is present twice.
    expect(find.text('Type something and press Send.'), findsNothing);
    expect(find.text('hello from widget test'), findsNWidgets(2));
  });
}