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
    python3 scripts/rules-bump.py --check BASE HEAD
                                             CI: fail if HEAD bumps a number
                                             that BASE already has (or
                                             passed), or moves goldens
                                             without a bump

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


def goldens_moved(rev_a, rev_b, kinds="ACDMR"):
    return [p for p in changed(rev_a, rev_b, "replays", kinds=kinds) if p.endswith(".gcr")]


def dirty():
    return bool(git("status", "--porcelain", "--untracked-files=no").stdout.strip())


def run(*cmd):
    print("$", " ".join(cmd), flush=True)
    if subprocess.run(cmd, cwd=ROOT).returncode != 0:
        sys.exit(f"{' '.join(cmd)} failed")


def check(base, head):
    """CI mode: every number HEAD bumps must be ahead of BASE's."""
    mb = git("merge-base", base, head).stdout.strip()
    bad = []
    for name in NUMBERS:
        at_mb, at_head, at_base = (number_at(r, name) for r in (mb, head, base))
        bumped = at_head != at_mb or (name == "SIM_VERSION" and goldens_moved(mb, head))
        if bumped and at_head <= at_base:
            bad.append(f"{name} is {at_head} here, but main is already at {at_base}")
        print(f"{name}: merge base {at_mb}, this branch {at_head}, main {at_base}")
    if bad:
        print("\n".join(bad))
        sys.exit(
            "Another pull request took these numbers first. Merge main and take the next ones:\n"
            "    python3 scripts/rules-bump.py\n"
            "then push (replays/README.md, 'Taking the next rules version')."
        )
    print("ok: no number clashes with main")


def bump(base, take):
    if dirty():
        sys.exit("commit or stash your changes first")
    for state in ("MERGE_HEAD", "rebase-merge", "rebase-apply"):
        if (ROOT / git("rev-parse", "--git-path", state).stdout.strip()).exists():
            sys.exit(f"a merge or rebase is in progress ({state}): finish it, then run this again")
    remote, _, branch = base.partition("/")
    git("fetch", remote, branch)
    mb = git("merge-base", "HEAD", base).stdout.strip()

    take = set(take)
    take |= {n for n in NUMBERS if number_at("HEAD", n) != number_at(mb, n)}
    if goldens_moved(mb, "HEAD"):
        take.add("SIM_VERSION")
    rebench = bool(changed(mb, "HEAD", BASELINE))

    # 1. Put the branch's numbers, pins, goldens and baseline back to the
    # merge base, so main's come in without a conflict. Goldens the branch
    # added stay (they don't exist on main) and are blessed again below.
    for name in take:
        set_number(name, number_at(mb, name))
    if "SIM_VERSION" in take:
        test = ROOT / GOLDENS_TEST
        old = PINNED.search(show(mb, GOLDENS_TEST)).group(0)
        test.write_text(PINNED.sub(lambda _: old, test.read_text(), count=1))
    existing = goldens_moved(mb, "HEAD", kinds="M") + changed(mb, "HEAD", BASELINE, kinds="M")
    if existing:
        git("checkout", mb, "--", *existing)
    if dirty():
        git("commit", "-aqm", "Put the rules numbers back to the merge base before merging main\n\n"
            "Made by scripts/rules-bump.py; the next commit takes the next free ones.")

    # 2. Merge main.
    r = git("merge", "--no-edit", base, check=False)
    if r.returncode != 0:
        names = ",".join(sorted(take - {"SIM_VERSION"}))
        again = "python3 scripts/rules-bump.py" + (" --bump" if "SIM_VERSION" in take else "")
        again += f" --take {names}" if names else ""
        sys.exit(
            f"{r.stdout}{r.stderr}\nMerging {base} conflicts in code (not in the rules numbers). "
            f"Resolve it, `git commit`, then run:\n    {again}"
        )

    # 3. Take main's + 1 for each number this branch bumps.
    if not take:
        print(f"merged {base}; this branch bumps none of {', '.join(NUMBERS)}")
        return
    taken = {name: number_at(base, name) + 1 for name in sorted(take)}
    for name, value in taken.items():
        set_number(name, value)
    if "SIM_VERSION" in taken:
        run("cargo", "run", "--release", "-q", "-p", "gc_replay", "--", "bless", "replays")
        run("cargo", "run", "--release", "-q", "-p", "gc_replay", "--", "pin")
        run("cargo", "test", "--release", "-q", "-p", "gc_replay", "--test", "goldens")
    if rebench:
        if shutil.which("valgrind"):
            run(sys.executable, "scripts/bench-gate.py", "bless")
        else:
            print(f"WARNING: this branch changes {BASELINE}, and valgrind isn't here to "
                  "re-measure it: run `scripts/bench-gate.py bless` on Linux before you push")

    # 4. One commit.
    git("add", "-u")
    git("add", "replays", GOLDENS_TEST)
    summary = ", ".join(f"{n} {v}" for n, v in taken.items())
    git("commit", "-qm", f"Take {summary} (after merging main)\n\n"
        "Made by scripts/rules-bump.py: main's numbers + 1, goldens re-blessed and pinned.")
    print(f"\ntook {summary}; committed. Next:")
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
    p.add_argument("--check", nargs=2, metavar=("BASE", "HEAD"), help="CI: check HEAD's numbers against BASE")
    a = p.parse_args()
    if a.check:
        return check(*a.check)
    take = [n for n in a.take.split(",") if n] + (["SIM_VERSION"] if a.bump else [])
    unknown = [n for n in take if n not in NUMBERS]
    if unknown:
        sys.exit(f"unknown numbers {unknown}; known: {', '.join(NUMBERS)}")
    bump(a.base, take)


if __name__ == "__main__":
    main()
