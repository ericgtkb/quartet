"""Round-trip tests through a real in-process gRPC server."""

import asyncio
from pathlib import Path

import grpc
import numpy as np
import pytest
import torch
from grpc_health.v1 import health_pb2, health_pb2_grpc

import melody
from melody import checkpoint
from melody.generated.quartet.score import info_pb2, melody_pb2, melody_pb2_grpc
from melody.policy import ActorCritic
from melody.server import SERVICE_NAME, start
from melody.tensor import decode, encode

OBS_DIM = 6


async def _round_trip(tmp_path: Path) -> tuple[info_pb2.ServiceInfo, list[int]]:
    server, port, _ = await start('127.0.0.1:0', torch.device('cpu'), tmp_path)
    try:
        async with grpc.aio.insecure_channel(f'127.0.0.1:{port}') as channel:
            response = await melody_pb2_grpc.MelodyServiceStub(channel).GetServiceInfo(
                info_pb2.GetServiceInfoRequest()
            )
            health = health_pb2_grpc.HealthStub(channel)
            statuses = [
                (await health.Check(health_pb2.HealthCheckRequest(service=s))).status
                for s in ('', SERVICE_NAME)
            ]
        return response.info, statuses
    finally:
        await server.stop(None)


def test_service_info_and_health(tmp_path: Path) -> None:
    info, statuses = asyncio.run(_round_trip(tmp_path))

    assert info.name == 'melody'
    assert info.version == melody.__version__
    assert info.runtime.startswith('Python 3.14')
    assert info.details['device'] == 'cpu'
    assert info.details['torchVersion'] == torch.__version__
    assert statuses == [health_pb2.HealthCheckResponse.SERVING] * 2


def save_policy(runs_dir: Path, name: str, version: int) -> None:
    run_dir = runs_dir / name
    run_dir.mkdir(parents=True)
    meta = {
        'observation_version': version,
        'obs_dim': OBS_DIM,
        'hidden': 8,
        'created_at': 1,
        'global_step': 42,
        'status': 'finished',
    }
    checkpoint.save(run_dir, ActorCritic(OBS_DIM, 8), meta)
    checkpoint.write_record(run_dir, meta)


async def _act(tmp_path: Path, requests: list[melody_pb2.ActRequest]):
    server, port, _ = await start('127.0.0.1:0', torch.device('cpu'), tmp_path)
    try:
        async with grpc.aio.insecure_channel(f'127.0.0.1:{port}') as channel:
            stub = melody_pb2_grpc.MelodyServiceStub(channel)
            policies = await stub.ListPolicies(melody_pb2.ListPoliciesRequest())
            results = []
            for r in requests:
                try:
                    results.append(await stub.Act(r))
                except grpc.aio.AioRpcError as e:
                    results.append(e.code())
            runs = await stub.ListTrainingRuns(melody_pb2.ListTrainingRunsRequest())
            try:
                await stub.GetTrainingRun(melody_pb2.GetTrainingRunRequest(id='nope'))
            except grpc.aio.AioRpcError as e:
                missing = e.code()
        return policies, results, runs, missing
    finally:
        await server.stop(None)


def test_list_policies_and_act(tmp_path: Path) -> None:
    save_policy(tmp_path, 'trained', version=3)
    obs = encode(np.zeros((8, OBS_DIM), dtype=np.float32))
    wrong_dim = encode(np.zeros((8, OBS_DIM + 1), dtype=np.float32))
    request = melody_pb2.ActRequest
    policies, results, runs, missing = asyncio.run(
        _act(
            tmp_path,
            [
                request(policy='trained', observations=obs, observation_version=3),
                request(policy='trained', observations=obs, observation_version=4),
                request(policy='absent', observations=obs, observation_version=3),
                request(
                    policy='trained', observations=wrong_dim, observation_version=3
                ),
            ],
        )
    )
    [p] = policies.policies
    assert (p.name, p.observation_version, p.global_step) == ('trained', 3, 42)
    assert p.status == melody_pb2.TRAINING_STATUS_FINISHED

    ok, *errors = results
    actions = decode(ok.actions)
    assert actions.shape == (8, 3)
    assert np.all(np.abs(actions[:, :2]) <= 1) and set(actions[:, 2]) <= {0.0, 1.0}
    assert errors == [
        grpc.StatusCode.FAILED_PRECONDITION,
        grpc.StatusCode.NOT_FOUND,
        grpc.StatusCode.INVALID_ARGUMENT,
    ]
    assert [r.id for r in runs.runs] == ['trained']
    assert runs.runs[0].status == melody_pb2.TRAINING_STATUS_FINISHED
    assert missing == grpc.StatusCode.NOT_FOUND


@pytest.mark.parametrize('bad', [{'slots': 6}, {'worlds': 100_000}])
def test_start_training_validates(tmp_path: Path, bad: dict) -> None:
    async def scenario():
        server, port, _ = await start('127.0.0.1:0', torch.device('cpu'), tmp_path)
        try:
            async with grpc.aio.insecure_channel(f'127.0.0.1:{port}') as channel:
                stub = melody_pb2_grpc.MelodyServiceStub(channel)
                with pytest.raises(grpc.aio.AioRpcError) as e:
                    await stub.StartTraining(
                        melody_pb2.StartTrainingRequest(
                            config=melody_pb2.TrainingConfig(**bad)
                        )
                    )
                return e.value.code()
        finally:
            await server.stop(None)

    assert asyncio.run(scenario()) == grpc.StatusCode.INVALID_ARGUMENT
