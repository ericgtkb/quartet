"""Packed tensors (`quartet.score.Tensor`) <-> NumPy arrays.

Data is little-endian and row-major. Decoding wraps the message's bytes
without copying; the arrays are read-only views.
"""

import numpy as np

from melody.generated.quartet.score import tensor_pb2

_DTYPES = {
    tensor_pb2.DTYPE_FLOAT32: np.dtype('<f4'),
    tensor_pb2.DTYPE_UINT8: np.dtype('u1'),
}
_CODES = {dtype: code for code, dtype in _DTYPES.items()}


def encode(array: np.ndarray) -> tensor_pb2.Tensor:
    """Encode a float32 or uint8 array."""
    dtype = array.dtype.newbyteorder('<') if array.dtype.kind == 'f' else array.dtype
    code = _CODES.get(dtype)
    if code is None:
        raise TypeError(f'unsupported tensor dtype {array.dtype}; use float32 or uint8')
    data = np.ascontiguousarray(array, dtype=dtype).tobytes()
    return tensor_pb2.Tensor(dtype=code, shape=list(array.shape), data=data)


def decode(tensor: tensor_pb2.Tensor) -> np.ndarray:
    """Decode a tensor into a read-only array, validating its size."""
    dtype = _DTYPES.get(tensor.dtype)
    if dtype is None:
        raise ValueError(f'unsupported tensor dtype code {tensor.dtype}')
    shape = tuple(tensor.shape)
    expected = int(np.prod(shape, dtype=np.int64)) * dtype.itemsize
    if len(tensor.data) != expected:
        raise ValueError(
            f'tensor has {len(tensor.data)} bytes, expected {expected} for shape {shape}'
        )
    return np.frombuffer(tensor.data, dtype=dtype).reshape(shape)
