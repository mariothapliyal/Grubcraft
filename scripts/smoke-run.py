#!/usr/bin/env python3
"""Smoke run of the real game binary, for CI (docs/10-architecture.md, Testing).

    smoke-run.py PATH-TO-GRUBCRAFT [FRAMES] [menu|recruit|veteran|general|warlord]

Starts the game straight into a battle against the computer, draws FRAMES
frames (default 300) on whatever software GPU the machine has, and quits. It
fails if the game panics, exits with an error, logs a lost GPU, takes longer
than fifteen minutes, or ends using more than the 6 GB memory budget. It also
fails if the GPU driver reports any error: a single bad pipeline throws away
every frame and leaves the window black (the October 2026 Mac black screen).
With "menu" it opens the main menu instead, with every effect on, as a player
first sees the game. With a computer player's level it plays against that
level instead of a Recruit (scripts/match-shots.py plays a General). The game's
settings, log and crash reports go in a temporary folder.
"""
import os
import pathlib
import re
import subprocess
import sys
import tempfile

MEMORY_BUDGET_MB = 6144
TIME_LIMIT_S = 900


def main():
    args = sys.argv[1:]
    mode = "recruit"
    if args[-1:] and args[-1] in ("menu", "recruit", "veteran", "general", "warlord"):
        mode = args.pop()
    menu = mode == "menu"
    if not 1 <= len(args) <= 2 or not all(a.isdigit() for a in args[1:]):
        sys.exit(__doc__)
    binary = pathlib.Path(args[0]).resolve()
    frames = args[1] if len(args) == 2 else "300"
    app_dir = pathlib.Path(tempfile.mkdtemp(prefix="grubcraft-smoke-"))
    env = dict(os.environ, GRUBCRAFT_SETTINGS=str(app_dir / "settings.ron"))
    # Shadows and glow are left out only because a software GPU draws them
    # very slowly; everything else is what a player runs.
    cmd = [str(binary), "--frames", frames, "--no-audio"]
    if menu:
        cmd += ["--menu"]
    else:
        cmd += ["--battle", mode, "--no-shadows", "--no-glow"]
    try:
        run = subprocess.run(
            cmd, env=env, capture_output=True, text=True, timeout=TIME_LIMIT_S
        )
    except subprocess.TimeoutExpired as e:
        print(e.stdout or "")
        sys.exit(f"FAIL: still running after {TIME_LIMIT_S} s")
    print(run.stdout)
    print(run.stderr, file=sys.stderr)

    problems = []
    if run.returncode != 0:
        problems.append(f"exit code {run.returncode}")
    log = app_dir / "logs" / "grubcraft.log"
    log_text = log.read_text(errors="replace") if log.exists() else ""
    for word in ("PANIC", "GPU device lost", "GPU error"):
        if word in log_text or word in run.stderr:
            problems.append(f"the log says {word!r}")
    crashes = list((app_dir / "crashes").glob("*")) if (app_dir / "crashes").exists() else []
    if crashes:
        problems.append(f"crash report written: {crashes[0].name}")
    memory = re.search(r"([\d.]+) MB process", run.stdout)
    if memory is None:
        problems.append("no memory figure in the game's final line (did it draw its frames?)")
    elif float(memory.group(1)) > MEMORY_BUDGET_MB:
        problems.append(f"{memory.group(1)} MB used, budget {MEMORY_BUDGET_MB} MB")
    if problems:
        print(f"FAIL: {'; '.join(problems)}")
        if log_text:
            print(f"--- {log} ---\n{log_text}")
        sys.exit(1)
    print(f"ok: {frames} frames, {memory.group(1)} MB process (budget {MEMORY_BUDGET_MB} MB)")


main()
