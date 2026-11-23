import numpy as np
import pytest

from melody.generated.quartet.score import tensor_pb2
from melody.tensor import decode, encode


def test_float32_round_trip() -> None:
    a = np.arange(12, dtype=np.float32).reshape(3, 4) / 3
    t = encode(a)
    assert t.dtype == tensor_pb2.DTYPE_FLOAT32
    assert list(t.shape) == [3, 4]
    np.testing.assert_array_equal(decode(t), a)


def test_uint8_round_trip() -> None:
    a = np.array([0, 1, 1, 0], dtype=np.uint8)
    np.testing.assert_array_equal(decode(encode(a)), a)


def test_data_is_little_endian() -> None:
    t = encode(np.array([1.0], dtype=np.float32))
    assert t.data == b'\x00\x00\x80\x3f'


def test_rejects_unsupported_dtypes_and_bad_sizes() -> None:
    with pytest.raises(TypeError):
        encode(np.zeros(3, dtype=np.float64))
    bad = tensor_pb2.Tensor(dtype=tensor_pb2.DTYPE_FLOAT32, shape=[2], data=b'\x00' * 4)
    with pytest.raises(ValueError, match='expected 8'):
        decode(bad)
