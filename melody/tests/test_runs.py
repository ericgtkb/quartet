"""The training-run manager, with a scripted child process instead of PPO."""

import asyncio
import json
import sys
from pathlib import Path

import pytest

from melody import checkpoint
from melody.runs import RunConflict, TrainingRuns
from melody.train import Config

# Prints `count` metrics lines `delay` seconds apart; stops early on SIGTERM.
FAKE_TRAINER = """
import json, signal, sys, time
count, delay = int(sys.argv[1]), float(sys.argv[2])
stop = False
def request_stop(*_):
    global stop
    stop = True
signal.signal(signal.SIGTERM, request_stop)
for update in range(1, count + 1):
    print(json.dumps({'metrics': {'update': update, 'global_step': 100 * update}}), flush=True)
    if stop:
        print(json.dumps({'status': 'stopped'}), flush=True)
        sys.exit(0)
    time.sleep(delay)
if count < 0:
    print('boom', file=sys.stderr)
    sys.exit(3)
print(json.dumps({'status': 'finished'}), flush=True)
"""


def manager(tmp_path: Path, count: int, delay: float) -> TrainingRuns:
    def command(_config: Config) -> list[str]:
        return [sys.executable, '-c', FAKE_TRAINER, str(count), str(delay)]

    return TrainingRuns(tmp_path, 'unused:1', 'cpu', command=command)


async def collect(runs: TrainingRuns, run_id: str) -> list[dict]:
    return [m async for m in runs.watch(run_id)]


def test_start_watch_finish(tmp_path: Path) -> None:
    async def scenario():
        runs = manager(tmp_path, count=3, delay=0.01)
        view = await runs.start(total_steps=10_000, worlds=2, slots=8, seed=4)
        assert view.status == 'running'
        assert view.config == {
            'total_steps': 10_000,
            'worlds': 2,
            'slots': 8,
            'seed': 4,
        }
        with pytest.raises(RunConflict):
            await runs.start()
        metrics = await collect(runs, view.id)
        # A late watcher gets the full history too.
        replay = await collect(runs, view.id)
        return runs, view.id, metrics, replay

    runs, run_id, metrics, replay = asyncio.run(scenario())
    assert [m['update'] for m in metrics] == [1, 2, 3]
    assert replay == metrics
    final = runs.get(run_id)
    assert final.status == 'finished'
    assert final.latest == {'update': 3, 'global_step': 300}
    assert [v.id for v in runs.list()] == [run_id]


def test_stop(tmp_path: Path) -> None:
    async def scenario():
        runs = manager(tmp_path, count=1000, delay=0.05)
        view = await runs.start()
        await asyncio.sleep(0.2)
        requested = await runs.stop(view.id)
        await collect(runs, view.id)  # ends when the run does
        return requested, runs.get(view.id), await runs.stop(view.id, wait=True)

    requested, view, again = asyncio.run(scenario())
    assert requested.status == 'running', 'stop returns without waiting'
    assert view.status == 'stopped'
    assert 1 <= view.latest['update'] < 1000
    assert again.status == 'stopped', 'stopping an ended run is a no-op'


def test_failure_reports_stderr(tmp_path: Path) -> None:
    async def scenario():
        runs = manager(tmp_path, count=-1, delay=0)
        view = await runs.start()
        await collect(runs, view.id)
        return runs.get(view.id)

    view = asyncio.run(scenario())
    assert view.status == 'failed'
    assert view.error == 'boom'


def test_invalid_config(tmp_path: Path) -> None:
    with pytest.raises(ValueError):
        asyncio.run(manager(tmp_path, 1, 0).start(slots=6))


def test_lists_runs_from_disk(tmp_path: Path) -> None:
    run_dir = tmp_path / 'old-run'
    run_dir.mkdir()
    checkpoint.write_record(
        run_dir,
        {
            'status': 'running',
            'pid': 2**22 + 12345,  # not a live process
            'created_at': 5,
            'config': {'total_steps': 9, 'worlds': 1, 'slots': 4, 'seed': 2},
            'latest': {'update': 1},
        },
    )
    (run_dir / checkpoint.METRICS).write_text(json.dumps({'update': 1}) + '\n{"upd')
    runs = manager(tmp_path, 1, 0)
    view = runs.get('old-run')
    assert view.status == 'failed' and 'interrupted' in view.error
    assert view.config['slots'] == 4
    assert asyncio.run(collect(runs, 'old-run')) == [{'update': 1}]
    assert runs.get('missing') is None and runs.get('../x') is None
    with pytest.raises(KeyError):
        asyncio.run(collect(runs, 'missing'))
