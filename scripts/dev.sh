#!/usr/bin/env bash
# Run harmony, melody, rhythm, and timbre natively with prefixed logs.
# Ctrl-C (or any exit) stops all four. Expects `just gen` to have run.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"

echo "building..."
# Release: training runs started from the UI drive rehearsals on this harmony.
(cd "$root/harmony" && cargo build -q --release -p quartet-harmony-server)
(cd "$root/rhythm" && go build -o bin/quartet-rhythm ./cmd/quartet-rhythm)
(cd "$root/melody" && uv sync -q --frozen)
(cd "$root/timbre" && pnpm install --silent --frozen-lockfile)

pids=()
cleanup() {
  trap - INT TERM EXIT
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
}
trap cleanup INT TERM EXIT

# run NAME DIR CMD...: start CMD in DIR, prefixing each output line with [NAME].
# exec makes the recorded PID the service itself, not a wrapper shell.
run() {
  local name=$1 dir=$2
  shift 2
  (cd "$root/$dir" && exec "$@") > >(sed -u "s/^/[$name] /") 2>&1 &
  pids+=("$!")
}

run harmony harmony target/release/quartet-harmony-server
run melody melody .venv/bin/quartet-melody serve
run rhythm rhythm bin/quartet-rhythm
run timbre timbre node_modules/.bin/vite

echo "timbre: http://localhost:5173  rhythm: http://127.0.0.1:8080/api/ensemble"
wait
