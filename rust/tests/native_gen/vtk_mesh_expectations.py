#!/usr/bin/env python3
"""Generate the STL/PLY fixture corpus and its VTK-verified expectations.

Run inside WSL with VTK's Python bindings (VTK 9.5):

    python3 vtk_mesh_expectations.py <fixtures-dir>

The script is the tool that produced `CSXCAD/rust/tests/fixtures/mesh`:

* it copies the two native CSXCAD test meshes (binary STL, binary little
  endian PLY) from `CSXCAD/python/tests`,
* it writes additional meshes with VTK's own STL/PLY writers (ASCII and binary,
  triangles and quads) so both parser paths and both face arities are covered,
* it writes one multi-solid ASCII STL and the malformed files by hand,
* for every valid mesh it reads the file back with `vtkSTLReader` (merging on,
  exactly like native `CSPrimPolyhedronReader::ReadFile`) or `vtkPLYReader`
  and stores the point and polygon arrays in VTK order as
  `<name>.expected`.

The expectation file format is line oriented:

    format|<stl|ply>|<ascii|binary|binary_little_endian>
    vertices|<count>
    vertex|<x>|<y>|<z>            (VTK point order)
    faces|<count>
    face|<i0>,<i1>,...            (VTK polygon order and winding)
    error|<reason>                (malformed fixtures only)

Values are printed with 17 significant digits of the double VTK returns, so a
reader that reproduces VTK's point/face order and float rounding matches the
file exactly.
"""

import math
import os
import shutil
import struct
import sys

import vtk
from vtk.util.numpy_support import numpy_to_vtk, vtk_to_numpy
import numpy as np


def write_expectation(path, file_format, points, polys, error=None):
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        if error is not None:
            handle.write("error|%s\n" % error)
            return
        handle.write("format|%s\n" % file_format)
        handle.write("vertices|%d\n" % len(points))
        for point in points:
            handle.write(
                "vertex|%.17g|%.17g|%.17g\n" % (point[0], point[1], point[2])
            )
        handle.write("faces|%d\n" % len(polys))
        for poly in polys:
            handle.write(
                "face|%s\n" % ",".join(str(int(index)) for index in poly)
            )


def read_stl(path):
    reader = vtk.vtkSTLReader()
    reader.SetFileName(path)
    reader.SetMerging(1)
    reader.Update()
    return reader.GetOutput()


def read_ply(path):
    reader = vtk.vtkPLYReader()
    reader.SetFileName(path)
    reader.Update()
    return reader.GetOutput()


def polydata(points, polygons):
    """Build a vtkPolyData from a python point list and polygon list."""
    data = vtk.vtkPolyData()
    vtk_points = vtk.vtkPoints()
    vtk_points.SetDataTypeToDouble()
    for point in points:
        vtk_points.InsertNextPoint(*point)
    data.SetPoints(vtk_points)
    cells = vtk.vtkCellArray()
    for polygon in polygons:
        cells.InsertNextCell(len(polygon))
        for index in polygon:
            cells.InsertCellPoint(index)
    data.SetPolys(cells)
    return data


def cube():
    points = [
        (0.0, 0.0, 0.0),
        (1.0, 0.0, 0.0),
        (1.0, 1.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, 0.0, 1.0),
        (1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0),
        (0.0, 1.0, 1.0),
    ]
    triangles = [
        (0, 2, 1), (0, 3, 2),
        (4, 5, 6), (4, 6, 7),
        (0, 1, 5), (0, 5, 4),
        (1, 2, 6), (1, 6, 5),
        (2, 3, 7), (2, 7, 6),
        (3, 0, 4), (3, 4, 7),
    ]
    return points, triangles


def quad_grid():
    points = [
        (0.0, 0.0, 0.0),
        (1.0, 0.0, 0.0),
        (2.0, 0.0, 0.0),
        (0.0, 1.0, 0.0),
        (1.0, 1.0, 0.0),
        (2.0, 1.0, 0.0),
    ]
    quads = [(0, 1, 4, 3), (1, 2, 5, 4)]
    return points, quads


def write_stl(path, data, binary):
    writer = vtk.vtkSTLWriter()
    writer.SetFileName(path)
    writer.SetInputData(data)
    if binary:
        writer.SetFileTypeToBinary()
    else:
        writer.SetFileTypeToASCII()
    if not writer.Write():
        raise SystemExit("failed to write %s" % path)


def write_ply(path, data, binary):
    writer = vtk.vtkPLYWriter()
    writer.SetFileName(path)
    writer.SetInputData(data)
    writer.SetFileTypeToBinary() if binary else writer.SetFileTypeToASCII()
    if not writer.Write():
        raise SystemExit("failed to write %s" % path)


TWO_SOLIDS_ASCII = """solid first
facet normal 0 0 1
  outer loop
    vertex 0 0 0
    vertex 1 0 0
    vertex 0 1 0
  endloop
endfacet
endsolid first
solid second
facet normal 0 0 1
  outer loop
    vertex 0 0 1
    vertex 1 0 0
    vertex 0 1 0
  endloop
endfacet
endsolid second
"""


def write_malformed(directory):
    # Binary STL header that declares one facet but the payload stops inside
    # the normal vector, so VTK cannot produce a facet record.
    data = bytearray(b"binary truncated STL".ljust(80, b" "))
    data += struct.pack("<I", 1)
    data += struct.pack("<f", 0.0) * 3
    with open(os.path.join(directory, "truncated_binary.stl"), "wb") as handle:
        handle.write(data)

    with open(os.path.join(directory, "no_solid_ascii.stl"), "w", encoding="ascii") as handle:
        handle.write("facet normal 0 0 1\nouter loop\nvertex 0 0 0\n")

    with open(os.path.join(directory, "unknown_format.ply"), "w", encoding="ascii") as handle:
        # Header with an unsupported format keyword; VTK refuses cleanly.
        # (A PLY that ends before `end_header` makes VTK's PLY reader crash, so
        # that case is deliberately not part of the fixture corpus.)
        handle.write(
            "ply\n"
            "format weird 1.0\n"
            "element vertex 3\n"
            "property float x\n"
            "property float y\n"
            "property float z\n"
            "element face 1\n"
            "property list uchar int vertex_indices\n"
            "end_header\n"
            "0 0 0\n"
            "1 0 0\n"
            "0 1 0\n"
            "3 0 1 2\n"
        )

    with open(
        os.path.join(directory, "points_only.ply"), "w", encoding="ascii"
    ) as handle:
        # Valid PLY with vertices but no face element: VTK reads the points and
        # produces zero polygon cells, which is the native "file invalid or
        # empty" condition.
        handle.write(
            "ply\n"
            "format ascii 1.0\n"
            "element vertex 3\n"
            "property float x\n"
            "property float y\n"
            "property float z\n"
            "end_header\n"
            "0 0 0\n"
            "1 0 0\n"
            "0 1 0\n"
        )


def main(argv):
    if len(argv) != 2:
        raise SystemExit("usage: vtk_mesh_expectations.py <fixtures-dir>")
    directory = argv[1]
    os.makedirs(directory, exist_ok=True)
    repo = os.path.abspath(
        os.path.join(os.path.dirname(__file__), "..", "..", "..")
    )
    native_tests = os.path.join(repo, "python", "tests")
    shutil.copyfile(os.path.join(native_tests, "sphere.stl"),
                    os.path.join(directory, "sphere_native_stl.stl"))
    shutil.copyfile(os.path.join(native_tests, "sphere.ply"),
                    os.path.join(directory, "sphere_native_ply.ply"))

    points, triangles = cube()
    write_stl(os.path.join(directory, "cube_ascii.stl"), polydata(points, triangles), False)
    write_stl(os.path.join(directory, "cube_binary.stl"), polydata(points, triangles), True)

    points, quads = quad_grid()
    write_ply(os.path.join(directory, "quads_ascii.ply"), polydata(points, quads), False)
    write_ply(os.path.join(directory, "quads_binary.ply"), polydata(points, quads), True)

    with open(os.path.join(directory, "two_solids_ascii.stl"), "w", encoding="ascii") as handle:
        handle.write(TWO_SOLIDS_ASCII)

    write_malformed(directory)

    for name, reader in [
        ("sphere_native_stl.stl", read_stl),
        ("cube_ascii.stl", read_stl),
        ("cube_binary.stl", read_stl),
        ("two_solids_ascii.stl", read_stl),
        ("sphere_native_ply.ply", read_ply),
        ("quads_ascii.ply", read_ply),
        ("quads_binary.ply", read_ply),
    ]:
        data = reader(os.path.join(directory, name))
        if data.GetNumberOfPoints() == 0 or data.GetNumberOfPolys() == 0:
            raise SystemExit("VTK could not read %s" % name)
        points = [data.GetPoint(index) for index in range(data.GetNumberOfPoints())]
        polys = []
        cells = data.GetPolys()
        cells.InitTraversal()
        while True:
            cell = vtk.vtkIdList()
            if not cells.GetNextCell(cell):
                break
            polys.append([cell.GetId(index) for index in range(cell.GetNumberOfIds())])
        if name.endswith(".stl"):
            base = name[: -len(".stl")]
            file_format = "stl"
        else:
            base = name[: -len(".ply")]
            file_format = "ply"
        kind = "binary"
        if base.endswith("_ascii"):
            kind = "ascii"
        write_expectation(
            os.path.join(directory, base + ".expected"),
            "%s|%s" % (file_format, kind),
            points,
            polys,
        )
        print("wrote %s.expected (%d points, %d faces)" % (base, len(points), len(polys)))

    for name in [
        "truncated_binary.stl",
        "no_solid_ascii.stl",
        "unknown_format.ply",
        "points_only.ply",
    ]:
        base = os.path.splitext(name)[0]
        # Ground the "malformed" expectation: VTK (the reader native uses) must
        # not yield a usable mesh, which is exactly the native
        # "file invalid or empty, skipping" condition (no points or no cells).
        reader = read_stl if name.endswith(".stl") else read_ply
        data = reader(os.path.join(directory, name))
        if data.GetNumberOfPoints() != 0 and data.GetNumberOfPolys() != 0:
            raise SystemExit(
                "VTK unexpectedly read %s (%d points, %d cells)"
                % (name, data.GetNumberOfPoints(), data.GetNumberOfCells())
            )
        write_expectation(
            os.path.join(directory, base + ".expected"),
            "malformed",
            [],
            [],
            error="VTK/native CSXCAD rejects this file",
        )
        print("wrote %s.expected (malformed, VTK returns an empty mesh)" % base)


if __name__ == "__main__":
    main(sys.argv)

