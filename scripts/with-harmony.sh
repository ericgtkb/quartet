#!/usr/bin/env bash
# Run a command next to a release build of harmony listening on ADDR, then
# stop harmony. Usage: with-harmony.sh ADDR COMMAND [ARGS...]
# The command runs in melody/ with QUARTET_HARMONY_ADDR set to ADDR.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
addr="$1"
shift

(cd "$root/harmony" && cargo build -q --release -p quartet-harmony-server)
QUARTET_HARMONY_ADDR="$addr" RUST_LOG=warn "$root/harmony/target/release/quartet-harmony-server" &
pid=$!
trap 'kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null || true' EXIT

for _ in $(seq 100); do
  nc -z 127.0.0.1 "${addr##*:}" >/dev/null 2>&1 && break
  sleep 0.1
done
nc -z 127.0.0.1 "${addr##*:}" >/dev/null 2>&1 || { echo "harmony did not start" >&2; exit 1; }

cd "$root/melody"
QUARTET_HARMONY_ADDR="$addr" "$@"
