# quartet-melody

Quartet's learning service: PPO training of quartetlings against harmony rehearsals, and policy inference for live performances. See the [repository README](../README.md).

## Layout

```text
melody/
├── src/melody/
│   ├── cli.py            quartet-melody: serve, train, evaluate, bench
│   ├── server.py         gRPC MelodyService (grpc.aio): Act, policies, training runs
│   ├── train.py          The PPO loop and its Config (CleanRL style)
│   ├── policy.py         Actor-critic network and harmony's action layout
│   ├── checkpoint.py     Run directories: checkpoints, run.json, metrics.jsonl
│   ├── inference.py      PolicyStore and batched inference for Act
│   ├── runs.py           Training runs as child processes of the server
│   ├── evaluate.py       Same protocol as harmony's baselines probe
│   ├── rehearsal.py      Gymnasium-style client for the Rehearse stream
│   ├── tensor.py         Packed tensors <-> NumPy
│   ├── bench.py          Rehearsal throughput benchmark
│   ├── device.py         --device auto|mps|cpu
│   └── generated/        Generated gRPC code (Git-ignored)
├── tests/                pytest; fakes.py holds the bandit task for the PPO smoke test
├── scripts/gen-proto.sh  Code generation (also run by `just gen` and the Dockerfile)
├── runs/                 Training runs and checkpoints (Git-ignored)
└── Dockerfile            CPU-only PyTorch image
```

## Commands

```sh
uv sync
uv run quartet-melody serve                                     # Act and training runs (gRPC)
uv run quartet-melody train --total-steps 20000000 --worlds 16  # PPO; see `train --help`
uv run quartet-melody evaluate RUN_ID                           # compare with harmony's baselines
uv run quartet-melody bench --worlds 1,16,64                    # rehearsal throughput
uv run pytest
uv run ruff check && uv run ruff format --check
```

`train`, `evaluate`, and `bench` need a running harmony. From the repository root, `just train`, `just evaluate`, and `just bench` start a release harmony for you.
