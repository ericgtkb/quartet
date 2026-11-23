"""Command-line entry point: `quartet-melody serve | train | evaluate | bench`."""

import argparse
import asyncio
import dataclasses
import json
import logging
import os
import signal
import sys
import threading
from pathlib import Path
from typing import Any

from melody.device import DEVICE_CHOICES, resolve_device

DEFAULT_ADDR = '127.0.0.1:50052'
DEFAULT_HARMONY = '127.0.0.1:50051'
DEFAULT_RUNS = 'runs'


def _harmony_default() -> str:
    return os.environ.get('QUARTET_HARMONY_ADDR') or DEFAULT_HARMONY


def _runs_default() -> str:
    return os.environ.get('QUARTET_MELODY_RUNS') or DEFAULT_RUNS


def _int_list(value: str) -> list[int]:
    try:
        items = [int(v) for v in value.split(',') if v]
    except ValueError:
        raise argparse.ArgumentTypeError(
            f'expected comma-separated integers, got {value!r}'
        ) from None
    if not items or any(i < 1 for i in items):
        raise argparse.ArgumentTypeError(f'expected positive integers, got {value!r}')
    return items


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(prog='quartet-melody', description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)

    serve = commands.add_parser('serve', help='run the gRPC inference service')
    serve.add_argument(
        '--addr',
        default=os.environ.get('QUARTET_MELODY_ADDR') or DEFAULT_ADDR,
        help='listen address (env QUARTET_MELODY_ADDR, default %(default)s)',
    )
    serve.add_argument(
        '--device',
        choices=DEVICE_CHOICES,
        default=os.environ.get('QUARTET_MELODY_DEVICE') or 'auto',
        help='torch device (env QUARTET_MELODY_DEVICE, default %(default)s)',
    )
    serve.add_argument(
        '--runs-dir',
        default=_runs_default(),
        help='training runs and checkpoints (env QUARTET_MELODY_RUNS, default %(default)s)',
    )
    serve.add_argument(
        '--harmony',
        default=_harmony_default(),
        help='harmony target for training runs (env QUARTET_HARMONY_ADDR, default %(default)s)',
    )

    train = commands.add_parser(
        'train', help='train a policy with PPO against harmony (see melody.train)'
    )
    _add_config_arguments(train)
    train.add_argument(
        '--json',
        action='store_true',
        help='print one JSON object per update to stdout (used by `serve`)',
    )

    evaluate = commands.add_parser(
        'evaluate',
        help="measure trained policies like harmony's baseline probe",
    )
    evaluate.add_argument('policies', nargs='+', help='run ids')
    evaluate.add_argument('--harmony', default=_harmony_default())
    evaluate.add_argument('--runs-dir', default=_runs_default())
    evaluate.add_argument('--device', choices=DEVICE_CHOICES, default='cpu')

    bench = commands.add_parser(
        'bench', help='measure rehearsal throughput against harmony'
    )
    bench.add_argument(
        '--target',
        action='append',
        help='harmony target, host:port; repeat to compare '
        f'(default env QUARTET_HARMONY_ADDR or {DEFAULT_HARMONY})',
    )
    bench.add_argument(
        '--worlds',
        type=_int_list,
        default=[1, 4, 16, 64],
        help='comma-separated world counts',
    )
    bench.add_argument(
        '--slots', type=int, default=64, help='agents per world (multiple of 4)'
    )
    bench.add_argument(
        '--streams', type=_int_list, default=[1], help='comma-separated stream counts'
    )
    bench.add_argument(
        '--steps', type=int, default=200, help='timed steps per configuration'
    )
    bench.add_argument(
        '--device',
        choices=DEVICE_CHOICES,
        help='also move observations to this torch device',
    )

    args = parser.parse_args(argv)
    logging.basicConfig(
        level=logging.INFO, format='%(asctime)s %(levelname)s %(name)s: %(message)s'
    )

    if args.command == 'train':
        _train(args)
        return

    if args.command == 'evaluate':
        from melody import evaluate as evaluate_module

        evaluate_module.main(
            args.harmony,
            Path(args.runs_dir),
            args.policies,
            resolve_device(args.device),
        )
        return

    if args.command == 'bench':
        from melody import bench as bench_module

        targets = args.target or [
            os.environ.get('QUARTET_HARMONY_ADDR') or DEFAULT_HARMONY
        ]
        device = str(resolve_device(args.device)) if args.device else None
        bench_module.main(
            targets, args.worlds, args.slots, args.streams, args.steps, device
        )
        return

    try:
        device = resolve_device(args.device)
    except ValueError as e:
        parser.error(str(e))

    # Imported here so `--help` stays fast; the server imports grpc and torch.
    from melody.server import serve as run_server

    asyncio.run(run_server(args.addr, device, Path(args.runs_dir), args.harmony))


def _add_config_arguments(parser: argparse.ArgumentParser) -> None:
    """One option per training Config field, so the dataclass stays the single
    list of hyperparameters."""
    from melody.train import Config

    defaults = Config()
    env_defaults = {'harmony': _harmony_default(), 'runs_dir': _runs_default()}
    for f in dataclasses.fields(Config):
        if not f.init:
            continue
        default = env_defaults.get(f.name, getattr(defaults, f.name))
        flag = '--' + f.name.replace('_', '-')
        if isinstance(default, bool):
            parser.add_argument(
                flag, action=argparse.BooleanOptionalAction, default=default
            )
        elif f.name == 'target_kl':
            parser.add_argument(flag, type=float, default=default)
        elif f.name == 'device':
            parser.add_argument(flag, choices=DEVICE_CHOICES, default=default)
        else:
            parser.add_argument(
                flag, type=type(default), default=default, help=f'default {default}'
            )


def _train(args: argparse.Namespace) -> None:
    from melody.train import Config, train

    config = Config(
        **{f.name: getattr(args, f.name) for f in dataclasses.fields(Config) if f.init}
    )
    # The first SIGINT or SIGTERM stops after the current update and saves a
    # checkpoint; a second SIGINT interrupts immediately.
    stop = threading.Event()

    def request_stop(signum: int, _frame: Any) -> None:
        stop.set()
        signal.signal(signum, signal.SIG_DFL)

    signal.signal(signal.SIGINT, request_stop)
    signal.signal(signal.SIGTERM, request_stop)

    def emit(metrics: dict[str, Any]) -> None:
        if args.json:
            print(json.dumps({'metrics': metrics}), flush=True)
            return
        print(
            f'update {metrics["update"]:>4}/{config.num_updates} '
            f'step {metrics["global_step"]:>11,} '
            f'sps {metrics["steps_per_second"]:>9,.0f} '
            f'return {metrics["episode_return"]:>7.2f} '
            f'chord {metrics["chord_rate"]:.4f} '
            f'deaths/1k {metrics["deaths_per_1k_beats"]:>6.1f} '
            f'energy {metrics["mean_energy"]:.3f} '
            f'kl {metrics["approx_kl"]:.4f}',
            flush=True,
        )

    try:
        config.validate()
        status = train(config, emit=emit, should_stop=stop.is_set)
    except Exception as e:
        logging.getLogger('melody.train').exception('training failed')
        if args.json:
            print(json.dumps({'status': 'failed', 'error': str(e)}), flush=True)
        sys.exit(1)
    if args.json:
        print(json.dumps({'status': status}), flush=True)
    else:
        print(f'{status}: {Path(config.runs_dir) / config.run_id}')


if __name__ == '__main__':
    main()
