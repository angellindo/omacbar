"""External-process fixtures for the public doctor/triage CLIs; never run host tools."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
IDS = ["platform", "build.tools", "rust.toolchain", "rust.rustfmt", "rust.clippy",
       "libs.xkbcommon", "libs.wayland", "libs.fontconfig", "vulkan.loader", "font.monospace",
       "runtime.probe", "runtime.config", "runtime.theme", "runtime.gpu", "runtime.window",
       "input.fn", "input.uinput", "hardware.bar"]
SECRET = "private-secret-from-config-or-environment"


class CliFixture:
    def __init__(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="bar CLI spaces ; ")
        self.base = Path(self.tmp.name)
        self.root = self.base / "checkout with spaces"
        self.home = self.base / "isolated home"
        self.bin = self.base / "installed tools"
        for path in (self.root, self.home, self.bin):
            path.mkdir()
        shutil.copytree(ROOT / "scripts", self.root / "scripts", ignore=shutil.ignore_patterns("__pycache__"))
        shutil.copy2(ROOT / "rust-toolchain.toml", self.root / "rust-toolchain.toml")
        shutil.copy2(ROOT / "Cargo.toml", self.root / "Cargo.toml")
        self.calls = self.base / "calls.jsonl"
        self.settings = self.base / "tools.json"
        self.settings.write_text("{}")
        self.env = {"HOME": str(self.home), "PATH": str(self.bin), "LANG": "C.UTF-8",
                    "PYTHONDONTWRITEBYTECODE": "1", "FIXTURE_CALLS": str(self.calls),
                    "FIXTURE_SETTINGS": str(self.settings), "PRIVATE_TOKEN": SECRET,
                    "RUSTUP_HOME": str(self.home / ".rustup"),
                    "CARGO_HOME": str(self.home / ".cargo")}
        self.forbidden = self.base / "forbidden.jsonl"
        self.env["FIXTURE_FORBIDDEN"] = str(self.forbidden)
        self.font = self.base / "monospace.ttf"
        self.font.write_bytes(b"font metadata fixture")
        tools = "git cc gcc clang ld pkg-config bash nice flock setsid rustup mise rustc cargo rustfmt clippy-driver cargo-clippy fc-match pacman ldconfig uname".split()
        for tool in tools:
            self.tool(tool)
        toolchain = self.home / ".rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin"
        toolchain.mkdir(parents=True)
        for tool in ("rustc", "cargo", "rustfmt", "clippy-driver", "cargo-clippy"):
            shutil.copy2(self.bin / tool, toolchain / tool)
        self.configure({"font": str(self.font), "toolchain": str(toolchain)})
        # Audit only the process under test: device events and network cannot be accessed.
        audit = self.base / "audit"
        audit.mkdir()
        (audit / "sitecustomize.py").write_text('''import sys, socket, os
def reject(reason):
    with open(os.environ["FIXTURE_FORBIDDEN"], "a") as log: log.write(reason+"\\n")
    raise AssertionError(reason)
def guard(event, args):
    if event == "open":
        p = str(args[0])
        if p.startswith("/dev/input/") or p == "/dev/uinput":
            reject("doctor must inspect metadata without opening devices")
    if event == "socket.connect" and args[0].family != socket.AF_UNIX:
        reject("doctor and triage must collect offline")
    if event == "subprocess.Popen" and str(args[0]).startswith(("/usr/", "/bin/")) and args[0] != sys.executable:
        reject("all external tools in CLI tests must be fixture executables")
sys.addaudithook(guard)
''')
        self.env["PYTHONPATH"] = str(audit)

    def close(self):
        self.tmp.cleanup()

    def configure(self, changes):
        values = json.loads(self.settings.read_text())
        values.update(changes)
        self.settings.write_text(json.dumps(values))

    def executable(self, path, source):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"#!{sys.executable} -B\n" + source)
        path.chmod(0o755)

    def tool(self, name):
        self.executable(self.bin / name, '''import json, os, sys, time
from pathlib import Path
name = Path(sys.argv[0]).name
cfg = json.loads(Path(os.environ["FIXTURE_SETTINGS"]).read_text())
with open(os.environ["FIXTURE_CALLS"], "a") as log:
    log.write(json.dumps({"tool":name,"executable":str(Path(sys.argv[0]).resolve()),"argv":sys.argv[1:],"cwd":os.getcwd()})+"\\n")
if name in cfg.get("sleep", []): time.sleep(7)
if name in cfg.get("fail", []): sys.exit(1)
if name == "rustup":
    args = sys.argv[1:]
    if args[:2] == ["toolchain", "list"]: print("1.99.1-x86_64-unknown-linux-gnu (default)")
    elif "which" in args: print(str(Path(cfg["toolchain"]) / args[-1]))
    elif args[:2] == ["component", "list"]: print("rustfmt-x86_64-unknown-linux-gnu (installed)\\nclippy-x86_64-unknown-linux-gnu (installed)")
    else: print(str(Path(cfg["toolchain"]).parent))
elif name in ("rustc", "cargo", "rustfmt", "clippy-driver", "cargo-clippy"):
    print(name + " 1.99.1")
elif name == "pkg-config": print("1.0.0")
elif name == "fc-match": print(cfg["font"])
elif name == "pacman": print("vulkan-icd-loader 1.0\\nvulkan-intel 1.0")
elif name == "ldconfig": print("libvulkan.so.1 => /usr/lib/libvulkan.so.1")
elif name == "git":
    if "rev-parse" in sys.argv: print("a"*40)
    elif "status" in sys.argv: print(" M private-filename-should-not-be-collected")
elif name == "uname": print("Linux x86_64")
''')

    def native(self, mode="valid"):
        path = self.root / "target/debug/touchbar-doctor-probe"
        self.executable(path, f'''import json, sys
mode = {mode!r}
if mode == "invalid": print("not native JSON"); sys.exit(0)
if mode == "oversize": print("x"*70000); sys.exit(0)
if mode == "crash": sys.exit(7)
kind = sys.argv[sys.argv.index("--check")+1]
def row(id, status, required):
    return {{"id":id,"status":status,"required":required,"message":"fixture check", "evidence":[],"next":[]}}
checks = {{"config":[row("runtime.config","ok",True),row("runtime.theme","warning",True)],
          "gpu":[row("runtime.gpu","failed",True)],
          "input":[row("input.fn","warning",False),row("input.uinput","warning",False)]}}[kind]
print(json.dumps({{"schema_version":1,"checks":checks}}))
''')

    def run(self, script, *args, timeout=35):
        result = subprocess.run([sys.executable, "-B", str(self.root / "scripts" / script), *args],
                                cwd=self.root, env=self.env, capture_output=True, text=True, timeout=timeout)
        assert not self.forbidden.exists(), self.forbidden.read_text() if self.forbidden.exists() else ""
        return result

    def invocations(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []


def blocker_report(profile="runtime"):
    return {"schema_version": 1, "profile": profile,
            "checks": [{"id": "runtime.probe", "status": "skipped", "required": True,
                        "message": "Build native probe first", "evidence": [],
                        "next": ["cargo build --workspace --locked"]}],
            "summary": {"ok": 0, "warning": 0, "failed": 0, "skipped": 1, "ready": False}}
