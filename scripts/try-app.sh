#!/usr/bin/env bash
# One persistent, isolated manual-test session; never reads a physical keyboard.
set -euo pipefail

repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$repo"
session="$repo/.scratch/manual-test"

for tool in cargo nice flock setsid; do
  command -v "$tool" >/dev/null || {
    printf 'Falta %s; instálalo antes de ejecutar las pruebas.\n' "$tool" >&2
    exit 1
  }
done

mkdir -p -- "$session"
exec 9>"$session/launch.lock"
if ! flock -n 9; then
  printf 'Ya hay una sesión de prueba abierta. Cierra sus ventanas antes de iniciar otra.\n' >&2
  exit 1
fi

pids=()
build_pid=""
cleanup() {
  local status=$?
  trap - EXIT INT TERM
  # The build owns a session so cancellation also stops its compiler descendants.
  if [[ -n "$build_pid" ]]; then
    kill -- "-$build_pid" 2>/dev/null || true
    wait "$build_pid" 2>/dev/null || true
  fi
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  for pid in "${pids[@]}"; do wait "$pid" 2>/dev/null || true; done
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Cargo needs the caller's real HOME for its toolchain and cache.
printf 'Preparando el configurador y el simulador…\n'
setsid --wait nice -n 19 env CARGO_BUILD_JOBS=2 cargo build --locked \
  -p tb-app -p tb-daemon --target-dir "$repo/target" &
build_pid=$!
status=0
wait "$build_pid" || status=$?
build_pid=""
if (( status != 0 )); then exit "$status"; fi

configurator="$repo/target/debug/touchbar-customizer"
daemon="$repo/target/debug/touchbard"
for binary in "$configurator" "$daemon"; do
  if [[ ! -x "$binary" ]]; then
    printf 'No se encontró el ejecutable %s después de compilar.\n' "$binary" >&2
    exit 1
  fi
done

test_home="$session/home"
mkdir -p -- "$test_home" "$session/frames" "$session/logs"
printf '\nConfig de prueba: %s\nLogs: %s\nImágenes: %s\nTeclas registradas: %s\n\n' \
  "$test_home/.config/touchbar-customizer/config.toml" "$session/logs" \
  "$session/frames" "$session/keys.bin"
printf '%s\n' \
  'Edita o reordena botones y pulsa Save para actualizar el simulador.' \
  'Haz clic en el simulador: las teclas se registran en un archivo.' \
  'Se reutiliza la config guardada; logs y registros corresponden a esta ejecución.' \
  'El HOME aislado usa el tema incluido salvo que configures un tema de prueba.' \
  'Cierra cualquiera de las ventanas o pulsa Ctrl+C para terminar ambas.'

HOME="$test_home" "$configurator" >"$session/logs/configurator.log" 2>&1 &
pids+=("$!")
HOME="$test_home" "$daemon" --sim-window --sim "$session/frames" \
  --emit "$session/keys.bin" >"$session/logs/simulator.log" 2>&1 &
pids+=("$!")

status=0
wait -n "${pids[@]}" || status=$?
if (( status != 0 )); then
  printf 'Una ventana terminó con código %s. Consulta los logs en %s\n' \
    "$status" "$session/logs" >&2
fi
exit "$status"
