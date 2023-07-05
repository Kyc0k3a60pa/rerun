# NOTE: This file was autogenerated by re_types_builder; DO NOT EDIT.

from __future__ import annotations

from typing import Sequence, Tuple, Union

import numpy as np
import numpy.typing as npt
import pyarrow as pa
from attrs import define, field

from .._baseclasses import (
    BaseExtensionArray,
    BaseExtensionType,
)
from .._converters import (
    to_np_float32,
)
from ._overrides import vec2d_native_to_pa_array  # noqa: F401

__all__ = ["Vec2D", "Vec2DArray", "Vec2DArrayLike", "Vec2DLike", "Vec2DType"]


@define
class Vec2D:
    """A vector in 2D space."""

    xy: npt.NDArray[np.float32] = field(converter=to_np_float32)

    def __array__(self, dtype: npt.DTypeLike = None) -> npt.ArrayLike:
        return np.asarray(self.xy, dtype=dtype)


Vec2DLike = Union[Vec2D, Tuple[float, float]]

Vec2DArrayLike = Union[
    Vec2D, Sequence[Vec2DLike], npt.NDArray[np.float32], Sequence[Tuple[float, float]], Sequence[float]
]


# --- Arrow support ---


class Vec2DType(BaseExtensionType):
    def __init__(self) -> None:
        pa.ExtensionType.__init__(self, pa.list_(pa.field("item", pa.float32(), False, {}), 2), "rerun.datatypes.Vec2D")


class Vec2DArray(BaseExtensionArray[Vec2DArrayLike]):
    _EXTENSION_NAME = "rerun.datatypes.Vec2D"
    _EXTENSION_TYPE = Vec2DType

    @staticmethod
    def _native_to_pa_array(data: Vec2DArrayLike, data_type: pa.DataType) -> pa.Array:
        return vec2d_native_to_pa_array(data, data_type)


Vec2DType._ARRAY_TYPE = Vec2DArray

# TODO(cmc): bring back registration to pyarrow once legacy types are gone
# pa.register_extension_type(Vec2DType())
