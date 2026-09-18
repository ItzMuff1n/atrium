# Atrium — resolver attack list for Phase 1b (paths that do not exist yet)

**Status:** PROPOSED by the orchestrating Hermes session, 13 Sep 2026, for
Muffin's approval. Written **before** the code, per `BUILD-PLAN.md` Phase 1b.
Nothing in Phase 1b is built, stubbed, or flagged today.

**Revision note (same day, second pass):** the original section E treated
`/etc/…`, `/root/…`, `/proc/…` as escapes. That was wrong, and deriving its
verdicts exposed the error. The model is now confirmed and the namespace ruled
on by Muffin (`DECISIONS.md` "The environment namespace follows the Linux
tree"), so the section is gone and its coverage lives in §K as must-ACCEPT —
which is what it should always have been. Section letters were renumbered: what
was F is now E, and so on through I.

**Why this file exists.** Phase 1 resolves existing paths only. Every absent
path currently rejects with *"… does not exist inside the environment root
(Phase 1 resolves existing paths only)"* (observed 13 Sep 2026). Phase 1b adds:
canonicalise the longest existing prefix, then validate the non-existent
remainder so it cannot traverse.

**Provenance.** Authored by the agent that will review the implementation. As
with Phase 1's list, **this list does not on its own satisfy the independence
requirement.** `BUILD-PLAN.md` line 211 calls for "the same shape as Phase 1's
two lists" — so Phase 1b needs its own blind second list before sign-off.

---

## The model — what an escape actually is

A virtual path's leading `/` means **the environment root**, not the host root.
`/home/documents` means `<root>/home/documents`, and `/etc/passwd` means
`<root>/etc/passwd`. Source: `resolver/README.md` lines 14–15 ("Virtual paths
are absolute *within* the environment"), and the behaviour below.

**Observed 13 Sep 2026**, against a fixture root:

    resolve --root /tmp/atrium-rcheck /etc/passwd
      REJECT  "`/etc` does not exist inside the environment root"
    mkdir -p /tmp/atrium-rcheck/etc && echo hi > /tmp/atrium-rcheck/etc/passwd
    resolve --root /tmp/atrium-rcheck /etc/passwd
      ACCEPT  /tmp/atrium-rcheck/etc/passwd

The same virtual path is a rejection and then an acceptance, and creating a
*file inside the sandbox* is what changed it. The path was never a host path.
Escape is reached **only** two ways:

1. a `..` step that walks above the environment root, or
2. a symlink whose *target* resolves outside the environment root.

Nothing else. In particular **no spelling of a path is an escape.** A name that
looks like a host path is a sandbox path. This is the fact every section below
is derived from; it was confirmed by Muffin on 13 Sep 2026 and recorded in
`DECISIONS.md`.

---

## The guarantee Phase 1b must keep

A path that does not exist yet, whose resolved location sits **inside** the
environment root, must be **ACCEPT**ed — otherwise nothing can ever create a
file. Absence must stop being a rejection reason. Containment must not relax:
if the resolved location would sit outside the root, REJECT with a named reason.

**The step rule does not change.** Phase 1 checks containment at every step, not
just at the final destination. `/link-to-etc/../home/documents` rejects because
the step through `/etc` left the root, though the final path looks legal. Phase
1b must keep that, including for steps in the part that does not exist.

---

## Why the *reason* matters more here than in Phase 1

Every line in sections A–I has an absent target. A resolver that has not
implemented 1b — the current one — rejects **all** of them, citing absence. The
verdict column would look correct for the wrong reason.

For sections A–I the required reason names the **escape**: which `..` step left
the root, or which symlink's target is outside. A rejection that says only
"does not exist" is a **failure of these sections, not a pass**. This is what
separates a working Phase 1b from one that appears to work.

---

## How to read a result

    atrium-resolver resolve --root <env-root> '<path>'

Quote every path; many contain characters the shell will otherwise eat.

**ACCEPT** — prints the real resolved path. Confirm it sits under the
environment root. An accept pointing outside is the worst possible outcome and
the reason this list exists.

**REJECT** — prints a reason. For sections A–I, confirm the reason names the
escape, not merely that the target is absent.

**Anything else** — a hang, a crash, a raw operating-system error, a stack
trace — is a failure even when nothing escaped. The resolver decides; it does
not fall over.

---

## Fixtures

Phase 1b needs Phase 1's fixture set **plus dangling symlinks**:

    /link-to-nothing      -> /home/documents/absent.txt   (link exists, target absent, inside root)
    /link-to-nothing-out  -> /etc/absent.txt              (link exists, target absent, outside root)

**A symlink's target is a real host path, as in Phase 1's fixtures** —
`/link-to-etc -> /etc` points at the host's `/etc`. So `/link-to-nothing-out`'s
target is the host's `/etc/absent.txt`, which is outside the root and stays
outside whether or not it exists. Do not confuse that with the *virtual* name
`/etc/absent.txt`, which means `<root>/etc/absent.txt` and is inside (§K). Same
spelling, two different things; only the symlink's side is a host path.

The absent paths in sections A–I **must not be created** — creating them would
test something else. The same holds for the names in §K: `/etc`, `/proc`, `/root`
and the rest are deliberately *not* fixtures. The environment is not
pre-populated, so they resolve as absent, which is the outcome `DECISIONS.md`
asks for.

Nothing is created for `/etc`, `/root`, `/proc`, `/sys` or `/dev`. Whether they
exist is not part of the test: under Phase 1b they ACCEPT either way, and their
absence is the honest state the namespace decision chose.

---

## A. Traversal in the part that already exists, absent tail — must REJECT

    /../newfile
    /../../newfile
    /../../..
    /home/../../newfile
    /home/documents/../../../etc/newfile
    /a/./../../newfile

The tail is a clean name; the traversal precedes it. A resolver that
canonicalises the longest existing prefix and then validates only the remainder
sees `newfile` and accepts. This section exists to catch exactly that.

## B. `..` inside the part that does not exist — must REJECT

    /newdir/../../newfile
    /newdir/../..
    /newfile/../..
    /home/newdir/../../../newfile
    /a/b/newdir/../../../../newfile
    /home/documents/newdir/../../../../etc/newfile

Nothing on disk can be canonicalised for these steps, so the remainder must be
normalised and walked as text, applying the same step rule as section A.

## C. Absent tail on an existing symlink that points outside — must REJECT

    /link-to-etc/newfile
    /link-to-home/newfile
    /link-to-root/newfile
    /link-to-parent/newfile
    /chain-a/newfile
    /link-to-etc/sub/newfile
    /good-dir/inner-link/newfile
    /link-to-etc/../home/newfile

The existing prefix resolves fine; the escape is in *where it resolves to*.
Checking only that the prefix exists accepts every line here.

## D. Escapes that appear only once the absent tail is appended — must REJECT

    /link-to-inside/../../../newfile
    /link-to-inside/newdir/../../../../newfile
    /good-dir/../../../newfile
    /link-to-root/../etc/newfile

`BUILD-PLAN.md` line 210 names this shape: the escape is not in the existing
part or the absent part alone, but in the combination.

## E. Out, back, then absent — must REJECT

    /link-to-etc/../../home/documents/newfile
    /home/../link-to-root/home/newfile
    //../newfile

The final path looks legal. The route to it did not — the same shape Muffin
tested by hand for Phase 1.

## F. Absent parent, escaping remainder — must REJECT

    /newdir/../../etc/newfile
    /home/newdir/../../../root/newfile

## G. Separator and normalisation tricks, absent tail — must REJECT

    /home//..//..//newfile
    /home/documents//../..//../newfile
    /home/documents/newdir//../../../..//newfile

Repeated separators collapse, so these normalise to the traversal forms above
and must be caught after normalisation, not before.

## H. Nasty combinations — must REJECT

    /link-to-etc/../../newfile
    /home/../link-to-root/newfile
    /link-to-parent/newdir/newfile
    /..//newfile
    /home/documents/../../../newfile
    /link-to-etc/./../newfile

---

## I. Must be ACCEPTED

A resolver that rejects everything absent passes sections A–G perfectly. These
prove it is useful rather than merely secure. **These paths must not exist.**

    /newfile
    /newdir/newfile
    /newdir/
    /new file.txt
    /newdir/.hidden
    /newdir/file.name.with.dots.txt
    /newdir/-leading-dash.txt
    /home/newfile
    /home/documents/newfile
    /home/documents/newdir/newfile
    /home/documents/./newfile
    /home//newfile
    //newfile
    /home/documents/../newfile
    /home/newdir/../newfile
    /home/newdir/../../newfile
    /newdir/../newfile
    /home/documents/newdir/../../newfile
    /link-to-inside/newfile
    /link-to-inside/newdir/newfile
    /новый/файл.txt
    /新しい/ファイル.txt
    /newdir/новый/файл.txt

The important ones are `/home/documents/../newfile` and
`/link-to-inside/newfile`: the first uses `..` and stays inside, the second
appends an absent tail to a symlink that points inside. A resolver that rejects
every `..`, or every path through a symlink, breaks ordinary agent work.

---

## J. Component length — one accept, three rejects

Decided by Muffin, 13 Sep 2026: **follow Linux.** `DECISIONS.md`, "Phase 1b —
three resolver rulings" is the authority. In outline: a single component over
**255 bytes** rejects, and the resolver measures this **itself** rather than
waiting for the filesystem to raise `ENAMETOOLONG`, so the verdict is identical
whether the path exists or not. **Bytes, not characters** — `NAME_MAX` is 255
bytes. **Per component, not per path**; a long path of short names is fine, and
`PATH_MAX` (4096) is deliberately not enforced.

    /<255 a's>            ACCEPT   exactly at the limit
    /<256 a's>            REJECT   one byte over
    /<200 Hebrew chars>   REJECT   400 bytes, only 200 characters
    /a/a/a/…(200×)        ACCEPT   long path, short components — see below

The Hebrew line is the one a character-counting resolver gets wrong: 200
characters is comfortably "short" by count and 400 bytes by measure. Observed
against the real filesystem 13 Sep 2026, to confirm `NAME_MAX` is bytes and not
characters:

    255 × 'a'          CREATE ok
    256 × 'a'          CREATE fails  File name too long
    127 Hebrew chars   CREATE ok     (254 bytes)
    128 Hebrew chars   CREATE fails  File name too long  (256 bytes)
    63 emoji           CREATE ok     (252 bytes)
    64 emoji           CREATE fails  File name too long  (256 bytes)

The **200-nested-components** line is a long path of short names and is a
**must-ACCEPT** under 1b. It rejects today only because it is absent and
resolves nothing:

    REJECT Rejected: `/a` does not exist inside the environment root (Phase 1 resolves existing paths only)

That is the phase's own defect, not a length verdict. It sits here rather than
in §K merely to sit beside its length sibling, but it accepts for the same
reason everything in §K does. **`attack-list.md` §G lists this line as a
REJECT**; under 1b it inverts. `hand-test-1b.sh` must create the 255-byte name
as a fixture so this section's accept is a real accept and not a lucky one.

---

## K. Names that read as escapes and are not — RESOLVED, these ACCEPT

Settled by `DECISIONS.md` "The environment namespace follows the Linux tree"
(Muffin, 13 Sep 2026). Read that section before the pass; it is the authority
here, not this summary.

A leading `/` is the **environment root**, not the host root
(`resolver/README.md:14-15`, and observed: `/etc/passwd` rejected as absent
against a fixture root, then accepted at `<root>/etc/passwd` once that file was
created inside the sandbox). **No spelling of a path is an escape.** Escape is
only a `..` past the root or a symlink target outside it.

The environment uses standard Linux directory names — `/home`, `/etc`, `/tmp`,
`/var`, `/usr`, `/opt`, `/srv`, `/mnt`, `/root` — and **is not pre-populated**:
a directory exists because something needs it. `/proc`, `/sys` and `/dev` are
excluded by design, because Atrium has no kernel and reproducing them means
empty directories wearing the name of something real.

**These all ACCEPT**, and they are the lines most likely to be misread as
catastrophic failures when watched by hand:

    /etc/passwd           -> ACCEPT (<root>/etc/passwd)
    /etc/newdir/newfile   -> ACCEPT
    /root/newfile         -> ACCEPT
    /proc/self/newfile    -> ACCEPT  (resolves as absent; /proc is excluded, so nothing is ever there)
    /sys/class/newfile    -> ACCEPT
    /dev/newfile          -> ACCEPT

They resolve inside the sandbox because that is what they are: ordinary absent
names under the root. Nothing here is reserved, forbidden, or special-cased —
`DECISIONS.md` rejected reserving host-looking names, because the resolver
contains everything by construction.

`attack-list.md` §B already carries the correction to its own framing (the old
heading "Absolute host paths" and the `/proc/self/cwd` claim were both false
about what the resolver does). Its verdicts were and remain correct.

### K.1 Three rulings made 13 Sep 2026 (implementation calls, not Muffin's)

Recorded in `DECISIONS.md`; carried as requirements in the brief.

**K.1a A dangling symlink inside the root is ACCEPTed.** `/link-to-nothing`
exists, its target `/home/documents/absent.txt` does not → ACCEPT. Phase 2 must
be able to create files through such a path. Its counterpart `/link-to-nothing-out`
(→ `/etc/absent.txt`) → REJECT: the target is outside whether or not it exists.

**K.1b A file followed by `..` is REJECTed, and the operating system decides.**
`/home/documents/notes.txt/..` normalises lexically to `/home/documents`, inside
the root, but `notes.txt/..` is `ENOTDIR` on a real filesystem. The prefix does
exist, so the filesystem answers. Phase 1 does exactly this today — observed:

    REJECT Rejected: the operating system refused `/home/documents/notes.txt/..`
    while resolving (`Not a directory (os error 20)`)

No line for these two needs to sit in this file: K.1a's links are fixtures, and
K.1b's paths are covered by `resolver/`'s own tests. They are stated here so the
verdicts are on the record before the code.

**K.1c Backslash is a filename character, never a separator.**
`/home\..\..\newfile` is one filename on Linux. It is not traversal and must
never reject *as* traversal; absent, it accepts like any other name inside the
root. Phase 1's `attack-list.md` §E already states this for existing paths.

---

## M. Blind-list lines — added 13 Sep 2026, after the build

**Source: `blind-attack-list-1b.md`**, written by a subagent that saw none of the
resolver source, the brief, `attack-list.md`, or this file. The Independence
section below makes running these a precondition of accepting Phase 1b, so they
sit here, on the list that gets watched, rather than in a separate file nobody
drives.

Most of its lines restate sections A–I in different words. These are the ones
this list did not already cover. **Fixtures needed beyond the existing set** are
named per group.

### M.1 Dot-runs that are not traversal — must ACCEPT

    /.../newfile
    /..../newfile
    "/.. /newfile"

Three and four dots are ordinary filenames; so is `..` followed by a space. If
any of these rejects *as traversal*, the check is matching substrings rather than
walking components — which would be a real finding. Same principle as K.1c for
backslash.

### M.2 Encoded traversal — must ACCEPT

    /%2e%2e/newfile
    /%2E%2E%2Fhost/newfile
    /..%00/newfile

Percent-encoding is not filesystem syntax and `%00` is three literal characters,
not a NUL byte. All three are ordinary absent names inside the root. They reject
only if something in the path decodes them, which nothing should. Cheap, and the
failure it catches is severe.

### M.3 Relative symlink targets that climb out — must REJECT

**New fixture:** `/link-rel-out` → `../../outside` (relative target, climbs above
the root).

    /link-rel-out/newfile
    /link-rel-out/../newfile

The existing outside-pointing fixtures use absolute targets. A relative target is
resolved against the link's own directory and introduces traversal steps that
never appear in the requested path — so a resolver that clamps only what the
caller typed misses it entirely. `/link-to-parent` (→ `..`) is the one-step case;
these climb past the root.

### M.4 Chain plus traversal after the hop — must REJECT

    /chain-a/newdir/../../newfile
    /chain-a/../../newfile

§C tests a chain with a plain absent tail. These add traversal *after* the chain
has landed outside, testing that depth accounting restarts at each hop instead of
being counted once against the original string.

### M.5 Degenerate tails on a dangling symlink — one accept, one reject

    /link-to-nothing/..        ACCEPT   target's location is inside the root
    /link-to-nothing-out/..    REJECT   the symlink step already left the root

**This is the case the implementing child hit with no rule covering it.** Ruled
on after the fact — `DECISIONS.md`, ruling 4 in "Phase 1b — three resolver
rulings" (there are four; the heading is stale by choice). The accept is the
sharper of the two: it must not be confused with K.1b, where the prefix exists as
a file and the operating system supplies the rejection.

### M.6 Length interacting with traversal — must ACCEPT

    /<255 a's>/../<255 b's>

Both components sit exactly at the limit and the `..` cancels the first. A
resolver that truncates a long component changes what the `..` cancels, and lands
somewhere other than `<root>/<255 b's>`. §J tests length alone; this tests length
and normalisation in the same path.

### M.7 Depth accounting at 20 levels — one accept, one reject

    /a/…(20×)/..…(20×)/newfile      ACCEPT   balanced, lands at <root>/newfile
    /a/…(20×)/..…(21×)/newfile      REJECT   one step above the root

The same shape as §A and §B at depth 2, repeated deep. It is here because an
off-by-one in a depth counter is invisible at shallow depth and the pair brackets
it exactly: if both come back the same verdict, the counter is wrong whichever
verdict it is.

---

## L. Open design questions — none remain

Nothing. Every item that was open here has been ruled on above or in
`DECISIONS.md`. This section is kept, empty, so a reader knows the questions
were asked and not skipped.

---

## Independence

`BUILD-PLAN.md` requires the attack list come from outside the head that built
the resolver. This list does not satisfy that on its own. **Required before
Phase 1b sign-off:** a second list produced by a subagent that has seen **none**
of the resolver source, the Phase 1b delegation brief, `attack-list.md`, or this
file — given only a plain description of what the sandbox is meant to guarantee,
and asked what paths it would try. Anything it produces that this list missed is
genuine independent signal and must be run before the phase is accepted. Phase
1's equivalent is `blind-attack-list.md` (`deleg_c6716023`). This is a gate on
accepting the phase, not an extra.

**Satisfied 13 Sep 2026.** The blind list exists at `blind-attack-list-1b.md`.
Its lines that this file did not already cover were folded into **§M** above, so
the precondition is discharged by driving this list rather than a second one.
§M.3 needs one fixture that does not exist yet — `/link-rel-out` → `../../outside`.
The blind list's remaining novelty was two things no path string can test
(TOCTOU symlink swapping, and bind mounts), both already recorded as non-goals.

---

## How this list was derived

Every verdict above was computed by walking the path component by component from
the environment root, applying Phase 1's rule: **every step must stay inside the
root**, and `//` collapses, `.` is elided, `x/..` removes `x`. Sections were
assigned by that computation, not by appearance — a line that normalises inside
the root sits in section K even when it contains `..`. This is the discipline
Phase 1's list got wrong on five lines, corrected 12 Sep 2026.

Sections A–H and I were then re-walked mechanically (a throwaway script, every
non-symlink line) and no line's required direction disagreed with its computed
direction. The 20 symlink lines in C, D, E and H cannot be walked lexically and
were checked by hand against each link's target.
