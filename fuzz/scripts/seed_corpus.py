#!/usr/bin/env python3
"""Build the committed seed corpus for the `resolve` fuzz target.

One file per distinct input, written to `fuzz/seeds/resolve/`.

**Why this is Python and not a shell loop.** Bash's `read` cannot carry a NUL
byte, so the obvious shell version silently drops exactly the seeds that cover
the resolver's named NUL rejection -- the check that runs before any filesystem
call. The first version of this script did that, and the NUL seeds vanished with
no error. Python reads and writes bytes directly, so nothing is lost.

**Where the seeds come from**, both extracted rather than copied by hand so the
corpus cannot drift away from the things it is meant to seed from:

1. The `/...` string literals in the resolver's own tests
   (`crates/resolver/tests/`) and its demo cases (`src/main.rs`).
2. The path arguments of `crates/resolver/hand-test-1b.sh` -- the hands-on attack
   list the phase gates are signed off with.

Those literals are **Rust source**, so they carry Rust escapes and must be
decoded: a literal two-character `\\0` and a real NUL byte are different inputs,
and they take different paths through `resolve()`. They are decoded here the way
Rust decodes them.

**And a third source, added deliberately:** a block of seeds naming this target's
own fixture tree. The attack lists name *their* fixtures (`link-to-etc`,
`chain-a`, `q-back`); this target builds its own (`link-to-etc` too, but also
`link-rel-out-2`, `dangle-sibling`, `q-anc`), and without a few seeds the fuzzer
starts blind to the links it is supposed to be exercising.

`--check` verifies the committed corpus is still there and still big enough, which
is what CI runs. A seed corpus that has been silently emptied would make every run
slower for weeks without anything failing.
"""

import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
RESOLVER = os.path.join(REPO, "crates", "resolver")
OUT = os.path.join(REPO, "fuzz", "seeds", "resolve")

# Rust string escapes, decoded the way Rust decodes them.
SIMPLE_ESCAPES = {
    "0": 0,
    "n": 10,
    "r": 13,
    "t": 9,
    "\\": 92,
    '"': 34,
    "'": 39,
}

# Seeds naming this target's fixture tree. See the module docstring.
FIXTURE_SEEDS = [
    # Ordinary paths inside, existing and absent.
    "/",
    "/home",
    "/home/documents",
    "/home/documents/notes.txt",
    "/home/documents/sub/inner.txt",
    "/home/documents/absent.txt",
    "/afile",
    "/good-dir",
    "/good-dir/inner.txt",
    "/café/notes.txt",
    "/日本語",
    # The rest of the outside content, reachable only through a link that has
    # already left -- naming it directly is not possible, which is the point.
    "/link-out/keep.txt",
    "/.hidden/inner.txt",
    "/deep/deep/deep/leaf.txt",
    "/nope/../home/documents/notes.txt",
    # Links that stay inside.
    "/link-to-inside",
    "/link-to-inside/notes.txt",
    "/rel-inside",
    "/rel-inside/../home/documents",
    "/rel-inside/../../home",
    "/rel-empty",
    "/rel-empty/..",
    "/link-to-file",
    "/link-to-file/..",
    # Links that leave.
    "/link-to-etc",
    "/link-to-etc/passwd",
    "/link-to-etc/../home/documents",
    "/link-to-home",
    "/link-to-root",
    "/link-to-parent",
    "/link-to-parent/root",
    "/link-out",
    "/link-out/secret.txt",
    "/link-rel-out",
    "/link-rel-out/secret.txt",
    "/link-rel-out-2",
    "/link-to-etc-file",
    "/link-to-proc-self",
    # Dangling links, both sides of the boundary.
    "/dangle-inside",
    "/dangle-inside/..",
    "/dangle-rel",
    "/dangle-out",
    "/dangle-etc",
    "/dangle-sibling",
    # A loop.
    "/loop-a",
    "/loop-b",
    "/loop-a/x",
    "/loop-self",
    # Chains.
    "/chain-1",
    "/chain-1/passwd",
    # The intermediate links as well as the outermost one: a seed naming the
    # inner hop starts the fuzzer already knowing that hop exists.
    "/chain-2",
    "/chain-end-out",
    "/chain-in-1",
    "/chain-in-1/notes.txt",
    "/chain-in-2",
    "/nope/../chain-1",
    # Out and back, and the ancestor landing.
    "/q-out",
    "/q-back",
    "/q-back/home/documents/notes.txt",
    "/nope/../q-back",
    "/q-mid",
    "/q-mid/notes.txt",
    "/q-mid-2",
    "/q-out/relay",
    "/q-anc",
    "/q-anc/root/home/documents",
    "/q-sib",
    "/q-sib/secret.txt",
    # The same shapes crossed with traversal and absent prefixes.
    "/chain-1/../chain-in-1/notes.txt",
    "/./link-to-etc",
    "//link-to-etc",
    "/../../etc/passwd",
    "/../..",
    "/a/b/c/../../../../link-to-etc",
    "/nope/deeper/../../../link-to-etc",
]


def decode_rust_escapes(raw: str) -> bytes:
    """Decode Rust string escapes in a test literal into the bytes it denotes."""
    out = bytearray()
    i = 0
    while i < len(raw):
        c = raw[i]
        if c != "\\" or i + 1 >= len(raw):
            out.extend(c.encode())
            i += 1
            continue
        nxt = raw[i + 1]
        if nxt == "u":
            m = re.match(r"u\{([0-9a-fA-F]+)\}", raw[i + 1 :])
            if m:
                out.extend(chr(int(m.group(1), 16)).encode())
                i += 1 + m.end()
                continue
            out.extend(nxt.encode())
            i += 2
            continue
        if nxt in SIMPLE_ESCAPES:
            out.append(SIMPLE_ESCAPES[nxt])
            i += 2
            continue
        # An escape Rust does not define. Left as written rather than guessed at.
        out.extend(nxt.encode())
        i += 2
    return bytes(out)


def grep(pattern: str, *paths: str) -> list:
    """Lines matching `pattern` across `paths`, as bytes, with the file name kept."""
    res = subprocess.run(
        ["grep", "-rhoE", pattern, *paths], capture_output=True, check=False
    )
    return [line for line in res.stdout.split(b"\n") if line]


def collected() -> list:
    """Every seed: test literals, attack-list paths, and the fixture seeds."""
    found = []

    # 1. Test and demo literals: "/something".
    for raw in grep(
        '"/[^"]*"',
        os.path.join(RESOLVER, "tests") + "/",
        os.path.join(RESOLVER, "src", "main.rs"),
    ):
        text = raw.decode("utf-8", "surrogateescape")
        if text.startswith('"') and text.endswith('"'):
            found.append(decode_rust_escapes(text[1:-1]))

    # 2. Attack-list lines: run <section> <A|R> '<path>'.
    pat = "^run [A-Z0-9.]+ [AR] '([^']*)'"
    res = subprocess.run(
        ["grep", "-hoE", pat, os.path.join(RESOLVER, "hand-test-1b.sh")],
        capture_output=True,
        check=False,
    )
    for line in res.stdout.split(b"\n"):
        if not line:
            continue
        text = line.decode("utf-8", "surrogateescape")
        m = re.match(pat + r"$", text)
        if m:
            found.append(decode_rust_escapes(m.group(1)))

    # 3. This target's own fixture tree.
    found.extend(s.encode() for s in FIXTURE_SEEDS)

    return found


def write_corpus() -> int:
    entries = sorted({e for e in collected() if e})
    os.makedirs(OUT, exist_ok=True)
    for i, body in enumerate(entries):
        with open(os.path.join(OUT, "seed-%d" % i), "wb") as fh:
            fh.write(body)
    return len(entries)


def main() -> int:
    if "--check" in sys.argv:
        have = len(os.listdir(OUT)) if os.path.isdir(OUT) else 0
        print("corpus files present: %d" % have)
        # An emptied seed corpus is an error, not a warning: the fuzzer would
        # still run, just worse, and nothing would say so.
        if have < 200:
            print(
                "seed corpus is suspiciously small (%d files); expected at least 200"
                % have,
                file=sys.stderr,
            )
            return 1
        return 0

    n = write_corpus()
    print("corpus files: %d" % n)
    print("wrote to: %s" % OUT)
    return 0


if __name__ == "__main__":
    sys.exit(main())
