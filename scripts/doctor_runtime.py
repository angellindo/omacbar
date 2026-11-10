"""Native validation and local socket reachability, with honest scope limits."""
import os
import socket
from pathlib import Path

from doctor_support import ROOT, BUILD, capture, decode_report, row

IDS = ['runtime.probe', 'runtime.config', 'runtime.theme', 'runtime.gpu', 'runtime.window',
       'input.fn', 'input.uinput', 'hardware.bar']


def native_checks():
    target = Path(os.environ.get('CARGO_TARGET_DIR', 'target'))
    if not target.is_absolute():
        target = ROOT / target
    probe = target / 'debug/touchbar-doctor-probe'
    dependent = ['runtime.config', 'runtime.theme', 'runtime.gpu', 'input.fn', 'input.uinput']
    found = probe.is_file() and os.access(probe, os.X_OK)
    rows = {id: row(id, 'skipped', id.startswith('runtime.'), 'Native validation unavailable', next=[BUILD]) for id in dependent}
    state = 'ok' if found else 'skipped'
    message = 'Native report protocol verified' if found else 'Native probe not built'
    if found:
        for kind, expected in [('config', dependent[:2]), ('gpu', dependent[2:3]), ('input', dependent[3:])]:
            argv = [str(probe), '--check', kind]
            if kind == 'config':
                home = os.environ.get('HOME')
                if not home or not Path(home).is_absolute():
                    for id in expected:
                        rows[id] = row(id, 'failed', True, 'Absolute HOME required for config validation', next=['Set HOME to your home directory and rerun doctor'])
                    continue
                argv += ['--home', home]
            code, data, problem = capture(argv, timeout=20 if kind == 'gpu' else 5, limit=65536)
            try:
                value = decode_report(data)
                if code or problem or [c['id'] for c in value['checks']] != expected:
                    raise ValueError('unusable native protocol')
                for c in value['checks']:
                    c['required'] = c['id'].startswith('runtime.')
                    rows[c['id']] = c
            except (ValueError, TypeError, UnicodeError):
                state, message = 'failed', 'Native probe failed: ' + (problem or 'invalid JSON protocol')
                for id in expected:
                    rows[id] = row(id, 'failed', id.startswith('runtime.'), message, next=[BUILD, 'Rerun scripts/doctor.py --profile runtime'])
    return row('runtime.probe', state, True, message, next=[] if state == 'ok' else [BUILD]), rows


def window_check():
    wayland = os.environ.get('WAYLAND_DISPLAY')
    display = os.environ.get('DISPLAY', '')
    path = None
    if wayland:
        if Path(wayland).is_absolute():
            path = wayland
        elif os.environ.get('XDG_RUNTIME_DIR'):
            path = str(Path(os.environ['XDG_RUNTIME_DIR']) / wayland)
    elif display.startswith(':') and display[1:].split('.')[0].isdigit():
        path = '/tmp/.X11-unix/X' + display[1:].split('.')[0]
    elif display:
        return row('runtime.window', 'skipped', True, 'Remote/unsupported DISPLAY: network X11 not probed', next=['Run in a local Wayland/X11 desktop session'])
    if path:
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                connection.settimeout(1)
                connection.connect(path)
            return row('runtime.window', 'ok', True, 'Local window-session socket reachable', ['Reachability does not prove GUI creation or presentation'])
        except OSError:
            pass
    return row('runtime.window', 'failed', True, 'No reachable local window-session socket',
               ['No GUI creation or presentation tested'], ['Run in a local Wayland/X11 desktop session'])


def checks(profile):
    if profile == 'build':
        return [row(id, 'skipped', False, 'Outside build profile; request --profile runtime') for id in IDS]
    probe, native = native_checks()
    return [probe, native['runtime.config'], native['runtime.theme'], native['runtime.gpu'],
            window_check(), native['input.fn'], native['input.uinput'],
            row('hardware.bar', 'skipped', False, 'Physical pixel-mode support pending; manual checklist required',
                next=['Read docs/hil-checklist.md and docs/research/0001-t1-touch-bar-hardware.md'])]
