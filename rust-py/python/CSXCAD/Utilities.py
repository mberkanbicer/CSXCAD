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
