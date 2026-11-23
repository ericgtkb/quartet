# quartet-rhythm

Quartet's conductor: drives the beat loop of every live performance, fans snapshots out to browsers, and serves the public REST and SSE API, relaying to harmony and melody over gRPC. See the [repository README](../README.md).

## Layout

```text
rhythm/
├── cmd/quartet-rhythm/   Entry point: configuration, gRPC clients, HTTP server, shutdown
├── internal/
│   ├── api/              chi router and handlers, hand-written JSON types (mirrored in timbre)
│   ├── performance/      Beat loops, the subscriber hub, and frame encoding
│   ├── upstream/         gRPC wrappers for harmony and melody
│   └── generated/        Generated gRPC code (Git-ignored)
├── scripts/gen-proto.sh  Code generation (also run by `just gen` and the Dockerfile)
└── Dockerfile            Static binary on distroless
```

## Commands

```sh
go build -o bin/quartet-rhythm ./cmd/quartet-rhythm
go test -race ./...
go vet ./...
golangci-lint run ./...
gofmt -l .
```
