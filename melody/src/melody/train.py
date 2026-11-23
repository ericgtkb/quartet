"""PPO for quartetlings: one shared policy trained on a harmony rehearsal.

Adapted from CleanRL's `ppo_continuous_action.py`
(https://github.com/vwxyzjn/cleanrl, MIT License, Copyright (c) 2019 CleanRL
developers), with details from "The 37 Implementation Details of PPO"
(https://iclr-blog-track.github.io/2022/03/25/ppo-implementation-details/):
orthogonal init, Adam with eps 1e-5, learning-rate annealing, GAE, advantage
normalization, clipped value loss, and gradient-norm clipping.

What differs from CleanRL:

- The environment is one rehearsal of `worlds × slots` agents. Every agent is
  a row of the batch, and all of them share the policy (their voice is in the
  observation).
- The action mixes a Normal (steering) and a Bernoulli (sing); see
  `melody.policy`.
- Autoreset is same-step with no final observation, so GAE treats a
  truncation (retirement at old age) like a termination. Retirement is rare,
  so the bias is small.
- Metrics go to a callback and `metrics.jsonl` instead of TensorBoard.
"""

import json
import os
import random
import time
from collections.abc import Callable
from dataclasses import asdict, dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Protocol

import numpy as np
import torch
from torch import nn, optim

from melody import checkpoint
from melody.device import resolve_device
from melody.policy import ACTION_DIM, ActorCritic, to_env
from melody.rehearsal import Rehearsal

MAX_AGENTS = 1 << 17  # harmony's limit on worlds × slots
DEFAULT_HARMONY = '127.0.0.1:50051'


def default_run_id(seed: int) -> str:
    return f'ppo-{datetime.now(UTC):%Y%m%d-%H%M%S}-s{seed}'


@dataclass
class Config:
    # Run
    seed: int = 1
    device: str = 'auto'
    harmony: str = DEFAULT_HARMONY
    runs_dir: str = 'runs'
    run_id: str = ''  # default: ppo-<UTC time>-s<seed>
    checkpoint_every: int = 10  # updates
    # Rehearsal: worlds × slots agents step together
    worlds: int = 16
    slots: int = 64
    # PPO
    total_steps: int = 50_000_000  # agent-steps
    num_steps: int = 128  # rollout length per update
    learning_rate: float = 3e-4
    anneal_lr: bool = True
    gamma: float = 0.99
    gae_lambda: float = 0.95
    num_minibatches: int = 16
    update_epochs: int = 4
    norm_adv: bool = True
    clip_coef: float = 0.2
    clip_vloss: bool = True
    ent_coef: float = 0.001
    vf_coef: float = 0.5
    max_grad_norm: float = 0.5
    target_kl: float | None = None
    hidden: int = 128
    # Filled in by `validate`
    batch_size: int = field(default=0, init=False)
    minibatch_size: int = field(default=0, init=False)
    num_updates: int = field(default=0, init=False)

    def validate(self) -> None:
        """Check ranges and fill in derived sizes; raise ValueError if invalid."""
        if self.worlds < 1 or self.slots < 4 or self.slots % 4:
            raise ValueError(
                f'need worlds >= 1 and slots a positive multiple of 4, got {self.worlds}, {self.slots}'
            )
        if self.worlds * self.slots > MAX_AGENTS:
            raise ValueError(f'worlds × slots must be at most {MAX_AGENTS}')
        if self.num_steps < 1 or self.num_minibatches < 1 or self.update_epochs < 1:
            raise ValueError('num_steps, num_minibatches, update_epochs must be >= 1')
        self.batch_size = self.worlds * self.slots * self.num_steps
        if self.batch_size % self.num_minibatches:
            raise ValueError(
                f'batch size {self.batch_size} is not divisible by num_minibatches {self.num_minibatches}'
            )
        if self.total_steps < 1:
            raise ValueError('total_steps must be >= 1')
        self.minibatch_size = self.batch_size // self.num_minibatches
        self.num_updates = max(1, self.total_steps // self.batch_size)
        if not self.run_id:
            self.run_id = default_run_id(self.seed)
        checkpoint.check_run_id(self.run_id)


class Env(Protocol):
    """What training needs from an environment; `Rehearsal` provides it."""

    num_envs: int
    obs_dim: int
    action_dim: int
    observation_version: int

    def reset(self) -> tuple[np.ndarray, dict]: ...

    def step(
        self, actions: np.ndarray
    ) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, dict]: ...

    def close(self) -> None: ...


Metrics = dict[str, Any]


def tensor(array: np.ndarray, device: torch.device) -> torch.Tensor:
    # Decoded arrays are read-only views of the message; copy before handing
    # them to torch, which expects writable memory.
    return torch.from_numpy(array.copy()).to(device)


def train(
    config: Config,
    env: Env | None = None,
    emit: Callable[[Metrics], None] | None = None,
    should_stop: Callable[[], bool] = lambda: False,
) -> str:
    """Train until `total_steps` or `should_stop()`; return the final status,
    `finished` or `stopped`. Raises on failure after recording it."""
    config.validate()
    run_dir = Path(config.runs_dir) / config.run_id
    run_dir.mkdir(parents=True, exist_ok=False)
    random.seed(config.seed)
    np.random.seed(config.seed)
    torch.manual_seed(config.seed)
    device = resolve_device(config.device)

    if env is None:
        env = Rehearsal(config.harmony, config.worlds, config.slots, seed=config.seed)
    record: dict[str, Any] = {
        'run_id': config.run_id,
        'status': 'running',
        'created_at': int(time.time()),
        'pid': os.getpid(),
        'config': asdict(config),
        'git_revision': checkpoint.git_revision(),
        'device': device.type,
        'global_step': 0,
        'latest': None,
        'error': '',
    }
    try:
        next_obs_np, _ = env.reset()
        if env.num_envs != config.worlds * config.slots:
            raise ValueError(
                f'environment has {env.num_envs} agents, config says {config.worlds} × {config.slots}'
            )
        if env.action_dim != ACTION_DIM:
            raise ValueError(
                f'harmony expects {env.action_dim} action values, the policy makes {ACTION_DIM}'
            )
        record |= {
            'observation_version': env.observation_version,
            'obs_dim': env.obs_dim,
            'hidden': config.hidden,
            'seed': config.seed,
        }
        checkpoint.write_record(run_dir, record)
        status = _ppo(
            config, env, next_obs_np, device, run_dir, record, emit, should_stop
        )
    except BaseException as e:
        record |= {'status': 'failed', 'error': f'{type(e).__name__}: {e}'}
        checkpoint.write_record(run_dir, record)
        raise
    finally:
        env.close()
    return status


def _ppo(
    config: Config,
    env: Env,
    first_obs: np.ndarray,
    device: torch.device,
    run_dir: Path,
    record: dict[str, Any],
    emit: Callable[[Metrics], None] | None,
    should_stop: Callable[[], bool],
) -> str:
    n, T = env.num_envs, config.num_steps
    agent = ActorCritic(env.obs_dim, config.hidden).to(device)
    optimizer = optim.Adam(agent.parameters(), lr=config.learning_rate, eps=1e-5)

    # Rollout storage: one row per (step, agent).
    obs = torch.zeros((T, n, env.obs_dim), device=device)
    steers = torch.zeros((T, n, 2), device=device)
    sings = torch.zeros((T, n), device=device)
    logprobs = torch.zeros((T, n), device=device)
    rewards = torch.zeros((T, n), device=device)
    dones = torch.zeros((T, n), device=device)
    values = torch.zeros((T, n), device=device)

    next_obs = tensor(first_obs, device)
    life_return = np.zeros(n)
    life_length = np.zeros(n)
    global_step = 0
    start = time.perf_counter()
    status = 'finished'

    for update in range(1, config.num_updates + 1):
        if config.anneal_lr:
            frac = 1.0 - (update - 1.0) / config.num_updates
            optimizer.param_groups[0]['lr'] = frac * config.learning_rate

        # 1. Rollout: T beats of every world with the current policy.
        ended_returns: list[float] = []
        ended_lengths: list[float] = []
        deaths = motes = chord = 0
        energy = 0.0
        for t in range(T):
            global_step += n
            obs[t] = next_obs
            with torch.no_grad():
                s = agent.step(next_obs)
            steers[t], sings[t], logprobs[t], values[t] = (
                s.steer,
                s.sing,
                s.log_prob,
                s.value,
            )
            next_obs_np, reward, terminated, truncated, info = env.step(
                to_env(s.steer, s.sing)
            )
            # A life ended at step t, so observation t + 1 is a newcomer's.
            done = terminated | truncated
            rewards[t] = tensor(reward, device)
            dones[t] = tensor(done.astype(np.float32), device)
            next_obs = tensor(next_obs_np, device)

            life_return += reward
            life_length += 1
            ended_returns.extend(life_return[done].tolist())
            ended_lengths.extend(life_length[done].tolist())
            life_return[done] = 0.0
            life_length[done] = 0.0
            stats = info.get('stats')
            if stats is not None:
                deaths += stats.deaths
                motes += stats.motes_eaten
                chord += stats.chord_members
                energy += stats.energy

        # 2. Advantages with GAE. Same-step autoreset leaves no final
        #    observation, so truncation bootstraps like termination (zero).
        with torch.no_grad():
            next_value = agent.value(next_obs)
            advantages = torch.zeros_like(rewards)
            last_gae = torch.zeros(n, device=device)
            for t in reversed(range(T)):
                next_nonterminal = 1.0 - dones[t]
                next_values = next_value if t == T - 1 else values[t + 1]
                delta = (
                    rewards[t]
                    + config.gamma * next_values * next_nonterminal
                    - values[t]
                )
                last_gae = (
                    delta
                    + config.gamma * config.gae_lambda * next_nonterminal * last_gae
                )
                advantages[t] = last_gae
            returns = advantages + values

        # 3. Flatten the batch.
        b_obs = obs.reshape(-1, env.obs_dim)
        b_steers = steers.reshape(-1, 2)
        b_sings = sings.reshape(-1)
        b_logprobs = logprobs.reshape(-1)
        b_advantages = advantages.reshape(-1)
        b_returns = returns.reshape(-1)
        b_values = values.reshape(-1)

        # 4. Optimize the policy and value network.
        clipfracs = []
        for _epoch in range(config.update_epochs):
            order = torch.randperm(config.batch_size, device=device)
            for begin in range(0, config.batch_size, config.minibatch_size):
                mb = order[begin : begin + config.minibatch_size]
                new = agent.step(b_obs[mb], b_steers[mb], b_sings[mb])
                logratio = new.log_prob - b_logprobs[mb]
                ratio = logratio.exp()

                with torch.no_grad():
                    approx_kl = ((ratio - 1) - logratio).mean()
                    clipfracs.append(
                        ((ratio - 1.0).abs() > config.clip_coef).float().mean()
                    )

                mb_adv = b_advantages[mb]
                if config.norm_adv:
                    mb_adv = (mb_adv - mb_adv.mean()) / (mb_adv.std() + 1e-8)

                # Policy loss
                pg_loss1 = -mb_adv * ratio
                pg_loss2 = -mb_adv * torch.clamp(
                    ratio, 1 - config.clip_coef, 1 + config.clip_coef
                )
                pg_loss = torch.max(pg_loss1, pg_loss2).mean()

                # Value loss
                if config.clip_vloss:
                    v_loss_unclipped = (new.value - b_returns[mb]) ** 2
                    v_clipped = b_values[mb] + torch.clamp(
                        new.value - b_values[mb], -config.clip_coef, config.clip_coef
                    )
                    v_loss_clipped = (v_clipped - b_returns[mb]) ** 2
                    v_loss = 0.5 * torch.max(v_loss_unclipped, v_loss_clipped).mean()
                else:
                    v_loss = 0.5 * ((new.value - b_returns[mb]) ** 2).mean()

                entropy = new.entropy.mean()
                loss = pg_loss - config.ent_coef * entropy + config.vf_coef * v_loss

                optimizer.zero_grad()
                loss.backward()
                nn.utils.clip_grad_norm_(agent.parameters(), config.max_grad_norm)
                optimizer.step()

            if config.target_kl is not None and approx_kl > config.target_kl:
                break

        # 5. Metrics, logged once per update.
        y_pred, y_true = b_values, b_returns
        var_y = y_true.var()
        # 0 rather than NaN when returns are constant: metrics become JSON.
        explained_var = (
            (1 - (y_true - y_pred).var() / var_y).item() if var_y > 0 else 0.0
        )
        elapsed = time.perf_counter() - start
        agent_steps = T * n
        metrics: Metrics = {
            'update': update,
            'global_step': global_step,
            'elapsed_seconds': elapsed,
            'steps_per_second': global_step / elapsed,
            'episode_return': float(np.mean(ended_returns)) if ended_returns else 0.0,
            'episode_length': float(np.mean(ended_lengths)) if ended_lengths else 0.0,
            'reward_per_step': rewards.mean().item(),
            'chord_rate': chord / agent_steps,
            'deaths_per_1k_beats': deaths / agent_steps * 64_000,
            'motes_per_1k_beats': motes / agent_steps * 64_000,
            'mean_energy': energy / agent_steps,
            'policy_loss': pg_loss.item(),
            'value_loss': v_loss.item(),
            'entropy': entropy.item(),
            'approx_kl': approx_kl.item(),
            'clip_fraction': torch.stack(clipfracs).mean().item(),
            'explained_variance': explained_var,
            'learning_rate': optimizer.param_groups[0]['lr'],
        }
        with (run_dir / checkpoint.METRICS).open('a') as f:
            f.write(json.dumps(metrics) + '\n')
        if emit is not None:
            emit(metrics)

        stopping = should_stop()
        if stopping:
            status = 'stopped'
        last = update == config.num_updates
        if stopping or last or update % config.checkpoint_every == 0:
            record |= {'global_step': global_step, 'latest': metrics}
            if stopping or last:
                record['status'] = status
            checkpoint.save(run_dir, agent, record)
            checkpoint.write_record(run_dir, record)
        if stopping:
            break
    return status
