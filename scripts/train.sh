#!/usr/bin/env bash
# Train a policy: a release build of harmony plus `quartet-melody train`
# (natively, so MPS is available). Extra arguments go to the trainer
# (e.g. --total-steps 20000000 --worlds 16 --seed 3). Ctrl-C stops after the
# current update and saves a checkpoint. Runs land in melody/runs/.
set -euo pipefail
exec "$(dirname "$0")/with-harmony.sh" 127.0.0.1:50071 uv run --frozen quartet-melody train "$@"
