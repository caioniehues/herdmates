#!/usr/bin/env python3
# PROTOTYPE — THROWAWAY. Answers issue #7: what should the native board pane
# show and how should it be laid out. Three structurally different variants,
# switch with <- / -> (or a/d), q quits. Reads REAL run dirs read-only.
# Dependencies between tasks are MOCKED (no task model yet) and marked [MOCK].
import json, os, sys, termios, tty, glob, time, re

STATE = os.path.expanduser("~/.local/state/herdr/plugins/caioniehues.agent-team/runs")
BOLD, DIM, RST = "\033[1m", "\033[2m", "\033[0m"
FG = {"green": "\033[32m", "yellow": "\033[33m", "red": "\033[31m",
      "cyan": "\033[36m", "mag": "\033[35m", "blue": "\033[34m", "white": "\033[37m"}
GLYPH = {"running": ("●", "green"), "ended": ("○", "white"), "released": ("◇", "cyan"),
         "orphaned": ("✖", "red"), "failed": ("✖", "red"), "pending": ("◌", "yellow")}

def parse_toml_lite(path):
    # cheap line-parser, enough for run.toml shapes we own (PROTOTYPE ONLY)
    runs, cur, out = None, None, {"workers": {}, "top": {}}
    section = None
    for line in open(path):
        line = line.strip()
        if not line or line.startswith("#"): continue
        m = re.match(r"\[workers\.(.+)\]", line)
        if m: section = ("w", m.group(1)); out["workers"][m.group(1)] = {}; continue
        if line.startswith("["): section = ("o", line); continue
        if "=" in line:
            k, v = [x.strip() for x in line.split("=", 1)]
            v = v.strip('"')
            if section and section[0] == "w": out["workers"][section[1]][k] = v
            elif section is None or section[1] in ("[spec]",): out["top"][k] = v
            if section and section[0] == "o": out["top"].setdefault(k, v)
    return out

def load_runs():
    rs = []
    for d in sorted(glob.glob(STATE + "/*/"), key=os.path.getmtime, reverse=True):
        rt = os.path.join(d, "run.toml")
        if not os.path.exists(rt): continue
        t = parse_toml_lite(rt)
        name = t["top"].get("name", os.path.basename(d.rstrip("/")))
        workers = []
        for wname, w in t["workers"].items():
            report = os.path.join(d, "inbox", wname + ".md")
            rep = os.path.exists(report)
            workers.append(dict(name=wname, lifecycle=w.get("lifecycle", "?"),
                                pane=w.get("pane_id", "-"), role=w.get("role", "")[:46],
                                report=report if rep else None, adopted=w.get("adopted") == "true"))
        ev = os.path.join(d, "inbox", "events.jsonl")
        events = []
        if os.path.exists(ev):
            for ln in open(ev):
                try:
                    e = json.loads(ln); dd = e.get("data", {})
                    events.append((dd.get("pane_id", "?"), dd.get("agent_status", e.get("event", "?"))))
                except json.JSONDecodeError: pass
        rs.append(dict(dir=d, name=name, lifecycle=t["top"].get("lifecycle", "?"),
                       workers=workers, events=events))
    return rs

MOCK_DEPS = {"kill-b": [], "hook-a": ["kill-b"], "protocol-d": [], "session-c": ["protocol-d"]}
MOCK_TASK = {"hook-a": "#10+#4 hook transitions", "kill-b": "#11+#3 kill/adopt",
             "session-c": "#5 agent_session", "protocol-d": "#15 git contract",
             "metadata-e": "#6 metadata tokens", "dod-worker": "DoD probe",
             "adopt-builder": "#1 team adopt"}

def size():
    try: return os.get_terminal_size()
    except OSError:
        import collections
        return collections.namedtuple("S", "columns lines")(120, 40)

def header(title, run):
    cols = size().columns
    line = f" BOARD · {run['name']} · run {run['lifecycle'].upper()} · {len(run['workers'])} workers "
    print(BOLD + FG["cyan"] + line.ljust(cols - len(title) - 1) + RST + DIM + title + RST)
    print(DIM + "─" * cols + RST)

def wline(w, wide=True):
    g, c = GLYPH.get(w["lifecycle"], ("?", "white"))
    task = MOCK_TASK.get(w["name"], "")
    rep = FG["blue"] + "report:" + os.path.basename(w["report"]) + RST if w["report"] else DIM + "no report" + RST
    ad = FG["mag"] + " adopted" + RST if w["adopted"] else ""
    base = f" {FG[c]}{g}{RST} {BOLD}{w['name']:<12}{RST} {w['lifecycle']:<9} {w['pane']:<8}"
    return base + (f" {task:<26} {rep}{ad}" if wide else "")

def variant_a(run):
    header("A — mission-control table", run)
    print(BOLD + f"   {'WORKER':<12} {'STATE':<9} {'PANE':<8} {'TASK':<26} REPORT" + RST)
    for w in run["workers"]: print(wline(w))
    print()
    print(BOLD + " MAILBOX " + RST + DIM + f"{len(run['events'])} events in inbox; last 3:" + RST)
    for p, s in run["events"][-3:]: print(f"   {DIM}{p:<9}{RST}{s}")
    print()
    print(DIM + " controls: [k]ill run  [m]sg worker  [a]dopt pane  [o]pen report  (inert in prototype)" + RST)

def variant_b(run):
    header("B — state columns (kanban)", run)
    cols = {"active": [], "attention": [], "done": []}
    for w in run["workers"]:
        k = "active" if w["lifecycle"] in ("running", "pending") else \
            "attention" if w["lifecycle"] in ("orphaned", "failed") else "done"
        cols[k].append(w)
    width = size().columns // 3 - 2
    titles = [("active", "ACTIVE", "green"), ("attention", "NEEDS ATTENTION", "red"), ("done", "ENDED/RELEASED", "white")]
    rows = max(len(v) for v in cols.values()) if any(cols.values()) else 0
    print("  " + "".join(BOLD + FG[c] + t.ljust(width + 2) + RST for k, t, c in titles))
    for i in range(rows):
        line = "  "
        for k, _, _ in titles:
            if i < len(cols[k]):
                w = cols[k][i]
                deps = MOCK_DEPS.get(w["name"], [])
                dep = (FG["yellow"] + f"⇠{','.join(deps)}[MOCK]" + RST) if deps else ""
                cell = f"{BOLD}{w['name']}{RST} {DIM}{MOCK_TASK.get(w['name'],'')[:14]}{RST} {dep}"
            else: cell = ""
            # pad ignoring ANSI
            vis = re.sub(r"\033\[[0-9;]*m", "", cell)
            line += cell + " " * max(0, width + 2 - len(vis))
        print(line)
    print()
    print(DIM + " cards carry task + deps; reports open via link handler on click" + RST)

def variant_c(run):
    header("C — team strip + event stream", run)
    strip = "  ".join(f"{FG[GLYPH.get(w['lifecycle'],('?','white'))[1]]}{GLYPH.get(w['lifecycle'],('?','white'))[0]}{RST}{w['name']}"
                      for w in run["workers"])
    print(" " + strip)
    print(DIM + "─" * size().columns + RST)
    tail = run["events"][-(size().lines - 8):]
    for p, s in tail:
        col = "green" if s == "working" else "yellow" if s in ("idle", "done") else "red" if s == "blocked" else "white"
        print(f" {DIM}{p:<9}{RST} {FG[col]}{s}{RST}")
    print()
    print(DIM + " pointer/report lines would be link-handler hot; newest at bottom" + RST)

SEL = {"row": 0, "cmd": ""}  # control-deck state (prototype-only)

def variant_d(run):
    header("D — CONTROL DECK (j/k select · m/K/o/g act · inert)", run)
    print(BOLD + f"     {'WORKER':<12} {'STATE':<9} {'PANE':<8} {'TASK':<26} REPORT" + RST)
    SEL["row"] %= max(1, len(run["workers"]))
    for i, w in enumerate(run["workers"]):
        cursor = FG["cyan"] + BOLD + " ▶ " + RST if i == SEL["row"] else "   "
        print(cursor + wline(w).lstrip())
    w = run["workers"][SEL["row"]] if run["workers"] else None
    print()
    strip = "  ".join(f"{FG[GLYPH.get(x['lifecycle'],('?','white'))[1]]}{GLYPH.get(x['lifecycle'],('?','white'))[0]}{RST}" for x in run["workers"])
    print(" team " + strip + "   " + DIM + f"mailbox: {len(run['events'])} events" + RST)
    print(DIM + "─" * size().columns + RST)
    print(BOLD + " ACTIONS on selected row:" + RST)
    print("   [m] msg    [g] msg --attention-ack    [K] kill worker    [o] open report    [p] adopt a pane")
    print()
    if SEL["cmd"]:
        print(FG["yellow"] + " WOULD RUN: " + RST + SEL["cmd"])
    else:
        print(DIM + " (press an action key to preview the exact command — nothing executes)" + RST)

def deck_action(ch, run):
    w = run["workers"][SEL["row"]] if run["workers"] else None
    if not w: return
    b = "herdr-agent-team"
    r = run["dir"].rstrip("/")
    cmds = {"m": f"{b} msg {w['name']} '<text>' --run {r}",
            "g": f"{b} msg {w['name']} 'ack' --run {r}   # answers --attention ping",
            "K": f"{b} kill-worker {w['name']} --run {r}   # NEW VERB needed (kill is run-wide today)",
            "o": f"herdr pane run <viewer> 'less {w['report'] or '<no report yet>'}'  # or link handler",
            "p": f"{b} adopt <pane-id> --name <new> --run {r}"}
    if ch in cmds: SEL["cmd"] = cmds[ch]

VARIANTS = [("A", variant_a), ("B", variant_b), ("C", variant_c), ("D", variant_d)]

def draw(i, runs, ri):
    os.system("clear")
    run = runs[ri]
    VARIANTS[i][1](run)
    cols = size().columns
    bar = f"  ← {VARIANTS[i][0]} of A/B/C/D →   [r] cycle run ({run['name']})   [q] quit  "
    print("\n" + BOLD + "\033[7m" + bar.center(cols) + RST)

def main():
    runs = load_runs()
    if not runs: sys.exit("no runs found")
    i, ri = 0, 0
    fd = sys.stdin.fileno(); old = termios.tcgetattr(fd)
    try:
        tty.setcbreak(fd)
        draw(i, runs, ri)
        while True:
            ch = sys.stdin.read(1)
            if ch == "q": break
            n = len(VARIANTS)
            if ch == "\033":
                nxt = sys.stdin.read(2)
                if nxt == "[C": i = (i + 1) % n
                elif nxt == "[D": i = (i - 1) % n
            elif ch == "d": i = (i + 1) % n
            elif ch == "a": i = (i - 1) % n
            elif ch == "r": ri = (ri + 1) % len(runs); SEL["row"] = 0; SEL["cmd"] = ""
            elif VARIANTS[i][0] == "D" and ch == "j": SEL["row"] += 1; SEL["cmd"] = ""
            elif VARIANTS[i][0] == "D" and ch == "k": SEL["row"] -= 1; SEL["cmd"] = ""
            elif VARIANTS[i][0] == "D": deck_action(ch, runs[ri])
            draw(i, runs, ri)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)
        os.system("clear")

if __name__ == "__main__": main()
