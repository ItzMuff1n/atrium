# Attack list — Phase 2a: file operations

**Written 13 Sep 2026, before the code exists.** That ordering is the point:
this file says what the operations must do. If it is written after the code, it
becomes a description of the code instead of a test of it. Phase 1b was built
this way and the ordering caught a real gap.

**Authority:** `BUILD-PLAN.md` PHASE 2, §2a — "Create, read, write, move,
delete, list — all routed through Phase 1's resolver." `DESIGN.md` §3.1 (the
environment is closed) and §3.2 (the resolver is the only way a virtual path
becomes a real one) are the behaviour source of truth.

**This is a working copy.** If it and `DESIGN.md` disagree, stop and say so —
do not pick one silently (`AGENT-RULES.md` §2).

---

## What is being tested, in plain words

Phase 1 built the thing that decides **whether** a path is allowed — inside the
sandbox or not. Phase 2a builds the things that **do work** at that path:
make a folder, write a file, read it, move it, delete it, list a folder.

The risk moves, and this is the whole reason the list exists. Phase 1 could only
be wrong by *answering* wrongly. Phase 2a can be wrong by **acting** on the
wrong thing — deleting a real file on your computer because a path was read
differently than the checker read it.

So every line below is one of three questions:

1. **Does a refusal actually refuse?** Nothing happens anywhere.
2. **Does a success affect only the inside path?** It worked, and the thing it
   worked on is the copy inside the sandbox, not the real thing outside.
3. **After the operation, is everything outside the root byte-for-byte
   untouched?** This is checked by the harness itself, not by the program's
   own report — see §J.

**Every path in this list is a *virtual* path. A leading `/` means the
environment root, not the host root.** `/etc/passwd` means
`<root>/etc/passwd`. This was decided for Phase 1 and confirmed by Muffin; it
is why §B exists and why several lines in it read as catastrophic and are not.

---

## How to read a result

Each line has one of these requirements:

- **MUST REFUSE** — the operation must not happen. The program must say why, and
  the reason must name the specific thing that was wrong (the `..` step, the
  symlink, the missing parent) — not a bare "denied" or a raw operating-system
  error code with no explanation.
- **MUST DO IT INSIDE** — the operation succeeds, and it affects the path inside
  the sandbox only.
- **MUST NOT EXIST** — a refusal whose whole purpose is to prove nothing was
  created. Checked by looking on disk, not by trusting the message.

Two independent checks run alongside every line, and either one failing fails the
run regardless of what the program printed:

- **`!! OUTSIDE TOUCHED`** — the harness has a real directory outside the root
  holding a sentinel file. It records that directory's full contents and the
  sentinel's checksum before the run, and re-checks after every line. Any change
  is reported as an escape. This is the check that matters most, because it does
  not depend on the program telling the truth about itself.
- **`!! ROOT GONE` / `!! UNEXPECTED`** — the root itself, or a file the line was
  not supposed to touch, changed.

A line reading `ok` and a line reading `FAIL` differ by one printed word, so a
run cannot be checked by eye for correctness — only for the absence of that
word. Reading the script is what establishes that the right lines ran. Same
caveat as Phase 1 and 1b; stated again because it keeps being worth stating.

---

## The one mechanism these operations all rest on

**Every argument that is a virtual path goes through `resolve()` before anything
touches the disk.** No operation may construct a real path by any other means
(`AGENT-RULES.md` §6, `DESIGN.md` §3.2). There is no exception for convenience,
for a special case, or temporarily.

**`resolve()` canonicalises symlinks.** For a path that exists, it returns the
*real target* — so a symlink is transparent to it. That single fact drives
several expected outcomes below, including the ones in §K, which are a
**recorded limitation rather than a correct behaviour**. They are in this list
so the limitation is visible and watched, not so it looks intentional.

**Inside a recursive delete, descent never follows a symlink.** Links are
removed *as links*. This is the only way a recursive delete can be safe, and it
is tested directly in §E. Note the asymmetry, which is deliberate and must be
stated rather than discovered: the *entry-point* path is resolved (so its
symlinks are followed), but *descent* during the walk is not.

**All output names virtual paths.** The agent never learns the real path exists
(`DESIGN.md` §3.1). Nothing an operation prints may contain the root's location
or the location of the outside test directory — §H.

---

## Fixtures

The harness builds a throwaway root, and separately a throwaway directory
**outside** it. Nothing is created inside the project, and both are deleted at
the end.

```
<root>/
  home/documents/            a directory
      notes.txt              a file, contents known
      subdir/                an empty directory
  etc/                       (created by §B, not pre-made)
  outside-link        -> <outside>/            absolute, lands outside the root
  inside-link         -> <root>/home/documents absolute, lands inside
  rel-outside-link    -> ../../outside-dir     relative, climbs out
  rel-inside-link     -> home/documents        relative, stays in
  chain-a             -> chain-b
  chain-b             -> <outside>/            a two-hop route out
  dangling-out        -> <outside>/absent.txt  target absent and outside
  dangling-in         -> <root>/home/documents/absent.txt   absent and inside
  dangling-in2        -> <root>/home/documents/absent2.txt  absent and inside,
                        a second one so C.13's creation does not collide with
                        E.8's expectation that absent.txt is still absent
  trap/               -> a directory containing:
        keepme.txt           an ordinary file
        escape      -> <outside>/            the outside link, *inside* a tree
  loopdir/            -> contains loop-a -> loop-b and loop-b -> loop-a
  loop-a, loop-b      -> each other
```

Outside the root:

```
<outside>/
  sentinel.txt         contents known, checksummed
  sub/keep.txt         nested, so a recursive mistake is caught
```

---

## A. Traversal in the path argument — MUST REFUSE

The classic. The path climbs above the root on its own, with no symlink
involved.

| # | operation | expected |
|---|---|---|
| A.1 | `write /home/../../escaped.txt --content x` | MUST REFUSE; MUST NOT EXIST anywhere |
| A.2 | `create-dir /../escaped-dir` | MUST REFUSE |
| A.3 | `delete /home/documents/../../escaped.txt --recursive` | MUST REFUSE |
| A.4 | `read /../../etc/passwd` | MUST REFUSE |
| A.5 | `list /home/../..` | MUST REFUSE |
| A.6 | `move /home/documents/notes.txt /home/../../moved.txt` | MUST REFUSE; `notes.txt` still present |
| A.7 | `delete /.. --recursive` | MUST REFUSE |
| A.8 | `create-dir /..../deep` | MUST DO IT INSIDE — `....` is a *name*, not traversal. Four dots is a legal filename. `<root>/..../deep` exists afterwards. |

A.8 matters: it is the line that catches an implementation which "cleans up"
dots with a string replace instead of walking components.

---

## B. Absolute paths that look like host paths — MUST DO IT INSIDE

These read as catastrophic by hand and are not. A leading `/` is the environment
root. The host file of the same name must be untouched, and that is checked by
checksum, not by assumption.

| # | operation | expected |
|---|---|---|
| B.1 | `create-dir /etc/passwd` | MUST DO IT INSIDE — creates the *directory* `<root>/etc/passwd`. The real `/etc/passwd` is a real file on this host; its checksum and its modification time must both be unchanged after the run. |
| B.2 | `write /etc/passwd/x --content hi` | MUST DO IT INSIDE — `<root>/etc/passwd/x` exists with content `hi`. Real `/etc/passwd` still unchanged. |
| B.3 | `write /etc/shadow --content hi` | MUST DO IT INSIDE **or** MUST REFUSE — but never MUST FAIL BECAUSE OF PERMISSIONS. If the refusal reason is a permission error from the host, the operation was aimed at the host and that is a failure of the line. |
| B.4 | `delete /etc/passwd --recursive` | MUST DO IT INSIDE — removes `<root>/etc/passwd` only. Real `/etc/passwd` checksum unchanged. |
| B.5 | `create-dir /root/secret` | MUST DO IT INSIDE — `<root>/root/secret`; the host's real `/root` untouched (it is not readable by this user, so a permission error here is also a failure). |
| B.6 | `write /usr/bin/evil --content x` | MUST DO IT INSIDE or MUST REFUSE, never a host permission error. |

B.3, B.5 and B.6 are the informative ones: they cannot be satisfied by luck.
Either the operation aimed inside, or it aimed at the host and was saved only by
the operating system refusing. The distinction is in the *reason*, which is why
the reason must be readable and specific.

---

## C. Symlink routes out — MUST REFUSE for every operation

An existing symlink whose target is outside the root. Every operation offered by
Phase 2a is tried through it, because each one is a different code path and a
single check in one of them proves nothing about the others.

| # | operation | expected |
|---|---|---|
| C.1 | `write /outside-link/created.txt --content x` | MUST REFUSE; `<outside>/created.txt` MUST NOT EXIST |
| C.2 | `create-dir /outside-link/newdir` | MUST REFUSE; nothing new under `<outside>` |
| C.3 | `read /outside-link/sentinel.txt` | MUST REFUSE — reading outside is as bad as writing outside; the sentinel's contents must not appear in the output |
| C.4 | `list /outside-link` | MUST REFUSE |
| C.5 | `delete /outside-link/sentinel.txt` | MUST REFUSE; the sentinel must still exist with the same checksum |
| C.6 | `move /home/documents/notes.txt /outside-link/moved.txt` | MUST REFUSE; `notes.txt` still inside |
| C.7 | `write /chain-a/created.txt --content x` | MUST REFUSE — a two-hop chain out is still out |
| C.8 | `create-dir /chain-a/newdir` | MUST REFUSE |
| C.9 | `write /rel-outside-link/created.txt --content x` | MUST REFUSE — a *relative* target is resolved against the link's own directory, so the escape never appears in the requested path |
| C.10 | `list /rel-outside-link` | MUST REFUSE |
| C.11 | `write /dangling-out/newfile --content x` | MUST REFUSE — the target is absent *and outside*. Absence must not launder the escape. |
| C.12 | `write /dangling-in/newfile --content x` | MUST REFUSE, and for the ordinary reason — here the absent target is the **parent** of what is being written, and write-file never creates parents (§F.2). The reason must name the missing parent, and must **not** be a symlink-escape refusal: the link is inside the root and is not the problem. |
| C.13 | `write /dangling-in --content x` | MUST DO IT INSIDE — this creates the absent target *through* the dangling link (`<root>/home/documents/absent.txt`). This is the case Phase 1b exists for: nothing can ever be created through such a link otherwise. |

C.11 against C.13 is the pair that matters. A resolver that treats "absent" as
"no opinion" everywhere accepts both; a resolver that treats "outside" as
deciding everywhere rejects both. Only the pair together pins the boundary.

C.12 pins a second, independent boundary: the absent target *as a parent
directory* is refused by the ordinary parent rule, not by a symlink rule —
because the link is inside the root and leading to somewhere legal.

**Correction, 14 Sep 2026.** C.12 originally read exactly as it does now, and
C.13 did not exist — C.12 was written as `MUST DO IT INSIDE` on the strength of
Phase 1b accepting absent-inside paths. That was **my error**: the absent target
is that path's *parent*, and write-file does not create parents, so the line as
first written contradicted §F.2. It was split into C.12 (the refusal) and C.13
(the creation) so both are tested separately and neither is satisfied by
accident. Recorded here rather than silently fixed, because the attack list is
the phase's spec and a spec that changes without a trace is worthless.

---

## D. Move — both ends are paths, so both are attacked

| # | operation | expected |
|---|---|---|
| D.1 | `move /outside-link/sentinel.txt /home/documents/copied.txt` | MUST REFUSE — the *source* is outside |
| D.2 | `move /home/documents/notes.txt /home/../../out.txt` | MUST REFUSE |
| D.3 | `move /home/documents/notes.txt /outside-link/dest.txt` | MUST REFUSE |
| D.4 | `move /home/../../x /home/documents/y` | MUST REFUSE |
| D.5 | `move / /home/movedroot` | MUST REFUSE — moving the root is refused outright |
| D.6 | `move /home/documents /outside-link/sub` | MUST REFUSE |

D.1 is the one people miss: it is natural to check the destination and forget the
source. The source is a virtual path like any other and is resolved like any
other.

---

## E. Delete — the most dangerous operation in the phase

Delete is the only one where a mistake destroys something irreversibly, and it is
the only one that walks a tree. Five distinct things are being tested: an
entry-point escape, descent that must not follow links, loops, the non-empty
rule, and the root itself.

| # | operation | expected |
|---|---|---|
| E.1 | `delete /trap --recursive` | MUST DO IT INSIDE. `<root>/trap` and everything under it is gone — **including the `escape` link** — and `<outside>/sentinel.txt` and `<outside>/sub/keep.txt` MUST both still exist with unchanged checksums. This is the single most important line in the file: a recursive delete that follows links destroys the host. |
| E.2 | `delete /loopdir --recursive` | MUST DO IT INSIDE and MUST TERMINATE. A recursive walk that follows links hangs forever here. The links are removed as links; neither is followed. |
| E.3 | `delete /home/documents --recursive` | MUST REFUSE — the directory is not empty. The reason must say so specifically, and must not be a bare failure. |
| E.4 | `delete /home/documents/subdir --recursive` | MUST DO IT INSIDE — an empty directory is removable |
| E.5 | `delete /home/documents/subdir` (no flag) | MUST DO IT INSIDE — an empty directory is removable without the flag |
| E.6 | `delete /` | MUST REFUSE — the root itself is never deleted, whatever flags are given |
| E.7 | `delete /home/documents/notes.txt` | MUST DO IT INSIDE — an ordinary file |
| E.8 | `delete /home/documents/absent.txt` | MUST REFUSE — it does not exist |
| E.9 | `delete /outside-link` | MUST REFUSE — **recorded limitation, §K** |
| E.10 | `delete /../../../etc/passwd` | MUST REFUSE |

E.3 and E.5 together pin the non-empty rule in both directions: the same
directory, refused without the flag when it has contents, accepted without the
flag when it is empty.

---

## F. Create and write — the ordinary mistakes

| # | operation | expected |
|---|---|---|
| F.1 | `write /home/documents/notes.txt --content replaced` | MUST DO IT INSIDE — overwrites, inside. The old contents are gone; this is intended, and the file is inside the sandbox. |
| F.2 | `write /home/documents/no-such-dir/x --content y` | MUST REFUSE — the parent does not exist. The reason must name the missing parent. Writing must **not** quietly create parent directories; `create-dir` is what does that. |
| F.3 | `create-dir /home/newdir/deep/deeper` | MUST DO IT INSIDE — creates intermediate directories |
| F.4 | `write /home/documents --content x` | MUST REFUSE — that path is a directory |
| F.5 | `create-dir /home/documents/notes.txt` | MUST REFUSE — that path is an existing file |
| F.6 | `create-dir /home/documents` | MUST DO IT INSIDE — already exists and is a directory; this is not an error |
| F.7 | `write /home/link-to-file/x --content y` | MUST REFUSE — an intermediate component is a file, not a directory |

F.2 against F.3 is deliberate: writing does not invent parents, and making
directories does. Two different behaviours, both tested, neither left to
assumption.

---

## G. Reading and listing the wrong kind of thing

| # | operation | expected |
|---|---|---|
| G.1 | `read /home/documents` | MUST REFUSE — that is a directory. The reason must say so, not fail obscurely. |
| G.2 | `list /home/documents/notes.txt` | MUST REFUSE — that is not a directory |
| G.3 | `read /home/documents/absent.txt` | MUST REFUSE — it does not exist |
| G.4 | `list /home/documents` | MUST DO IT INSIDE — and the entries must be listed in a stable order (by name), because a listing whose order changes between runs cannot be verified by eye |
| G.5 | `read /inside-link/notes.txt` | MUST DO IT INSIDE — a symlink pointing inside is transparent, and the file it points at is inside |

---

## H. Host-path disclosure — nothing outside is ever named

`DESIGN.md` §3.1: the agent never learns the real path exists.

| # | check | expected |
|---|---|---|
| H.1 | the output of every refusing line in §A–§G | names the **virtual** path that was given, and the specific step that failed |
| H.2 | the same output | MUST NOT contain the location of the outside test directory |
| H.3 | the same output | MUST NOT name a real host path as something that was operated on |
| H.4 | the output of every **succeeding** line | reports the **virtual** path, not the resolved one. A success that prints `/tmp/atrium-2a-root/home/work/a.txt` has just told the agent where the sandbox is on disk, which `DESIGN.md` §3.1 forbids as much as a refusal doing it. |
| H.5 | every line, both outcomes | the real location is available only behind an explicit debug flag, never in the default output |

H.1 is checked positively — a refusal that does not repeat the path is not
readable and does not meet the project's standard for reasons. H.2 and H.3 are
checked negatively, by searching the output.

**Known, inherited, and not fixed here — and it is larger than Phase 1's.**
Phase 1's resolver reasons describe where a traversal step *reached* (naming a
real host directory) in order to be specific. That phrasing is Phase 1's
signed-off behaviour, and this phase does not change it.

But Phase 1b changed something that matters more here. Observed 13 Sep 2026
against the current binary: `atrium-resolver resolve /link-to-inside` prints
`ACCEPT <root>/home/documents` — **the resolved real path on stdout, every
time**. Phase 1b's CLI prints the real path to prove containment; that is
correct for a testing tool and is not a defect in it. It becomes a defect the
moment Phase 2a's operations inherit that habit, because then the *program*
hands the agent the sandbox's location on disk.

So H.4/H.5 are not a stylistic preference. The operations must deliberately
diverge from the resolver's own CLI output style, and the reason should be
recorded, or the same pattern will be copied forward again. See §L.3.

---

## I. Operations that must work — the boring half

A list of only-refusals can be satisfied by refusing everything. These lines are
the counterweight, and they are the ones Muffin is actually judging: does the
sandbox do its job.

| # | operation | expected |
|---|---|---|
| I.1 | `create-dir /home/work` | succeeds; directory exists |
| I.2 | `write /home/work/a.txt --content alpha` | succeeds; exactly the bytes `alpha`, length 5 |
| I.3 | `write /home/work/b.txt --content beta` | succeeds |
| I.4 | `read /home/work/a.txt` | returns exactly `alpha`, and reports length 5 |
| I.5 | `list /home/work` | exactly two entries, `a.txt` then `b.txt`, each identified as a file |
| I.6 | `move /home/work/a.txt /home/work/c.txt` | succeeds; `c.txt` exists with contents `alpha`; `a.txt` does not exist |
| I.7 | `delete /home/work/c.txt` | succeeds; gone |
| I.8 | `delete /home/work --recursive` | succeeds; gone, and `<root>` still exists |
| I.9 | `list /` | succeeds; lists what is actually at the root, in stable order |
| I.10 | `read /home/documents/notes.txt` | returns the fixture file's exact contents |

I.8's second half matters: deleting the last thing in a tree must not take the
environment with it.

---

## J. The independent containment check

Checked by the harness, in a way that does not depend on anything the program
says about itself:

| # | check | expected |
|---|---|---|
| J.1 | full listing of `<outside>`, and the checksum of `sentinel.txt`, recorded before the run and compared after every line | **byte-identical** at the end of the run |
| J.2 | checksum and modification time of the real host `/etc/passwd`, recorded before and after | **unchanged** |
| J.3 | the root itself | still exists at the end of every line except where a line is defined to remove part of it; never removed by an operation that did not name it |
| J.4 | after every MUST NOT EXIST line | the specific file is absent on disk — checked by looking, not by reading the message |
| J.5 | after every MUST DO IT INSIDE line that creates something | the created path exists **inside** the root, checked at its real location — so a line that "succeeded" in the wrong place is caught |

J.1 is the escape detector. J.5 is the mirror image of it and is easy to omit:
without J.5, a program that reports success while doing nothing passes every
refusal test.

---

## K. The symlink-object limitation — recorded, not asserted as correct
`resolve()` canonicalises an existing path **including its final component**, so
it returns a symlink's *target* rather than the link. Observed 13 Sep 2026
against the current binary, on links built for this purpose:

```
resolve /s-to-dir        -> ACCEPT <root>/home/documents     (the target, not the link)
resolve /s-to-file       -> ACCEPT <root>/home/documents/notes.txt
resolve /s-to-subdir     -> ACCEPT <root>/home/documents/subdir
resolve /s-out           -> REJECT  symlink `/s-out` points outside the environment root
resolve /s-dangling      -> ACCEPT <root>/home/absent        (target absent, inside)
resolve /home/documents/rel-notes -> ACCEPT <root>/home/documents/notes.txt
```

The consequence is that **a symlink cannot be operated on as an object**:
whatever you aim at the link, lands on its target.

| # | operation | expected today | what is actually true |
|---|---|---|---|
| K.1 | `delete /outside-link` | REFUSES | The link itself is inside the root and harmless, but resolving it lands outside, so the operation is refused. Fail-closed: safe, and it means such a link can never be removed. |
| K.2 | `delete /inside-link` | removes `<root>/home/documents` and **leaves `/inside-link` in place, now dangling** | The link was inside, so no escape — but the thing deleted is not the thing named. |
| K.3 | `move /inside-link /home/elsewhere` | moves the *target*, leaving the link dangling | Same shape as K.2 |

**These three are a limitation, not a design decision.** They are safe in the
sense that nothing outside the root is ever touched, and they are wrong in the
sense that the operation does not do what the path says. K.2 and K.3 in
particular are exactly the class of bug `DESIGN.md` §6 warns about — an
operation that reports something confidently false.

They are in the list because they must be seen, and because the fix is not
obviously this phase's to make: operating on a link as an object needs a way to
resolve a path *without* following the last component, which means changing
Phase 1's verified resolver — a `stop and ask` item under `AGENT-RULES.md` §5,
not something 2a may do quietly.

**Decision needed soon, not now:** before the explorer (Phase 5+) or the watcher
(2d) depends on links, decide whether the resolver grows a non-following variant
(its own phase, its own attack list) or whether link handling is defined as
"links are always followed, and a link whose target is outside cannot be
addressed at all". §L carries this as an open item.

**One thing §K does *not* leave open, and it must be settled in this phase:** a
recursive delete's *descent* must never follow a symlink (§E.1, §E.2). The
entry-point resolution above is Phase 1's behaviour and is not 2a's to change;
descent is 2a's own code and is entirely 2a's responsibility.

---

## N. Hard links — a hole in this list, found by the blind list

**This section did not exist when the phase was built.** The first list above
missed hard links entirely; the independent blind list found them
(`blind-attack-list-2a.md` Group C, cases 15–17), and they were run against the
built code. Recorded here as a finding of the independence gate.

**What was observed, 14 Sep 2026.** A hard link is a second *name* for one file.
It is not a path escape — there is no `..`, no symlink, nothing for a path check
to see. Observed on this host:

```
ln <outside>/secret.txt <root>/hardlink-out    # same filesystem: allowed
atrium-fileops run --root <root> read /hardlink-out
  -> OK read /hardlink-out length=15
     SECRET-OUTSIDE                            # the OUTSIDE file's contents

atrium-fileops run --root <root> write /hardlink-out --content OVERWRITTEN
  -> OK write /hardlink-out (22 bytes)
     <outside>/secret.txt is now 22 bytes, contents OVERWRITTEN-BY-SANDBOX
```

So the promise "nothing outside the environment root is ever read / modified" is
**false for hard links**, and no amount of path validation can make it true. The
containment model assumes a file inside the root is a file inside the root; a
hard link breaks that assumption at the inode level, below where paths exist.

**Why this is a deployment constraint, not a 2a bug.** A hard link cannot be
created *across* filesystems — observed here: `ln /home/muffin/.bashrc /tmp/xfslink`
→ `Invalid cross-device link`, because `/tmp` is device 50 and `/home` is device
49. Therefore a hard link from the host into the environment root can only exist
if the host file shares the root's filesystem. **The environment root must be its
own mount point** (its own filesystem/tmpfs) for this class to be impossible. That
is a deployment requirement of the environment, not something the six operations
can enforce.

**What 2a does do correctly, observed:** a hard link *inside* the root, deleted
recursively as part of a tree, leaves the outside file intact — deletion decrements
the link count, it does not destroy the other name.

**Owed consequence.** `DESIGN.md` does not yet say the environment root must be
its own mount. Until it does, a host hard link into the root is an open reading and
writing channel. This is recorded as open item L.4 and belongs in `DESIGN.md` §3.1.

> **Discharged, 14 Sep 2026.** `DESIGN.md` §3.1 now states it: the environment
> root is its own mount point (its own filesystem or a tmpfs), with the
> cross-device reasoning. The same requirement is carried into `BUILD-PLAN.md` 2e,
> which is the phase that has to honour it.

---

## L. Open items this list raises

Written down, not built (`AGENT-RULES.md` §4).

1. **The symlink-object limitation (§K).** Needs a decision before anything
   depends on links. Options: a resolver addition with its own verification, or
   an explicit, documented "links are transparent" rule.
2. **A dependency edge for the new crate.** The file operations must use Phase
   1's resolver, so the new crate depends on it. This is a *path* dependency on
   code already in this project — no external crate is pulled in, and no
   third-party code enters the build. Recorded because `AGENT-RULES.md` §5
   treats adding a dependency as a stop-and-ask, and the honest position is that
   this is not what that rule is guarding — but saying so explicitly is better
   than deciding it silently.
3. **Where host-path-free error text is enforced.** `DESIGN.md` §3.1 says the
   agent never learns the real path exists; Phase 1's rejection reasons are
   specific enough to name where a traversal reached, and Phase 1b's CLI prints
   the resolved real path on every accept. Something has to own that translation
   before an agent sees these messages. Not this phase's to solve — but 2a must
   not make it worse, which is why §H.4/H.5 require virtual paths in 2a's own
   output. Phase 5 owns the fix; 2a owns not adding to the pile.

   **Resolved in 2a, 14 Sep 2026, and this was a real defect, not theory.** The
   first build passed the resolver's `Display` text straight through, so
   thirteen refusals named the symlink's real on-disk target — the sandbox's
   location — while every success was clean. Observed: `write /outside-link/…`
   → "its target is `/tmp/atrium-2a-outside`". The operations now re-word
   `SymlinkEscapes`, `TraversalAboveRoot` and `EscapesRoot` from the resolver's
   STRUCTURED fields, naming the virtual path and dropping the real one; the
   harness's §H check is unconditional (no exempt line). It needed **no resolver
   change**. Standing consequence: if the resolver ever grows a reason that names
   a real path in a new variant, that match must be updated — noted in
   `src/lib.rs` beside it.
4. **The environment root must be its own mount point.** §N: a hard link from the
   host into the root reads and writes the host file, invisibly to every path
   check. Cross-filesystem hard links are impossible (observed: EXDEV), so this
   is closed by the root being its own filesystem/tmpfs — a deployment fact that
   belongs in `DESIGN.md` §3.1 and is not yet stated there. Until it is, this
   remains an open reading/writing channel.

---

## Independence

This list was written by the same head that wrote the brief and will review the
build, so it shares assumptions with them. Phase 1 and Phase 1b both required a
**second, blind list** from a subagent that has seen none of the source, the
brief, or this file — and both times it found lines the first list missed. The
same gate applies here: `blind-attack-list-2a.md`, produced before the phase is
accepted, with every novel line run against the built code.

**Discharged, 14 Sep 2026 — and it earned its place.** The blind child returned
36 cases (Group A TOCTOU, B symlink placement, C hard links, D the recursive
delete walk, E move semantics, F encoding, G reporting lies); verbatim copy at
`blind-attack-list-2a.md`, sha256 `d1f1d3952e…28c3d`, one tool call, no file
reads, no source or brief seen. Every mechanism was run against the built code:

- **Hard links (§N) — a real hole in this list.** Read *and write* through a
  hard link into the root reached and modified a file outside it. My first list
  never mentioned hard links. Fixed in the spec: §N records it, and the
  environment-root-must-be-its-own-mount requirement follows from it.
- **Symlink as a middle component pointing out, then `..` back in** (Group B) —
  refused correctly; nothing new, but worth having pinned.
- **TOCTOU races between resolve and use** (Group A) — real in principle, needs a
  concurrent writer, and not reachable through the six operations alone. Grouped
  with §K's link-object limitation as the class a future resolution-hardening
  phase owns. Not silently dropped.
- **Reporting and partial-failure lies** (Group G) — checked; the operations
  report what they did, and the J.4/J.5 mirror checks exist precisely to catch a
  success that did nothing.

The blind list also flagged its own blind spots (it cannot know whether ops
re-validate per component, and hard-link isolation needs the mount fact). Those
limits are recorded in its `notes_on_limits` and are honest, not hand-waved.
