"""Rehearsal throughput benchmark: agent-steps per second through harmony.

Each configuration opens `streams` rehearsals that split `worlds` between
them, sends random actions, and times `steps` round trips after a warm-up.
With more than one stream, actions for every stream are sent before any
result is read, so harmony steps them concurrently. `--device` adds the cost
of moving observations to a torch device, as training would.
"""

import time
from dataclasses import dataclass

import numpy as np

from melody.rehearsal import Rehearsal

WARMUP_STEPS = 10


@dataclass(frozen=True)
class Result:
    target: str
    worlds: int
    slots: int
    streams: int
    steps: int
    seconds: float

    @property
    def agents(self) -> int:
        return self.worlds * self.slots

    @property
    def agent_steps_per_second(self) -> float:
        return self.agents * self.steps / self.seconds

    @property
    def ms_per_step(self) -> float:
        return 1000 * self.seconds / self.steps


def run(
    target: str, worlds: int, slots: int, streams: int, steps: int, device: str | None
) -> Result:
    if worlds % streams:
        raise ValueError(
            f'worlds ({worlds}) must divide evenly across streams ({streams})'
        )
    to_device = None
    if device:
        import torch

        dev = torch.device(device)

        def to_device(obs: np.ndarray) -> None:
            torch.from_numpy(obs.copy()).to(dev)

    rehearsals = [
        Rehearsal(target, worlds // streams, slots, seed=i) for i in range(streams)
    ]
    try:
        for r in rehearsals:
            r.reset()
        rng = np.random.default_rng(0)
        actions = [
            rng.uniform(-1, 1, size=(r.num_envs, r.action_dim)).astype(np.float32)
            for r in rehearsals
        ]

        def step() -> None:
            for r, a in zip(rehearsals, actions, strict=True):
                r.send(a)
            for r in rehearsals:
                obs, *_ = r.recv()
                if to_device:
                    to_device(obs)

        for _ in range(WARMUP_STEPS):
            step()
        start = time.perf_counter()
        for _ in range(steps):
            step()
        seconds = time.perf_counter() - start
    finally:
        for r in rehearsals:
            r.close()
    return Result(target, worlds, slots, streams, steps, seconds)


def main(
    targets: list[str],
    worlds: list[int],
    slots: int,
    streams: list[int],
    steps: int,
    device: str | None,
) -> None:
    print(
        f'slots={slots} steps={steps} warmup={WARMUP_STEPS} device={device or "none"}'
    )
    header = f'{"target":<32} {"worlds":>6} {"agents":>7} {"streams":>7} {"ms/step":>8} {"agent-steps/s":>14}'
    print(header)
    print('-' * len(header))
    for target in targets:
        for w in worlds:
            for s in streams:
                if w % s:
                    continue
                r = run(target, w, slots, s, steps, device)
                print(
                    f'{r.target:<32} {r.worlds:>6} {r.agents:>7} {r.streams:>7} '
                    f'{r.ms_per_step:>8.2f} {r.agent_steps_per_second:>14,.0f}'
                )
