"""Measure a trained policy the way harmony's baseline probe measures builtins.

Run it next to `cargo run --release -p quartet-harmony --example baselines`:
both use default 64-slot worlds, 500 warm-up beats, then 2,000 measured
beats over 8 worlds, and report the same metrics. The policy samples its
actions, as it does in a live performance.
"""

from dataclasses import dataclass
from pathlib import Path

import torch

from melody import checkpoint
from melody.inference import act
from melody.rehearsal import Rehearsal

WORLDS = 8
SLOTS = 64
WARMUP = 500
BEATS = 2000


@dataclass(frozen=True)
class Evaluation:
    deaths_per_1k_beats: float
    mean_energy: float
    chord_rate: float
    motes_per_1k_beats: float


def evaluate(
    target: str,
    run_dir: Path,
    device: torch.device,
    seed: int = 0,
    worlds: int = WORLDS,
    warmup: int = WARMUP,
    beats: int = BEATS,
) -> Evaluation:
    deaths = motes = chord = 0
    energy = 0.0
    with Rehearsal(target, worlds, SLOTS, seed=seed) as env:
        obs, _ = env.reset()
        policy = checkpoint.load(run_dir, device, env.observation_version)
        for beat in range(warmup + beats):
            obs, _, _, _, info = env.step(act(policy, obs, device))
            if beat < warmup:
                continue
            stats = info['stats']
            deaths += stats.deaths
            motes += stats.motes_eaten
            chord += stats.chord_members
            energy += stats.energy
    world_beats = worlds * beats
    agent_steps = world_beats * SLOTS
    return Evaluation(
        deaths_per_1k_beats=deaths / world_beats * 1000,
        mean_energy=energy / agent_steps,
        chord_rate=chord / agent_steps,
        motes_per_1k_beats=motes / world_beats * 1000,
    )


def main(target: str, runs_dir: Path, names: list[str], device: torch.device) -> None:
    print(
        f'{WORLDS} worlds × {SLOTS} slots, {WARMUP} warm-up beats, {BEATS} measured beats'
    )
    print(
        f'{"policy":<28} {"deaths/1k beats":>16} {"mean energy":>12} '
        f'{"chord rate":>11} {"motes/1k beats":>15}'
    )
    for name in names:
        e = evaluate(target, runs_dir / checkpoint.check_run_id(name), device)
        print(
            f'{name:<28} {e.deaths_per_1k_beats:>16.1f} {e.mean_energy:>12.3f} '
            f'{e.chord_rate:>11.4f} {e.motes_per_1k_beats:>15.1f}'
        )
