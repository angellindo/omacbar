# Hardware runbook: staged, reversible Touch Bar work

Every customizer test must be stoppable and returnable to the observable state it started from.
Stage S0 proves that for the config file, agent-runnable, on a throwaway profile. Stages S1–S4
take the physical laptop from baseline A to baseline B and through the pixel-mode tickets, each
with its own return. The full return B → A is its own procedure.

Rules for every stage:

- **Physical stages are human only.** Agents never load modules, edit udev/systemd/ESP, change
  device permissions or write `/dev` (CLAUDE.md). Agents run only S0, and `baseline` only on
  fixture roots.
- Physical facts are cited, never restated as verified here: service files, rollback and the
  staged stop/disable/observe/remove come from [research 0007](research/0007-retiring-touchbar-local-service.md)
  (§1, §Staged procedure); config 1 vs 2, firmware risk and the external backup gate from
  [research 0001](research/0001-t1-touch-bar-hardware.md) (§Hard prerequisites) and
  [research 0006](research/0006-macos-reference-stack.md); display, input and backlight owners and
  first-frame verification from [research 0010](research/0010-ewweberlin-touchbar-control-path.md)
  (§T1 and Rune decision input).
- Nothing personal is inlined. `$SERVICE_SOURCES` is the directory that holds the human's sources
  of the local service (the three installed files). `$PRIVATE` is a private directory outside the
  repo for baselines. `$IBRIDGE` is the iBridge's USB device directory under
  `/sys/bus/usb/devices` (find it by `idVendor` 05ac, `idProduct` 8600).
- A failed return blocks the next stage. Log exceptions privately; share a redacted summary.

## Baselines

**Baseline A** is the laptop before TB-28. As noted on 2026-11-22: service active and enabled,
iBridge in USB configuration 1, configurator profile absent. Never trust that line: re-record A
at the start of every stage.

**Baseline B** is key mode verified after TB-28: service stopped, disabled and removed, iBridge
still in configuration 1, the bar lit with its firmware keys, the suspend mechanisms the service
does not own still in place.

Record and compare (human, read-only; output private, never committed):

```sh
scripts/rehearsal.py baseline --label A --out "$PRIVATE/A"
scripts/rehearsal.py baseline --label B --out "$PRIVATE/B"
scripts/rehearsal.py compare "$PRIVATE/A" "$PRIVATE/B"
```

`baseline` reads the iBridge `bConfigurationValue`, `systemctl is-active` / `is-enabled` of the
service, SHA-256 and `stat` of the three managed files, of the service manifest and of the key-mode
files the service does not own (research 0007 §1 tables), `/sys/power/mem_sleep`, the kernel
release, the installed module trees, loaded `srcversion`s and `/proc/cmdline`. A root-only file is
recorded as "needs sudo — human step", never read with privilege; check it by hand
(`sudo sha256sum`, `sudo stat`) and note the result next to the baseline. `compare` exits 0 when
two baselines match and lists every difference otherwise.

### S0 · Simulator rehearsal (agent)

| Owner | Authorized target | Entry state | Expected changes | Stop criterion | Return procedure | Evidence (after-comparison) |
|---|---|---|---|---|---|---|
| Agent; no hardware, no sudo | A fresh private profile from `mktemp` (0700, outside the repo and the caller's HOME) with a fixture theme; simulator frames and keys files inside it | `git status` recorded; the tool records the persistent session's metadata (never its content) | Only files inside the profile: config scenarios, snapshots, frames, keys file, the profile-local mark | Any scenario, the simulator stage or the session check fails; any refusal | `run` restores each scenario under the config-directory lock with compare-and-swap; afterwards `scripts/rehearsal.py cleanup --manifest <report>` removes only the run's own unchanged artifacts | Report: five scenarios (absent without and with parents, commented 0640, invalid, symlink) restored and verified; F5 pressed once into the keys file, `none` wrote nothing, the mark holds one `x`; session metadata identical |

```sh
nice -n 19 env CARGO_BUILD_JOBS=2 cargo build -p tb-app -p tb-daemon
R=$(mktemp -d)
scripts/rehearsal.py run --bin-dir target/debug --report "$R/rehearsal.json"
scripts/rehearsal.py cleanup --manifest "$R/rehearsal.json"
```

The single-config steps are available on their own for any manual test on a throwaway profile:
`capture --home H --store S --manifest M` before the test, `settle --manifest M --expect-config E
[--expect-target E] [--pid N] [--artifact PATH SHA256]` after it, then `restore`, `verify`,
`cleanup`. `E` is what the producing step left (`absent`, the SHA-256 of the saved bytes, or
`symlink:<link text>`; `--expect-target` for a captured symlink's target): settle never adopts
what it finds, a different state is a conflict and authorizes no restore. An artifact is the
test's own only when it has the hash the producing step expects. Exit codes: 0 ok, 1 failure or
refusal (nothing changed), 3 conflict (current version kept in place, original kept in the store).
`capture` refuses the caller's HOME, the persistent session and a store inside the tree it would
replace.
`run` takes each scenario's expectation from the producing save itself: the hash of what the save
left, read as soon as the configurator exits 0 and before any other process of the run starts,
and accepted only if it is TOML holding the edited label. A failed save or reopen, or a config or
old link target that changed after the save, stops the run with nothing settled or restored. Limit:
a writer that replaces the config between the save's exit and that read is taken for the save.

Two editors and an external edit: the tool half is a conflict (exit 3, both versions kept) and a
restore that waits for a writer holding the lock. The configurator half is TB-43's sealed Runtime
cases in `crates/tb-app/tests/configurator_conflict.rs` (clean and dirty stale editors, overwrite
with `config.toml.bak`, symlinked configs), cited here, not duplicated. A writer that does not take
the lock: restore compares again after the file to put back is written, synced and chmodded, right
before each rename or unlink; such a writer's edit is lost only if it lands in the short window
between that last comparison and the rename or unlink (ADR-0013's accepted residual). A change seen
by the comparison is a conflict that keeps the
other writer's version in place and the original in the store. `cleanup` reaches every path through
directories opened from `/` without following symlinks: a parent or store swapped for a symlink is
kept, not walked.

### S1 · TB-27 gate: external firmware copy

| Owner | Authorized target | Entry state | Expected changes | Stop criterion | Return procedure | Evidence (after-comparison) |
|---|---|---|---|---|---|---|
| Human | The external copy made for TB-27 (EFI/T1 set) and the ESP, read-only | TB-27 is Done with its USB copy verified; baseline A re-recorded | None: reading and hashing only | The external copy is absent, unreadable, or its hash list differs from TB-27's | Nothing to return: nothing was written | The hash list of the external copy matches TB-27's list today; any stage that can affect the T1 starts only after this check on the same day |

### S2 · TB-28: retire the local service (key mode stays)

Follow research 0007 §Staged procedure for the exact commands; the rows below add the entry and
evidence each sub-stage needs. Every command is typed on the internal keyboard.

| Step | Owner | Authorized target | Entry state | Expected changes | Stop criterion | Return procedure | Evidence (after-comparison) |
|---|---|---|---|---|---|---|---|
| 2.0 Preserve and prepare | Human | `$SERVICE_SOURCES`, the three installed files, the service manifest, a private notes directory | Baseline A re-recorded; S1 passed today; charger on, bar lit | Private copies only: `cp -a` of the three files and the manifest (`installed.json`) into `$PRIVATE/2.0/`, `stat` of each, service state and suspend configuration noted; the internal-keyboard rollback of 0007 stage 0 prepared | Any source file differs in hash from its installed file; a pending `restore.json` (0007) | Nothing changed on the system yet | Sources ↔ installed files hash-equal; private copies and `stat` output saved; rollback script typed and readable |
| 2.1 Stop | Human | `macbook-touchbar-local.service` (stop only) | 2.0 complete | Service inactive; files and enablement unchanged | Any check of 0007 stage 1 fails: dark bar, Esc/F1/F12 not seen in `cat -v`, keyboard or trackpad missing | Start the service again; if the bar is dark, reboot (the service is still enabled) | Bar lit; Esc, F1 and F12 produce their sequences; resume after a short suspend shows the bar within 5 s; `baseline` shows only the service state changed |
| 2.2 Disable | Human | The service's enablement only | 2.1 verified | Service disabled; the three files still present | Bar changes, or a file goes missing | Run the prepared rollback (enable --now) | `is-enabled` = disabled; `compare` against the 2.1 baseline shows only enablement |
| 2.3 Observe | Human | Normal use: one reboot, one long suspend on battery, long idle | 2.2 verified | None beyond 2.2 | A dark bar after any resume or the reboot | Reboot if dark, then the prepared rollback | Bar lit and key row working after at least three resumes and one reboot; journal shows the drop-in ran without device writes (0007 stage 3) |
| 2.4 Remove | Human | The three managed files, through the service's own `manage.py remove` from `$SERVICE_SOURCES` | 2.3 verified | The three files removed; the service manifest `installed.json` rewritten with `state: removed` (its content, size and times change; research 0007 §1); key-mode files the service does not own unchanged (`mbp143-t1-socw`, fnmode and idle modprobe preferences, `99-ibridge.rules`) | `remove` refuses (edited file or pending restore): stop and report, do not delete by hand | B → A below | `baseline --label B`; `compare` A B lists only the service state, the three removed files and the manifest `installed.json` (content now `state: removed`; root-only: `sudo cat` shows the state, `sudo sha256sum` differs from the 2.0 copy); this is baseline B |

### B → A · Full return from baseline B to baseline A

| Owner | Authorized target | Entry state | Expected changes | Stop criterion | Return procedure | Evidence (after-comparison) |
|---|---|---|---|---|---|---|
| Human | The three service files reinstalled from `$SERVICE_SOURCES`; the service manifest `installed.json` put back from `$PRIVATE/2.0/`; systemd units | Baseline B recorded; sources still hash-equal to the copies preserved in 2.0; the preserved manifest copy and its `stat` from 2.0 readable | The three files back with their recorded modes; the manifest `installed.json` back to the preserved bytes (`state: enabled`), mode, owner and mtime; service enabled and active | A source hash differs from the preserved copy; the put-back manifest differs from the 2.0 copy in bytes, mode, owner or mtime; `start` refuses ("Expected panel ON"): reboot first, then retry once, then stop | Remove the reinstalled files again (2.4) and stay at B | Manifest `installed.json`: `cmp` with the 2.0 copy silent, `stat` mode/owner/size/mtime equal to the 2.0 copy (ctime and inode differ, expected); `baseline --label A2`; `compare` A A2 shows no difference except timestamps the reinstall changes |

```sh
sudo install -m 0755 "$SERVICE_SOURCES/macbook-touchbar-local.py" /usr/local/lib/macbook-touchbar-local.py
sudo install -m 0644 "$SERVICE_SOURCES/macbook-touchbar-local.service" /etc/systemd/system/macbook-touchbar-local.service
sudo install -m 0644 "$SERVICE_SOURCES/94-macbook-touchbar-display.conf" /etc/systemd/system/systemd-suspend.service.d/94-macbook-touchbar-display.conf
# the manifest: preserved bytes and metadata from 2.0 (cp -a keeps mode, owner and times)
sudo cp -a "$PRIVATE/2.0/installed.json" /var/lib/macbook-touchbar-local/installed.json
sudo cmp "$PRIVATE/2.0/installed.json" /var/lib/macbook-touchbar-local/installed.json && echo same bytes
diff <(sudo stat -c '%a %U:%G %s %y' "$PRIVATE/2.0/installed.json") \
     <(sudo stat -c '%a %U:%G %s %y' /var/lib/macbook-touchbar-local/installed.json) && echo same metadata
sudo systemctl daemon-reload && sudo systemctl enable --now macbook-touchbar-local.service
```

(Research 0007 §Staged procedure, stage 4 rollback; `manage.py stage` refuses while the manifest
exists, so the files are installed directly. That rollback reinstalls only the three files; the
manifest `remove` marked `state: removed` is put back by hand from the copy preserved in 2.0, before
the service starts, and compared with it: bytes with `cmp`, mode, owner, size and mtime with `stat`.
Any difference is a stop: keep both, the copy in `$PRIVATE/2.0/` stays the reference.)

### S3 · TB-9: pixel-mode trial (kernel-specific)

Prepared for kernel `7.2.15-1.1-omarchy-mbp143ab`. TB-9 has not fixed its temporary route yet.
Every driver-specific command below is **TBD by TB-9**; a blank is a **no-go**, never a guess.
Nothing in this runbook claims a verified return from pixel mode.

| Step | Owner | Authorized target | Entry state | Expected changes | Stop criterion | Return procedure | Evidence (after-comparison) |
|---|---|---|---|---|---|---|---|
| 3.0 Entry check | Human | Read-only: `uname -r`, installed kernel and module set, boot entries, the external copy | Baseline B verified; S1 passed today; `uname -r` and the installed module trees equal the prepared kernel | None | Kernel or module set differs from the prepared one: stop and re-prepare; no boot entry usable with the internal keyboard only; external copy unreadable | Nothing changed | Entry `baseline` equals B; known-good boot entry named; recovery matrix (next section) reviewed |
| 3.1 Runtime-only trial, then back to key mode | Human | TBD by TB-9 (runtime-only: nothing persisted in modules, udev, modprobe or boot options) | 3.0 passed; seat/owner isolation for the T1 DRM card in place before the card can appear | TBD by TB-9 | Unexpected state: `05ac:1281` (recovery), a dark bar after the return, keyboard or trackpad lost. Stop and use the human recovery of research 0006 / TB-27 | TBD by TB-9; blank = no-go. Last resort: cold boot of the known-good entry | Seen on the panel: a displayed test pattern in pixel mode, then the firmware key row (Esc, F1–F12) after the return; `baseline` equals B |
| 3.2 Persist | Human | TBD by TB-9 | 3.1's return demonstrated on the physical key row | TBD by TB-9 | Any 3.1 check fails | TBD by TB-9; restore modules, udev rules, modprobe options and boot options from the 3.0 records | TBD by TB-9; blank = no-go |

### What each copy can restore

None of these copies is evidence that the bar returned (the recovery matrix the trial above
reviews at its entry check).

| Copy | Restores | Does not restore |
|---|---|---|
| Local snapshot of the root filesystem | Service files, udev rules, modprobe options, the module tree of installed kernels | The ESP, the T1 firmware, device-side T1 state |
| ESP backup | Boot entries and images, the EFI/T1 firmware set on the ESP | Root filesystem files, device-side T1 state |
| External firmware copy (TB-27) | The T1 firmware set when the ESP lost it (recovery `05ac:1281`) | Anything else; it needs the human recovery procedure of research 0006 |

Booting a known-good kernel with its module tree, restoring service files and recovering the T1
firmware are different recoveries.
Physical return to config 1 after a pixel-mode trial is unverified.
Neither a cold reboot nor a filesystem snapshot demonstrates it; only the key row seen on the
panel does.

### Driver facts that bound the evidence


- Binding `appletbdrm` can blank the current frame before any app sets a mode: its probe calls
  `clear_display` after DRM registration, and its update callback discards the USB transfer
  result ([stock appletbdrm.c, lines 789–801](https://github.com/gregkh/linux/blob/497791a0a96b866d90add64041c47acdff49cae6/drivers/gpu/drm/tiny/appletbdrm.c#L789-L801));
  the T1 candidate keeps these calls. A successful ioctl, or restoring an inactive KMS object, is
  not evidence that the firmware keys returned.
- The compositor must never claim the T1 DRM card; the seat rule must hold before it appears
  (research 0001 §Hard prerequisites).
- Kernel policy observed on 2026-11-22 (one boot, historical): `/sys/kernel/security/lockdown` =
  `[none]`, `sig_enforce` = `N`, `modules_disabled` = `0`. Recheck all three before a trial; they
  do not show that a module will load or bind.

### S4 · TB-10 then TB-45: daemon on the panel

Starts only after S3's return was seen on the key row.

| Step | Owner | Authorized target | Entry state | Expected changes | Stop criterion | Return procedure | Evidence (after-comparison) |
|---|---|---|---|---|---|---|---|
| 4.1 TB-10 static frame | Human (display owned by `touchbard` alone; input and brightness owners named before start) | The display backend TB-10 delivers | S3 verified; one owner each for display, input and brightness written down | One static frame on the panel | Wrong orientation or colors, a second display owner, any input duplicated | The TB-10 return procedure, then S3's return to key mode | Photo or note of the static frame; on exit: config 1, Esc/F1–F12 row, Fn, keyboard and trackpad, brightness, suspend/resume, normal boot |
| 4.2 TB-45 one harmless tap | Human | One tap on a harmless key button of the shown layer | 4.1 verified | One key event to a disposable focused application | The tap acts twice, acts on another button, or the bar stops drawing | The TB-45 return procedure, then S3's return to key mode | The single expected key seen; the same exit checks as 4.1, logged with any exception |

## Driver compile experiment (compile-only)

Reproducible from a fresh checkout without an installer, privileged writes or a connected panel.
It builds two modules in a temp directory and stops; nothing is installed, signed or loaded.

Pinned inputs:

| Input | Revision | SHA-256 |
|---|---|---|
| Linux stable, `drivers/gpu/drm/tiny/appletbdrm.c` (stock) | `497791a0a96b866d90add64041c47acdff49cae6` | `44d7ed7d8d0fc7ee86c479f9123dd1640cf68066201a35922d783abd6f4e4d29` |
| Linux stable, `drivers/hid/hid-appletb-bl.c` (stock) | `497791a0a96b866d90add64041c47acdff49cae6` | `e1126767e0668f8b82d23e7904b4f180b5b5f6b51aa0a144339e601dce31dd25` |
| Seborider `kernel/patches/appletbdrm-t1.patch` | `3a00d8dceb2cfeb75c3e6b9f501ead095c1d9924` | `6621a2669b66afdd695c819e42deefd7b17f447f186ee393f1c2440deecede8e` |
| Seborider `kernel/patches/hid-appletb-bl-t1.patch` | `3a00d8dceb2cfeb75c3e6b9f501ead095c1d9924` | `932aec1bb0a4358179b81f90a17e84deec4362f7d7134d4b7ed982a7e1b9e642` |
| Patched `appletbdrm.c` (expected output) | — | `bd3348ed92234ea22dc7bc2f30ec521df1b4606549767e0dedf445fae2d9626b` |
| Patched `hid-appletb-bl.c` (expected output) | — | `761816cc6dd3c0e2fa04a056b51bdc3248385094e3857bea40589aa5c132756d` |

```sh
WORK=$(mktemp -d)
K=https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/plain
S=https://raw.githubusercontent.com/Seborider/mbp-t1-touchbar/3a00d8dceb2cfeb75c3e6b9f501ead095c1d9924
curl -fsSo "$WORK/appletbdrm.c" "$K/drivers/gpu/drm/tiny/appletbdrm.c?id=497791a0a96b866d90add64041c47acdff49cae6"
curl -fsSo "$WORK/hid-appletb-bl.c" "$K/drivers/hid/hid-appletb-bl.c?id=497791a0a96b866d90add64041c47acdff49cae6"
curl -fsSo "$WORK/appletbdrm.patch" "$S/kernel/patches/appletbdrm-t1.patch"
curl -fsSo "$WORK/hid-appletb-bl.patch" "$S/kernel/patches/hid-appletb-bl-t1.patch"
cd "$WORK"
sha256sum appletbdrm.c hid-appletb-bl.c appletbdrm.patch hid-appletb-bl.patch  # compare with the table
patch --fuzz=0 appletbdrm.c appletbdrm.patch
patch --fuzz=0 hid-appletb-bl.c hid-appletb-bl.patch
sha256sum appletbdrm.c hid-appletb-bl.c                                         # the two expected outputs
echo 'obj-m := appletbdrm.o hid-appletb-bl.o' > Kbuild
nice -n 19 make -C /usr/lib/modules/7.2.15-1.1-omarchy-mbp143ab/build M="$WORK" -j1 modules
```

Observed on 2026-11-22 against `7.2.15-1.1-omarchy-mbp143ab` (installed headers, gcc 16.2.1,
binutils 2.47), run offline inside a bubblewrap sandbox with no network and read-only `/usr`:
the patched sources build both modules with exit 0 and zero warnings, `vermagic` matching the
installed kernel and the `05ac:8600` aliases. Seborider's shipped `kernel/appletbdrm.c` (SHA-256
`6d67d0489fc24c35f50960f1e5664fbbb0c62126422cd7302acb7bd2fe8d710d`, the file its installer hands to
DKMS) fails with exit 2: the `drm_atomic_state` plane-helper signatures are gone in 7.2.15.

Limits: compile-only. It says nothing about loading (the modules are unsigned), binding to the T1,
drawing a frame, suspend, or returning to key mode. The `.ko` hashes are for that one build and are
not claimed reproducible. Do not copy the modules into any module tree: the patched backlight
module's new HID alias could autoload in key mode. Rebuild from the hashed sources for a human trial
rather than reusing old build output.

## Three things that are not the same

### Factory-default layout reset in the configurator

Not implemented in TB-46; it is a follow-up ticket (TB-48). Preconditions it must meet: it changes
only the draft; it shows the preview before anything is saved; it can be cancelled; and before Save
it guarantees a recoverable copy of the current file and goes through TB-43's compare-and-swap save.

### Restoring a personal config copy

Putting back a copy of the user's own config: S0's `restore` on a throwaway profile, or a manual
copy. It never touches the hardware and never saves over the personal layout from an agent.

### Physical return to key mode

The S2 and S3 return procedures and B → A: service files, module tree and boot options, firmware.
Shown only by the firmware key row on the panel.

## Resumen para el usuario

- **S0** lo corre un agente sin hardware: `scripts/rehearsal.py run` crea un perfil privado
  desechable con un tema de fixtures, prueba cinco escenarios de config (ausente, con comentario y
  modo 0640, inválido, symlink) y el simulador, restaura con el lock de TB-43 y compara; `cleanup`
  borra solo lo propio sin cambios. La sesión persistente de `try-app.sh` no se lee ni se toca.
- **S1–S4 y B → A** son solo tuyos: cada etapa tiene responsable, objetivo autorizado, estado de
  entrada, cambios esperados, criterio de parada, retorno y evidencia. La línea base A se vuelve a
  registrar al empezar cada etapa con `rehearsal.py baseline`; B es modo teclas verificado tras TB-28.
- **TB-9** sigue sin ruta temporal acordada: sus comandos están marcados `TBD by TB-9` y en blanco
  significa no seguir. El retorno físico a la configuración 1 no está comprobado; solo la fila de
  teclas vista en el panel lo demuestra.
- La compilación de los drivers está fijada por commit y hash y se puede repetir en un directorio
  temporal; solo prueba que compila, nada de carga, enlace ni retorno.
- El reset de fábrica en el configurador **no** está hecho aquí (TB-48).
