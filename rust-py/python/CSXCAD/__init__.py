# -*- coding: utf-8 -*-
"""CSXCAD Python package backed by the Rust `csxcad_py` extension.

The import paths and class names mirror the native package:
``CSXCAD.ContinuousStructure``, ``CSXCAD.CSProperties``, ``CSXCAD.CSPrimitives``,
``CSXCAD.CSRectGrid`` and ``CSXCAD.CSTransform``.
"""

AppCSXCAD_BIN = "AppCSXCAD"

try:
    from importlib.metadata import version

    __version__ = version("CSXCAD")
except Exception:  # pragma: no cover - only used without installed metadata
    __version__ = "0.0.0"

from CSXCAD import Utilities  # noqa: F401
from CSXCAD.CSXCAD import ContinuousStructure  # noqa: F401,E402
from CSXCAD import CSProperties  # noqa: F401,E402
from CSXCAD import CSPrimitives  # noqa: F401,E402
from CSXCAD import CSRectGrid  # noqa: F401,E402
from CSXCAD import CSTransform  # noqa: F401,E402

__all__ = [
    "ContinuousStructure",
    "CSProperties",
    "CSPrimitives",
    "CSRectGrid",
    "CSTransform",
    "Utilities",
]
