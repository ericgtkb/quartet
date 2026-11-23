#!/usr/bin/env bash
# Compare policies on the same protocol: harmony's builtin baselines, then
# the given trained runs (ids in melody/runs/) via `quartet-melody evaluate`.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
(cd "$root/harmony" && cargo run -q --release -p quartet-harmony --example baselines)
echo
[ "$#" -eq 0 ] && exit 0
exec "$root/scripts/with-harmony.sh" 127.0.0.1:50072 uv run --frozen quartet-melody evaluate "$@"
