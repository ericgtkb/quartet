import pytest
import torch

from melody.device import resolve_device


def test_cpu_is_always_available() -> None:
    assert resolve_device('cpu') == torch.device('cpu')


def test_auto_prefers_mps_when_available(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(torch.backends.mps, 'is_available', lambda: True)
    assert resolve_device('auto').type == 'mps'


def test_auto_falls_back_to_cpu(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(torch.backends.mps, 'is_available', lambda: False)
    assert resolve_device('auto').type == 'cpu'


def test_explicit_mps_fails_when_unavailable(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(torch.backends.mps, 'is_available', lambda: False)
    with pytest.raises(ValueError, match='MPS is not available'):
        resolve_device('mps')


def test_unknown_device_is_rejected() -> None:
    with pytest.raises(ValueError, match='unknown device'):
        resolve_device('cuda')
