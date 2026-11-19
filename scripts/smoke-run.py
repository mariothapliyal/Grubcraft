#!/usr/bin/env python3
"""Smoke run of the real game binary, for CI (docs/10-architecture.md, Testing).

    smoke-run.py PATH-TO-GRUBCRAFT [FRAMES] [menu|recruit|veteran|general|warlord]

Starts the game straight into a battle against the computer, draws FRAMES
frames (default 300) on whatever software GPU the machine has, then plays on
for HIDDEN_FRAMES frames as if its window were hidden, and quits. It fails if
the game panics, exits with an error, logs a lost GPU, takes longer than
fifteen minutes, or peaks above the 6 GB memory budget; and if the hidden
frames grow the process or go by without pausing (docs/12-performance.md,
"Hidden windows"). It also
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
# Frames played as if the window were hidden. Without the fix the process
# grew about 0.3 MB a frame in a battle (185 MB over 600 frames on Linux);
# with it, under 1 MB in all. Growth is judged over the second half: on
# Windows' software GPU the working set is still settling from the drawn
# frames early on.
# SMOKE_HIDDEN_FRAMES=0 leaves them out (scripts/match-shots.py, whose
# screenshots would show a frozen picture).
HIDDEN_FRAMES = int(os.environ.get("SMOKE_HIDDEN_FRAMES", "600"))
HIDDEN_GROWTH_LIMIT_MB = 32  # over the second half
# A hidden frame waits a 60th of a second; well under that is spinning.
HIDDEN_FRAME_MIN_S = 0.012


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
    cmd = [str(binary), "--frames", str(int(frames) + HIDDEN_FRAMES), "--no-audio"]
    if HIDDEN_FRAMES:
        cmd += ["--hidden-frames", str(HIDDEN_FRAMES)]
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
    peak = re.search(r"peak process memory: ([\d.]+) MB", run.stdout)
    used = max(float(m.group(1)) for m in (peak, memory) if m) if memory else None
    if memory is None:
        problems.append("no memory figure in the game's final line (did it draw its frames?)")
    elif used > MEMORY_BUDGET_MB:
        problems.append(f"{used:.1f} MB used at the peak, budget {MEMORY_BUDGET_MB} MB")
    hidden = re.search(
        r"hidden window: (\d+) frames in ([\d.]+) s, process ([\d.?]+) MB before, "
        r"([\d.?]+) MB halfway, ([\d.?]+) MB after",
        run.stdout,
    )
    growth = seconds = 0.0
    if HIDDEN_FRAMES and hidden is None:
        problems.append("no hidden window line (did it play the hidden frames?)")
    elif HIDDEN_FRAMES and "?" in "".join(hidden.groups()[2:]):
        problems.append("no memory figure for the hidden frames")
    elif HIDDEN_FRAMES:
        n, seconds, before, halfway, after = (float(g) for g in hidden.groups())
        growth = after - halfway
        if growth > HIDDEN_GROWTH_LIMIT_MB:
            problems.append(
                f"the last {n / 2:.0f} hidden frames grew the process {growth:.1f} MB "
                f"(limit {HIDDEN_GROWTH_LIMIT_MB} MB): undrawn frames are piling up memory"
            )
        if seconds < n * HIDDEN_FRAME_MIN_S:
            problems.append(
                f"{n:.0f} hidden frames took {seconds:.1f} s: a hidden window is spinning, not pausing"
            )
    if problems:
        print(f"FAIL: {'; '.join(problems)}")
        if log_text:
            print(f"--- {log} ---\n{log_text}")
        sys.exit(1)
    print(
        f"ok: {frames} frames, {used:.1f} MB process at the peak (budget {MEMORY_BUDGET_MB} MB), "
        f"the last {HIDDEN_FRAMES // 2} of {HIDDEN_FRAMES} hidden frames grew it {growth:+.1f} MB "
        f"({seconds:.1f} s in all)"
    )


main()
