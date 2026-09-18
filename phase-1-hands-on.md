# Phase 1 — hands-on pass: what to do and what counts as working

Written 12 Sep 2026. Everything below is a step you do by hand, in a terminal
window. Nothing needs installing; it is already built.

> **Completed 13 Sep 2026 — Muffin ran this pass and Phase 1 is signed off.**
> This file is now a historical record of the pass, not a set of pending
> instructions. **One line below is superseded:** "If it holds, Phase 1 is
> signed off and Phase 2 ... begins" — Phase 2 no longer follows Phase 1
> directly. **Phase 1b (paths that do not exist yet) comes first**, by decision
> of 12 Sep 2026; Phase 2 is gated behind it. See `BUILD-PLAN.md` and
> `STATUS.md` "Verified hands-on".

---

## What this is proving

Every path an agent asks for gets checked before it is used. `/home/documents`
means a folder *inside* the sealed-off area, not the one on your real computer.
This pass proves that a path cannot be written in a way that slips outside that
area.

---

## Before you start: two things that will look like bugs but are not

**1. Five lines in section E have been corrected since yesterday.**
They used to be listed as "must REJECT". They are now in section I, as accepts,
because they are the same path as an accept written differently. This is now
settled in the file, not a contradiction you have to spot.

**2. Three lines cannot be tested this way at all — the NUL byte ones in
section F.** A NUL byte is a zero byte, and a command-line argument is stopped
by a zero byte by definition. The shell refuses them before the resolver ever
sees them. Those three are checked by the automated test suite instead, not by
hand. That gap is recorded at the bottom of section F.

---

## Step 1 — build the fake world

Copy-paste this whole line into a terminal and press Enter:

```
cd "/home/muffin/Desktop/Nexus project/resolver" && rm -rf /tmp/atrium-handtest && ./target/debug/atrium-resolver fixtures --root /tmp/atrium-handtest
```

**What you should see:** one line saying `fixtures created under
/tmp/atrium-handtest`.

That command builds a throwaway folder that stands in for the sealed area: it
creates the files the accept-list expects, plus the eleven symlinks the attack
list tries to abuse. A symlink is a shortcut that points at another file.

---

## Step 2 — run the whole list in one go

The easy version. Copy-paste:

```
cd "/home/muffin/Desktop/Nexus project/resolver" && bash hand-test.sh
```

It prints every line of every section with its verdict, then a count. **It also
checks one thing the list does not ask for:** that every ACCEPT really lands
inside the throwaway folder. A line marked `!! ESCAPE FROM ROOT` would be the
worst possible result.

**What counts as working:**
- The last line reads `hand-test: every line behaved as required`
- No line contains `FAIL` or `ESCAPE`

**What counts as broken:** any line with `FAIL`, any `!! ESCAPE FROM ROOT`, any
line that hangs, and any line whose reason is just `invalid path` with no
explanation of what specifically was wrong.

---

## Step 3 — do a few by hand, so you see it yourself

The script is a convenience; the point is watching it happen. Run these one at
a time. Each should print one line starting `REJECT`.

```
cd "/home/muffin/Desktop/Nexus project/resolver"

./target/debug/atrium-resolver resolve --root /tmp/atrium-handtest '/../../etc/passwd'
./target/debug/atrium-resolver resolve --root /tmp/atrium-handtest '/etc/passwd'
./target/debug/atrium-resolver resolve --root /tmp/atrium-handtest '/link-to-etc'
./target/debug/atrium-resolver resolve --root /tmp/atrium-handtest '/link-to-etc/../home/documents'
./target/debug/atrium-resolver resolve --root /tmp/atrium-handtest '/home/documents/../../../etc/passwd'
```

The fourth one is the important one. It walks *through* a shortcut that points
at your real `/etc`, then comes back to a path that looks innocent. A checker
that only looks at the final destination would let it through.

Then these, which should each print one line starting `ACCEPT`:

```
./target/debug/atrium-resolver resolve --root /tmp/atrium-handtest '/home/documents'
./target/debug/atrium-resolver resolve --root /tmp/atrium-handtest '/home/subdir/../documents'
./target/debug/atrium-resolver resolve --root /tmp/atrium-handtest '/link-to-inside'
```

The second one uses `..` and stays inside. A checker that rejects every `..`
would fail it, and would break ordinary work.

**The one thing to actually look at:** on every ACCEPT, the printed path starts
with `/tmp/atrium-handtest`. If any ACCEPT prints a path that does not start
with that, stop and tell me.

---

## Step 4 — the two lists the resolver never saw

```
bash probe-parent.sh      # my own list, written after reading the code
bash probe-blind.sh       # a second agent's list; it never saw the code at all
```

`probe-blind.sh` plants nastier shortcuts than the main list: ones that point
at `/proc/self/root` (a magic path that always means "the whole disk"), ones
that point at themselves in a loop, one that points at a live feed from another
program.

**What counts as working:** each ends with `no escapes, no hangs`.

---

## When you are done

Tell me "holds" or tell me which line misbehaved. **A single escape stops the
project** — everything after this assumes the sealed area cannot be broken.

If it holds, Phase 1 is signed off and Phase 2 (the environment itself:
creating, moving, deleting files inside it) begins.

---

## If a command errors out or prints something strange

Paste the whole terminal window back to me, exactly as it appears. Do not
summarise it or tidy it up. I need the exact text.

---

## Detail, if you want it later

- `phase-1-evidence.txt` — every check above, with its verbatim output and what
  each result means.
- `attack-list.md` — the list itself, with the corrections marked.
- `blind-attack-list.md` — the second agent's list, now stored in the project.
- `resolver/hand-test.sh`, `probe-parent.sh`, `probe-blind.sh` — the scripts
  you just ran.

You do not need to read any of these to do the steps above.
