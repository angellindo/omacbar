# 0007 · Retiring `macbook-touchbar-local.service` safely (laptop only)

Status: resolved (TB-26) · Machine: `MacBookPro14,3`, kernel `7.2.15-1-omarchy`, `apple-ib-drv-dkms 0.3.0-5`,
T1 in key mode (USB config 1) · Investigated read-only on 2026-11-07; the human runs every command below.

## Question

Can the local Touch Bar display service be stopped, disabled and removed without losing the internal
keyboard or touchpad, or the ability to recover with the internal keyboard alone? If yes, what is the
staged procedure, with a rollback and a go/no-go at every stage, and where does it sit relative to
the external T1 firmware backup (TB-9 gate)?

Sources: `S1` = `/usr/local/lib/macbook-touchbar-local.py` (sha256 `f6e3b706…39db`, identical to
`~/Documents/mac/experiments/touchbar-display-local/macbook-touchbar-local.py`); `S2` = `manage.py`
in the same folder; `S3` = that folder's `README.md`, `APPLIED-20260921.json`, `validation-1789967932.json`;
`S4` = `~/Documents/mac/HANDOFF.md` (top section, 2026-09-20/21); `S5` = `/usr/src/apple-ib-drv-0.3.0/apple-touchbar.c`;
`S6` = `~/Documents/mac/mac-pc6/HANDOFF.md`, `TAREAS.md` and `05-gpu-pantalla/t56{b,c}-*/README.md` (SOCW, Oct 2026);
`J` = `journalctl -u macbook-touchbar-local.service` (history back to 2026-09-21). Live commands are quoted inline.

## Findings

### 1. What the service installs and changes

`manage.py` manages exactly three files (`S2`, `FILES`); live hashes match `APPLIED-20260921.json` (`sha256sum`).

| Item | What it is | Removed by `manage.py remove` |
|---|---|---|
| `/usr/local/lib/macbook-touchbar-local.py` | The helper (`run`, `recover`, `sleep`, `wake`, `status`, `trial`) | yes |
| `/etc/systemd/system/macbook-touchbar-local.service` | `ExecStart … run`, `ExecStopPost … recover`, `Restart=on-failure`, `RestartPreventExitStatus=78`, `CapabilityBoundingSet=` (empty), `ProtectKernelTunables/Modules=yes`, `ProtectSystem=strict`, only `/run/macbook-touchbar-local` writable; enabled, `WantedBy=multi-user.target` (`systemctl cat`) | yes |
| `/etc/systemd/system/systemd-suspend.service.d/94-macbook-touchbar-display.conf` | **A systemd-suspend drop-in, not a modprobe file.** `ExecStartPre=-… sleep`, `ExecStopPost=-… wake` (the `-` prefix ignores failures). Not present in `/etc/modprobe.d` or `/usr/lib/modprobe.d` (`ls`) | yes |
| `/run/macbook-touchbar-local/` | `control.sock` (root-only), `lock`, `restore.json` (saved ON report, exists only while running), possibly `restore-stale-*.json` (`S1`). Kept after stop (`RuntimeDirectoryPreserve=yes`), gone at reboot | no |
| `/var/lib/macbook-touchbar-local/` | `installed.json` (manifest with hashes and state), `helper-before-*.py` if `update-helper` ever ran (`S2`) | no (evidence kept; manifest set to `state: removed`) |

What the service relies on but does **not** own (all untouched by `remove`):

| File / param | Owner | Value now (`/sys/module/*/parameters`) |
|---|---|---|
| `/usr/lib/modprobe.d/apple-touchbar.conf` | package `apple-ib-drv-dkms` (`pacman -Qo`) | `apple_ibridge skip_acpi_power=1` (service refuses to run without it) |
| `/etc/modprobe.d/90-macbook-pr298-fn.conf` | local preference | `apple_touchbar fnmode=2` |
| `/etc/modprobe.d/apple-touchbar-idle.conf` (2026-09-22) | local preference | `idle_timeout=-1 dim_timeout=-1` |
| `/usr/lib/udev/rules.d/99-ibridge.rules` | package | sets T1 `bConfigurationValue=1`, `power/control=on` |
| `/etc/udev/rules.d/99-mbp143-t1-nowake.rules` | local (Sept 24) | T1 `power/wakeup=disabled` |
| `/usr/lib/systemd/system-sleep/mbp143-t1-socw` | local (T-56d, 2026-10-15) | ACPI `SOCW(0)` before S3, `SOCW(1)` after (see §3) |

The service also checks module `srcversion` (`apple_ibridge 5DE667D5…`, `apple_touchbar 60828F8B…`) and the
hash of `apple-touchbar.c` (`ed4e076a…`); both still match (`cat /sys/module/*/srcversion`, `sha256sum`).
On any mismatch it exits 78 and is not restarted (`S1` `known_driver`).

What `recover` writes (`S1` `restore_saved`): if `restore.json` exists and the T1 identity (boot id, USB path,
inode, devnum, config 1, hidraw node) is unchanged, one `HIDIOCSFEATURE` of report 2 with the saved ON
bytes on `/dev/hidraw0`, then a `HIDIOCGFEATURE` readback, then it deletes the record. If identity changed it
renames the record and writes nothing. With no record it writes nothing.

What `manage.py remove` does, in order (`S2`): verify all three files against the manifest hashes (refuses
on any edit) → `systemctl disable` → `systemctl stop` → if `restore.json` still exists, stop with an error
and keep everything → unlink the three files → `systemctl daemon-reload` → manifest `state: removed`.
Note: after `remove`, `manage.py stage` refuses to reinstall because the manifest still exists.

### 2. Input path: the service cannot take the keyboard or touchpad

Live `/proc/bus/input/devices`:

| Device | Bus / path | Handlers |
|---|---|---|
| `Apple SPI Keyboard` | `0x001c` (SPI), `spi-APP000D:00`, driver `applespi` | `sysrq kbd leds event5 tbkbd` |
| `Apple SPI Touchpad` | SPI, `spi-APP000D:00` | `event6 mouse0 tbtpad` |
| `iBridge Virtual HID …/1d6b:0301` (Esc, F1-F12, media) | USB `1-3:1.2`, driver `apple-touchbar` | `kbd event15` |
| T1 display report interface | USB `1-3:1.3`, driver `hid-sensor-hub`, `/dev/hidraw0` | n/a (descriptor sha256 `80e03f01…` = the one `S1` accepts) |

T1: `/sys/bus/usb/devices/1-3` = `05ac:8600`, `bConfigurationValue 1`, `power/control on`, `power/wakeup disabled`.

| Risk | Finding | Source |
|---|---|---|
| Exclusive grab | Input nodes are opened `O_RDONLY`; the only input ioctl is `EVIOCGSW` (read lid switch). No `EVIOCGRAB` anywhere in the helper or `manage.py` | `S1` `open_inputs`, `lid_closed`; `grep EVIOCGRAB\|0x90` = none |
| Driver parameters | Never written: the unit has `ProtectKernelTunables=yes` and no capabilities; the helper only reads `idle_timeout` | `S1` line 259; unit file |
| Module unload | Impossible from the unit (`ProtectKernelModules=yes`, empty capability set); `manage.py` only runs `systemctl` and unlinks files | `S2` |
| udev rule | None installed | `S2` `FILES` |
| `tbkbd` / `tbtpad` | These handles belong to the `apple_touchbar` **driver** (passive `input_handler`, `input_open_device`, no grab), not the service; stopping the service does not touch them | `S5` `appletb_inp_connect` |
| Keyboard independent of the T1 | Keyboard and touchpad worked through `applespi` while the T1 was dead in recovery (`05ac:1281`) in Sept | `~/Documents/mac/PLAN-CORREGIDO.md` "Estado real…17 de septiembre" |

Confidence: high. Stopping or removing the service closes read-only file descriptors and nothing else on
the input side.

### 3. Bar display state without the service

Background facts:
- On T1 the driver's display ops are a no-op (`appletb_set_tb_disp` returns 0 when `is_t1`), so
  `idle_timeout`/`dim_timeout` never turn the panel off; HANDOFF records idle-off and mode-OFF both
  **failed visually**. Only the HID feature report 2 on `hidraw0` (ON=1, OFF=4) switches the panel (`S5`; `S4`).
- On system suspend the driver sets mode OFF and on resume restores the mode (`appletb_suspend`, `appletb_reset_resume`, `S5`).
- Since 2026-10-15 the `mbp143-t1-socw` hook calls ACPI `SOCW(0)` before S3 and `SOCW(1)` after. Awake,
  `SOCW(0)` turned the bar off and `SOCW(1)` turned it back on, seen by the user, with the service idle (T-56b, `S6`).
  Before this hook the bar did **not** come back after S3 (T-33, T-36, T-44) **even with the service running**;
  a reboot recovered it (`S6` `TAREAS.md`). Since the hook, every S3 shows `SOCW(0x1) retorno '0x0'` and
  `Touchbar resumed` (`journalctl -t mbp143-t1-socw`, `journalctl -k`), and the user reported no bar failures (2026-10-16, `S6`).
- `mem_sleep` is `s2idle [deep]`; logind `HandleLidSwitch=suspend` (`busctl get-property … HandleLidSwitch`).

| Scenario | With the service (today) | After stop / removal (expected) | Source | Confidence |
|---|---|---|---|---|
| Right after `systemctl stop` | — | **ON**: the exit path restores the saved ON report and verifies it by readback | `S1` `finally`; `validation-1789967932.json` `normal_stop_readback = 02050100000000e8030000`; `J`: 216 "Original display state restored on exit", 0 failures | high |
| Awake, idle | ON (service idle-off disabled: `idle_timeout=-1` since 09-22, journal "Ready … idle_timeout: -1") | ON, unchanged (driver idle is a no-op on T1) | `S5`; `/etc/modprobe.d/apple-touchbar-idle.conf` | high |
| Lid close → suspend | OFF at lid close, ~2 s before `PM: suspend entry` | Stays lit those ~2 s, then `SOCW(0)` turns it off for the S3 | `J` vs `journalctl -k` timestamps; T-56b | high |
| Lid closed, **no** suspend (docked/clamshell, inhibitor, failed suspend) | OFF | **Stays lit** with the lid closed | `S1` `Policy.reason`; Omarchy `omarchy-system-lid-close` (docked lid does not suspend) | high |
| Resume from S3 | `SOCW(1)` (hook) then the service's `wake` re-sends ON 3 s later | Expected ON via `SOCW(1)` alone, as in T-56b awake; **not yet observed across a real S3 without the service** | `journalctl -t mbp143-t1-socw` 22:30:59 vs `J` 22:31:02 "control-wake"; T-56b | medium (unverified for S3) |
| s2idle | Hook does nothing; service OFF/ON | Unknown; this machine uses `deep` | hook source | low (not exercised) |
| Reboot | Panel ON at boot: the service refuses to start unless report = ON, and it started 240 times without that refusal | ON | `S1` `run()` "Expected panel ON"; `J` 240 "Ready", 0 refusals; Sept 18 physical test before the service existed (`PLAN-CORREGIDO.md`) | high |

The service has already been down for whole sessions (start-limit-hit on 2026-09-23 and four times on
2026-10-22, `J`), so "no service" is not a new machine state.

### 4. Why it was installed and whether the problem returns

Installed 2026-09-21 because the driver cannot switch the T1 panel off, so idle-off did not work; the
service drove report 2 for idle-off, input-wake and lid-closed-off (`S3` README "Qué corrige"; `S4`).
Since 2026-09-22 idle-off is deliberately disabled (`idle_timeout=-1`, "keep the Touch Bar lit while awake"),
and since 2026-10-15 the S3 off/on is done by `SOCW`. What returns once it is gone:

- The bar stays lit with the lid closed when the machine does not suspend (docked, inhibited, failed suspend).
- No idle-off: same as today by choice. Pixel mode will own idle dim/off (TB-24).
- The only open question is the resume case above (medium); stage 1 tests it with a free rollback.

### 5. What is lost if the bar stays dark, and fallbacks to set up first

The A1707 keyboard has no function row: Esc, F1-F12 and the Fn media layer come only from the bar device
(`event15`, `S4` key capture "Esc, F1–F12 without Fn … media with Fn"). Fn itself is on the SPI keyboard
(`applespi` keymap, `/usr/src/mbp143-applespi-0752b/applespi.c`).

| Lost | Fallback (prepare before stage 1) | Source / status |
|---|---|---|
| Esc in terminals, vim, TUIs | `Ctrl+[` sends ESC (ASCII 0x1b) | standard; verify with `cat -v` |
| Esc in GUI apps | Optional Hyprland binding in `~/.config/hypr/bindings.lua`, modelled on Omarchy's own `send_key_state` helper (`/usr/share/omarchy/default/hypr/bindings/clipboard.lua`); see stage 0 | syntax for an empty `mods` **unverified**: test while the bar works |
| `SUPER+ESCAPE` system menu | Power button (short press) → system menu (`XF86PowerOff`, `locked=true`); `SUPER+SPACE` → Omarchy menu | `/usr/share/omarchy/default/hypr/bindings/utilities.lua` |
| Ctrl+Alt+F1-F12 (VT switch) | `sudo chvt 2` from a terminal (`/usr/bin/chvt`) | live `which` |
| Brightness, volume, media (Fn+bar) | `brightnessctl set 50%`, `wpctl set-volume @DEFAULT_AUDIO_SINK@ 50%` | live `which` |
| F9 push-to-talk (dictation) | `SUPER+CTRL+X` toggle | `~/.config/hypr/bindings.lua` |
| Everything, if the bar is dark after resume | `systemctl reboot` (typed, or power-button menu): the bar comes up ON at boot | §3 reboot row |

The magic SysRq key is not a fallback: the keyboard has no SysRq key.

## Decision input

**Verdict: safe with preconditions.** Stopping, disabling and removing the service cannot disable the
internal keyboard or touchpad (high confidence), and every rollback below is typeable on the internal
keyboard. Preconditions: (1) keep `mbp143-t1-socw` installed with `mem_sleep=deep`, because it, not the
service, brings the bar back after S3; (2) set up the fallbacks in stage 0; (3) run it awake, on the
charger, with the bar lit and no `restore.json` pending.

**Ordering with the external T1 firmware backup:** no hard ordering. Retirement touches neither the ESP,
the T1 firmware, the driver nor the USB configuration. Doing the backup first is recommended because it
is cheap and the last-resort recovery of every stage is a reboot, which loads the T1 from the ESP set.
Both must be done before the TB-9 driver/config change.

### Staged procedure (human, internal keyboard only)

Every command is plain text plus Enter; no Esc or F-key is needed. `sudo` asks for the password.

| Stage | Do | Check (all must pass) | Rollback | Go / no-go |
|---|---|---|---|---|
| 0 Prepare | Open two terminals. Create `~/tb26-rollback.sh` containing `sudo systemctl enable --now macbook-touchbar-local.service`. Learn `Ctrl+[`. Optional: add and test the Esc binding (below). Plug in the charger. | `systemctl is-active macbook-touchbar-local.service` = `active`; `cat /sys/bus/usb/devices/1-3/bConfigurationValue` = `1`; `sudo cat /var/lib/macbook-touchbar-local/installed.json` shows `state: enabled`; `sudo ls /run/macbook-touchbar-local/` lists `restore.json` (normal while running); in `cat -v`, `Ctrl+[` prints `^[` | nothing changed | Go when all checks match |
| 1 Stop | `sudo systemctl stop macbook-touchbar-local.service` | `systemctl is-active …` = `inactive`; `journalctl -u macbook-touchbar-local -n 3` shows "Original display state restored on exit"; `sudo ls /run/macbook-touchbar-local/` has **no** `restore.json`; bar lit; in `cat -v`: Esc → `^[`, F1 → `^[OP`, F12 → `^[[24~`; typing and touchpad work; `grep -A5 'Apple SPI' /proc/bus/input/devices` still lists `tbkbd` / `tbtpad`. Then one short lid-close suspend (2-3 min): after opening, bar lit within 5 s, keys work, `journalctl -t mbp143-t1-socw -n 3` shows `SOCW(0x1) … retorno '0x0'` | `sudo systemctl start macbook-touchbar-local.service`. If the bar is dark: `systemctl reboot` (service is still enabled and returns) | Go only if every check passes. No-go → rollback, report back |
| 2 Disable | `sudo systemctl disable macbook-touchbar-local.service` | `systemctl is-enabled …` = `disabled`; the three files still exist | `bash ~/tb26-rollback.sh` | Go if disabled and bar unchanged |
| 3 Observe (1-2 days) | Normal use, including one reboot, one S3 of ≥ 30 min on battery, and long idle while awake | After each: bar lit and keys work in `cat -v`; after reboot `systemctl is-active …` = `inactive`; `journalctl -u systemd-suspend.service -b \| grep 'Display service unavailable'` (drop-in ran with no device writes) | `bash ~/tb26-rollback.sh`; if the bar is dark first `systemctl reboot`, then the script | Go if no dark bar across ≥ 3 resumes and 1 reboot |
| 4 Remove | `pkexec /usr/bin/python3 -I /home/angellindo/Documents/mac/experiments/touchbar-display-local/manage.py remove` | Prints "Removed managed files…"; `systemctl cat macbook-touchbar-local.service` → not found; `systemctl cat systemd-suspend.service \| grep 94-` → nothing; one more lid-close suspend: bar lit after resume | Reinstall from the sources (hashes equal the installed ones), see below | Done. If `remove` refuses (edited file or pending restore), stop and report; do not delete by hand |

Stage 4 rollback (`manage.py stage` refuses while the manifest exists):

```sh
D=/home/angellindo/Documents/mac/experiments/touchbar-display-local
sudo install -m 0755 $D/macbook-touchbar-local.py /usr/local/lib/macbook-touchbar-local.py
sudo install -m 0644 $D/macbook-touchbar-local.service /etc/systemd/system/macbook-touchbar-local.service
sudo install -m 0644 $D/94-macbook-touchbar-display.conf /etc/systemd/system/systemd-suspend.service.d/94-macbook-touchbar-display.conf
sudo systemctl daemon-reload && sudo systemctl enable --now macbook-touchbar-local.service
```

`start` refuses if the panel report is not ON ("Expected panel ON"); in that case reboot first.

Optional GUI Escape binding for stage 0 (unverified; keep it only if pressing it closes a menu while the
bar still works):

```lua
o.bind("SUPER + CTRL + GRAVE", "Escape (Touch Bar fallback)", function()
  hl.dispatch(hl.dsp.send_key_state({ mods = "", key = "Escape", state = "down" }))
  hl.timer(function()
    hl.dispatch(hl.dsp.send_key_state({ mods = "", key = "Escape", state = "up" }))
  end, { timeout = 50, type = "oneshot" })
end)
```

After removal, the human's own notes will flag drift: `mac-pc6` checklist items C24, C31 and S2 and
`/usr/local/share/mbp143-local-patches/INVENTARIO.md` list these files. Keep `90-macbook-pr298-fn.conf`,
`apple-touchbar-idle.conf` and `mbp143-t1-socw`: they are key-mode preferences and the S3 fix, not part
of the service.

## Sources

- Live, 2026-11-07 (read-only): `systemctl cat|show macbook-touchbar-local.service`, `sha256sum` of the
  three managed files, `/proc/bus/input/devices`, `/sys/bus/usb/devices/1-3/*`, `/sys/class/hidraw/*`,
  `/sys/bus/hid/devices/*/driver`, `/sys/module/{apple_touchbar,apple_ibridge,applespi}/parameters/*`,
  `/sys/power/mem_sleep`, `busctl get-property org.freedesktop.login1 … HandleLidSwitch`,
  `journalctl -u macbook-touchbar-local.service`, `journalctl -t mbp143-t1-socw`, `journalctl -k -b 0`,
  `ls /etc/modprobe.d /usr/lib/modprobe.d /etc/udev/rules.d /usr/lib/systemd/system-sleep`, `pacman -Qo`.
- Root-only, not read: `/var/lib/macbook-touchbar-local/`, `/run/macbook-touchbar-local/` (stage 0 commands).
- `~/Documents/mac/experiments/touchbar-display-local/`: `macbook-touchbar-local.py`, `manage.py`,
  `README.md`, `APPLIED-20260921.json`, `validation-1789967932.json`, `94-macbook-touchbar-display.conf`.
- `~/Documents/mac/HANDOFF.md` (2026-09-20/21 sections), `~/Documents/mac/PLAN-CORREGIDO.md`.
- `~/Documents/mac/mac-pc6/HANDOFF.md`, `TAREAS.md` (T-33, T-36, T-44, T-56b/c/d),
  `05-gpu-pantalla/t56c-s3-socw-2026-10-15/README.md`, `/usr/lib/systemd/system-sleep/mbp143-t1-socw`.
- `/usr/src/apple-ib-drv-0.3.0/apple-touchbar.c` (sha256 `ed4e076a…`): `appletb_set_tb_disp`,
  `appletb_inp_connect`, `appletb_suspend`, `appletb_reset_resume`.
- `/usr/share/omarchy/default/hypr/bindings/{utilities,clipboard,media}.lua`, `omarchy-system-lid-close`.
