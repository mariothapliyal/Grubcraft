# TEMPORARY: patches cs_pose for one crash-hunt variant. Removed before merge.
import sys, re
v = sys.argv[1].rstrip("2")
p = "crates/gc_render/src/shaders/world.wgsl"
if v == "x":  # main's posing (depth loop), only the early returns moved
    import subprocess
    subprocess.run(["git", "checkout", "origin/main", "--", p, "crates/gc_render/src/soldiers.rs"], check=True)
    s = open(p, encoding="utf-8").read()
    def sub(a, b):
        global s
        assert a in s, a
        s = s.replace(a, b, 1)
    sub("""    if k >= jobs.count.w {
        return;
    }
""", "    let live = k < jobs.count.w;\n")
    sub("    let fit = job.pose.z;\n", "    let fit = select(0u, job.pose.z, live);\n    let fitted = fit != 0u;\n")
    sub("""    if fit == 0u {
        if bone < bones {
            pose_unfitted(i, slot, bone);
        }
        return;
    }
""", "")
    sub("    let deepest = head.z;", "    let deepest = select(0u, head.z, fitted);")
    sub("""    var local = mat4x4<f32>();
    if bone < joints {""", """    var local = mat4x4<f32>();
    if fitted && bone < joints {""")
    sub("        if bone < joints && depth == d {", "        if fitted && bone < joints && depth == d {")
    sub("    let rigged = feet > 0.0 || look > 0.0;", "    let rigged = fitted && (feet > 0.0 || look > 0.0);")
    sub("""    workgroupBarrier();
    if bone > joints || bone >= bones {""", """    workgroupBarrier();
    if !fitted {
        if live && bone < bones {
            pose_unfitted(i, slot, bone);
        }
        return;
    }
    if bone > joints || bone >= bones {""")
    open(p, "w", encoding="utf-8").write(s)
    print("variant x applied")
    raise SystemExit(0)
if v == "y":  # the branch as it is
    print("variant y applied")
    raise SystemExit(0)
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
elif v in ("e", "f"):  # no early returns before the barriers (f: only the unfitted one)
    unfit = """    if fit == 0u {
        if bone < bones {
            pose_unfitted(i, slot, bone);
        }
        return;
    }
"""
    assert unfit in body
    body = body.replace(unfit, "")
    if v == "e":
        ret = """    if k >= jobs.count.w {
        return;
    }
"""
        assert ret in body
        body = body.replace(ret, "")
elif v == "g":  # no job search loop
    assert "jobs.job[find_job(k, true)]" in body
    body = body.replace("jobs.job[find_job(k, true)]", "jobs.job[0u]")
elif v == "h":  # no layer loop
    a = body.index("        for (var l = 1u; l <= POSE_LAYERS; l++) {")
    b = body.index("        local = pose_matrix(p);")
    body = body[:a] + body[b:]
else:
    raise SystemExit("unknown variant " + v)
s = s[:start] + body + s[end:]
open(p, "w", encoding="utf-8").write(s)
print("variant", v, "applied")
