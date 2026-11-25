#!/usr/bin/env python3
"""Probe: does `touchbard_hardware` keep every generation out of the caller's XDG_STATE_HOME?

Runs `cargo test -p tb-daemon --test touchbard_hardware` with XDG_STATE_HOME set to a decoy that
holds a `theme-set` file, then reports (JSON, paths relative to the decoy and to cargo's temp dir):
- every change to the decoy tree (mode, size, mtime, bytes, inode of each entry);
- the test outcomes from libtest;
- per HUP/INT/TERM round, the simulator dir (first frame shown) and the private HOME started with
  it, and the generations that HOME's own state dir holds (first write of the absent config).
Usage: probe_xdg_isolation.py <decoy> <report.json> <cargo log>   (env: CARGO_TARGET_DIR)
Exit 0 when the decoy is unchanged, both tests pass and each signal round kept exactly one absent
generation in its private HOME; 1 otherwise."""
import json, os, re, subprocess, sys, tomllib
from pathlib import Path

decoy, report_path, log_path = map(Path, sys.argv[1:4])
tmp = Path(os.environ["CARGO_TARGET_DIR"]) / "tmp" / "touchbard_hardware"

def snapshot(root):
    out = {}
    for p in [root, *sorted(root.rglob("*"))]:
        st = p.lstat()
        data = p.read_bytes().hex() if p.is_file() and not p.is_symlink() else None
        out[p.relative_to(root).as_posix() or "."] = [st.st_mode, st.st_size, st.st_mtime_ns, st.st_ino, data]
    return out

def generations(home):
    store = home / ".local/state/touchbar-customizer/backups"
    found = []
    for m in sorted(store.glob("*/manifest.toml")):
        d = tomllib.loads(m.read_text())
        found.append({"id": m.parent.name, "kind": d["kind"], "dir-existed": d.get("dir-existed"),
                      "config_in_this_home": Path(d["config"]).is_relative_to(home),
                      "manifest_mode": oct(m.stat().st_mode & 0o777),
                      "generation_dir_mode": oct(m.parent.stat().st_mode & 0o777),
                      "config_copy": (m.parent / "config.toml").exists()})
    return found

decoy.mkdir(parents=True, exist_ok=True)
(decoy / "touchbar-customizer").mkdir(exist_ok=True)
(decoy / "touchbar-customizer/theme-set").write_text("caller\n")
subprocess.run(["rm", "-rf", "--", str(tmp)], check=True)
before = snapshot(decoy)
env = dict(os.environ, XDG_STATE_HOME=str(decoy))
cmd = ["cargo", "test", "--locked", "-p", "tb-daemon", "--test", "touchbard_hardware"]
p = subprocess.run(cmd, env=env, capture_output=True, text=True)
log_path.write_text(" ".join(cmd) + f"\nXDG_STATE_HOME=<decoy>\nexit {p.returncode}\n\n" + p.stdout + p.stderr)
after = snapshot(decoy)
changed = sorted(k for k in set(before) | set(after) if before.get(k) != after.get(k))
decoy_generations = sorted(m.parent.name for m in decoy.glob("touchbar-customizer/backups/*/manifest.toml"))
tests = dict(re.findall(r"^test (\S+) \.\.\. (\w+)", p.stdout, re.M))

def n(d):
    return int(d.name.rsplit("-", 1)[1])

dirs = sorted((d for d in tmp.iterdir() if d.is_dir()), key=n) if tmp.is_dir() else []
homes = {n(d): d for d in dirs if d.name.startswith("home-")}
rounds = []
for d in dirs:
    if d.name.startswith("signal-sim-"):
        home = homes.get(n(d) + 1)
        rounds.append({"sim_dir": d.name, "first_frame": (d / "sim/frame-0001.png").is_file(),
                       "home": home.name if home else None,
                       "home_generations": generations(home) if home else []})
other = [{"home": h.name, "home_generations": generations(h)} for k, h in homes.items()
         if all(r["home"] != h.name for r in rounds)]
ok = (p.returncode == 0 and not changed and not decoy_generations and len(rounds) == 3
      and all(t == "ok" for t in tests.values()) and len(tests) == 2
      and all(r["first_frame"] and len(r["home_generations"]) == 1
              and r["home_generations"][0]["kind"] == "absent" and r["home_generations"][0]["config_in_this_home"]
              for r in rounds))
report = {"head": subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip(),
          "worktree_diff_sha256": subprocess.run(["bash", "-c", "git diff HEAD -- crates | sha256sum"],
                                                 capture_output=True, text=True).stdout.split()[0],
          "cargo_exit": p.returncode, "tests": tests,
          "decoy": {"entries_before": len(before), "entries_after": len(after), "changed": changed,
                    "generations": decoy_generations,
                    "theme_set": (decoy / "touchbar-customizer/theme-set").read_text()},
          "signal_rounds": rounds, "other_private_homes": other, "pass": ok}
report_path.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: report[k] for k in ("head", "cargo_exit", "tests", "pass")}))
print("decoy changed:", changed, "decoy generations:", decoy_generations)
for r in rounds:
    print(r["sim_dir"], "frame", r["first_frame"], r["home"], [g["kind"] for g in r["home_generations"]])
sys.exit(0 if ok else 1)
