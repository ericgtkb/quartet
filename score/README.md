# score

Quartet's internal gRPC contracts: proto package `quartet.score`, shared by `harmony`, `melody`, and `rhythm`. Each language generates its own code from these files; generated code is never committed. See the [repository README](../README.md) and the contract rules in [AGENTS.md](../AGENTS.md).

## Layout

```text
score/
├── harmony.proto   HarmonyService: live worlds, census, and the Rehearse stream
├── melody.proto    MelodyService: policies, Act, and training runs
├── world.proto     World config, quartetlings, snapshots, beat stats, census
├── tensor.proto    Packed little-endian tensors (Tensor, Dtype)
└── info.proto      ServiceInfo, returned by every service
```

## Commands

```sh
just gen   # from the repository root: regenerate Python and Go code (Rust generates in build.rs)
```
