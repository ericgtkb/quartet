"""Trained policies for live performances: discovery, loading, and inference.

`PolicyStore` lists the runs that have a checkpoint and caches loaded models.
A cached model is reloaded when its checkpoint file changes, so a performance
started from a run that is still training picks up newer weights.
"""

import threading
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import torch

from melody import checkpoint
from melody.policy import to_env


@torch.no_grad()
def act(policy: checkpoint.Loaded, obs: np.ndarray, device: torch.device) -> np.ndarray:
    """Sample actions for every agent in one forward pass (agents × 3 float32)."""
    if obs.ndim != 2 or obs.shape[1] != policy.model.obs_dim:
        raise ValueError(
            f'observations must have shape (agents, {policy.model.obs_dim}), got {obs.shape}'
        )
    s = policy.model.step(torch.from_numpy(obs.copy()).to(device))
    return to_env(s.steer, s.sing)


@dataclass(frozen=True)
class PolicyRecord:
    name: str
    observation_version: int
    global_step: int
    created_at: int
    status: str


class PolicyStore:
    def __init__(self, runs_dir: Path, device: torch.device) -> None:
        self.runs_dir = runs_dir
        self.device = device
        self._lock = threading.Lock()
        self._cache: dict[str, tuple[int, checkpoint.Loaded]] = {}

    def list(self) -> list[PolicyRecord]:
        """Runs with a checkpoint, newest first."""
        records = []
        if not self.runs_dir.is_dir():
            return records
        for run_dir in self.runs_dir.iterdir():
            record = checkpoint.read_record(run_dir)
            if not (run_dir / checkpoint.CHECKPOINT).is_file() or record is None:
                continue
            if 'observation_version' not in record:
                continue
            records.append(
                PolicyRecord(
                    name=run_dir.name,
                    observation_version=int(record['observation_version']),
                    global_step=int(record.get('global_step', 0)),
                    created_at=int(record.get('created_at', 0)),
                    status=str(record.get('status', '')),
                )
            )
        records.sort(key=lambda r: (r.created_at, r.name), reverse=True)
        return records

    def get(self, name: str, observation_version: int) -> checkpoint.Loaded:
        """The policy `name`, loaded on first use or when its checkpoint
        changed. Raises `FileNotFoundError` for an unknown policy and
        `CheckpointError` for an incompatible one. Blocking (disk and torch);
        call it off the event loop."""
        run_dir = self.runs_dir / checkpoint.check_run_id(name)
        try:
            mtime = (run_dir / checkpoint.CHECKPOINT).stat().st_mtime_ns
        except FileNotFoundError:
            raise FileNotFoundError(f'no trained policy {name!r}') from None
        with self._lock:
            cached = self._cache.get(name)
            if cached is None or cached[0] != mtime:
                cached = (mtime, checkpoint.load(run_dir, self.device))
                self._cache[name] = cached
        loaded = cached[1]
        if loaded.observation_version != observation_version:
            raise checkpoint.CheckpointError(
                f'{name} was trained on observation version {loaded.observation_version}, '
                f'but harmony uses version {observation_version}'
            )
        return loaded
