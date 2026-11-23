"""Training runs on disk: checkpoints, run records, and metrics.

Each run is a directory `<runs>/<run id>/` holding:

- `checkpoint.pt`: the latest weights plus metadata (see `save`);
- `run.json`: the same metadata as JSON, plus status and latest metrics, so
  runs can be listed without loading weights;
- `metrics.jsonl`: one JSON object per PPO update.

Files are replaced atomically (write to a temporary file, then rename), so a
reader never sees a partial checkpoint even while training is writing one.
"""

import json
import os
import re
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import torch

from melody.policy import ActorCritic

FORMAT = 1
RUN_ID = re.compile(r'^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$')

CHECKPOINT = 'checkpoint.pt'
RUN_RECORD = 'run.json'
METRICS = 'metrics.jsonl'


class CheckpointError(ValueError):
    """A checkpoint is missing, unreadable, or incompatible."""


def check_run_id(run_id: str) -> str:
    """Reject ids that aren't a single safe path component."""
    if not RUN_ID.match(run_id):
        raise CheckpointError(
            f'invalid run id {run_id!r}: use letters, digits, ".", "_", "-" (max 64)'
        )
    return run_id


def git_revision() -> str:
    """The repository's current commit (with `-dirty` if modified), or `unknown`."""
    try:
        out = subprocess.run(
            ['git', 'describe', '--always', '--dirty', '--abbrev=12'],
            check=False,
            capture_output=True,
            text=True,
            timeout=5,
            cwd=Path(__file__).parent,
        )
    except OSError, subprocess.TimeoutExpired:
        return 'unknown'
    return (
        out.stdout.strip() if out.returncode == 0 and out.stdout.strip() else 'unknown'
    )


def _atomic_write(path: Path, write: Any) -> None:
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=f'.{path.name}.')
    try:
        with os.fdopen(fd, 'wb') as f:
            write(f)
        os.replace(tmp, path)
    except BaseException:
        Path(tmp).unlink(missing_ok=True)
        raise


def write_record(run_dir: Path, record: dict[str, Any]) -> None:
    _atomic_write(
        run_dir / RUN_RECORD,
        lambda f: f.write(json.dumps(record, indent=2).encode()),
    )


def read_record(run_dir: Path) -> dict[str, Any] | None:
    try:
        return json.loads((run_dir / RUN_RECORD).read_text())
    except FileNotFoundError, json.JSONDecodeError:
        return None


def read_metrics(run_dir: Path) -> list[dict[str, Any]]:
    try:
        lines = (run_dir / METRICS).read_text().splitlines()
    except FileNotFoundError:
        return []
    metrics = []
    for line in lines:
        try:
            metrics.append(json.loads(line))
        except json.JSONDecodeError:
            break  # a line still being written
    return metrics


def save(run_dir: Path, model: ActorCritic, meta: dict[str, Any]) -> None:
    """Save weights and metadata. `meta` must hold only plain Python values
    (it is loaded with `weights_only=True`) and include `observation_version`."""
    payload = {
        'format': FORMAT,
        'model': {k: v.detach().cpu() for k, v in model.state_dict().items()},
        'meta': meta,
    }
    _atomic_write(run_dir / CHECKPOINT, lambda f: torch.save(payload, f))


@dataclass(frozen=True)
class Loaded:
    model: ActorCritic
    meta: dict[str, Any]

    @property
    def observation_version(self) -> int:
        return int(self.meta['observation_version'])


def load(
    run_dir: Path,
    device: torch.device,
    observation_version: int | None = None,
) -> Loaded:
    """Load a run's latest checkpoint in eval mode. When `observation_version`
    is given, refuse a checkpoint trained on a different one."""
    path = run_dir / CHECKPOINT
    try:
        payload = torch.load(path, map_location='cpu', weights_only=True)
    except FileNotFoundError:
        raise CheckpointError(f'no checkpoint at {path}') from None
    except Exception as e:
        raise CheckpointError(f'cannot read {path}: {e}') from e
    if payload.get('format') != FORMAT:
        raise CheckpointError(
            f'{path} has format {payload.get("format")!r}, expected {FORMAT}'
        )
    meta = payload['meta']
    version = int(meta['observation_version'])
    if observation_version is not None and version != observation_version:
        raise CheckpointError(
            f'{run_dir.name} was trained on observation version {version}, '
            f'but harmony uses version {observation_version}'
        )
    model = ActorCritic(int(meta['obs_dim']), int(meta['hidden']))
    model.load_state_dict(payload['model'])
    return Loaded(model.to(device).eval(), meta)
