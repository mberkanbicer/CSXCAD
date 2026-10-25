# -*- coding: utf-8 -*-
"""Parameter set objects for CSXCAD.

The parameters themselves are owned by the Rust structure; ``ParameterSet`` is
the handle that properties are created with, matching the native API where a
property keeps a reference to the parameter set of its structure.
"""

from CSXCAD.CSXCAD import ParameterSet

__all__ = ["ParameterSet"]
