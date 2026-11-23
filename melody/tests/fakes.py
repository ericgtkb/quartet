"""Small environments for training tests."""

import numpy as np

from melody.rehearsal import StepStats


class BanditEnv:
    """A trivial task: steer `turn` toward a target and sing when told to.

    Observation: [target, sing bit, 0, 0], fresh every step. Reward:
    -(turn - target)² + (1 if sing matches the bit). A random policy scores
    about -0.4 per step; a good one approaches 1.
    """

    obs_dim = 4
    action_dim = 3
    observation_version = 7

    def __init__(self, num_envs: int = 64, seed: int = 0) -> None:
        self.num_envs = num_envs
        self.rng = np.random.default_rng(seed)
        self.obs = self._observe()
        self.closed = False

    def _observe(self) -> np.ndarray:
        obs = np.zeros((self.num_envs, self.obs_dim), dtype=np.float32)
        obs[:, 0] = self.rng.uniform(-1, 1, self.num_envs)
        obs[:, 1] = self.rng.integers(0, 2, self.num_envs)
        return obs

    def reset(self) -> tuple[np.ndarray, dict]:
        return self.obs, {}

    def step(self, actions: np.ndarray):
        target, bit = self.obs[:, 0], self.obs[:, 1]
        sing = (actions[:, 2] >= 0.5).astype(np.float32)
        reward = -((actions[:, 0] - target) ** 2) + (sing == bit)
        self.obs = self._observe()
        done = self.rng.random(self.num_envs) < 0.01
        stats = StepStats(0, int(done.sum()), 0, float(self.num_envs))
        return (
            self.obs,
            reward.astype(np.float32),
            done,
            np.zeros(self.num_envs, dtype=bool),
            {'stats': stats},
        )

    def close(self) -> None:
        self.closed = True
