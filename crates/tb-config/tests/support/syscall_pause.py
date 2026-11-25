#!/usr/bin/env python3
"""Pause a program at one step of a protected write, so a test can change the config there.

Test support only: the product has no hook. The program runs under a seccomp filter that reports
its `rename*`, `mkdir*` and `fsync`/`fdatasync` calls to this supervisor (user notification);
every call is let through unchanged (`SECCOMP_USER_NOTIF_FLAG_CONTINUE`). Once `<signals>/armed`
exists, the first call that matches the step is held before it runs:

  stage    the program creates a temp generation directory (`.tmp-*`) in <store>: the
           generation is not published yet and the program's last check has not run.
  publish  the program syncs <store> after renaming a temp generation directory into <store>
           under a numeric name: the generation is published, the config not yet replaced.

While it is held, this writes `<signals>/paused` (JSON: step, generation, syscall) and waits for
`<signals>/resume` (the test changes the config in between), then lets the call run. It holds at
most one call. Its log is `<signals>/supervisor.log`; it exits with the program's status
(128 + signal when killed). x86_64 Linux >= 5.5 only; anything else exits 70 before running it.

usage: syscall_pause.py <signals> <store> <stage|publish> -- <program> [args...]
"""

import ctypes
import fcntl
import json
import os
import select
import socket
import struct
import sys
import time

SYS = {"rename": 82, "mkdir": 83, "fsync": 74, "fdatasync": 75, "mkdirat": 258, "renameat": 264, "renameat2": 316}
NAMES = {nr: name for name, nr in SYS.items()}
SYS_SECCOMP = 317
PR_SET_NO_NEW_PRIVS = 38
SECCOMP_SET_MODE_FILTER = 1
SECCOMP_FILTER_FLAG_NEW_LISTENER = 1 << 3
AUDIT_ARCH_X86_64 = 0xC000003E
RET_ALLOW = 0x7FFF0000
RET_USER_NOTIF = 0x7FC00000
# struct seccomp_notif (80 bytes) and struct seccomp_notif_resp (24 bytes); _IOWR('!', n, size).
NOTIF = struct.Struct("=QIIiIQ6Q")
RESP = struct.Struct("=QqiI")
IOCTL_RECV = (3 << 30) | (NOTIF.size << 16) | (ord("!") << 8) | 0
IOCTL_SEND = (3 << 30) | (RESP.size << 16) | (ord("!") << 8) | 1
FLAG_CONTINUE = 1
AT_FDCWD = -100
HOLD_LIMIT = 600.0


class SockFprog(ctypes.Structure):
    _fields_ = [("len", ctypes.c_ushort), ("filter", ctypes.c_void_p)]


def program() -> bytes:
    """BPF: on x86_64, notify the listed syscalls, allow everything else."""
    ld_abs, jeq, ret = 0x20, 0x15, 0x06
    nrs = sorted(SYS.values())
    ins = [(ld_abs, 0, 0, 4), (jeq, 1, 0, AUDIT_ARCH_X86_64), (ret, 0, 0, RET_ALLOW), (ld_abs, 0, 0, 0)]
    for i, nr in enumerate(nrs):
        ins.append((jeq, len(nrs) - i, 0, nr))
    ins += [(ret, 0, 0, RET_ALLOW), (ret, 0, 0, RET_USER_NOTIF)]
    return b"".join(struct.pack("=HBBI", code, jt, jf, k) for code, jt, jf, k in ins)


class Supervisor:
    def __init__(self, signals: str, store: str, step: str) -> None:
        self.signals = signals
        self.store = os.path.realpath(store)
        self.step = step
        self.published = None
        self.held = False
        self.log = open(os.path.join(signals, "supervisor.log"), "a", encoding="utf-8")

    def note(self, text: str) -> None:
        self.log.write(f"{time.monotonic():.3f} {text}\n")
        self.log.flush()

    def armed(self) -> bool:
        return os.path.exists(os.path.join(self.signals, "armed"))

    @staticmethod
    def string(tid: int, address: int) -> str:
        with open(f"/proc/{tid}/mem", "rb", buffering=0) as mem:
            mem.seek(address)
            raw = mem.read(4096)
        return raw.split(b"\0", 1)[0].decode("utf-8", "surrogateescape")

    @staticmethod
    def directory(tid: int, fd: int) -> str:
        fd = fd & 0xFFFFFFFF
        fd = fd - (1 << 32) if fd >= 1 << 31 else fd
        return os.readlink(f"/proc/{tid}/cwd" if fd == AT_FDCWD else f"/proc/{tid}/fd/{fd}")

    def resolve(self, tid: int, fd: int, path: str) -> str:
        return os.path.normpath(path if path.startswith("/") else os.path.join(self.directory(tid, fd), path))

    def event(self, tid: int, nr: int, args: tuple) -> str | None:
        """The step this call reaches, or None. Updates what has been published."""
        name = NAMES.get(nr)
        if name in ("renameat", "renameat2", "rename"):
            if name == "rename":
                old, new = self.resolve(tid, AT_FDCWD, self.string(tid, args[0])), self.resolve(
                    tid, AT_FDCWD, self.string(tid, args[1]))
            else:
                old = self.resolve(tid, args[0], self.string(tid, args[1]))
                new = self.resolve(tid, args[2], self.string(tid, args[3]))
            self.note(f"{name} {old} -> {new}")
            if (self.armed() and os.path.dirname(new) == self.store and os.path.basename(new).isdigit()
                    and os.path.basename(old).startswith(".tmp-")):
                self.published = os.path.basename(new)
            return None
        if name in ("mkdirat", "mkdir"):
            fd, at = (args[0], args[1]) if name == "mkdirat" else (AT_FDCWD, args[0])
            made = self.resolve(tid, fd, self.string(tid, at))
            self.note(f"{name} {made}")
            if (self.step == "stage" and self.armed() and os.path.dirname(made) == self.store
                    and os.path.basename(made).startswith(".tmp-")):
                return "stage"
            return None
        synced = self.directory(tid, args[0])
        self.note(f"{name} {synced}")
        if self.step == "publish" and self.published and os.path.normpath(synced) == self.store:
            return "publish"
        return None

    def hold(self, step: str, nr: int) -> None:
        paused = os.path.join(self.signals, "paused")
        with open(paused + ".tmp", "w", encoding="utf-8") as out:
            json.dump({"step": step, "generation": self.published, "syscall": NAMES[nr]}, out)
        os.rename(paused + ".tmp", paused)
        self.note(f"held at {step} ({NAMES[nr]}), generation {self.published}")
        resume = os.path.join(self.signals, "resume")
        deadline = time.monotonic() + HOLD_LIMIT
        while not os.path.exists(resume) and time.monotonic() < deadline:
            time.sleep(0.01)
        self.note("resumed" if os.path.exists(resume) else "hold limit reached, resumed")
        self.held = True

    def serve(self, listener: int, child: int) -> int:
        poller = select.poll()
        poller.register(listener, select.POLLIN)
        while True:
            for _, mask in poller.poll(200):
                if mask & select.POLLIN:
                    self.answer(listener)
                elif mask & (select.POLLHUP | select.POLLERR):
                    return self.reap(child, block=True)
            done = self.reap(child, block=False)
            if done is not None:
                return done

    def answer(self, listener: int) -> None:
        buf = bytearray(NOTIF.size)
        try:
            fcntl.ioctl(listener, IOCTL_RECV, buf, True)
        except OSError as e:
            self.note(f"recv: {e}")
            return
        nid, tid, _flags, nr, _arch, _ip, *args = NOTIF.unpack(buf)
        if not self.held:
            try:
                step = self.event(tid, nr, tuple(args))
            except OSError as e:
                self.note(f"{NAMES.get(nr)}: cannot read arguments: {e}")
                step = None
            if step is not None:
                self.hold(step, nr)
        try:
            fcntl.ioctl(listener, IOCTL_SEND, bytearray(RESP.pack(nid, 0, 0, FLAG_CONTINUE)), True)
        except OSError as e:
            self.note(f"send: {e}")

    @staticmethod
    def reap(child: int, block: bool) -> int | None:
        pid, status = os.waitpid(child, 0 if block else os.WNOHANG)
        if pid == 0:
            return None
        return os.waitstatus_to_exitcode(status) if os.WIFEXITED(status) else 128 + os.WTERMSIG(status)


def launch(argv: list[str]) -> tuple[int, int]:
    """Fork the program under the filter; returns its pid and the notification listener."""
    libc = ctypes.CDLL(None, use_errno=True)
    libc.syscall.restype = ctypes.c_long
    code = program()
    buf = ctypes.create_string_buffer(code, len(code))
    fprog = SockFprog(len(code) // 8, ctypes.cast(buf, ctypes.c_void_p))
    ours, theirs = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
    child = os.fork()
    if child == 0:
        try:
            ours.close()
            if libc.prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0:
                raise OSError(ctypes.get_errno(), "prctl(PR_SET_NO_NEW_PRIVS)")
            listener = libc.syscall(SYS_SECCOMP, SECCOMP_SET_MODE_FILTER, SECCOMP_FILTER_FLAG_NEW_LISTENER,
                                    ctypes.byref(fprog))
            if listener < 0:
                raise OSError(ctypes.get_errno(), "seccomp(SECCOMP_FILTER_FLAG_NEW_LISTENER)")
            socket.send_fds(theirs, [b"l"], [listener])
            os.close(listener)
            theirs.recv(1)
            theirs.close()
            os.execvp(argv[0], argv)
        except BaseException as e:  # noqa: BLE001 - report anything before exec, never return
            os.write(2, f"syscall_pause: cannot start {argv[0]}: {e}\n".encode())
            os._exit(70)
    theirs.close()
    _, fds, _, _ = socket.recv_fds(ours, 1, 1)
    if not fds:
        os.waitpid(child, 0)
        sys.exit(70)
    ours.sendall(b"g")
    ours.close()
    return child, fds[0]


def main() -> int:
    args = sys.argv[1:]
    if len(args) < 5 or args[3] != "--" or args[2] not in ("stage", "publish"):
        sys.stderr.write(__doc__ or "")
        return 64
    if os.uname().machine != "x86_64":
        sys.stderr.write("syscall_pause: x86_64 only\n")
        return 70
    signals, store, step, argv = args[0], args[1], args[2], args[4:]
    supervisor = Supervisor(signals, store, step)
    child, listener = launch(argv)
    return supervisor.serve(listener, child)


if __name__ == "__main__":
    sys.exit(main())
