# -*- coding: utf-8 -*-
"""Pure-Python utilities for the CSXCAD Python API.

The geometry classes are provided by the Rust extension; these helpers are
shared by the openEMS Python modules and keep the native import path
(`from CSXCAD.Utilities import CheckNyDir`).
"""


def CheckNyDir(ny):
    """Translate 'x'/'y'/'z' (or 'r'/'a'/'z') into 0/1/2."""
    cartesian = ('x', 'y', 'z')
    cylindrical = ('r', 'a', 'z')
    if ny in [0, 1, 2]:
        return ny
    if ny in cartesian:
        return cartesian.index(ny)
    if ny in cylindrical:
        return cylindrical.index(ny)
    raise ValueError(
        f'Invalid direction `{ny}`, valid directions are (0, 1, 2), '
        f'{cartesian} in Cartesian coordinates or {cylindrical} in cylindrical coordinates.'
    )


def GetMultiDirs(dirs):
    """Expand a direction string ('x', 'xy', 'all', ...) into a list."""
    assert type(dirs) is str, 'GetMultiDirs: dirs must be of type str'
    dirs = ''.join(sorted(dirs))
    if dirs in ('all', 'xyz', 'arz'):
        return [0, 1, 2]
    if dirs in ('x', 'r'):
        return [0]
    if dirs in ('y', 'a'):
        return [1]
    if dirs == 'z':
        return [2]
    if dirs in ('xy', 'ar'):
        return [0, 1]
    if dirs in ('yz', 'az'):
        return [1, 2]
    if dirs in ('xz', 'rz'):
        return [0, 2]
    raise ValueError(f'Invalid direction combination `{dirs}`')


# ---------------------------------------------------------------------------
# Wrapper factory per CSObject::ObjectKind.
#
# Native source: ``CSXCAD/python/CSXCAD/Utilities.pyx:63-74``. The native
# wrappers register a factory per object kind on import, keyed by
# ``int(kind)``, and ``WrapperFromAddress`` returns ``None`` when no factory has
# been registered for that kind (``Utilities.pyx:72-74``).
#
# The Rust port has no C++ address space: every wrapper is resolved from the
# owning structure plus a stable native id (``ContinuousStructure._wrap_property``
# and ``CSPrimitives._wrap``), so no module registers a factory. The two
# functions are kept because they are part of the native public surface; they
# behave exactly like the native pair for callers that do register a factory.
_WRAPPER_FACTORIES = {}


def RegisterWrapperFactory(kind, factory):
    """Register the wrapper factory for one ``CSObject::ObjectKind``."""
    _WRAPPER_FACTORIES[int(kind)] = factory


def WrapperFromAddress(addr, kind):
    """Wrapper for the native object at ``addr`` of the given object kind."""
    factory = _WRAPPER_FACTORIES.get(int(kind), None)
    if factory is None:
        return None
    return factory(addr)
