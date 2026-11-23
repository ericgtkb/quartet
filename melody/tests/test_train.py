"""PPO smoke test on a trivial task, plus checkpoints and run records."""

from pathlib import Path

import numpy as np
import pytest
import torch
from fakes import BanditEnv

from melody import checkpoint
from melody.inference import PolicyStore, act
from melody.train import Config, train


def config(tmp_path: Path, **overrides) -> Config:
    base = {
        'device': 'cpu',
        'runs_dir': str(tmp_path),
        'run_id': 'bandit',
        'worlds': 1,
        'slots': 64,
        'num_steps': 32,
        'num_minibatches': 4,
        'total_steps': 64 * 32 * 40,
        'learning_rate': 3e-3,
        'gamma': 0.5,
        'checkpoint_every': 10,
        'hidden': 32,
    }
    return Config(**(base | overrides))


def test_ppo_learns_a_trivial_task(tmp_path: Path) -> None:
    metrics = []
    env = BanditEnv()
    status = train(config(tmp_path), env=env, emit=metrics.append)

    assert status == 'finished'
    assert env.closed
    assert len(metrics) == 40
    first = np.mean([m['reward_per_step'] for m in metrics[:3]])
    last = np.mean([m['reward_per_step'] for m in metrics[-3:]])
    assert first < 0.3
    assert last > 0.6, f'reward per step went from {first:.2f} to {last:.2f}'

    # The checkpoint is loadable, acts sensibly, and records its provenance.
    loaded = checkpoint.load(tmp_path / 'bandit', torch.device('cpu'), 7)
    assert loaded.meta['seed'] == 1
    assert loaded.meta['config']['learning_rate'] == 3e-3
    assert loaded.meta['git_revision']
    obs = np.zeros((2, 4), dtype=np.float32)
    obs[:, 0] = [0.5, -0.5]
    obs[:, 1] = [1, 0]
    actions = np.mean([act(loaded, obs, torch.device('cpu')) for _ in range(50)], 0)
    assert actions[0, 0] > actions[1, 0], 'steers toward the target'
    assert actions[0, 2] > 0.8 and actions[1, 2] < 0.2, 'sings when told to'

    record = checkpoint.read_record(tmp_path / 'bandit')
    assert record['status'] == 'finished'
    assert record['observation_version'] == 7
    assert len(checkpoint.read_metrics(tmp_path / 'bandit')) == 40


def test_refuses_other_observation_versions(tmp_path: Path) -> None:
    train(config(tmp_path, total_steps=1), env=BanditEnv())
    with pytest.raises(checkpoint.CheckpointError, match='observation version 7'):
        checkpoint.load(tmp_path / 'bandit', torch.device('cpu'), 8)
    store = PolicyStore(tmp_path, torch.device('cpu'))
    assert [p.name for p in store.list()] == ['bandit']
    with pytest.raises(checkpoint.CheckpointError):
        store.get('bandit', 8)
    with pytest.raises(FileNotFoundError):
        store.get('missing', 7)
    with pytest.raises(checkpoint.CheckpointError, match='invalid run id'):
        store.get('../etc', 7)


def test_stops_early_and_saves(tmp_path: Path) -> None:
    calls = []

    def should_stop() -> bool:
        calls.append(1)
        return len(calls) >= 2

    status = train(config(tmp_path), env=BanditEnv(), should_stop=should_stop)
    assert status == 'stopped'
    record = checkpoint.read_record(tmp_path / 'bandit')
    assert record['status'] == 'stopped'
    assert record['latest']['update'] == 2
    assert (tmp_path / 'bandit' / checkpoint.CHECKPOINT).is_file()


def test_validation(tmp_path: Path) -> None:
    for bad in (
        {'slots': 6},
        {'worlds': 0},
        {'num_minibatches': 3},
        {'run_id': 'a/b'},
    ):
        with pytest.raises(ValueError):
            config(tmp_path, **bad).validate()
    with pytest.raises(ValueError, match='64 agents'):
        train(config(tmp_path, worlds=2), env=BanditEnv())
    assert checkpoint.read_record(tmp_path / 'bandit')['status'] == 'failed'
