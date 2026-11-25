# Human verification checklist

Automated doctor results stop at config validation, an in-memory GPU frame, session-socket
reachability and input access metadata. Record the revision, what you exercised and the observed
result locally; share a redacted summary.

## Configurator and simulator

1. Run the README bootstrap/build/runtime doctor flow from a local desktop terminal. Open
   `scripts/try-app.sh`; confirm both windows appear and the simulator shows the F-key layer.
2. Edit a label or width and Save. Confirm the simulator updates. Close either window and
   confirm both processes stop. Reopen and confirm the isolated config persisted.
3. On a machine with an accessible Fn keyboard, hold Fn, then Fn+Ctrl, and release. Confirm
   the expected layers and return to default, including when the windows lack focus. If unavailable,
   record the limitation and inspect `.scratch/manual-test/logs/simulator.log` locally.
4. Tap an ordinary key button and inspect the file recording locally. The launcher records keys;
   command buttons execute their configured command. Keep recordings/config/log contents private.
5. Real `--emit uinput` operation requires existing human-configured permissions. Test only with
   an appropriate harmless key and a disposable focused application; report the observed result.
6. In the simulator window, press a key button, slide off it (onto another button and, separately,
   out of the window) and release. Confirm nothing runs and no button stays highlighted.

## Physical bar: pending

The repository does not install or enable the pixel-mode display stack. A green simulator/doctor
cannot establish panel operation. Follow the prerequisites and human gates in
[hardware research](research/0001-t1-touch-bar-hardware.md) and
[firmware findings](research/0006-macos-reference-stack.md) before separate hardware work.
The staged procedure, with an owner, entry state, stop criterion, return and evidence for every
stage and the full return B → A, is the [hardware runbook](hardware-runbook.md).
Agents never load modules, edit udev/systemd/ESP, change device permissions, or write `/dev`.

Once an explicitly planned human hardware procedure provides a supported display backend,
record orientation, colors, touch alignment, latency, Fn/modifier behavior, theme switching and
recovery/suspend results. Until that work occurs, mark each physical result **unverified**.

## Opt-in Linux adapters (TB-47)

Only the human operator runs `touchbard --drm` on the T1. The display and digitizer are explicit
opt-ins. Work from a foreground terminal using the internal keyboard.
[ADR-0014](adr/0014-kms-and-digitizer-adapters.md)

### Before starting

1. Start from baseline B, with the firmware key row working and the local key-mode service
   retired by TB-28. Follow the runbook's external firmware copy check on the day of the trial.
   [Baselines](hardware-runbook.md#baselines) · [S2 · TB-28](hardware-runbook.md#s2--tb-28-retire-the-local-service-key-mode-stays)
2. Wait for TB-9 to define and demonstrate the temporary pixel-mode route and its return to key
   mode. Recheck the runbook entry conditions on the running kernel and follow that route for
   this trial. A missing command, a different kernel or module set, or a failed return is a
   stop. [S3 · TB-9 pixel-mode trial](hardware-runbook.md#s3--tb-9-pixel-mode-trial-kernel-specific)
3. Keep the Touch Bar card off the desktop compositor's seat before the card appears. Record
   who owns display, input and brightness and who should own them after the run. Confirm
   tiny-dfr and other clients hold neither DRM master on the Touch Bar card nor the digitizer
   grab. The operator needs existing permissions for each selected device. If access or
   ownership is unclear, stop before launching the daemon.
   [T1 hardware prerequisites](research/0001-t1-touch-bar-hardware.md#hard-prerequisites-and-risks)

### TB-10 · Display

1. In a test profile, save a default layer with distinct labels and colors at both ends and
   harmless actions. Launch from the same profile. With the TB-9 route in pixel mode, run
   `touchbard --drm auto`. `auto` requires one matching card. If the trial calls for explicit
   selection, `--drm PATH` takes the verified `/dev/dri/cardN` node and still checks its
   identity and capabilities. Leave `--touch` and `--emit` out for this static test. Keep
   terminal stdin open while observing the panel. Without `--emit`, key actions are disabled.
   The daemon refuses an active or foreign KMS state before any modeset and leaves that
   presentation untouched. On a refusal, record the reason and follow the return procedure.
2. Check that the panel is lit and legible, the expected label is at the physical left end and
   colors and orientation match the layer. Record the driver, mode, orientation property,
   formats and sysfs identity shape where available. For a TB-10 damage check after the static
   frame passes, use `down <x>` (a bar pixel inside a harmless button) and `up` on stdin. Record
   whether the highlight appears and clears, and its visible latency if measured. Stop on a
   dark or stale panel, wrong orientation or colors, unexpected actions, or a device error.
   [S4 · TB-10 then TB-45](hardware-runbook.md#s4--tb-10-then-tb-45-daemon-on-the-panel)
3. The candidate `appletbdrm` source lists `BGR888` and `XRGB8888` and sets `RIGHT_UP` and
   `non-desktop`. Binding the driver can clear the display before the daemon starts. The
   driver discards the USB flush and clear results in its update and disable callbacks. A
   successful `SETCRTC` or `DIRTYFB` establishes only that the frame was submitted to the
   driver. Observe the panel to establish that it is lit and legible.
   [Driver source](https://github.com/gregkh/linux/blob/497791a0a96b866d90add64041c47acdff49cae6/drivers/gpu/drm/tiny/appletbdrm.c)
   · [Pinned T1 patch](https://github.com/Seborider/mbp-t1-touchbar/blob/3a00d8dceb2cfeb75c3e6b9f501ead095c1d9924/kernel/patches/appletbdrm-t1.patch)

### TB-45 · Digitizer

1. After TB-10 and its return pass, use TB-45's human evidence procedure to identify the
   digitizer. Record the evdev identity and name, reported multitouch capabilities and ranges,
   which position axis follows the length of the bar and which end has the lower value.
   Observe contact, movement and release with the TB-45 procedure so the profile has cited
   evidence for the axis, exact range and direction.
2. `--touch auto` selects a unique digitizer candidate. `--touch PATH` takes a matching
   `/dev/input/eventN` candidate. With `VERIFIED_DIGITIZERS` empty, either selection refuses
   every digitizer before the exclusive grab and touch reading. A matching candidate can report
   its identity, name and exposed axis bounds in the refusal. This metadata cannot establish
   which axis follows the bar or where its physical endpoints lie. Use TB-45's independent
   observation for those facts. There is no calibration flag or config override.
3. Once TB-45 adds a verified profile, use `touchbard --drm auto --touch auto --emit uinput`
   for the one-key test with the operator's existing device access. Tap a harmless key button
   once with a disposable application focused and confirm the expected key arrives once. Stop
   if the tap acts twice, hits another button, or drawing fails. Note the observed contact behavior and
   alignment locally, including any missed taps. A contact spanning startup or lost sync can
   be discarded. Observe a fresh contact after recovery.
   [S4 · TB-45 one harmless tap](hardware-runbook.md#s4--tb-10-then-tb-45-daemon-on-the-panel)

### Stop and verify the return

- Use Ctrl-C (or SIGTERM or SIGHUP) and wait for the process to exit before returning devices to their
  prior owners. The daemon stops its readers and releases the digitizer grab if it acquired
  one. Restore attempts to return a modeset card to its original inactive KMS state and
  release its resources and DRM master. The daemon reports restore failures and a vanished
  card. Treat either as a stop and inspect the panel and input devices.
- After SIGKILL, a device disconnect or power loss, there is no guaranteed frame, dark panel
  or return of the firmware keys.
- Follow the TB-10 or TB-45 return in the hardware runbook, then the S3 return to key mode.
  Verify USB config 1, observe the firmware Esc and F1–F12 row and check that the keys work.
  Check Fn, the internal keyboard and trackpad, brightness, suspend/resume and normal boot as
  specified in S4. Compare the post-return baseline with B and check the recorded owners.
  Stop for recovery-mode enumeration, a dark bar on return, missing keyboard or trackpad,
  or any failed return. Use the runbook's human recovery route and record the exception
  before continuing. The full B → A procedure is for reinstating the retired service when
  returning to baseline A. [Hardware runbook](hardware-runbook.md)
  · [Full return B → A](hardware-runbook.md#b--a--full-return-from-baseline-b-to-baseline-a)
- After each run, record the revision and kernel, selected mode and owners, visible result
  or refusal, exit status and cleanup messages, and what was seen and tested after return.
  Keep raw logs, photos and device details private. Share a redacted summary.

Recording-backend tests establish frame conversion, damage and cleanup calls. Synthetic evdev
replay establishes touch and cancellation behavior through the daemon. Actual T1 driver binding,
metadata, visible orientation and colors, first-frame and damage latency, digitizer mapping and
contacts, suspend/resume and the physical return remain **unverified**.
