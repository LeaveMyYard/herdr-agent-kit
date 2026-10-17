"""Print the markdown method index of a herdr recordings dir."""
import json, sys
from pathlib import Path
d = Path(sys.argv[1])
order = ["meta", "workspace", "tab", "pane", "agent", "api", "plugin", "notification", "machine", "server", "socket", "events", "errors"]
print("| file:seq | name | call | socket methods on the wire | outcome | used by |")
print("|---|---|---|---|---|---|")
for f in order:
    for line in open(d / f"{f}.jsonl"):
        r = json.loads(line)
        if r["kind"] == "cli":
            call = "`" + " ".join((a if len(a) < 40 else "…" + a[-25:]).replace("\n", "\\n") for a in r["argv"][1:]) + "`"
            js = r.get("stdout_json") or {}
            if r["exit_code"] == 0:
                res = js.get("result") if isinstance(js, dict) else None
                out = "ok" + (f" `{res.get('type')}`" if isinstance(res, dict) and res.get("type") else "")
            else:
                err = None
                try:
                    err = json.loads(r["stderr"]).get("error", {}).get("code")
                except (ValueError, AttributeError):
                    pass
                out = f"exit {r['exit_code']}" + (f" `{err}`" if err else "")
        else:
            m = r["request"]["method"] if isinstance(r["request"], dict) and "method" in r["request"] else "(raw)"
            call = f"socket `{m}`"
            rep = r["replies"][0] if r["replies"] else {}
            out = f"error `{rep['error']['code']}`" if "error" in rep else (
                "ok" + (f" `{(rep.get('result') or {}).get('type')}`" if (rep.get("result") or {}).get("type") else ""))
            if len(r["replies"]) > 1:
                out += f" + {len(r['replies']) - 1} events"
        methods = []
        for fr in r.get("socket", []):
            if fr["dir"] == "request" and isinstance(fr.get("frame"), dict) and fr["frame"].get("method"):
                if fr["frame"]["method"] not in methods:
                    methods.append(fr["frame"]["method"])
        print(f"| {f}:{r['seq']} | {r['name']} | {call} | {', '.join(methods)} | {out} | {r.get('used_by', '')} |")
