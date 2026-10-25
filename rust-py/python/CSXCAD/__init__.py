# -*- coding: utf-8 -*-
"""CSXCAD Python package backed by the Rust `csxcad_py` extension.

The native package exposes `ContinuousStructure` from `CSXCAD.CSXCAD`. The
Rust extension currently provides the `PyStructure`/`PyProperty` classes; they
are re-exported here under the native names so the scripting layers can import
the package. The full native class surface is tracked in RM-090.
"""

AppCSXCAD_BIN = 'AppCSXCAD'

try:
    from importlib.metadata import version

    __version__ = version("CSXCAD")
except Exception:  # pragma: no cover - only used without installed metadata
    __version__ = "0.0.0"

from CSXCAD import Utilities  # noqa: F401

try:
    import csxcad_py as _rust

    ContinuousStructure = _rust.PyStructure
    CSProperties = None
    CSPrimitives = None
except ImportError:  # the extension is optional for the pure utilities
    _rust = None

    class ContinuousStructure:  # pragma: no cover - import guard only
        def __init__(self, *args, **kwargs):
            raise ImportError(
                "the Rust CSXCAD extension (csxcad_py) is not installed"
            )
