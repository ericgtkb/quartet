"""gRPC server for `quartet.score.MelodyService`.

Inference (`Act`) runs off the event loop in a worker thread; training runs
are child processes managed by `melody.runs`.
"""

import asyncio
import logging
import platform
import signal
from collections.abc import AsyncIterator
from pathlib import Path

import grpc
import torch
from grpc_health.v1 import health_pb2, health_pb2_grpc
from grpc_health.v1.health import aio as health_aio

import melody
from melody import inference
from melody.checkpoint import CheckpointError
from melody.generated.quartet.score import info_pb2, melody_pb2, melody_pb2_grpc
from melody.runs import RunConflict, RunView, TrainingRuns
from melody.tensor import decode, encode

log = logging.getLogger(__name__)

SERVICE_NAME = melody_pb2.DESCRIPTOR.services_by_name['MelodyService'].full_name
SHUTDOWN_GRACE_SECONDS = 5.0
# Observations for a large performance; matches harmony's limit.
MAX_MESSAGE_BYTES = 64 << 20

_STATUS = {
    'running': melody_pb2.TRAINING_STATUS_RUNNING,
    'finished': melody_pb2.TRAINING_STATUS_FINISHED,
    'stopped': melody_pb2.TRAINING_STATUS_STOPPED,
    'failed': melody_pb2.TRAINING_STATUS_FAILED,
}
_METRIC_FIELDS = [f.name for f in melody_pb2.TrainingMetrics.DESCRIPTOR.fields]


def metrics_pb(m: dict) -> melody_pb2.TrainingMetrics:
    return melody_pb2.TrainingMetrics(**{k: m[k] for k in _METRIC_FIELDS if k in m})


def run_pb(view: RunView) -> melody_pb2.TrainingRun:
    run = melody_pb2.TrainingRun(
        id=view.id,
        status=_STATUS.get(view.status, melody_pb2.TRAINING_STATUS_UNSPECIFIED),
        config=melody_pb2.TrainingConfig(**view.config),
        created_at=view.created_at,
        error=view.error,
    )
    if view.latest:
        run.latest.CopyFrom(metrics_pb(view.latest))
    return run


class MelodyServicer(melody_pb2_grpc.MelodyServiceServicer):
    def __init__(
        self,
        device: torch.device,
        policies: inference.PolicyStore,
        runs: TrainingRuns,
    ) -> None:
        self._device = device
        self._policies = policies
        self._runs = runs

    async def GetServiceInfo(
        self,
        request: info_pb2.GetServiceInfoRequest,
        context: grpc.aio.ServicerContext,
    ) -> info_pb2.GetServiceInfoResponse:
        return info_pb2.GetServiceInfoResponse(
            info=info_pb2.ServiceInfo(
                name='melody',
                version=melody.__version__,
                runtime=f'Python {platform.python_version()}',
                details={
                    'device': self._device.type,
                    'torchVersion': torch.__version__,
                },
            )
        )

    async def ListPolicies(
        self,
        request: melody_pb2.ListPoliciesRequest,
        context: grpc.aio.ServicerContext,
    ) -> melody_pb2.ListPoliciesResponse:
        records = await asyncio.to_thread(self._policies.list)
        return melody_pb2.ListPoliciesResponse(
            policies=[
                melody_pb2.PolicyInfo(
                    name=r.name,
                    observation_version=r.observation_version,
                    global_step=r.global_step,
                    created_at=r.created_at,
                    status=_STATUS.get(r.status, 0),
                )
                for r in records
            ]
        )

    async def Act(
        self, request: melody_pb2.ActRequest, context: grpc.aio.ServicerContext
    ) -> melody_pb2.ActResponse:
        try:
            obs = decode(request.observations)
        except ValueError as e:
            await context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(e))

        def infer():
            policy = self._policies.get(request.policy, request.observation_version)
            return inference.act(policy, obs, self._device)

        try:
            actions = await asyncio.to_thread(infer)
        except FileNotFoundError as e:
            await context.abort(grpc.StatusCode.NOT_FOUND, str(e))
        except CheckpointError as e:
            await context.abort(grpc.StatusCode.FAILED_PRECONDITION, str(e))
        except ValueError as e:
            await context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(e))
        return melody_pb2.ActResponse(actions=encode(actions))

    async def StartTraining(
        self,
        request: melody_pb2.StartTrainingRequest,
        context: grpc.aio.ServicerContext,
    ) -> melody_pb2.StartTrainingResponse:
        c = request.config
        try:
            view = await self._runs.start(c.total_steps, c.worlds, c.slots, c.seed)
        except RunConflict as e:
            await context.abort(grpc.StatusCode.RESOURCE_EXHAUSTED, str(e))
        except ValueError as e:
            await context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(e))
        return melody_pb2.StartTrainingResponse(run=run_pb(view))

    async def ListTrainingRuns(
        self,
        request: melody_pb2.ListTrainingRunsRequest,
        context: grpc.aio.ServicerContext,
    ) -> melody_pb2.ListTrainingRunsResponse:
        views = await asyncio.to_thread(self._runs.list)
        return melody_pb2.ListTrainingRunsResponse(runs=[run_pb(v) for v in views])

    async def GetTrainingRun(
        self,
        request: melody_pb2.GetTrainingRunRequest,
        context: grpc.aio.ServicerContext,
    ) -> melody_pb2.GetTrainingRunResponse:
        view = self._runs.get(request.id)
        if view is None:
            await context.abort(
                grpc.StatusCode.NOT_FOUND, f'no training run {request.id!r}'
            )
        return melody_pb2.GetTrainingRunResponse(run=run_pb(view))

    async def StopTraining(
        self,
        request: melody_pb2.StopTrainingRequest,
        context: grpc.aio.ServicerContext,
    ) -> melody_pb2.StopTrainingResponse:
        view = await self._runs.stop(request.id)
        if view is None:
            await context.abort(
                grpc.StatusCode.NOT_FOUND, f'no training run {request.id!r}'
            )
        return melody_pb2.StopTrainingResponse(run=run_pb(view))

    async def WatchTraining(
        self,
        request: melody_pb2.WatchTrainingRequest,
        context: grpc.aio.ServicerContext,
    ) -> AsyncIterator[melody_pb2.TrainingMetrics]:
        try:
            async for m in self._runs.watch(request.id):
                yield metrics_pb(m)
        except KeyError:
            await context.abort(
                grpc.StatusCode.NOT_FOUND, f'no training run {request.id!r}'
            )


async def start(
    addr: str,
    device: torch.device,
    runs_dir: Path = Path('runs'),
    harmony: str = '127.0.0.1:50051',
) -> tuple[grpc.aio.Server, int, TrainingRuns]:
    """Start the server on `addr`; return it, the bound port, and its runs."""
    server = grpc.aio.server(
        options=[
            ('grpc.max_send_message_length', MAX_MESSAGE_BYTES),
            ('grpc.max_receive_message_length', MAX_MESSAGE_BYTES),
        ]
    )
    runs = TrainingRuns(runs_dir, harmony, device.type)
    servicer = MelodyServicer(device, inference.PolicyStore(runs_dir, device), runs)
    melody_pb2_grpc.add_MelodyServiceServicer_to_server(servicer, server)

    health = health_aio.HealthServicer()
    health_pb2_grpc.add_HealthServicer_to_server(health, server)
    for service in ('', SERVICE_NAME):
        await health.set(service, health_pb2.HealthCheckResponse.SERVING)

    port = server.add_insecure_port(addr)
    if port == 0:
        raise OSError(f'could not bind gRPC server to {addr!r}')
    await server.start()
    return server, port, runs


async def serve(addr: str, device: torch.device, runs_dir: Path, harmony: str) -> None:
    """Run the server until SIGINT or SIGTERM."""
    server, _, runs = await start(addr, device, runs_dir, harmony)
    log.info(
        'quartet-melody listening on %s (device=%s, runs=%s, harmony=%s)',
        addr,
        device.type,
        runs_dir,
        harmony,
    )

    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)
    await stop.wait()

    log.info('shutting down')
    await runs.close()
    await server.stop(SHUTDOWN_GRACE_SECONDS)
