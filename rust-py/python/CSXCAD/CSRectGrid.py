# -*- coding: utf-8 -*-
"""Rectilinear grid class for CSXCAD.

The implementation is the Rust ``PyCSRectGrid`` class; this module keeps the
native import path (``from CSXCAD import CSRectGrid``), the native class name and
the ``CoordinateSystem`` enum.
"""

from csxcad_py import CSRectGrid


class CoordinateSystem:
    """Native ``CoordinateSystem`` enum."""

    CARTESIAN = 0
    CYLINDRICAL = 1
    UNDEFINED_CS = 2


__all__ = ["CSRectGrid", "CoordinateSystem"]
