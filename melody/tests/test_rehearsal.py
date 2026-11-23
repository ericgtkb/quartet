"""Rehearsal client against a fake harmony that speaks the same protocol."""

from collections.abc import Iterator
from concurrent import futures

import grpc
import numpy as np
import pytest

from melody.generated.quartet.score import harmony_pb2, harmony_pb2_grpc, world_pb2
from melody.rehearsal import Rehearsal, RehearsalError
from melody.tensor import decode, encode

OBS_DIM = 5
ACTION_DIM = 3


class FakeHarmony(harmony_pb2_grpc.HarmonyServiceServicer):
    """Observations count steps; rewards echo each agent's turn action."""

    def Rehearse(
        self,
        request_iterator: Iterator[harmony_pb2.RehearseRequest],
        context: grpc.ServicerContext,
    ) -> Iterator[harmony_pb2.RehearseResponse]:
        first = next(request_iterator)
        if not first.HasField('config'):
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                'the first rehearsal message must be a config',
            )
        agents = first.config.worlds * first.config.world.slots
        zeros = np.zeros(agents, dtype=np.float32)
        flags = np.zeros(agents, dtype=np.uint8)
        yield harmony_pb2.RehearseResponse(
            observations=encode(np.zeros((agents, OBS_DIM), dtype=np.float32)),
            rewards=encode(zeros),
            terminated=encode(flags),
            truncated=encode(flags),
            stats=world_pb2.BeatStats(),
            observation_version=1,
            obs_dim=OBS_DIM,
            action_dim=ACTION_DIM,
        )
        for step, request in enumerate(request_iterator, start=1):
            actions = decode(request.actions)
            if actions.shape != (agents, ACTION_DIM):
                context.abort(grpc.StatusCode.INVALID_ARGUMENT, 'wrong action shape')
            terminated = flags.copy()
            terminated[0] = 1
            yield harmony_pb2.RehearseResponse(
                observations=encode(np.full((agents, OBS_DIM), step, dtype=np.float32)),
                rewards=encode(actions[:, 0].copy()),
                terminated=encode(terminated),
                truncated=encode(flags),
                stats=world_pb2.BeatStats(deaths=1),
            )


def serve(address: str) -> tuple[grpc.Server, int]:
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=4))
    harmony_pb2_grpc.add_HarmonyServiceServicer_to_server(FakeHarmony(), server)
    port = server.add_insecure_port(address)
    server.start()
    return server, port


@pytest.fixture
def tcp_target() -> Iterator[str]:
    server, port = serve('127.0.0.1:0')
    yield f'127.0.0.1:{port}'
    server.stop(None)


def test_reset_and_step(tcp_target: str) -> None:
    with Rehearsal(tcp_target, worlds=2, slots=4) as env:
        obs, info = env.reset()
        assert obs.shape == (8, OBS_DIM)
        assert (env.num_envs, env.obs_dim, env.action_dim, env.observation_version) == (
            8,
            OBS_DIM,
            ACTION_DIM,
            1,
        )

        actions = np.zeros((8, ACTION_DIM), dtype=np.float32)
        actions[:, 0] = np.linspace(-1, 1, 8)
        for step in (1, 2):
            obs, rewards, terminated, truncated, info = env.step(actions)
            assert obs[0, 0] == step
            np.testing.assert_allclose(rewards, actions[:, 0])
            assert (
                terminated.dtype == np.bool_
                and terminated[0]
                and not terminated[1:].any()
            )
            assert not truncated.any()
            assert info['stats'].deaths == 1


def test_validates_actions_before_sending(tcp_target: str) -> None:
    with Rehearsal(tcp_target, worlds=1, slots=4) as env:
        env.reset()
        with pytest.raises(ValueError, match='shape'):
            env.step(np.zeros((3, ACTION_DIM), dtype=np.float32))


def test_server_errors_become_rehearsal_errors(tcp_target: str) -> None:
    env = Rehearsal(tcp_target, worlds=1, slots=4)
    env.reset()
    env.action_dim = 2  # make the client send the wrong shape
    env.num_envs = 4
    with pytest.raises(RehearsalError, match='INVALID_ARGUMENT'):
        env.step(np.zeros((4, 2), dtype=np.float32))
    env.close()


def test_rejects_bad_sizes() -> None:
    with pytest.raises(ValueError):
        Rehearsal('127.0.0.1:1', worlds=0)
    with pytest.raises(ValueError):
        Rehearsal('127.0.0.1:1', worlds=1, slots=6)
