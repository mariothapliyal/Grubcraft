# TEMPORARY: patches cs_pose for one crash-hunt variant. Removed before merge.
import sys, re
v = sys.argv[1]
p = "crates/gc_render/src/shaders/world.wgsl"
s = open(p, encoding="utf-8").read()
start = s.index("fn cs_pose(")
end = s.index("// Pose one vertex of one soldier", start)
body = s[start:end]
loop = """    for (var n = 1u; n < POSE_GROUP && up != ROOT_PARENT; n++) {
        chain = pose_globals[up] * chain;
        up = pose_parents[up];
    }
"""
assert loop in body
if v == "a":  # no chain walk at all
    body = body.replace(loop, "")
elif v == "b":  # no barriers in cs_pose
    body = body.replace("    workgroupBarrier();\n", "")
elif v == "c":  # chain walk unrolled by hand: no loop
    step = """    if up != ROOT_PARENT {
        chain = pose_globals[up] * chain;
        up = pose_parents[up];
    }
"""
    body = body.replace(loop, step * 12)
elif v == "d":  # no rig fit (and its barrier)
    old = """    if rigged && bone == 0u {
        fit_rig(i, rig, feet, look);
    }
    workgroupBarrier();
"""
    assert old in body
    body = body.replace(old, "")
else:
    raise SystemExit("unknown variant " + v)
s = s[:start] + body + s[end:]
open(p, "w", encoding="utf-8").write(s)
print("variant", v, "applied")
