"""Torch device selection: MPS for native macOS runs, CPU in Linux containers."""

import torch

DEVICE_CHOICES = ('auto', 'mps', 'cpu')


def resolve_device(name: str) -> torch.device:
    """Return the torch device for a --device value.

    `auto` prefers MPS when available and falls back to CPU. An explicit `mps`
    fails loudly when MPS is unavailable instead of silently using the CPU.
    """
    match name:
        case 'auto':
            return torch.device('mps' if torch.backends.mps.is_available() else 'cpu')
        case 'mps':
            if not torch.backends.mps.is_available():
                raise ValueError('device "mps" requested but MPS is not available')
            return torch.device('mps')
        case 'cpu':
            return torch.device('cpu')
        case _:
            raise ValueError(
                f'unknown device {name!r}; expected one of {DEVICE_CHOICES}'
            )
