# Atrium — resolver attack list

**Status:** authored by Claude (Sep 2026), standing in for the user-authored
list BUILD-PLAN calls for. See "Independence" at the bottom — this list does
not on its own satisfy that requirement.

**Independence requirement SATISFIED 11 Sep 2026.** The second attack list
required before Phase 1 sign-off was produced by a subagent that saw none of
the resolver source, the brief, or this file. It is stored at
`blind-attack-list.md`, and every novel line in it was run against the
resolver with zero escapes.

**Corrected 12 Sep 2026:** five lines moved from section E to section I, and
the bare `/` line removed from section G. Both were internal contradictions;
see the notes in those sections.

Every line is a path handed to the harness:

    atrium-resolver resolve --root <env-root> '<path>'

Quote every path. Many contain characters the shell will otherwise eat.

**Sections A–H: every line must come back REJECT.**
**Section I: every line must come back ACCEPT.**

**Scope warning, added 13 Sep 2026: those two lines describe Phase 1 only.**
Some section A–H lines reject because the path is *absent*, not because it
escapes — `/home⁄documents` (U+2044) among them. Phase 1b resolves paths that
do not exist yet, so those lines will correctly come back ACCEPT once it lands.
That is a verdict change, not a regression. `attack-list-1b.md` is the list
that governs after 1b; this file's verdicts are the Phase 1 record. Rulings and
reasoning in `DECISIONS.md`, "Phase 1b — three resolver rulings".

A rejection must also name a reason specific to that path. A bare
"invalid path" on everything is a failure of requirement §8.3 item 5 even
when the accept/reject verdict is correct.

---

## A. Plain traversal

    /..
    /../..
    /../../..
    /../../../../../../../../../../..
    /home/../..
    /home/documents/../../..
    /home/documents/../../../etc/passwd
    /./../.
    /a/./../../..
    /a/b/c/../../../..
    /a/b/../b/../b/../../..

The last one matters: it returns to a legal place repeatedly before leaving.
A resolver that only checks the final destination may pass it.

## B. Host-looking names

    /etc/passwd
    /etc/shadow
    /home/muffin
    /home/muffin/.ssh/id_rsa
    /root
    /proc/self/environ
    /proc/self/cwd
    /dev/null
    /sys/class
    /var/log/syslog

**Corrected 13 Sep 2026 — the old framing of this section was wrong.** It was
headed "Absolute host paths" and said `/proc/self/cwd` is a live symlink that
"can point outside the sandbox without containing a single `..`". Both are
false about what this resolver does. A leading `/` is the *environment* root,
not the host root — `resolver/README.md:14-15`. `/etc/passwd` means
`<env-root>/etc/passwd` and nothing else; the host's `/etc/passwd` is
unreachable by any spelling. `/proc/self/cwd` is likewise just a name, unless
someone creates that path inside the environment, in which case it is an
ordinary file under the root. **No spelling of a path is an escape.** Only two
things escape: a `..` that walks past the root, and a symlink whose target
lands outside it. Those are sections A and I.

**The verdicts in this section are unchanged and were correct.** The fixture
root has no `etc`, no `proc`, no `root`, so every line here rejects — as
absent, not as an escape. Under Phase 1 that distinction makes no difference
to the result.

**It makes a large difference once Phase 1b lands.** Phase 1b resolves paths
that do not exist yet, so `/etc/passwd`, `/root/newfile` and
`/proc/self/newfile` will then correctly come back **ACCEPT**. Read by hand
those look like catastrophic escapes and they are not. Anyone driving the 1b
pass should read this paragraph before starting, or a correct resolver will
look broken.

## C. Relative paths

Per decision 1 (accepted), these are rejected outright rather than silently
rooted.

    home/documents
    ./home/documents
    ../home
    ..
    .
    documents/../../..

## D. Symlinks

These need fixtures — real symlinks created inside the environment first.
Setup commands are generated after the resolver exists.

    /link-to-etc                  -> /etc
    /link-to-home                 -> /home/muffin
    /link-to-root                 -> /
    /link-to-parent               -> ..
    /chain-a                      -> chain-b, chain-b -> /etc
    /chain-deep                   -> 5 hops, last one lands outside
    /loop-a                       -> loop-b, loop-b -> loop-a
    /dir-link/file.txt            where dir-link -> outside directory
    /good-dir/inner-link          link sits deep, not at the root
    /link-to-etc/../home/documents  escapes, then returns to a legal-looking path

The last one is the real test of "check at every step, not just the end."
The final path looks fine. The route to it did not.

The loop case must reject cleanly with a reason — not hang, not crash.

## E. Separator and normalisation tricks

    /home//../..
    \
    \..\..
    /home\..\..
    /home/documents\..\..

Backslash is an ordinary filename character on Linux, not a separator. So
`\..\..` should be treated as a weird filename and either resolve inside the
root or reject as not-found — never as traversal. Tested because assuming is
how this goes wrong.

**Corrected 12 Sep 2026.** Five lines originally sat here — `//`,
`///home///documents`, `/home/documents//`, `/home/./documents/.`,
`/home/documents/..` — and this section said they must REJECT. That
contradicted section I, which requires the resolver to accept the paths they
normalise to. On Linux `//` *is* `/`, repeated separators collapse, and `.`
components are elided, so each of the five is the same path as a line section I
lists as a must-ACCEPT. The resolver resolved them; it did not escape.

Section I is the correct side of that conflict: accepting the environment root
is required, and a resolver that rejects it is broken rather than secure. The
five lines have been **moved into section I** so they are still tested, and are
no longer listed as reject-cases here.

## F. Unicode and lookalikes

    /home⁄documents          (U+2044 fraction slash)
    /home∕documents          (U+2215 division slash)
    /home／documents         (U+FF0F fullwidth solidus)
    /home<NUL>documents      (literal NUL byte, 0x00)
    /home/doc<NUL>uments
    /<NUL>
    /home/<U+202E>documents  (right-to-left override)
    /home/<U+FEFF>documents  (zero-width no-break space)
    /café/documents          (composed é, U+00E9)
    /café/documents          (decomposed é, e + U+0301)

The two café lines look identical and are different byte sequences. They must
not be silently treated as the same path.

NUL must be rejected by the resolver with the resolver's own named reason —
not by leaking an operating-system error message. Per brief §8.6, both layers
are tested.

**Limitation of this section, recorded 12 Sep 2026.** The three NUL lines
cannot be delivered through the `resolve` harness at all. A command-line
argument is terminated by a NUL byte by definition, so any attempt to pass one
stops at the NUL — the shell, not the resolver, is what refuses. These lines
are therefore verified only by the Rust test suite
(`tests/resolver_tests.rs::section_f_unicode_and_lookalikes` and
`::section_h_nasty_combinations`), which calls the resolver directly and can
hold a NUL in a string. That is a real gap in the hands-on pass and is stated
here rather than left implicit.

## G. Length and shape

    /aaaa…                   (a single component of 300 characters)
    /a/a/a/…                 (200 nested components)
    ""                       (empty string)
    " "                      (a single space)
    "/   "                   (root plus trailing spaces)
    /home/documents.
    /home/documents...
    "/home/documents "       (trailing space)
    "/home/ documents"       (leading space)

**Corrected 12 Sep 2026.** A bare `/` (environment root) was originally listed
here as a REJECT case as well as in section I as a must-ACCEPT. That is a
direct contradiction, and section I is correct — resolving the root is not an
escape, and rejecting it would be a bug. The line has been removed from this
section; `/` is tested in section I.

Trailing dots and spaces are legal Linux filenames and illegal on Windows.
They are on the list because software that has ever touched Windows tends to
strip them, and stripping changes which file you get.

Very long paths reject cleanly with a reason. A crash is a failure.

**Two lines in this section change under Phase 1b — added 13 Sep 2026.**

**The 200-nested-components line inverts to ACCEPT.** It rejects today only
because the path is absent. Its components are short, and the 255-byte limit is
per component, not per path, so a long path of short names is legal. Under 1b
it must resolve.

**The 300-character component keeps rejecting, but for a different reason.**
Today the filesystem raises ENAMETOOLONG and the resolver reports it. Under 1b
the path is never handed to the filesystem, so the resolver measures the length
itself — 300 bytes is over `NAME_MAX` (255) and it rejects on its own
authority. Same verdict, different source. **This distinction is the whole
point of the decision:** an implementation that never measures anything passes a
test built on a real 256-byte file, because the OS refuses it regardless. The
1b tests therefore use *absent* paths, and require the resolver's own reason
rather than a leaked OS error — the same standard §F sets for NUL.

Full reasoning in `DECISIONS.md`, "Phase 1b — three resolver rulings". The
length cases themselves live in `attack-list-1b.md` §J.

## H. Nasty combinations

    /link-to-etc/../../..
    /home/../link-to-root/home/muffin
    /home/<NUL>/../..
    //../..
    /home/documents/../../../
    /./link-to-parent/..

## I. Must be ACCEPTED

A resolver that rejects everything passes sections A–H perfectly. These
prove it is secure rather than merely broken.

    /
    /home
    /home/documents
    /home/documents/notes.txt
    /home/documents/
    /home/./documents
    /home/subdir/../documents
    /home/a/b/../../documents
    /café/notes.txt
    /home/my documents/file.txt
    /home/documents/file.name.with.dots.txt
    /home/documents/-leading-dash.txt
    /home/.hidden
    /home/.hidden/inner.txt
    /внутри/файл.txt
    /日本語/ファイル.txt
    /link-to-inside          symlink pointing to another place inside root
    /link-to-inside/file.txt
    //                       normalises to `/` (moved here from section E)
    ///home///documents       normalises to `/home/documents` (from section E)
    /home/documents//        trailing separator (moved here from section E)
    /home/./documents/.      `.` components are elided (moved from section E)
    /home/documents/..       resolves to `/home`, inside root (from section E)

`/home/subdir/../documents` is the important accept: it uses `..` and stays
inside. A resolver that rejects every `..` is too strict and agents will break
on ordinary paths.

---

## Fixtures

Every path in section I, and every symlink target in section D, must exist on disk before testing. Phase 1 does not resolve non-existent paths — that is Phase 1b — so a section I path that has not been created will reject correctly and look like a failure.

The demo mode builds its own fixtures. For hand-testing with resolve mode, the fixture set must be created first. Generating that setup is part of Phase 1 delivery: it creates every directory, file and symlink referenced in sections D and I, inside a throwaway environment root, and prints the root path.

## How to read a result

**ACCEPT** — prints the real resolved path. Confirm it sits under the
environment root. An accept pointing outside is the worst possible outcome and
the reason this list exists.

**REJECT** — prints a reason. Confirm the reason describes this specific path,
not a generic failure.

**Anything else** — a hang, a crash, a raw operating-system error, a stack
trace — is a failure even when nothing escaped. The resolver decides; it does
not fall over.

---

## Independence

BUILD-PLAN requires the attack list come from outside the head that built the
resolver: "The user writes the attack list. Not the agent."

This list does not satisfy that. Claude reviews the delegation brief and will
review the resolver, so it shares some assumptions with the implementation.

**Required before Phase 1 sign-off:** a second attack list produced by a
subagent that has seen neither the resolver source, the delegation brief, nor
this file — given only a description of what the sandbox is meant to guarantee.
Anything it produces that this list missed is genuine independent signal, and
must be run before the phase is accepted.

**Done 11 Sep 2026** (`deleg_c6716023`, model kimi-k3). The list is stored at
`blind-attack-list.md`; it was previously only in `/tmp` and a delegation cache
file, both outside the project. Every novel line was run against the resolver
with its fixtures rebuilt — zero escapes, zero hangs. Its own "most decisive
battery" (symlinks and prefix-siblings) held, and those cases were re-run
independently on 12 Sep 2026 with the same result.

The user does not review code and is not expected to author path-traversal
attacks. The two-list mechanism replaces that requirement rather than waiving
it.
