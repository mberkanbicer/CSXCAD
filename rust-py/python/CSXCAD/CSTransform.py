# -*- coding: utf-8 -*-
"""Affine transformations for CSXCAD primitives.

The implementation is the Rust `PyCSTransform` class; this module keeps the
native import path (`from CSXCAD import CSTransform`) and the native class name.
"""

from csxcad_py import CSTransform

__all__ = ["CSTransform"]
