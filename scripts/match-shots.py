#!/usr/bin/env python3
"""Screenshots of a real match, for the batched checks (.github/workflows/batch.yml).

    match-shots.py PATH-TO-GRUBCRAFT OUT-DIR [FRAMES] [EVERY-SECONDS]

Plays a battle against a General through scripts/smoke-run.py (so it fails
just as the smoke run does: a panic, a GPU error, the memory budget) and,
while the game runs, saves the screen to OUT-DIR/shot-NN.png every
EVERY-SECONDS (default 45) with ImageMagick's `import`. Run it under a
virtual screen: xvfb-run -a -s "-screen 0 1280x720x24" python3 ...
A software GPU draws about a frame a second, so the default 400 frames
stay well inside the smoke run's fifteen-minute limit.
"""
import pathlib
import subprocess
import sys

args = sys.argv[1:]
if not 2 <= len(args) <= 4 or not all(a.isdigit() for a in args[2:]):
    sys.exit(__doc__)
binary, out = args[0], pathlib.Path(args[1])
frames = args[2] if len(args) > 2 else "400"
every = int(args[3]) if len(args) > 3 else 45
out.mkdir(parents=True, exist_ok=True)

smoke = pathlib.Path(__file__).with_name("smoke-run.py")
game = subprocess.Popen([sys.executable, str(smoke), binary, frames, "general"])
shot = 0
while True:
    try:
        game.wait(timeout=every)
        break
    except subprocess.TimeoutExpired:
        shot += 1
        path = out / f"shot-{shot:02}.png"
        try:
            subprocess.run(["import", "-window", "root", str(path)], check=False, timeout=30)
        except subprocess.TimeoutExpired:
            print(f"screenshot {shot} timed out")
print(f"{shot} screenshots in {out}")
sys.exit(game.returncode)
