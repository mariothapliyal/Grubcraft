#!/usr/bin/env python3
"""Take the next free rules numbers for a branch, after merging main.

Simulation pull requests each bump SIM_VERSION, re-bless the golden replays
and re-pin their table, and some also bump the replay FORMAT or the network
PROTOCOL. Two branches that both take the same number used to conflict in
every golden on whichever merged second. This does the whole catch-up in one
command (replays/README.md, "Taking the next rules version"):

    python3 scripts/rules-bump.py            catch up with main, keep the bumps
    python3 scripts/rules-bump.py --bump     also take a SIM_VERSION when the
                                             branch shows none yet (after a
                                             conflicted merge you resolved)
    python3 scripts/rules-bump.py --take FORMAT,PROTOCOL
                                             also take those numbers
    python3 scripts/rules-bump.py --keep replays/field-1v1.gcr
                                             a golden re-recorded on this
                                             branch: keep its orders (main's
                                             are kept otherwise)
    python3 scripts/rules-bump.py --check BASE HEAD
                                             CI: fail if HEAD bumps a number
                                             that BASE already has (or
                                             passed), or makes goldens play
                                             differently without a bump

Catching up: (1) the numbers, the PINNED rows, the goldens and the bench
baseline the branch changed are put back to the merge base in one commit, so
merging main can't conflict on them; (2) main is merged (merge, never rebase);
(3) each number the branch bumped becomes main's + 1, the goldens are
blessed and pinned, the goldens test runs, and the bench baseline is
re-measured when the branch had changed it; (4) one commit. Push it yourself.
"""
import argparse
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# The numbers that are taken in merge order: name -> (file, pattern). Each
# declaration says beside it that this script reads it in this exact form.
NUMBERS = {
    "SIM_VERSION": ("crates/gc_sim/src/lib.rs", r"(pub const SIM_VERSION: u32 = )(\d+)(;)"),
    "FORMAT": ("crates/gc_replay/src/lib.rs", r"(pub const FORMAT: u16 = )(\d+)(;)"),
    "PROTOCOL": ("crates/gc_net/src/proto.rs", r"(pub const PROTOCOL: u8 = )(\d+)(;)"),
}
GOLDENS_TEST = "crates/gc_replay/tests/goldens.rs"
# From `const PINNED` to the first `\n);` after it, as `gc_replay pin` finds it.
PINNED = re.compile(r"const PINNED.*?\n\);", re.S)
BASELINE = "tools/gc_bench/baseline.json"


def git(*args, check=True):
    r = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True)
    if check and r.returncode != 0:
        sys.exit(f"git {' '.join(args)} failed:\n{r.stdout}{r.stderr}")
    return r


def show(rev, path):
    r = git("show", f"{rev}:{path}", check=False)
    return r.stdout if r.returncode == 0 else None


def number(text, name):
    m = re.search(NUMBERS[name][1], text)
    if not m:
        sys.exit(f"{name} not found in {NUMBERS[name][0]}")
    return int(m.group(2))


def number_at(rev, name):
    return number(show(rev, NUMBERS[name][0]), name)


def set_number(name, value):
    path = ROOT / NUMBERS[name][0]
    text = path.read_text()
    path.write_text(re.sub(NUMBERS[name][1], lambda m: f"{m.group(1)}{value}{m.group(3)}", text, count=1))


def changed(rev_a, rev_b, *paths, kinds="ACDMR"):
    out = git("diff", "--name-only", f"--diff-filter={kinds}", rev_a, rev_b, "--", *paths).stdout
    return [p for p in out.splitlines() if p]


def goldens_changed(rev_a, rev_b):
    """Goldens at both revisions whose files differ."""
    return [p for p in changed(rev_a, rev_b, "replays", kinds="M") if p.endswith(".gcr")]


def dirty():
    return bool(git("status", "--porcelain", "--untracked-files=no").stdout.strip())


def run(*cmd):
    print("$", " ".join(cmd), flush=True)
    if subprocess.run(cmd, cwd=ROOT).returncode != 0:
        sys.exit(f"{' '.join(cmd)} failed")


def pins(rev):
    """The PINNED table's fingerprints at `rev`, by golden."""
    return dict(re.findall(r'\("([^"]+)", (0x[0-9a-f]+)\)', PINNED.search(show(rev, GOLDENS_TEST)).group(0)))


def pins_moved(rev_a, rev_b):
    """Goldens pinned at both revisions whose fingerprints differ: they play
    differently, which needs a new SIM_VERSION. (Added goldens, and goldens
    only re-saved in a new FORMAT, don't.)"""
    a, b = pins(rev_a), pins(rev_b)
    return sorted(n for n in a.keys() & b.keys() if a[n] != b[n])


def check(base, head):
    """CI mode: every number HEAD bumps must be ahead of BASE's."""
    mb = git("merge-base", base, head).stdout.strip()
    taken, unbumped = [], []
    for name in NUMBERS:
        at_mb, at_head, at_base = (number_at(r, name) for r in (mb, head, base))
        print(f"{name}: merge base {at_mb}, this branch {at_head}, main {at_base}")
        if at_head != at_mb and at_head <= at_base:
            taken.append(f"{name} is {at_head} here, but main is already at {at_base}")
    moved = pins_moved(mb, head)
    if moved and number_at(head, "SIM_VERSION") == number_at(mb, "SIM_VERSION"):
        unbumped.append(f"{', '.join(moved)} play differently, but SIM_VERSION wasn't bumped")
    for line in taken + unbumped:
        print(line)
    if taken:
        sys.exit(
            "Another pull request took these numbers first. Merge main and take the next ones:\n"
            "    python3 scripts/rules-bump.py\n"
            "then push (replays/README.md, 'Taking the next rules version')."
        )
    if unbumped:
        sys.exit("Take the next SIM_VERSION: python3 scripts/rules-bump.py --bump")
    print("ok: no number clashes with main")


def unmerged():
    return git("diff", "--name-only", "--diff-filter=U").stdout.split()


def bump(base, take, keep, rebench):
    if dirty():
        sys.exit("commit or stash your changes first")
    stray = git("ls-files", "--others", "--exclude-standard", "replays").stdout.split()
    if any(p.endswith(".gcr") for p in stray):
        sys.exit(f"untracked replays would be blessed and pinned as goldens: {stray}; move them out")
    for state in ("MERGE_HEAD", "rebase-merge", "rebase-apply"):
        if (ROOT / git("rev-parse", "--git-path", state).stdout.strip()).exists():
            sys.exit(f"a merge or rebase is in progress ({state}): finish it, then run this again")
    remote, _, branch = base.partition("/")
    if branch and remote in git("remote").stdout.split():
        git("fetch", remote, branch)
    mb = git("merge-base", "HEAD", base).stdout.strip()

    take = set(take)
    take |= {n for n in NUMBERS if number_at("HEAD", n) != number_at(mb, n)}
    if pins_moved(mb, "HEAD"):
        take.add("SIM_VERSION")
    rebench = rebench or bool(changed(mb, "HEAD", BASELINE))

    # 1. Put the branch's numbers, pins, goldens and baseline back to the
    # merge base, so main's come in without a conflict. Goldens the branch
    # added stay (they don't exist on main), and so do goldens named with
    # --keep (re-recorded on this branch); all are blessed again below.
    for name in take:
        set_number(name, number_at(mb, name))
    test = ROOT / GOLDENS_TEST
    old = PINNED.search(show(mb, GOLDENS_TEST)).group(0)
    repin = PINNED.search(test.read_text()).group(0) != old
    test.write_text(PINNED.sub(lambda _: old, test.read_text(), count=1))
    reverted = [p for p in goldens_changed(mb, "HEAD") if p not in keep]
    reverted += changed(mb, "HEAD", BASELINE, kinds="M")
    if reverted:
        git("checkout", mb, "--", *reverted)
    unbumped = dirty()
    if unbumped:
        git("commit", "-aqm", "Put the rules numbers back to the merge base before merging main\n\n"
            "Made by scripts/rules-bump.py; the next commit takes the next free ones.")

    # 2. Merge main. Kept goldens that main changed too keep this branch's
    # recording.
    r = git("merge", "--no-edit", base, check=False)
    if r.returncode != 0:
        mine = [p for p in unmerged() if p in keep]
        if mine:
            git("checkout", "--ours", "--", *mine)
            git("add", "--", *mine)
        if unmerged():
            again = "python3 scripts/rules-bump.py"
            again += " --bump" if "SIM_VERSION" in take else ""
            names = ",".join(sorted(take - {"SIM_VERSION"}))
            again += f" --take {names}" if names else ""
            again += "".join(f" --keep {k}" for k in sorted(keep))
            again += " --rebench" if rebench else ""
            sys.exit(
                f"{r.stdout}{r.stderr}\nMerging {base} conflicts in code (not in the rules numbers). "
                f"Resolve it, `git commit`, then run:\n    {again}"
                + ("\n(Or give up for now: `git merge --abort && git reset --hard HEAD~1` puts "
                   "the branch back as it was.)" if unbumped else "")
            )
        git("commit", "--no-edit", "-q")

    # 3. Take main's + 1 for each number this branch bumps, bless and pin
    # the goldens, and re-measure the baseline.
    taken = {name: number_at(base, name) + 1 for name in sorted(take)}
    for name, value in taken.items():
        set_number(name, value)
    if taken or reverted or keep or repin:
        run("cargo", "run", "--release", "-q", "-p", "gc_replay", "--", "bless", "replays")
        run("cargo", "run", "--release", "-q", "-p", "gc_replay", "--", "pin")
        run("cargo", "test", "--release", "-q", "-p", "gc_replay", "--test", "goldens")
    if rebench:
        if shutil.which("valgrind"):
            run(sys.executable, "scripts/bench-gate.py", "bless")
        else:
            print(f"WARNING: this branch changes {BASELINE}, and valgrind isn't here to "
                  "re-measure it: run `scripts/bench-gate.py bless` on Linux before you push")
    if not dirty():
        print(f"merged {base}; this branch bumps none of {', '.join(NUMBERS)}")
        return

    # 4. One commit.
    git("add", "-u")
    git("add", "replays", GOLDENS_TEST)
    summary = ", ".join(f"{n} {v}" for n, v in taken.items()) or "main's numbers"
    git("commit", "-qm", f"Take {summary} (after merging main)\n\n"
        "Made by scripts/rules-bump.py: main's numbers + 1, goldens re-blessed and pinned.")
    print(f"\ntook {summary}; committed. Next:")
    if taken:
        print("  - check the docs this branch adds still give the right numbers:")
    for name, v in taken.items():
        print(f"      git grep -n '{name}' -- docs '*.md'   (now {v})")
    if "FORMAT" in taken:
        print("  - FORMAT moved: check any `format >=` tests and new replay tags you added")
    if "PROTOCOL" in taken:
        print("  - PROTOCOL moved: check any new message tags you added against main's")
    print("  - git push")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--base", default="origin/main", help="the branch to catch up with (default origin/main)")
    p.add_argument("--bump", action="store_true", help="take a SIM_VERSION even if the branch shows none")
    p.add_argument("--take", default="", help="also take these numbers, comma separated (FORMAT,PROTOCOL)")
    p.add_argument("--keep", action="append", default=[], metavar="GOLDEN",
                   help="a golden re-recorded on this branch: keep its orders over main's (repeatable)")
    p.add_argument("--rebench", action="store_true", help="re-measure the bench baseline")
    p.add_argument("--check", nargs=2, metavar=("BASE", "HEAD"), help="CI: check HEAD's numbers against BASE")
    a = p.parse_args()
    if a.check:
        return check(*a.check)
    take = [n for n in a.take.split(",") if n] + (["SIM_VERSION"] if a.bump else [])
    unknown = [n for n in take if n not in NUMBERS]
    if unknown:
        sys.exit(f"unknown numbers {unknown}; known: {', '.join(NUMBERS)}")
    keep = {k if k.startswith("replays/") else f"replays/{k}" for k in a.keep}
    bump(a.base, take, keep, a.rebench)


if __name__ == "__main__":
    main()
