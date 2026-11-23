#!/usr/bin/env bash
# Benchmark rehearsal throughput: a release build of harmony, measured from
# melody over TCP. Extra arguments go to `quartet-melody bench`
# (e.g. --worlds 1,16,64 --streams 1,2 --device mps).
set -euo pipefail
addr="127.0.0.1:50061"
exec "$(dirname "$0")/with-harmony.sh" "$addr" uv run --frozen quartet-melody bench --target "$addr" "$@"
