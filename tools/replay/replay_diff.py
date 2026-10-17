"""Differential replay: the Python and Rust session scanners side by side (plan §5.2 item 4).

Read-only over ~/.claude/projects. For each session with subagents, every file is truncated at
past instants T (herdr-panorama's replay oracle method and its own ``Truth.truncate``; copies
under ``--scratch``), with each copy's mtime set back to what it was at T. Both scanners then
read the same copy with ``now = T`` and ``since = T - 3600 - 600``:

- Python: herdr-panorama's ``scan.session.Session`` (``update(since)``, ``status_of``,
  ``parent_of``), with process liveness unknown;
- Rust: ``examples/replay_scan.rs`` (``Scanner::update(since)``, ``status_of`` with ``NoProcs``,
  ``parent_of``).

Panorama's hook records of the session are included, cut at T, for both sides.

Only counts and disagreements are printed: session and agent ids, field names, status codes,
parents and times. Never transcript content, prompts or paths. Exit status 1 when any field
disagrees (present, status, parent, inferred, ended), else 0.

    <panorama>/.venv/bin/python tools/replay/replay_diff.py --panorama <panorama checkout> \\
        --rust <target>/release/examples/replay_scan --scratch <dir> [--instants 24] \\
        [--newest N] [prefix ...]
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import random
import shutil
import subprocess
import sys

WINDOW = 3600.0
FIELDS = ("present", "status", "parent", "inferred", "ended")


def parse_args(argv):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--panorama", required=True, help="a herdr-panorama checkout (its src/ and tools/)")
    ap.add_argument("--rust", required=True, help="the built replay_scan example")
    ap.add_argument("--scratch", required=True, help="where the truncated copies go (wiped per instant)")
    ap.add_argument("--projects", default=os.path.join(os.path.expanduser("~"), ".claude", "projects"))
    ap.add_argument("--hooks-dir", default=None, help="panorama's hook records (default: its own default)")
    ap.add_argument("--instants", type=int, default=24, help="K spread instants plus K gap instants per session")
    ap.add_argument(
        "--boundary-agents",
        type=int,
        default=4,
        help="agents per session given instants at the timing boundaries (0: none)",
    )
    ap.add_argument("--newest", type=int, default=0, help="only the N newest sessions with subagents (0: all)")
    ap.add_argument("prefixes", nargs="*", help="session id prefixes to add")
    return ap.parse_args(argv)


def hook_lines(path, T):
    """The hook record lines written at or before T."""
    keep = []
    if not os.path.exists(path):
        return keep
    with open(path, "rb") as fh:
        for raw in fh:
            if not raw.endswith(b"\n"):
                break
            try:
                ts = json.loads(raw).get("ts")
            except (ValueError, AttributeError):
                continue
            if isinstance(ts, (int, float)) and not isinstance(ts, bool) and ts <= T:
                keep.append(raw)
    return keep


def set_mtimes(truth, dest, T):
    """A copy's mtime is what the file's was at T: a transcript's last kept record time, any
    other file's own mtime capped at T. The scanners' ``since`` skip and the workflow summary
    evidence read mtimes."""
    top = os.path.dirname(truth.root)
    last_kept = {}
    for path, ls in truth.lines.items():
        kept = [t for _, t, _ in ls if t is not None and t <= T]
        last_kept[os.path.join(dest, os.path.relpath(path, top))] = kept[-1] if kept else None
    for dirpath, _, names in os.walk(dest):
        for name in names:
            p = os.path.join(dirpath, name)
            src = os.path.join(top, os.path.relpath(p, dest))
            try:
                orig = os.path.getmtime(src)
            except OSError:
                orig = T
            m = last_kept.get(p) or min(orig, T)
            os.utime(p, (m, m))


def boundary_instants(truth, rng, n, limits):
    """Instants 1 s either side of each timing limit (ACTIVE, ASYNC_QUIET, PENDING_CAP) after
    a write, for up to ``n`` agents (seeded). A rule whose limit moves flips the status at these
    instants. Half the agents have a *silent* write: no write, notice or other terminal evidence
    of theirs follows it within the largest limit but one (``ASYNC_QUIET``), so the silence
    rules decide there; their base is such a write. For the rest, the base is the last write and
    one random write."""
    quiet = sorted(limits)[-2]

    def silent(a):
        w = truth.writes[a]
        after = sorted(w + truth.terminal.get(a, []) + truth.notices.get(a, []))
        return [x for x in w if not any(x < y <= x + quiet + 1 for y in after)]

    agents = sorted(a for a, w in truth.writes.items() if w)
    with_silent = [a for a in agents if silent(a)]
    picked = [(a, rng.choice(silent(a))) for a in rng.sample(with_silent, min(n // 2, len(with_silent)))]
    rest = [a for a in agents if a not in {p for p, _ in picked}]
    for a in rng.sample(rest, min(n - len(picked), len(rest))):
        picked += [(a, truth.writes[a][-1]), (a, rng.choice(truth.writes[a]))]
    return [base + lim + d for _, base in picked for lim in limits for d in (-1.0, 1.0)]


def python_side(session_mod, sid, root, hooks, since, now):
    s = session_mod.Session(sid, root)
    s.hook_file = session_mod.FileState(hooks, "hooks")
    s.update(since=since)
    out = {}
    for a in s.agents:
        status, ended, inferred = s.status_of(a, now)
        out[a] = {"status": status, "ended": ended, "inferred": bool(inferred), "parent": s.parent_of(a)}
    return out


def rust_side(proc, sid, root, hooks, since, now):
    req = {"id": sid, "root": root, "hooks": hooks if os.path.exists(hooks) else None, "since": since, "now": now}
    proc.stdin.write(json.dumps(req) + "\n")
    proc.stdin.flush()
    reply = json.loads(proc.stdout.readline())
    if "error" in reply:
        raise RuntimeError(f"rust side: {reply['error']}")
    return reply["agents"]


def same(field, a, b):
    if field == "ended" and a is not None and b is not None:
        return abs(a - b) < 1e-3
    return a == b


def main(argv=None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    panorama = os.path.abspath(args.panorama)
    sys.path.insert(0, os.path.join(panorama, "src"))
    sys.path.insert(0, os.path.join(panorama, "tools"))
    if args.hooks_dir:
        os.environ["PANORAMA_HOOKS_DIR"] = args.hooks_dir
    import replay_oracle  # herdr-panorama's tools/replay_oracle.py: Truth, truncate
    from herdr_panorama import hooks as hookmod
    from herdr_panorama.scan import session as session_mod

    # a past instant's processes cannot be checked: unknown on both sides (Rust: NoProcs)
    hookmod.process_alive = lambda *a, **k: None

    with_subs = [
        r for r in glob.glob(os.path.join(args.projects, "*", "*.jsonl")) if os.path.isdir(r[:-6] + "/subagents")
    ]
    roots = sorted(with_subs, key=os.path.getmtime, reverse=True)
    if args.newest:
        roots = roots[: args.newest]
    for p in args.prefixes:
        roots += [r for r in glob.glob(os.path.join(args.projects, "*", f"{p}*.jsonl")) if r not in roots]

    scratch = os.path.abspath(args.scratch)
    dest = os.path.join(scratch, "copy")
    proc = subprocess.Popen([args.rust], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    limits = (session_mod.ACTIVE, session_mod.ASYNC_QUIET, session_mod.PENDING_CAP)
    total = {
        "sessions": 0,
        "instants": 0,
        "boundary_instants": 0,
        "hook_instants": 0,
        "agent_instants": 0,
        "not_done": 0,
        **{f"diff_{f}": 0 for f in FIELDS},
        "statuses": {},
    }
    try:
        for root in roots:
            truth = replay_oracle.Truth(root)
            lo, hi = truth.span()
            if lo is None or hi - lo < 60:
                continue
            sid = os.path.basename(root)[:-6]
            c = dict.fromkeys(total, 0)
            c["statuses"] = {}
            c["sessions"] = 1
            rng = random.Random(root)  # seeded: the same instants every run (of an ended session)
            k = args.instants
            writes = [x for w in truth.writes.values() for x in w]
            gaps = [rng.choice(writes) + rng.uniform(130, 1800) for _ in range(k)] if writes else []
            instants = [lo + (hi - lo) * i / (k + 1) for i in range(1, k + 1)] + sorted(gaps) + [hi, hi + WINDOW]
            boundary = boundary_instants(truth, rng, args.boundary_agents, limits)
            c["boundary_instants"] = len(boundary)
            instants += boundary
            hooks_src = str(hookmod.hooks_path(sid))
            for T in instants:
                truth.truncate(T, dest)
                set_mtimes(truth, dest, T)
                hooks = os.path.join(scratch, "hooks.jsonl")
                if os.path.exists(hooks):
                    os.remove(hooks)
                kept = hook_lines(hooks_src, T)
                if kept:
                    c["hook_instants"] += 1
                    with open(hooks, "wb") as fh:
                        fh.write(b"".join(kept))
                copy_root = os.path.join(dest, os.path.basename(root))
                since = T - WINDOW - 600
                py = python_side(session_mod, sid, copy_root, hooks, since, T)
                rs = rust_side(proc, sid, copy_root, hooks, since, T)
                c["instants"] += 1
                for a in sorted(set(py) | set(rs)):
                    c["agent_instants"] += 1
                    p, r = py.get(a), rs.get(a)
                    if p is None or r is None:
                        c["diff_present"] += 1
                        print(f"  {sid[:8]} T+{T - lo:.0f}s agent={a} present py={p is not None} rs={r is not None}")
                        continue
                    c["statuses"][p["status"]] = c["statuses"].get(p["status"], 0) + 1
                    c["not_done"] += p["status"] != "done"
                    for f in FIELDS[1:]:
                        if not same(f, p[f], r[f]):
                            c[f"diff_{f}"] += 1
                            print(
                                f"  {sid[:8]} T+{T - lo:.0f}s agent={a} {f} py={p[f]} rs={r[f]}"
                                + (f" (status py={p['status']} rs={r['status']})" if f != "status" else "")
                            )
            shutil.rmtree(dest, ignore_errors=True)
            print(sid, json.dumps(c))
            for key, n in c.items():
                if key == "statuses":
                    for st, m in n.items():
                        total[key][st] = total[key].get(st, 0) + m
                else:
                    total[key] += n
    finally:
        proc.stdin.close()
        proc.wait()
        shutil.rmtree(dest, ignore_errors=True)
        try:
            os.remove(os.path.join(scratch, "hooks.jsonl"))
        except OSError:
            pass
    print("TOTAL", json.dumps(total))
    if total["hook_instants"] == 0:
        print(
            "LIMITS no hook records at any instant: the hook rules (stop, start reopen, resumed_after, lost) "
            "were not compared; they are covered by the kit's unit tests (tests/agent_session_claude_fold.rs)"
        )
    print("LIMITS process liveness unknown on both sides: 'lost' and live-task 'bg' are not compared")
    return 1 if any(total[f"diff_{f}"] for f in FIELDS) else 0


if __name__ == "__main__":
    sys.exit(main())
