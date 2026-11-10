"""Arch/Omarchy prebuild facts; never execute toolchain installation shims."""
import os
import platform
import shutil
from pathlib import Path

from doctor_support import ROOT, capture, row

PACKAGES = "sudo pacman -S --needed base-devel git pkgconf bash coreutils util-linux libxkbcommon wayland fontconfig vulkan-icd-loader ttf-dejavu"


def platform_check():
    try:
        release = Path('/etc/os-release').read_text()
    except OSError:
        release = ''
    supported = platform.system() == 'Linux' and any(
        line.startswith(('ID=arch', 'ID="arch"', 'ID_LIKE=arch', 'ID_LIKE="arch'))
        for line in release.splitlines())
    return row('platform', 'ok' if supported else 'failed', True,
               'Supported Linux Arch-family environment' if supported else 'Supported recipe requires Arch-family Linux/Omarchy',
               ['Linux; Arch family' if supported else 'Other platforms require separate dependency validation'],
               [] if supported else ['Use the supported Arch/Omarchy recipe in README.md'])


def tools_check():
    tools = ['git', 'pkg-config', 'bash', 'nice', 'flock', 'setsid']
    missing = [name for name in tools if not shutil.which(name)]
    if not any(shutil.which(name) for name in ('cc', 'gcc', 'clang')):
        missing.append('C compiler/linker')
    return row('build.tools', 'failed' if missing else 'ok', True,
               'Missing tools: ' + ', '.join(missing) if missing else 'Build and launcher tools available',
               ['Python 3.11+ bootstrap'], [PACKAGES] if missing else [])


def toolchain_checks():
    # Resolve only existing concrete toolchain directories, never PATH rustc/cargo/mise shims.
    import tomllib
    pin = tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    home = os.environ.get('RUSTUP_HOME')
    if not home and os.environ.get('HOME'):
        home = str(Path(os.environ['HOME']) / '.rustup')
    base = Path(home) / 'toolchains' if home else None
    candidates = sorted(base.glob(pin + '-*/bin')) if base and base.is_dir() else []
    checks = []
    for id, tool in [('rust.toolchain', 'rustc'), ('rust.rustfmt', 'rustfmt'), ('rust.clippy', 'clippy-driver')]:
        found = False
        reason = 'not installed'
        for directory in candidates:
            executable = directory / tool
            if executable.is_file() and os.access(executable, os.X_OK):
                code, output, problem = capture([str(executable), '--version'])
                found = code == 0 and not problem and (tool != 'rustc' or pin in output.decode('utf-8', 'replace').split())
                if found and tool == 'rustc':
                    cargo = directory / 'cargo'
                    cargo_code, cargo_output, cargo_problem = capture([str(cargo), '--version'])
                    found = cargo_code == 0 and not cargo_problem and pin in cargo_output.decode('utf-8', 'replace').split()
                reason = problem or 'version did not match pinned toolchain'
                if found:
                    break
        checks.append(row(id, 'ok' if found else 'failed', True,
                          f'Pinned {pin} {tool} executable verified' if found else f'Pinned {pin} {tool}: {reason}',
                          [], ['mise install'] if not found else []))
    return checks


def library_checks():
    checks = []
    pkg = shutil.which('pkg-config')
    for id, name in [('libs.xkbcommon', 'xkbcommon'), ('libs.wayland', 'wayland-client'), ('libs.fontconfig', 'fontconfig')]:
        code, _, problem = capture([pkg, '--modversion', name]) if pkg else (None, b'', 'pkg-config missing')
        checks.append(row(id, 'ok' if code == 0 and not problem else 'failed', True,
                          f'{name} pkg-config query succeeded' if code == 0 and not problem else f'{name}: {problem}',
                          [], [] if code == 0 and not problem else [PACKAGES]))
    return checks


def vulkan_check():
    # Supported Arch loader location; broad ldconfig output can exceed the capture cap.
    has_loader = Path('/usr/lib/libvulkan.so.1').is_file()
    dirs = [Path('/usr/share/vulkan/icd.d'), Path('/etc/vulkan/icd.d')]
    has_icd = any(any(p.glob('*.json')) for p in dirs if p.is_dir())
    ready = has_loader and has_icd
    return row('vulkan.loader', 'ok' if ready else 'failed', True,
               'Vulkan loader and ICD metadata found' if ready else 'Vulkan loader or driver metadata unavailable',
               ['Package/manifest presence does not prove a working GPU'],
               [] if ready else ['sudo pacman -S --needed vulkan-icd-loader vulkan-intel',
                                    'Software Vulkan alternative: sudo pacman -S --needed vulkan-swrast'])


def font_check():
    command = shutil.which('fc-match')
    code, output, problem = capture([command, '--format=%{file}', 'monospace']) if command else (None, b'', 'fc-match missing')
    value = output.decode('utf-8', 'replace').strip()
    found = code == 0 and not problem and bool(value) and Path(value).is_file()
    return row('font.monospace', 'ok' if found else 'failed', True,
               'Fontconfig resolves an existing monospace font' if found else 'No usable monospace font resolved',
               ['Nerd Font optional; icon glyphs may be unavailable with a basic monospace font'],
               [] if found else ['sudo pacman -S --needed fontconfig ttf-dejavu'])


def checks():
    return [platform_check(), tools_check(), *toolchain_checks(), *library_checks(), vulkan_check(), font_check()]
