#!/bin/sh
# Generate Go code for score/*.proto into internal/generated/quartet/score/.
# Requires protoc on PATH; the Go plugins are pinned as `tool` deps in go.mod.
set -eu

cd "$(dirname "$0")/.."
module=github.com/ericgtkb/quartet/rhythm/internal/generated
out=internal/generated

rm -rf "$out"
mkdir -p "$out"
protoc -I .. \
  --plugin=protoc-gen-go="$(go tool -n protoc-gen-go)" \
  --plugin=protoc-gen-go-grpc="$(go tool -n protoc-gen-go-grpc)" \
  --go_out="$out" --go_opt=module="$module" \
  --go-grpc_out="$out" --go-grpc_opt=module="$module" \
  ../score/*.proto
