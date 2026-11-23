"""A harmony rehearsal as a Gymnasium-style vector environment.

One `Rehearsal` owns one bidirectional `Rehearse` stream. harmony runs
`worlds` independent worlds of `slots` agents each; the batch is flat and
world-major (agent k = slot k % slots of world k // slots), so `num_envs` is
worlds × slots.

Conventions follow Gymnasium's vector API (`reset`, `step` returning
observations, rewards, terminated, truncated, info) without depending on it.
Autoreset is same-step: when an agent is terminated or truncated, the
observation returned for it already belongs to its replacement. There is no
final observation, so truncated agents can't be bootstrapped from it.
"""

import queue
from collections.abc import Iterator
from dataclasses import dataclass
from types import TracebackType
from typing import Self

import grpc
import numpy as np

from melody.generated.quartet.score import harmony_pb2, harmony_pb2_grpc, world_pb2
from melody.tensor import decode, encode

# Large rehearsals send tens of MB per step; harmony allows up to 64 MiB.
MAX_MESSAGE_BYTES = 64 << 20
CHANNEL_OPTIONS = [
    ('grpc.max_send_message_length', MAX_MESSAGE_BYTES),
    ('grpc.max_receive_message_length', MAX_MESSAGE_BYTES),
]

_END = object()


class RehearsalError(RuntimeError):
    """harmony rejected or ended the rehearsal."""


@dataclass(frozen=True)
class StepStats:
    """Counters summed over all worlds for one step."""

    motes_eaten: int
    deaths: int
    chord_members: int
    energy: float
    """Summed over every agent after the step."""


class Rehearsal:
    def __init__(
        self,
        target: str,
        worlds: int,
        slots: int = 64,
        seed: int = 0,
        *,
        motes: int | None = None,
        statics: int | None = None,
        channel: grpc.Channel | None = None,
    ) -> None:
        """Open a rehearsal on harmony at `target` (`host:port`).

        Nothing is sent until `reset()`.
        """
        if worlds < 1 or slots < 4 or slots % 4:
            raise ValueError(
                f'need worlds >= 1 and slots a positive multiple of 4, got {worlds}, {slots}'
            )
        self.worlds = worlds
        self.slots = slots
        self.num_envs = worlds * slots
        self._config = harmony_pb2.RehearsalConfig(
            worlds=worlds,
            world=world_pb2.WorldConfig(
                seed=seed, slots=slots, motes=motes, statics=statics
            ),
        )
        self._channel = channel or grpc.insecure_channel(
            target, options=CHANNEL_OPTIONS
        )
        self._owns_channel = channel is None
        self._requests: queue.Queue[object] = queue.Queue()
        self._responses: Iterator[harmony_pb2.RehearseResponse] | None = None
        self.obs_dim = 0
        self.action_dim = 0
        self.observation_version = 0

    def _request_stream(self) -> Iterator[harmony_pb2.RehearseRequest]:
        while (item := self._requests.get()) is not _END:
            yield item  # type: ignore[misc]

    def _recv(self) -> harmony_pb2.RehearseResponse:
        assert self._responses is not None, 'call reset() first'
        try:
            return next(self._responses)
        except StopIteration:
            raise RehearsalError('harmony ended the rehearsal') from None
        except grpc.RpcError as e:
            raise RehearsalError(
                f'rehearsal failed: {e.code().name}: {e.details()}'
            ) from e

    def reset(self) -> tuple[np.ndarray, dict]:
        """Start the rehearsal and return the initial observations."""
        if self._responses is not None:
            raise RuntimeError('a Rehearsal can only be reset once; open a new one')
        stub = harmony_pb2_grpc.HarmonyServiceStub(self._channel)
        self._responses = stub.Rehearse(self._request_stream())
        self._requests.put(harmony_pb2.RehearseRequest(config=self._config))
        first = self._recv()
        self.obs_dim = first.obs_dim
        self.action_dim = first.action_dim
        self.observation_version = first.observation_version
        return decode(first.observations), {}

    def send(self, actions: np.ndarray) -> None:
        """Send actions without waiting; pair with `recv()`. Lets several
        rehearsals step concurrently on harmony."""
        if actions.shape != (self.num_envs, self.action_dim):
            raise ValueError(
                f'actions must have shape {(self.num_envs, self.action_dim)}, got {actions.shape}'
            )
        self._requests.put(
            harmony_pb2.RehearseRequest(
                actions=encode(actions.astype(np.float32, copy=False))
            )
        )

    def recv(self) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, dict]:
        """Receive the result of the last `send()`."""
        r = self._recv()
        stats = StepStats(
            r.stats.motes_eaten, r.stats.deaths, r.stats.chord_members, r.stats.energy
        )
        return (
            decode(r.observations),
            decode(r.rewards),
            decode(r.terminated).astype(bool),
            decode(r.truncated).astype(bool),
            {'stats': stats},
        )

    def step(
        self, actions: np.ndarray
    ) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, dict]:
        """Apply `actions` (num_envs × action_dim float32: turn, thrust in
        [-1, 1]; sing when ≥ 0.5) and return the step's results."""
        self.send(actions)
        return self.recv()

    def close(self) -> None:
        self._requests.put(_END)
        if self._owns_channel:
            self._channel.close()

    def __enter__(self) -> Self:
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        self.close()
