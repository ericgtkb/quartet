"""The shared quartetling policy: an actor-critic MLP over one agent's observation.

Every agent runs the same network; its voice is part of the observation.
The action has two parts:

- steering (turn, thrust): a diagonal Normal over unbounded values, clipped
  to [-1, 1] only when sent to harmony, so log-probabilities stay exact;
- sing: a Bernoulli, sent as 0 or 1.

The action's log-probability and entropy are the sums of the two parts.
Initialization follows CleanRL (orthogonal weights; small policy-head gains).
"""

from typing import NamedTuple

import numpy as np
import torch
from torch import nn
from torch.distributions import Bernoulli, Normal

STEER_DIM = 2
ACTION_DIM = STEER_DIM + 1


def layer(in_dim: int, out_dim: int, std: float = 2**0.5) -> nn.Linear:
    linear = nn.Linear(in_dim, out_dim)
    nn.init.orthogonal_(linear.weight, std)
    nn.init.zeros_(linear.bias)
    return linear


class Step(NamedTuple):
    steer: torch.Tensor
    """agents × 2, unclipped."""
    sing: torch.Tensor
    """agents, 0.0 or 1.0."""
    log_prob: torch.Tensor
    entropy: torch.Tensor
    value: torch.Tensor


class ActorCritic(nn.Module):
    def __init__(self, obs_dim: int, hidden: int = 128) -> None:
        super().__init__()
        self.obs_dim = obs_dim
        self.hidden = hidden
        self.critic = nn.Sequential(
            layer(obs_dim, hidden),
            nn.Tanh(),
            layer(hidden, hidden),
            nn.Tanh(),
            layer(hidden, 1, std=1.0),
        )
        self.actor = nn.Sequential(
            layer(obs_dim, hidden),
            nn.Tanh(),
            layer(hidden, hidden),
            nn.Tanh(),
        )
        self.steer_mean = layer(hidden, STEER_DIM, std=0.01)
        self.steer_logstd = nn.Parameter(torch.zeros(STEER_DIM))
        self.sing_logit = layer(hidden, 1, std=0.01)

    def value(self, obs: torch.Tensor) -> torch.Tensor:
        return self.critic(obs).squeeze(-1)

    def step(
        self,
        obs: torch.Tensor,
        steer: torch.Tensor | None = None,
        sing: torch.Tensor | None = None,
    ) -> Step:
        """Sample an action (or score the given one) and evaluate the critic."""
        h = self.actor(obs)
        mean = self.steer_mean(h)
        steer_dist = Normal(mean, self.steer_logstd.expand_as(mean).exp())
        sing_dist = Bernoulli(logits=self.sing_logit(h).squeeze(-1))
        if steer is None:
            steer = steer_dist.sample()
        if sing is None:
            sing = sing_dist.sample()
        log_prob = steer_dist.log_prob(steer).sum(-1) + sing_dist.log_prob(sing)
        entropy = steer_dist.entropy().sum(-1) + sing_dist.entropy()
        return Step(steer, sing, log_prob, entropy, self.value(obs))


def to_env(steer: torch.Tensor, sing: torch.Tensor) -> np.ndarray:
    """harmony's action layout: agents × 3 float32 (turn, thrust, sing)."""
    actions = torch.cat([steer.clamp(-1.0, 1.0), sing.unsqueeze(-1)], dim=-1)
    return actions.to('cpu', torch.float32).numpy()
