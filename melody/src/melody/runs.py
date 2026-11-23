"""Training runs started through the melody server.

Each run is a child process, `python -m melody.cli train --json ...`, so
training never competes with the asyncio inference server for the GIL. The
child prints one JSON line per PPO update (`{"metrics": {...}}`) and a final
`{"status": ...}`; the manager keeps the metrics for watchers. On SIGTERM the
child finishes its current update, saves a checkpoint, and exits `stopped`.

The run directory (see `melody.checkpoint`) is the durable record, so runs
from earlier server sessions and from `quartet-melody train` are listed too.
"""

import asyncio
import collections
import contextlib
import json
import logging
import os
import signal
import sys
import time
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from melody import checkpoint
from melody.train import Config, default_run_id

log = logging.getLogger(__name__)

STOP_GRACE_SECONDS = 30.0

# Request fields with their defaults (from the training Config).
_DEFAULTS = Config()


class RunConflict(RuntimeError):
    """A run is already training."""


@dataclass
class RunView:
    id: str
    status: str
    config: dict[str, int]
    created_at: int
    error: str = ''
    latest: dict[str, Any] | None = None


@dataclass
class _LiveRun:
    id: str
    config: dict[str, int]
    created_at: int
    process: asyncio.subprocess.Process
    metrics: list[dict[str, Any]] = field(default_factory=list)
    status: str = 'running'
    error: str = ''
    done: bool = False
    changed: asyncio.Condition = field(default_factory=asyncio.Condition)
    stderr_tail: collections.deque[str] = field(
        default_factory=lambda: collections.deque(maxlen=20)
    )

    def view(self) -> RunView:
        latest = self.metrics[-1] if self.metrics else None
        return RunView(
            self.id, self.status, self.config, self.created_at, self.error, latest
        )


def _pid_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _request_config(record: dict[str, Any]) -> dict[str, int]:
    c = record.get('config', {})
    return {k: int(c.get(k, 0)) for k in ('total_steps', 'worlds', 'slots', 'seed')}


class TrainingRuns:
    def __init__(
        self,
        runs_dir: Path,
        harmony: str,
        device: str,
        command: Callable[[Config], list[str]] | None = None,
    ) -> None:
        """`command` builds the child's argv; tests replace the trainer with it."""
        self.runs_dir = runs_dir
        self.harmony = harmony
        self.device = device
        self._command = command or self._train_command
        self._live: dict[str, _LiveRun] = {}
        self._tasks: set[asyncio.Task[None]] = set()

    async def start(
        self, total_steps: int = 0, worlds: int = 0, slots: int = 0, seed: int = 0
    ) -> RunView:
        """Validate and start a run; zero values take the training defaults.
        Raises ValueError for a bad config and RunConflict if one is running."""
        if any(not r.done for r in self._live.values()):
            raise RunConflict('a training run is already in progress')
        config = Config(
            total_steps=total_steps or _DEFAULTS.total_steps,
            worlds=worlds or _DEFAULTS.worlds,
            slots=slots or _DEFAULTS.slots,
            seed=seed or _DEFAULTS.seed,
        )
        config.run_id = default_run_id(config.seed)
        while (self.runs_dir / config.run_id).exists() or config.run_id in self._live:
            await asyncio.sleep(1)  # ids have one-second resolution
            config.run_id = default_run_id(config.seed)
        config.validate()

        argv = self._command(config)
        process = await asyncio.create_subprocess_exec(
            *argv,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        run = _LiveRun(
            id=config.run_id,
            config={
                'total_steps': config.total_steps,
                'worlds': config.worlds,
                'slots': config.slots,
                'seed': config.seed,
            },
            created_at=int(time.time()),
            process=process,
        )
        self._live[run.id] = run
        task = asyncio.create_task(self._follow(run))
        self._tasks.add(task)
        task.add_done_callback(self._tasks.discard)
        log.info('training run %s started (pid %d)', run.id, process.pid)
        return run.view()

    def _train_command(self, config: Config) -> list[str]:
        return [
            sys.executable,
            '-m',
            'melody.cli',
            'train',
            '--json',
            f'--run-id={config.run_id}',
            f'--runs-dir={self.runs_dir}',
            f'--harmony={self.harmony}',
            f'--device={self.device}',
            f'--total-steps={config.total_steps}',
            f'--worlds={config.worlds}',
            f'--slots={config.slots}',
            f'--seed={config.seed}',
        ]

    async def _follow(self, run: _LiveRun) -> None:
        """Read the child's output until it exits, then record the outcome."""
        assert run.process.stdout is not None and run.process.stderr is not None

        async def read_stderr() -> None:
            assert run.process.stderr is not None
            async for raw in run.process.stderr:
                line = raw.decode(errors='replace').rstrip()
                run.stderr_tail.append(line)
                log.info('[%s] %s', run.id, line)

        stderr_task = asyncio.create_task(read_stderr())
        final_status = ''
        async for raw in run.process.stdout:
            try:
                message = json.loads(raw)
            except json.JSONDecodeError:
                log.warning('[%s] unexpected output: %r', run.id, raw[:200])
                continue
            if 'metrics' in message:
                async with run.changed:
                    run.metrics.append(message['metrics'])
                    run.changed.notify_all()
            elif 'status' in message:
                final_status = message['status']
                run.error = message.get('error', '')
        code = await run.process.wait()
        await stderr_task

        if code == 0 and final_status in ('finished', 'stopped'):
            status = final_status
        else:
            status = 'failed'
            if not run.error:
                tail = [line for line in run.stderr_tail if line.strip()]
                run.error = tail[-1] if tail else f'exited with code {code}'
        async with run.changed:
            run.status = status
            run.done = True
            run.changed.notify_all()
        log.info('training run %s %s', run.id, status)

    def _from_disk(self, run_id: str) -> RunView | None:
        record = checkpoint.read_record(self.runs_dir / run_id)
        if record is None:
            return None
        status = str(record.get('status', 'failed'))
        error = str(record.get('error', ''))
        pid = record.get('pid')
        if status == 'running' and not (isinstance(pid, int) and _pid_alive(pid)):
            status, error = 'failed', 'interrupted: the training process is gone'
        return RunView(
            id=run_id,
            status=status,
            config=_request_config(record),
            created_at=int(record.get('created_at', 0)),
            error=error,
            latest=record.get('latest'),
        )

    def get(self, run_id: str) -> RunView | None:
        if run_id in self._live:
            return self._live[run_id].view()
        try:
            checkpoint.check_run_id(run_id)
        except checkpoint.CheckpointError:
            return None
        return self._from_disk(run_id)

    def list(self) -> list[RunView]:
        """Every run, newest first."""
        views = {run_id: run.view() for run_id, run in self._live.items()}
        if self.runs_dir.is_dir():
            for run_dir in self.runs_dir.iterdir():
                if run_dir.name not in views and (
                    view := self._from_disk(run_dir.name)
                ):
                    views[run_dir.name] = view
        return sorted(views.values(), key=lambda v: (v.created_at, v.id), reverse=True)

    async def stop(self, run_id: str, wait: bool = False) -> RunView | None:
        """Ask a live run to stop: it finishes its update, saves, and exits
        `stopped` (it is killed after STOP_GRACE_SECONDS). Returns None if
        unknown. Without `wait`, returns at once with the run still running."""
        run = self._live.get(run_id)
        if run is None:
            return self.get(run_id)
        if not run.done:
            with contextlib.suppress(ProcessLookupError):
                run.process.send_signal(signal.SIGTERM)
            waiter = asyncio.create_task(self._wait(run))
            self._tasks.add(waiter)
            waiter.add_done_callback(self._tasks.discard)
            if wait:
                await waiter
        return run.view()

    async def _wait(self, run: _LiveRun) -> None:
        try:
            async with asyncio.timeout(STOP_GRACE_SECONDS):
                async with run.changed:
                    await run.changed.wait_for(lambda: run.done)
        except TimeoutError:
            log.warning('training run %s ignored SIGTERM; killing it', run.id)
            with contextlib.suppress(ProcessLookupError):
                run.process.kill()

    async def watch(self, run_id: str) -> AsyncIterator[dict[str, Any]]:
        """Metrics so far, then new ones until the run ends. Raises KeyError
        for an unknown run."""
        run = self._live.get(run_id)
        if run is None:
            if self.get(run_id) is None:
                raise KeyError(run_id)
            for m in checkpoint.read_metrics(self.runs_dir / run_id):
                yield m
            return
        sent = 0
        while True:
            async with run.changed:
                await run.changed.wait_for(
                    lambda sent=sent: len(run.metrics) > sent or run.done
                )
                batch = run.metrics[sent:]
                done = run.done
            for m in batch:
                yield m
            sent += len(batch)
            if done and sent == len(run.metrics):
                return

    async def close(self) -> None:
        """Stop every live run (each saves a final checkpoint)."""
        await asyncio.gather(
            *(
                self.stop(run_id, wait=True)
                for run_id, r in self._live.items()
                if not r.done
            )
        )
