#!/usr/bin/env python3
"""Verify the TB-46 s1, TB-47 s1-4 and TB-48 s1-4 seals in this checkout, and for each slice the
.ack, the evidence history (seen_red / never_red under the current seal) and, with --finals HEAD,
the final record (PASS, current seal, that HEAD). Seal recomputed with scripts/pipeline-gate.py's
own seal_of. Exit 0 when all hold."""
import importlib.util, json, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("gate", ROOT / "scripts/pipeline-gate.py")
gate = importlib.util.module_from_spec(spec); spec.loader.exec_module(gate)
head = sys.argv[sys.argv.index("--finals") + 1] if "--finals" in sys.argv else None
bad = 0
for ticket, slices in (("TB-46", [1]), ("TB-47", [1, 2, 3, 4]), ("TB-48", [1, 2, 3, 4])):
    ack = json.loads((ROOT / f".scratch/{ticket}/.ack").read_text())
    for n in slices:
        c = ROOT / f".scratch/{ticket}/contract"
        m = json.loads((c / f"slice-{n}.json").read_text())
        seal = gate.seal_of(c / f"slice-{n}.json", m)
        stored = (c / f"slice-{n}.seal").read_text().strip()
        runs = json.loads((c / f"slice-{n}.evidence.json").read_text())["runs"]
        req = {x["id"] for x in m.get("cases", []) if x["red"] == "required"}
        seen = {i for r in runs if r["seal"] == seal for i, s in r["results"].items() if s in gate.VALID_RED}
        never = sorted(req - seen)
        line = f"{ticket} s{n} seal {seal[7:19]} {'ok' if seal == stored else 'BROKEN'} ack {ack['confirmed']} " \
               f"runs {len(runs)} (under seal {sum(r['seal'] == seal for r in runs)}) required {len(req)} never_red {never or '-'}"
        ok = seal == stored and ack["confirmed"] is True and not never
        if head:
            f = c / f"slice-{n}.final.json"
            fin = json.loads(f.read_text()) if f.is_file() else {}
            fok = fin.get("result") == "PASS" and fin.get("seal") == seal and fin.get("head") == head
            line += f" final {'PASS@' + head[:8] if fok else 'MISSING/INVALID ' + str(fin.get('head', ''))[:8]}"
            ok = ok and fok
        bad += not ok
        print(line)
sys.exit(1 if bad else 0)
