# /// script
# requires-python = ">=3.9"
# ///
"""Idle-return gaps from Claude Code transcripts (story 143 task 1).

Usage: uv run scripts/ttl-replay.py <transcripts-root> <out-dir>
Reads only timestamps and message kinds, never text. Writes two new timestamped files:
the sorted gaps in seconds (numbers only) and a report for the table expiry values.
A returning turn is a typed user prompt after the last assistant message of a session;
its gap is the seconds between the two. Tool results are not returns.
"""
import datetime, glob, json, os, sys

TTLS = (300, 600, 3600, 86400)

def ts(s):
    return datetime.datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()

def main(root, out):
    gaps, runs_by_ttl, sessions, endings = [], {t: [] for t in TTLS}, 0, 0
    for path in glob.glob(os.path.join(os.path.expanduser(root), "*", "*.jsonl")):
        last_assistant, first_in_run, any_event = None, None, False
        session_gaps = []
        with open(path) as f:
            for line in f:
                try:
                    d = json.loads(line)
                except ValueError:
                    continue
                t = d.get("timestamp")
                if not t or d.get("type") not in ("user", "assistant") or d.get("isSidechain"):
                    continue
                now = ts(t)
                any_event = True
                if d["type"] == "user":
                    c = d.get("message", {}).get("content")
                    tool = isinstance(c, list) and any(isinstance(x, dict) and x.get("type") == "tool_result" for x in c)
                    if not tool and last_assistant is not None:
                        session_gaps.append((last_assistant, now))
                else:
                    last_assistant = now
        if not any_event:
            continue
        sessions += 1
        endings += 1
        gaps.extend(b - a for a, b in session_gaps)
        for ttl in TTLS:
            # An entry lives from a response end until the next return or ttl after it.
            for a, b in session_gaps:
                runs_by_ttl[ttl].append((a, min(b, a + ttl)))
            if last_assistant is not None:
                runs_by_ttl[ttl].append((last_assistant, last_assistant + ttl))
    gaps.sort()
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    os.makedirs(out, exist_ok=False) if not os.path.isdir(out) else None
    gaps_file = os.path.join(out, f"idle-return-gaps-{stamp}.txt")
    report_file = os.path.join(out, f"ttl-replay-{stamp}.txt")
    for p in (gaps_file, report_file):
        if os.path.exists(p):
            sys.exit(f"{p} exists; nothing is overwritten")
    with open(gaps_file, "x") as g:
        g.write("# seconds between the end of a response and the next typed prompt of the same session, sorted; numbers only\n")
        g.write("\n".join(f"{x:.1f}" for x in gaps) + "\n")
    def q(p):
        return gaps[min(len(gaps) - 1, int(p * len(gaps)))]
    lines = [
        "# Table expiry replay on idle-return gaps. Source: Claude Code transcripts on the owner machine (not harness traffic through the proxy).",
        f"sessions {sessions}  returning turns {len(gaps)}  median {q(.5):.1f} s  p90 {q(.9):.1f} s  p99 {q(.99):.1f} s",
        "ttl_s  returns_found  returns_total  found_percent  entries_expired_unreturned_or_late  peak_concurrent_entries  idle_entry_hours",
    ]
    for ttl in TTLS:
        found = sum(1 for x in gaps if x <= ttl)
        events = []
        idle = 0.0
        for a, b in runs_by_ttl[ttl]:
            events += [(a, 1), (b, -1)]
            idle += b - a
        events.sort(key=lambda e: (e[0], e[1]))
        cur = peak = 0
        for _, d in events:
            cur += d
            peak = max(peak, cur)
        late = len(gaps) - found
        lines.append(f"{ttl}  {found}  {len(gaps)}  {100 * found / len(gaps):.1f}  {late + endings}  {peak}  {idle / 3600:.1f}")
    with open(report_file, "x") as r:
        r.write("\n".join(lines) + "\n")
    print(report_file)
    print("\n".join(lines))

main(sys.argv[1], sys.argv[2])
