# -*- coding: utf-8 -*-
"""CSXCAD primitive classes.

This module is the Python surface of the native ``CSXCAD.CSPrimitives`` API. The
geometry lives in the Rust extension; a primitive wrapper is a handle on one
primitive of its property, which is why it keeps working after the property is
added to a :class:`CSXCAD.CSXCAD.ContinuousStructure`.
"""

import weakref

import numpy as np

from CSXCAD import CSProperties
from CSXCAD import CSRectGrid
from CSXCAD.Utilities import CheckNyDir


POINT = 0
BOX = 1
MULTIBOX = 2
SPHERE = 3
SPHERICALSHELL = 4
CYLINDER = 5
CYLINDRICALSHELL = 6
POLYGON = 7
LINPOLY = 8
ROTPOLY = 9
POLYHEDRON = 10
CURVE = 11
WIRE = 12
USERDEFINED = 13
POLYHEDRONREADER = 14

class PrimitiveType:
    """Native ``CSPrimitives::PrimitiveType`` enum."""

    POINT = POINT
    BOX = BOX
    MULTIBOX = MULTIBOX
    SPHERE = SPHERE
    SPHERICALSHELL = SPHERICALSHELL
    CYLINDER = CYLINDER
    CYLINDRICALSHELL = CYLINDRICALSHELL
    POLYGON = POLYGON
    LINPOLY = LINPOLY
    ROTPOLY = ROTPOLY
    POLYHEDRON = POLYHEDRON
    CURVE = CURVE
    WIRE = WIRE
    USERDEFINED = USERDEFINED
    POLYHEDRONREADER = POLYHEDRONREADER


_PRIMITIVE_TYPES = {
    "Point": POINT,
    "Box": BOX,
    "MultiBox": MULTIBOX,
    "Sphere": SPHERE,
    "SphericalShell": SPHERICALSHELL,
    "Cylinder": CYLINDER,
    "CylindricalShell": CYLINDRICALSHELL,
    "Polygon": POLYGON,
    "LinPoly": LINPOLY,
    "RotPoly": ROTPOLY,
    "Polyhedron": POLYHEDRON,
    "Curve": CURVE,
    "Wire": WIRE,
    "UserDefined": USERDEFINED,
    "PolyhedronReader": POLYHEDRONREADER,
}

_next_fallback_id = [0]


def _fallback_id():
    _next_fallback_id[0] += 1
    return _next_fallback_id[0]


class CSPrimitives:
    """Virtual base class for all primitives, cannot be created directly."""

    _instances = weakref.WeakValueDictionary()
    _type_name = None

    @staticmethod
    def fromType(prim_type, pset, prop, no_init=False, **kw):
        if no_init and len(kw) != 0:
            raise Exception("Unable to not init a primitive and set kw options!")
        for cls in (
            CSPrimPoint,
            CSPrimBox,
            CSPrimMultiBox,
            CSPrimSphere,
            CSPrimSphericalShell,
            CSPrimCylinder,
            CSPrimCylindricalShell,
            CSPrimPolygon,
            CSPrimLinPoly,
            CSPrimRotPoly,
            CSPrimPolyhedron,
            CSPrimCurve,
            CSPrimWire,
            CSPrimPolyhedronReader,
        ):
            if prim_type == cls._type_id:
                if no_init:
                    return cls(pset, prop, no_init=True)
                return cls(pset, prop, **kw)
        if prim_type == USERDEFINED:
            raise Exception('Primitive type "USERDEFINED" not yet implemented!')
        raise Exception('fromType:: Unknown/invalid primitive type "{}"!'.format(prim_type))

    # -- construction ------------------------------------------------------
    def __init__(self, pset, prop, *args, no_init=False, **kw):
        if no_init:
            self._prop = None
            self._holder = None
            self._index = -1
            self._fallback = None
            self._transform = None
            return

        if type(self) is CSPrimitives:
            raise Exception("Error, object cannot be created (protected)")

        self._prop = prop
        self._holder = None
        self._fallback = _fallback_id()
        self._transform = None
        self._index = getattr(self, "_create")(prop)
        self._register()

        self._apply_specific_kwargs(kw)
        if "priority" in kw:
            self.SetPriority(kw.pop("priority"))
        if kw:
            raise Exception('Unknown keyword arguments: "{}"'.format(kw))

    def _apply_specific_kwargs(self, kw):
        """Hook for the type specific keyword arguments."""

    def _create(self, prop):
        raise Exception("Error, object cannot be created (protected)")

    @staticmethod
    def _wrap(prop, index):
        key = (id(prop), index)
        existing = CSPrimitives._instances.get(key)
        if existing is not None and existing._prop is prop:
            return existing
        cls = primitive_class(prop._live().primitive_type_name(index))
        obj = cls.__new__(cls)
        obj._prop = prop
        obj._holder = None
        obj._index = index
        obj._fallback = _fallback_id()
        obj._transform = None
        CSPrimitives._instances[key] = obj
        return obj

    def _register(self):
        CSPrimitives._instances[(id(self._prop), self._index)] = self

    # -- handle ------------------------------------------------------------
    def _live(self):
        if self._holder is not None:
            return self._holder
        try:
            return self._prop._live()
        except RuntimeError as error:
            if 'has been deleted' in str(error):
                self._raise_deleted()
            raise

    def _ptr(self):
        return self._live()

    def _raise_deleted(self):
        raise RuntimeError(
            "wrapped C++ object of type {} has been deleted".format(type(self).__name__)
        )

    # -- generic accessors -------------------------------------------------
    def GetCopy(self, prop=None):
        if prop is not None:
            raise NotImplementedError(
                "GetCopy(prop): copying into another property is not supported"
            )
        holder = self._ptr().copy_primitive(self._index)
        cls = primitive_class(self.GetTypeName())
        obj = cls.__new__(cls)
        obj._prop = self._prop
        obj._holder = holder
        obj._index = 0
        obj._fallback = _fallback_id()
        obj._transform = None
        return obj

    copy = GetCopy

    def GetID(self):
        live = self._ptr().primitive_id_value(self._index)
        return live if live else self._fallback

    def GetProperty(self):
        return self._prop

    def GetParameterSet(self):
        return self._prop.GetParameterSet()

    def GetType(self):
        return self._ptr().primitive_type_id(self._index)

    def GetTypeName(self):
        return self._ptr().primitive_type_name(self._index)

    def SetPriority(self, val):
        self._ptr().set_primitive_priority(self._index, int(val))

    def GetPriority(self):
        return self._ptr().primitive_priority(self._index)

    def GetBoundBox(self):
        bounds = self._ptr().primitive_bound_box(self._index)
        box = np.zeros([2, 3])
        for n in range(3):
            box[0, n] = bounds[2 * n]
            box[1, n] = bounds[2 * n + 1]
        return box

    def GetDimension(self):
        return self._ptr().primitive_dimension(self._index)

    def IsInside(self, coord, tol=0):
        return self._ptr().primitive_is_inside(self._index, list(coord), float(tol))

    def GetPrimitiveUsed(self):
        return self._ptr().primitive_used(self._index)

    def SetPrimitiveUsed(self, val):
        self._ptr().set_primitive_used(self._index, bool(val))

    def GetTransform(self):
        if self._transform is None:
            self._transform = self._ptr().primitive_transform(self._index)
        return self._transform

    def AddTransform(self, transform, *args, **kw):
        self.GetTransform().AddTransform(transform, *args, **kw)

    def HasTransform(self):
        return self._ptr().primitive_has_transform(self._index)

    def SetCoordinateSystem(self, cs_type):
        assert cs_type in [
            CSRectGrid.CoordinateSystem.CARTESIAN,
            CSRectGrid.CoordinateSystem.CYLINDRICAL,
            None,
        ], "Unknown coordinate system: {}".format(cs_type)
        self._ptr().set_primitive_coordinate_system(self._index, cs_type)

    def GetCoordinateSystem(self):
        return self._ptr().primitive_coordinate_system(self._index)

    def Update(self):
        return self._ptr().primitive_update(self._index)

    def __repr__(self):
        return "<{} #{}>".format(type(self).__name__, self.GetID())


# ---------------------------------------------------------------------------
class CSPrimPoint(CSPrimitives):
    """Point primitive (single 3D coordinate)."""

    _type_name = "Point"
    _type_id = POINT

    def _create(self, prop):
        return prop._live().create_point([0.0, 0.0, 0.0])

    def _apply_specific_kwargs(self, kw):
        if "coord" in kw:
            self.SetCoord(kw.pop("coord"))

    def SetCoord(self, coord):
        assert len(coord) == 3, "CSPrimPoint:SetCoord: length of array needs to be 3"
        for n in range(3):
            self._ptr().set_primitive_coord(self._index, n, float(coord[n]))

    def GetCoord(self):
        return np.array(
            [self._ptr().primitive_coord(self._index, n) for n in range(3)]
        )


class CSPrimBox(CSPrimitives):
    """Box primitive (start/stop coordinate)."""

    _type_name = "Box"
    _type_id = BOX

    def _create(self, prop):
        origin = [0.0, 0.0, 0.0]
        return prop._live().create_box(origin, origin)

    def _apply_specific_kwargs(self, kw):
        if "start" in kw:
            self.SetStart(kw.pop("start"))
        if "stop" in kw:
            self.SetStop(kw.pop("stop"))

    def SetStart(self, coord):
        assert len(coord) == 3, "CSPrimBox:SetStart: length of array needs to be 3"
        for n in range(3):
            self._ptr().set_primitive_coord(self._index, 2 * n, float(coord[n]))

    def SetStop(self, coord):
        assert len(coord) == 3, "CSPrimBox:SetStop: length of array needs to be 3"
        for n in range(3):
            self._ptr().set_primitive_coord(self._index, 2 * n + 1, float(coord[n]))

    def GetStart(self):
        return np.array(
            [self._ptr().primitive_coord(self._index, 2 * n) for n in range(3)]
        )

    def GetStop(self):
        return np.array(
            [self._ptr().primitive_coord(self._index, 2 * n + 1) for n in range(3)]
        )


class CSPrimCylinder(CSPrimitives):
    """Cylinder primitive (axis + radius)."""

    _type_name = "Cylinder"
    _type_id = CYLINDER

    def _create(self, prop):
        origin = [0.0, 0.0, 0.0]
        return prop._live().create_cylinder(origin, origin, 0.0)

    def _apply_specific_kwargs(self, kw):
        if "start" in kw:
            self.SetStart(kw.pop("start"))
        if "stop" in kw:
            self.SetStop(kw.pop("stop"))
        if "radius" in kw:
            self.SetRadius(kw.pop("radius"))

    def SetStart(self, coord):
        assert len(coord) == 3, "CSPrimCylinder:SetStart: length of array needs to be 3"
        for n in range(3):
            self._ptr().set_primitive_coord(self._index, 2 * n, float(coord[n]))

    def SetStop(self, coord):
        assert len(coord) == 3, "CSPrimCylinder:SetStop: length of array needs to be 3"
        for n in range(3):
            self._ptr().set_primitive_coord(self._index, 2 * n + 1, float(coord[n]))

    def GetStart(self):
        return np.array(
            [self._ptr().primitive_coord(self._index, 2 * n) for n in range(3)]
        )

    def GetStop(self):
        return np.array(
            [self._ptr().primitive_coord(self._index, 2 * n + 1) for n in range(3)]
        )

    def SetRadius(self, val):
        self._ptr().set_primitive_f64(self._index, "radius", float(val))

    def GetRadius(self):
        return self._ptr().primitive_f64(self._index, "radius")


class CSPrimCylindricalShell(CSPrimCylinder):
    """Cylindrical shell primitive (hollow cylinder)."""

    _type_name = "CylindricalShell"
    _type_id = CYLINDRICALSHELL

    def _create(self, prop):
        origin = [0.0, 0.0, 0.0]
        return prop._live().create_cylindrical_shell(origin, origin, 0.0, 0.0)

    def _apply_specific_kwargs(self, kw):
        if "shell_width" in kw:
            self.SetShellWidth(kw.pop("shell_width"))
        super()._apply_specific_kwargs(kw)

    def SetShellWidth(self, val):
        self._ptr().set_primitive_f64(self._index, "shell_width", float(val))

    def GetShellWidth(self):
        return self._ptr().primitive_f64(self._index, "shell_width")


class CSPrimSphere(CSPrimitives):
    """Sphere primitive (center + radius)."""

    _type_name = "Sphere"
    _type_id = SPHERE

    def _create(self, prop):
        return prop._live().create_sphere([0.0, 0.0, 0.0], 0.0)

    def _apply_specific_kwargs(self, kw):
        if "center" in kw:
            self.SetCenter(kw.pop("center"))
        if "radius" in kw:
            self.SetRadius(kw.pop("radius"))

    def SetCenter(self, coord):
        assert len(coord) == 3, "CSPrimSphere:SetCenter: length of array needs to be 3"
        for n in range(3):
            self._ptr().set_primitive_coord(self._index, n, float(coord[n]))

    def GetCenter(self):
        return np.array(
            [self._ptr().primitive_coord(self._index, n) for n in range(3)]
        )

    def SetRadius(self, val):
        self._ptr().set_primitive_f64(self._index, "radius", float(val))

    def GetRadius(self):
        return self._ptr().primitive_f64(self._index, "radius")


class CSPrimSphericalShell(CSPrimSphere):
    """Spherical shell primitive (hollow sphere)."""

    _type_name = "SphericalShell"
    _type_id = SPHERICALSHELL

    def _create(self, prop):
        return prop._live().create_spherical_shell([0.0, 0.0, 0.0], 0.0, 0.0)

    def _apply_specific_kwargs(self, kw):
        if "shell_width" in kw:
            self.SetShellWidth(kw.pop("shell_width"))
        super()._apply_specific_kwargs(kw)

    def SetShellWidth(self, val):
        self._ptr().set_primitive_f64(self._index, "shell_width", float(val))

    def GetShellWidth(self):
        return self._ptr().primitive_f64(self._index, "shell_width")


class CSPrimPolygon(CSPrimitives):
    """Polygon primitive (2D points + normal direction + elevation)."""

    _type_name = "Polygon"
    _type_id = POLYGON

    def _create(self, prop):
        return prop._live().create_polygon(([], []), 0, 0.0)

    def _apply_specific_kwargs(self, kw):
        if "points" in kw:
            points = kw.pop("points")
            assert len(points) == 2, "points must be a list of length 2"
            self.SetCoords(points[0], points[1])
        if "norm_dir" in kw:
            self.SetNormDir(kw.pop("norm_dir"))
        if "elevation" in kw:
            self.SetElevation(kw.pop("elevation"))

    def SetCoords(self, x0, x1):
        assert len(x0) == len(x1), "SetCoords: x0/x1 do not have the same length"
        assert len(x0) > 0, "SetCoords: empty coordinates"
        self._ptr().set_primitive_polygon_points(
            self._index, (list(x0), list(x1))
        )

    def GetCoords(self):
        x0, x1 = self._ptr().primitive_polygon_points(self._index)
        return np.array(x0), np.array(x1)

    def GetQtyCoords(self):
        x0, _ = self._ptr().primitive_polygon_points(self._index)
        return len(x0)

    def ClearCoords(self):
        self._ptr().set_primitive_polygon_points(self._index, ([], []))

    def SetNormDir(self, ny):
        self._ptr().set_primitive_int(self._index, "norm_dir", CheckNyDir(ny))

    def GetNormDir(self):
        return self._ptr().primitive_int(self._index, "norm_dir")

    def SetElevation(self, val):
        self._ptr().set_primitive_f64(self._index, "elevation", float(val))

    def GetElevation(self):
        return self._ptr().primitive_f64(self._index, "elevation")


class CSPrimLinPoly(CSPrimPolygon):
    """Linear extruded polygon primitive."""

    _type_name = "LinPoly"
    _type_id = LINPOLY

    def _create(self, prop):
        return prop._live().create_linpoly(([], []), 0, 0.0, 0.0)

    def _apply_specific_kwargs(self, kw):
        if "length" in kw:
            self.SetLength(kw.pop("length"))
        super()._apply_specific_kwargs(kw)

    def SetLength(self, val):
        self._ptr().set_primitive_f64(self._index, "length", float(val))

    def GetLength(self):
        return self._ptr().primitive_f64(self._index, "length")


class CSPrimRotPoly(CSPrimPolygon):
    """Rotation extruded polygon primitive."""

    _type_name = "RotPoly"
    _type_id = ROTPOLY

    def _create(self, prop):
        return prop._live().create_rotpoly(([], []), 0, 0.0, 2, (0.0, 2 * np.pi))

    def _apply_specific_kwargs(self, kw):
        if "rot_axis" in kw:
            self.SetRotAxisDir(kw.pop("rot_axis"))
        if "angle" in kw:
            angle = kw.pop("angle")
            assert len(angle) == 2, "angle must be a list/array of length 2"
            self.SetAngle(angle[0], angle[1])
        super()._apply_specific_kwargs(kw)

    def SetRotAxisDir(self, ny):
        self._ptr().set_primitive_int(self._index, "rot_axis", CheckNyDir(ny))

    def GetRotAxisDir(self):
        return self._ptr().primitive_int(self._index, "rot_axis")

    def SetAngle(self, a0, a1):
        # the Rust accessor exposes the start angle under "angle"; set both ends
        self._ptr().set_primitive_rotation_angles(self._index, float(a0), float(a1))

    def GetAngle(self):
        return self._ptr().primitive_rotation_angles(self._index)


class CSPrimCurve(CSPrimitives):
    """Curve primitive (consecutive 3D points)."""

    _type_name = "Curve"
    _type_id = CURVE

    def _create(self, prop):
        return prop._live().create_curve([])

    def _apply_specific_kwargs(self, kw):
        if "points" in kw:
            points = kw.pop("points")
            assert len(points) == 3, "points list not of length 3"
            self.SetPoints(points[0], points[1], points[2])

    def AddPoint(self, point):
        assert len(point) == 3, "CSPrimPoint:SetCoord: length of array needs to be 3"
        self._ptr().primitive_add_point(self._index, [float(v) for v in point])

    def SetPoints(self, x, y, z):
        assert len(x) == len(y) == len(z), "SetPoints: each component must be of equal length"
        assert len(x) > 0, "SetPoints: empty list!"
        self._ptr().primitive_clear_points(self._index)
        for n in range(len(x)):
            self._ptr().primitive_add_point(
                self._index, [float(x[n]), float(y[n]), float(z[n])]
            )

    def GetPoint(self, idx):
        points = self._ptr().primitive_points(self._index)
        if idx < 0:
            idx += len(points)
        if idx < 0 or idx >= len(points):
            raise Exception("GetPoint: invalid index")
        return np.array(points[idx])

    def ClearPoints(self):
        self._ptr().primitive_clear_points(self._index)

    def GetNumberOfPoints(self):
        return len(self._ptr().primitive_points(self._index))


class CSPrimWire(CSPrimCurve):
    """Wire primitive (curve with a radius)."""

    _type_name = "Wire"
    _type_id = WIRE

    def _create(self, prop):
        return prop._live().create_wire([], 0.0)

    def _apply_specific_kwargs(self, kw):
        if "radius" in kw:
            self.SetWireRadius(kw.pop("radius"))
        super()._apply_specific_kwargs(kw)

    def SetWireRadius(self, val):
        self._ptr().set_primitive_f64(self._index, "wire_radius", float(val))

    def GetWireRadius(self):
        return self._ptr().primitive_f64(self._index, "wire_radius")


class CSPrimPolyhedron(CSPrimitives):
    """Polyhedron primitive (vertices and faces)."""

    _type_name = "Polyhedron"
    _type_id = POLYHEDRON

    def _create(self, prop):
        return prop._live().create_polyhedron()

    def Reset(self):
        self._ptr().primitive_reset(self._index)

    def AddVertex(self, x, y, z):
        self._ptr().primitive_add_vertex(self._index, [float(x), float(y), float(z)])

    def GetVertex(self, idx):
        assert 0 <= idx < self.GetNumVertices(), "Error: invalid vertex index"
        return np.array(self._ptr().primitive_vertex(self._index, idx))

    def GetNumVertices(self):
        return self._ptr().primitive_vertex_count(self._index)

    def GetFaceValid(self, idx):
        return 0 <= idx < self.GetNumFaces()

    def AddFace(self, verts):
        self._ptr().primitive_add_face(self._index, [int(v) for v in verts])

    def GetFace(self, idx):
        if idx < 0 or idx >= self.GetNumFaces():
            raise Exception("Error: invalid face index")
        return np.array(self._ptr().primitive_face(self._index, idx), np.int32)

    def GetNumFaces(self):
        return self._ptr().primitive_face_count(self._index)


class CSPrimPolyhedronReader(CSPrimPolyhedron):
    """Polyhedron created by reading an STL or PLY file."""

    _type_name = "PolyhedronReader"
    _type_id = POLYHEDRONREADER

    def _create(self, prop):
        return prop._live().create_polyhedron_reader("")

    def _apply_specific_kwargs(self, kw):
        if "filename" in kw:
            self.SetFilename(kw.pop("filename"))

    def SetFilename(self, fn):
        if fn.endswith(".stl"):
            self.SetFileType(1)
        elif fn.endswith(".ply"):
            self.SetFileType(2)
        else:
            self.SetFileType(0)
        self._ptr().set_primitive_string(self._index, "filename", fn)

    def GetFilename(self):
        return self._ptr().primitive_string(self._index, "filename")

    def SetFileType(self, t):
        self._ptr().set_primitive_int(self._index, "file_type", int(t))

    def GetFileType(self):
        return self._ptr().primitive_int(self._index, "file_type")

    def ReadFile(self):
        if not self._ptr().primitive_read_file(self._index):
            raise RuntimeError(
                "Cannot read file {}. Check file name, file location, or its content.".format(
                    self.GetFilename()
                )
            )
        return True


class CSPrimMultiBox(CSPrimitives):
    """Multi-box primitive (collection of axis-aligned boxes)."""

    _type_name = "MultiBox"
    _type_id = MULTIBOX

    def _create(self, prop):
        return prop._live().create_multibox([])

    def _apply_specific_kwargs(self, kw):
        if "boxes" in kw:
            for start, stop in kw.pop("boxes"):
                self.AddBox(start, stop)

    def AddBox(self, start, stop):
        assert (
            len(start) == 3 and len(stop) == 3
        ), "CSPrimMultiBox.AddBox: start and stop must have length 3"
        self._ptr().primitive_multibox_add(
            self._index, [float(v) for v in start], [float(v) for v in stop]
        )
        return self

    def GetBox(self, idx):
        assert idx < self.GetQtyBoxes(), "CSPrimMultiBox.GetBox: index out of range"
        start, stop = self._ptr().primitive_multibox_box(self._index, idx)
        return np.array(start), np.array(stop)

    def GetQtyBoxes(self):
        return self._ptr().primitive_multibox_count(self._index)

    def DeleteBox(self, idx):
        self._ptr().primitive_multibox_delete(self._index, idx)

    def ClearOverlap(self):
        raise NotImplementedError("ClearOverlap is not supported by the Rust core")


_PRIMITIVE_CLASSES = {
    "Point": CSPrimPoint,
    "Box": CSPrimBox,
    "Sphere": CSPrimSphere,
    "SphericalShell": CSPrimSphericalShell,
    "Cylinder": CSPrimCylinder,
    "CylindricalShell": CSPrimCylindricalShell,
    "Polygon": CSPrimPolygon,
    "LinPoly": CSPrimLinPoly,
    "RotPoly": CSPrimRotPoly,
    "Curve": CSPrimCurve,
    "Wire": CSPrimWire,
    "Polyhedron": CSPrimPolyhedron,
    "PolyhedronReader": CSPrimPolyhedronReader,
    "MultiBox": CSPrimMultiBox,
}


def primitive_class(type_name):
    return _PRIMITIVE_CLASSES.get(type_name, CSPrimitives)


__all__ = [
    "PrimitiveType",
    "CSPrimitives",
    "CSPrimPoint",
    "CSPrimBox",
    "CSPrimCylinder",
    "CSPrimCylindricalShell",
    "CSPrimSphere",
    "CSPrimSphericalShell",
    "CSPrimPolygon",
    "CSPrimLinPoly",
    "CSPrimRotPoly",
    "CSPrimCurve",
    "CSPrimWire",
    "CSPrimPolyhedron",
    "CSPrimPolyhedronReader",
    "CSPrimMultiBox",
    "primitive_class",
    "POINT",
    "BOX",
    "MULTIBOX",
    "SPHERE",
    "SPHERICALSHELL",
    "CYLINDER",
    "CYLINDRICALSHELL",
    "POLYGON",
    "LINPOLY",
    "ROTPOLY",
    "POLYHEDRON",
    "CURVE",
    "WIRE",
    "USERDEFINED",
    "POLYHEDRONREADER",
]
