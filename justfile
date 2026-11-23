# Cross-project commands for Quartet. Inside one subproject, use its native tools.
# Requires: just, protoc, uv, cargo, go, golangci-lint, pnpm.

set shell := ["bash", "-euo", "pipefail", "-c"]

# List recipes.
default:
    @just --list

# Generate gRPC code from score/*.proto for Python and Go (Rust generates in build.rs).
gen:
    melody/scripts/gen-proto.sh
    rhythm/scripts/gen-proto.sh

# Lint, format-check, and type-check every subproject (no auto-fixes).
check: gen
    cd melody && uv run --frozen ruff check && uv run --frozen ruff format --check
    cd harmony && cargo fmt --check && cargo clippy --all-targets -- -D warnings
    cd rhythm && test -z "$(gofmt -l .)" && go vet ./... && golangci-lint run ./...
    cd timbre && pnpm exec oxfmt --check src/ && pnpm exec oxlint . && pnpm exec eslint . && pnpm type-check

# Run every subproject's tests.
test: gen
    cd melody && uv run --frozen pytest
    cd harmony && cargo test
    cd rhythm && go test -race ./...
    cd timbre && pnpm exec vitest run

# Run all four services natively (Ctrl-C stops them).
dev: gen
    scripts/dev.sh

# Benchmark rehearsal throughput against a release build of harmony (args go to `quartet-melody bench`).
bench *args: gen
    scripts/bench-rehearsal.sh {{args}}

# Train a policy with PPO against a release build of harmony (args go to `quartet-melody train`).
train *args: gen
    scripts/train.sh {{args}}

# Compare builtin baselines and trained runs on the same metrics (args: run ids in melody/runs/).
evaluate *runs: gen
    scripts/evaluate.sh {{runs}}
