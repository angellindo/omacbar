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

## Physical bar: pending

The repository does not install or enable the pixel-mode display stack. A green simulator/doctor
cannot establish panel operation. Follow the prerequisites and human gates in
[hardware research](research/0001-t1-touch-bar-hardware.md) and
[firmware findings](research/0006-macos-reference-stack.md) before separate hardware work.
Agents never load modules, edit udev/systemd/ESP, change device permissions, or write `/dev`.

Once an explicitly planned human hardware procedure provides a supported display backend,
record orientation, colors, touch alignment, latency, Fn/modifier behavior, theme switching and
recovery/suspend results. Until that work occurs, mark each physical result **unverified**.
