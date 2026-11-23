# quartet-harmony

Quartet's world engine: the deterministic simulation, shared by live performances and training rehearsals, and its gRPC server. See the [repository README](../README.md).

## Layout

```text
harmony/
├── core/                 quartet-harmony (lib `harmony`): the world, no I/O
│   ├── src/world.rs      Rules, stepping, evolution, and every balance constant
│   ├── src/voice.rs      Voices, genomes, mutation, and the trained trait range
│   ├── src/observe.rs    The versioned observation layout
│   ├── src/policy.rs     Builtin baselines: random, forage, choir
│   ├── src/batch.rs      Parallel worlds for rehearsals (rayon)
│   ├── src/census.rs     Aggregates and per-world history
│   ├── src/rng.rs        SplitMix64
│   └── examples/         baselines.rs and evolution.rs probes
├── server/               quartet-harmony-server (bin): tonic gRPC server
│   ├── build.rs          Compiles ../score/*.proto with protox
│   └── src/              main, service, stage (live worlds), rehearsal, convert
├── .cargo/config.toml    Seven-day minimum publish age for crates
└── Dockerfile            Release build on a slim Debian runtime
```

## Commands

```sh
cargo build --release -p quartet-harmony-server
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo run --release -p quartet-harmony --example baselines            # builtin policies' metrics
cargo run --release -p quartet-harmony --example evolution forage 20000  # trait drift under selection
```
