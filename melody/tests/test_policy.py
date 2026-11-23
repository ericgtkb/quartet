import torch

from melody.policy import ACTION_DIM, ActorCritic, to_env


def test_step_shapes_and_rescoring() -> None:
    torch.manual_seed(0)
    model = ActorCritic(obs_dim=10, hidden=16)
    obs = torch.randn(5, 10)
    s = model.step(obs)
    assert s.steer.shape == (5, 2)
    assert s.sing.shape == s.log_prob.shape == s.entropy.shape == s.value.shape == (5,)
    assert set(s.sing.tolist()) <= {0.0, 1.0}
    # Scoring the sampled action again gives the same log-probability.
    again = model.step(obs, s.steer, s.sing)
    torch.testing.assert_close(again.log_prob, s.log_prob)


def test_env_actions_clip_steering_only() -> None:
    steer = torch.tensor([[3.0, -2.0], [0.25, 0.5]])
    sing = torch.tensor([1.0, 0.0])
    actions = to_env(steer, sing)
    assert actions.shape == (2, ACTION_DIM)
    assert actions.dtype.name == 'float32'
    assert actions.tolist() == [[1.0, -1.0, 1.0], [0.25, 0.5, 0.0]]
