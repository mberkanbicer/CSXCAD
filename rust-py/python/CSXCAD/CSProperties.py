# -*- coding: utf-8 -*-
"""CSXCAD property classes.

This module is the Python surface of the native ``CSXCAD.CSProperties`` API. The
data model and every accessor are implemented by the Rust extension
(``csxcad_py``); this layer provides the class names, the argument handling and
the numpy return shapes of the original Cython wrappers.
"""

import weakref

import numpy as np

from csxcad_py import PyProperty as _RustProperty
from CSXCAD.CSTransform import CSTransform
from CSXCAD.Utilities import CheckNyDir


# ---------------------------------------------------------------------------
# native type constants
class PropertyType:
    """Native ``CSProperties::PropertyType`` bit values."""

    ANY = 0xFFFF
    UNKNOWN = 0x001
    MATERIAL = 0x002
    METAL = 0x004
    EXCITATION = 0x008
    PROBEBOX = 0x010
    RESBOX = 0x020
    DUMPBOX = 0x040
    DISPERSIVEMATERIAL = 0x100
    LORENTZMATERIAL = 0x200
    DEBYEMATERIAL = 0x400
    DISCRETE_MATERIAL = 0x1000
    LUMPED_ELEMENT = 0x2000
    CONDUCTINGSHEET = 0x4000
    ABSORBING_BC = 0x8000


class ABCtype:
    """Native ``CSPropAbsorbingBC::ABCtype``."""

    UNDEFINED = 0
    MUR_1ST = 1
    MUR_1ST_SA = 2


LE_PARALLEL = 0
LE_SERIES = 1

ANY = PropertyType.ANY
UNKNOWN = PropertyType.UNKNOWN
MATERIAL = PropertyType.MATERIAL
METAL = PropertyType.METAL
EXCITATION = PropertyType.EXCITATION
PROBEBOX = PropertyType.PROBEBOX
RESBOX = PropertyType.RESBOX
DUMPBOX = PropertyType.DUMPBOX
DISPERSIVEMATERIAL = PropertyType.DISPERSIVEMATERIAL
LORENTZMATERIAL = PropertyType.LORENTZMATERIAL
DEBYEMATERIAL = PropertyType.DEBYEMATERIAL
DISCRETE_MATERIAL = PropertyType.DISCRETE_MATERIAL
LUMPED_ELEMENT = PropertyType.LUMPED_ELEMENT
CONDUCTINGSHEET = PropertyType.CONDUCTINGSHEET
ABSORBING_BC = PropertyType.ABSORBING_BC


def hex2color(color):
    if not type(color) is str:
        return color
    import matplotlib

    return tuple([int(x * 255) for x in matplotlib.colors.to_rgb(color)])


class CSProperties:
    """Virtual base class for all properties, cannot be created directly."""

    _instances = weakref.WeakValueDictionary()
    _type_name = None

    # -- construction ------------------------------------------------------
    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            self._py = None
            self._csx = None
            self._pid = None
            self._pset = pset
            self._generation = None
            self._fallback_id = None
            self._transform = None
            return

        if type(self) is CSProperties:
            raise Exception("Error, cannot create CSProperties (protected)")

        self._py = _RustProperty(self._type_name)
        self._py._set_class_name(type(self).__name__)
        self._csx = None
        self._pid = None
        self._pset = pset
        self._generation = None
        self._fallback_id = None
        self._transform = None
        self._register()

        alpha = 255
        if "alpha" in kw:
            alpha = kw.pop("alpha")
        if "color" in kw:
            self.SetColor(kw.pop("color"), alpha)
        if "name" in kw:
            self.SetName(kw.pop("name"))
        if kw:
            raise Exception("Unknown keywords: {}".format(kw))

    @classmethod
    def _from_rust(cls, rust_property):
        """Wrap an already created Rust property (copy/read paths)."""
        obj = cls.__new__(cls)
        obj._py = rust_property
        obj._py._set_class_name(cls.__name__)
        obj._csx = None
        obj._pid = None
        obj._pset = None
        obj._generation = None
        obj._fallback_id = None
        obj._transform = None
        obj._register()
        return obj

    # -- wrapper bookkeeping ----------------------------------------------
    def _register(self):
        if self._py is not None:
            CSProperties._instances[self._key()] = self

    def _key(self):
        if self._csx is None:
            return ("local", id(self._py))
        return (id(self._csx), self._pid)

    def _rekey(self):
        if self._py is not None:
            CSProperties._instances[self._key()] = self

    def _attach(self, csx, pid):
        self._csx = csx
        self._pid = pid
        self._generation = csx._generation
        self._pset = csx.GetParameterSet()
        self._rekey()

    def _detach(self, rust_property):
        self._py = rust_property
        self._py._set_class_name(type(self).__name__)
        self._csx = None
        self._pid = None
        self._generation = None
        self._rekey()

    def _mark_deleted(self):
        CSProperties._instances.pop(self._key(), None)
        self._py = None
        self._csx = None
        self._pid = None
        self._generation = None

    def _raise_deleted(self):
        raise RuntimeError(
            "wrapped C++ object of type {} has been deleted".format(
                type(self).__name__
            )
        )

    def _live(self):
        if self._py is None:
            self._raise_deleted()
        if self._csx is not None and self._csx._generation != self._generation:
            self._raise_deleted()
        return self._py

    # -- factory (native fromType/fromTypeName) ---------------------------
    @staticmethod
    def fromType(p_type, pset, no_init=False, **kw):
        if no_init and len(kw) != 0:
            raise Exception("Unable to not init a property and set kw options!")
        for cls in (
            CSPropConductingSheet,
            CSPropMetal,
            CSPropMaterial,
            CSPropLumpedElement,
            CSPropExcitation,
            CSPropProbeBox,
            CSPropDumpBox,
            CSPropLorentzMaterial,
            CSPropDebyeMaterial,
            CSPropAbsorbingBC,
            CSPropDiscMaterial,
        ):
            if p_type in cls._type_masks:
                if no_init:
                    return cls(pset, no_init=True)
                return cls(pset, **kw)
        return None

    @staticmethod
    def fromTypeName(type_str, pset, no_init=False, **kw):
        if no_init and len(kw) != 0:
            raise Exception("Unable to not init and set property options!")
        cls = _CLASS_BY_TYPE_NAME.get(type_str)
        if cls is None:
            return None
        if no_init:
            return cls(pset, no_init=True)
        return cls(pset, **kw)

    # -- general accessors -------------------------------------------------
    def GetCopy(self, incl_prim=False):
        return type(self)._from_rust(self._live().copy(incl_prim))

    copy = GetCopy

    def GetQtyPrimitives(self):
        return self._live().primitive_count()

    def GetPrimitive(self, index):
        count = self.GetQtyPrimitives()
        if index < 0:
            index = index % count if count else index
        if index < 0 or index >= count:
            # native fromPtr(NULL) returns None
            return None
        return _primitives().CSPrimitives._wrap(self, index)

    def GetAllPrimitives(self):
        return [self.GetPrimitive(n) for n in range(self.GetQtyPrimitives())]

    def GetType(self):
        return self._live().type_id()

    def GetTypeString(self):
        return self._live().type_string()

    def GetParameterSet(self):
        return self._pset

    def GetMaterial(self):
        return self._live().is_material()

    def GetID(self):
        live = self._live()
        pid = live._id()
        if pid is not None:
            return pid
        if self._fallback_id is None:
            self._fallback_id = _next_property_id()
        return self._fallback_id

    def SetName(self, name):
        self._live().set_name(str(name))

    def GetName(self):
        return self._live().name()

    def SetColor(self, color, alpha=255):
        self.SetFillColor(color, alpha)
        self.SetEdgeColor(color, alpha)

    def SetFillColor(self, color, alpha=255):
        rgb = hex2color(color)
        self._live().set_fill_color(int(rgb[0]), int(rgb[1]), int(rgb[2]), int(alpha))

    def GetFillColor(self):
        color = self._live().fill_color()
        return tuple(color) if color is not None else None

    def SetEdgeColor(self, color, alpha=255):
        rgb = hex2color(color)
        self._live().set_edge_color(int(rgb[0]), int(rgb[1]), int(rgb[2]), int(alpha))

    def GetEdgeColor(self):
        color = self._live().edge_color()
        return tuple(color) if color is not None else None

    def GetVisibility(self):
        return self._live().visible()

    def SetVisibility(self, val):
        self._live().set_visible(bool(val))

    def SetCoordInputType(self, ctype):
        self._live().set_coord_input_type(int(ctype))

    def GetCoordInputType(self):
        return self._live().coord_input_type()

    # -- attributes --------------------------------------------------------
    def ExistAttribute(self, name):
        return self._live().has_attribute(str(name))

    def GetAttributeValue(self, name):
        live = self._live()
        if not live.has_attribute(str(name)):
            return ""
        return live.attribute(str(name))

    def SetAttributeValue(self, name, val):
        self._live().set_attribute(str(name), str(val))

    AddAttribute = SetAttributeValue

    def RemoveAttribute(self, name):
        self._live().remove_attribute(str(name))

    def GetAttributeNames(self):
        return tuple(self._live().attribute_names())

    def GetAttributes(self):
        return {name: self.GetAttributeValue(name) for name in self.GetAttributeNames()}

    # -- primitive factories ----------------------------------------------
    def AddPoint(self, coord, **kw):
        return self._create_primitive("Point", coord=coord, **kw)

    def AddBox(self, start, stop, **kw):
        return self._create_primitive("Box", start=start, stop=stop, **kw)

    def AddCylinder(self, start, stop, radius, **kw):
        return self._create_primitive(
            "Cylinder", start=start, stop=stop, radius=radius, **kw
        )

    def AddCylindricalShell(self, start, stop, radius, shell_width, **kw):
        return self._create_primitive(
            "CylindricalShell",
            start=start,
            stop=stop,
            radius=radius,
            shell_width=shell_width,
            **kw
        )

    def AddSphere(self, center, radius, **kw):
        return self._create_primitive("Sphere", center=center, radius=radius, **kw)

    def AddSphericalShell(self, center, radius, shell_width, **kw):
        return self._create_primitive(
            "SphericalShell",
            center=center,
            radius=radius,
            shell_width=shell_width,
            **kw
        )

    def AddPolygon(self, points, norm_dir, elevation, **kw):
        return self._create_primitive(
            "Polygon", points=points, norm_dir=norm_dir, elevation=elevation, **kw
        )

    def AddLinPoly(self, points, norm_dir, elevation, length, **kw):
        return self._create_primitive(
            "LinPoly",
            points=points,
            norm_dir=norm_dir,
            elevation=elevation,
            length=length,
            **kw
        )

    def AddRotPoly(self, points, norm_dir, elevation, rot_axis, angle, **kw):
        return self._create_primitive(
            "RotPoly",
            points=points,
            norm_dir=norm_dir,
            elevation=elevation,
            rot_axis=rot_axis,
            angle=angle,
            **kw
        )

    def AddCurve(self, points, **kw):
        return self._create_primitive("Curve", points=points, **kw)

    def AddWire(self, points, radius, **kw):
        return self._create_primitive("Wire", points=points, radius=radius, **kw)

    def AddPolyhedron(self, **kw):
        return self._create_primitive("Polyhedron", **kw)

    def AddPolyhedronReader(self, filename, **kw):
        return self._create_primitive("PolyhedronReader", filename=filename, **kw)

    def AddMultiBox(self, boxes=None, **kw):
        if boxes is not None:
            kw["boxes"] = boxes
        return self._create_primitive("MultiBox", **kw)

    def _create_primitive(self, prim_type, **kw):
        primitives = _primitives()
        prim = primitives.CSPrimitives.fromType(
            primitives._PRIMITIVE_TYPES[prim_type], self._pset, self, **kw
        )
        if prim is None:
            raise Exception("CreatePrimitive: Unknown primitive type requested!")
        return prim

    def __repr__(self):
        return "<{} '{}'>".format(type(self).__name__, self.GetName())


_next_property_id_counter = [0]


def _next_property_id():
    _next_property_id_counter[0] += 1
    return _next_property_id_counter[0]


def _primitives():
    import CSXCAD.CSPrimitives as primitives

    return primitives


# ---------------------------------------------------------------------------
class CSPropMaterial(CSProperties):
    """General material property (epsilon, mue, kappa, sigma, density)."""

    _type_name = "Material"
    _type_masks = (MATERIAL,)

    _MATERIAL_FIELDS = ("epsilon", "mue", "kappa", "sigma")

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        material = {}
        for key in list(kw):
            if key in ("epsilon", "mue", "kappa", "sigma", "density"):
                material[key] = kw.pop(key)
        super().__init__(pset, *args, **kw)
        if material:
            self.SetMaterialProperty(**material)

    def SetIsotropy(self, val):
        self._live().set_bool_field("isotropy", bool(val))

    def GetIsotropy(self):
        return self._live().bool_field("isotropy")

    def SetMaterialProperty(self, **kw):
        for prop_name, val in kw.items():
            if prop_name == "density":
                self._live().set_f64_field("density", 0, float(val))
                continue
            if prop_name not in self._MATERIAL_FIELDS:
                raise Exception(
                    "SetMaterialPropertyDir: Error, unknown material property"
                )
            if np.ndim(val) == 0:
                self._live().set_f64_field(prop_name, 0, float(val))
                continue
            assert (
                len(val) == 3
            ), 'SetMaterialProperty: "{}" must be a list or array of length 3'.format(
                prop_name
            )
            for n in range(3):
                self._live().set_f64_field(prop_name, n, float(val[n]))

    def SetMaterialWeight(self, **kw):
        for prop_name, val in kw.items():
            if prop_name == "density":
                self._live().set_string_field("density_weight", 0, str(val))
                continue
            if prop_name not in self._MATERIAL_FIELDS:
                raise Exception("SetMaterialWeightDir: Error, unknown material property")
            if isinstance(val, str):
                self._live().set_string_field(prop_name + "_weight", 0, val)
                continue
            assert (
                len(val) == 3
            ), 'SetMaterialWeight: "{}" must be a list or array of length 3'.format(
                prop_name
            )
            for n in range(3):
                self._live().set_string_field(prop_name + "_weight", n, str(val[n]))

    def GetMaterialProperty(self, prop_name):
        if prop_name == "density":
            return self._live().f64_field("density", 0)
        if prop_name not in self._MATERIAL_FIELDS:
            raise Exception("GetMaterialPropertyDir: Error, unknown material property")
        if self.GetIsotropy():
            return self._live().f64_field(prop_name, 0)
        return np.array([self._live().f64_field(prop_name, n) for n in range(3)])

    def GetMaterialWeight(self, prop_name):
        if prop_name == "density":
            return self._live().string_field("density_weight", 0)
        if prop_name not in self._MATERIAL_FIELDS:
            raise Exception("GetMaterialWeightDir: Error, unknown material property")
        if self.GetIsotropy():
            return self._live().string_field(prop_name + "_weight", 0)
        return [
            self._live().string_field(prop_name + "_weight", n) for n in range(3)
        ]


class CSPropAbsorbingBC(CSProperties):
    """Local absorbing boundary condition property."""

    _type_name = "AbsorbingBC"
    _type_masks = (ABSORBING_BC,)

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        options = {}
        for key in ("NormalSignPositive", "PhaseVelocity", "AbsorbingBoundaryType"):
            if key in kw:
                options[key] = kw.pop(key)
        super().__init__(pset, *args, **kw)
        if "NormalSignPositive" in options:
            self.SetNormalSignPositive(options["NormalSignPositive"])
        if "PhaseVelocity" in options:
            self.SetPhaseVelocity(options["PhaseVelocity"])
        if "AbsorbingBoundaryType" in options:
            self.SetAbsorbingBoundaryType(options["AbsorbingBoundaryType"])

    def SetNormalSignPositive(self, val):
        self._live().set_bool_field("normal_sign_positive", bool(val))

    def GetNormalSignPositive(self):
        return self._live().bool_field("normal_sign_positive")

    def SetPhaseVelocity(self, val):
        self._live().set_f64_field("phase_velocity", 0, float(val))

    def GetPhaseVelocity(self):
        return self._live().f64_field("phase_velocity", 0)

    def SetAbsorbingBoundaryType(self, val):
        self._live().set_int_field("boundary_type", 0, int(val))

    def GetAbsorbingBoundaryType(self):
        return self._live().int_field("boundary_type", 0)


class CSPropLumpedElement(CSProperties):
    """Lumped element property (R, C, L in a given direction)."""

    _type_name = "LumpedElement"
    _type_masks = (LUMPED_ELEMENT,)

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        element_type_found = "LEtype" in kw
        resistance = kw.pop("R", None)
        inductance = kw.pop("L", None)
        capacitance = kw.pop("C", None)
        direction = kw.pop("ny", None)
        caps = kw.pop("caps", None)
        element_type = kw.pop("LEtype", None)
        super().__init__(pset, *args, **kw)
        if resistance is not None:
            self.SetResistance(resistance)
        if inductance is not None:
            self.SetInductance(inductance)
        if capacitance is not None:
            self.SetCapacity(capacitance)
        if direction is not None:
            self.SetDirection(direction)
        if caps is not None:
            self.SetCaps(caps)
        self.SetLEtype(element_type if element_type_found else LE_PARALLEL)

    def SetResistance(self, val):
        self._live().set_f64_field("resistance", 0, float(val))

    def GetResistance(self):
        return self._live().f64_field("resistance", 0)

    def SetCapacity(self, val):
        self._live().set_f64_field("capacitance", 0, float(val))

    def GetCapacity(self):
        return self._live().f64_field("capacitance", 0)

    def SetInductance(self, val):
        self._live().set_f64_field("inductance", 0, float(val))

    def GetInductance(self):
        return self._live().f64_field("inductance", 0)

    def SetDirection(self, ny):
        self._live().set_int_field("direction", 0, CheckNyDir(ny))

    def GetDirection(self):
        return self._live().int_field("direction", 0)

    def SetCaps(self, val):
        self._live().set_bool_field("caps", bool(val))

    def GetCaps(self):
        return self._live().bool_field("caps")

    def SetLEtype(self, val):
        self._live().set_int_field("element_type", 0, int(val))

    def GetLEtype(self):
        return self._live().int_field("element_type", 0)


class CSPropMetal(CSProperties):
    """Metal property (perfect electric conductor)."""

    _type_name = "Metal"
    _type_masks = (METAL,)

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        super().__init__(pset, *args, **kw)


class CSPropConductingSheet(CSPropMetal):
    """Conducting sheet property (finite conductivity and thickness)."""

    _type_name = "ConductingSheet"
    _type_masks = (CONDUCTINGSHEET,)

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        conductivity = kw.pop("conductivity", None)
        thickness = kw.pop("thickness", None)
        super().__init__(pset, *args, **kw)
        if conductivity is not None:
            self.SetConductivity(conductivity)
        if thickness is not None:
            self.SetThickness(thickness)

    def SetConductivity(self, val):
        self._live().set_f64_field("conductivity", 0, float(val))

    def GetConductivity(self):
        return self._live().f64_field("conductivity", 0)

    def SetThickness(self, val):
        self._live().set_f64_field("thickness", 0, float(val))

    def GetThickness(self):
        return self._live().f64_field("thickness", 0)


class CSPropExcitation(CSProperties):
    """Excitation property (field sources)."""

    _type_name = "Excitation"
    _type_masks = (EXCITATION,)

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        excit_type = kw.pop("exc_type", None)
        excit_val = kw.pop("exc_val", None)
        delay = kw.pop("delay", None)
        weight_file = kw.pop("weight_file", None)
        super().__init__(pset, *args, **kw)
        if excit_type is not None:
            self.SetExcitType(excit_type)
        if excit_val is not None:
            self.SetExcitation(excit_val)
        if delay is not None:
            self.SetDelay(delay)
        if weight_file is not None:
            self.SetWeightFile(weight_file)

    def SetExcitType(self, val):
        self._live().set_int_field("excitation_type", 0, int(val))

    def GetExcitType(self):
        return self._live().int_field("excitation_type", 0)

    def SetEnabled(self, val):
        self._live().set_bool_field("enabled", bool(val))

    def GetEnabled(self):
        return self._live().bool_field("enabled")

    def SetExcitation(self, val):
        assert len(val) == 3, "Error, excitation vector must be of dimension 3"
        for n in range(3):
            self._live().set_f64_field("excitation", n, float(val[n]))

    def GetExcitation(self):
        return np.array([self._live().f64_field("excitation", n) for n in range(3)])

    def SetPropagationDir(self, val):
        assert len(val) == 3, "Error, excitation vector must be of dimension 3"
        for n in range(3):
            self._live().set_f64_field("propagation_direction", n, float(val[n]))

    def GetPropagationDir(self):
        return np.array(
            [self._live().f64_field("propagation_direction", n) for n in range(3)]
        )

    def SetWeightFunction(self, func):
        assert (
            len(func) == 3
        ), "Error, excitation weighting function must be an array of length 3"
        for n in range(3):
            assert (
                type(func[n]) is str
            ), "Error, excitation weighting function must be a string"
            self._live().set_string_field("weight_function", n, func[n])

    def GetWeightFunction(self):
        return [self._live().string_field("weight_function", n) for n in range(3)]

    def SetWeightFile(self, fileName):
        assert type(fileName) is str, '"fileName" must be a str'
        self._live().set_string_field("weight_file", 0, fileName)

    def GetWeightFile(self):
        return self._live().string_field("weight_file", 0)

    def SetWeightOrigin(self, origin):
        assert len(origin) == 3, "origin must have 3 elements"
        for n in range(3):
            self._live().set_f64_field("weight_origin", n, float(origin[n]))

    def GetWeightOrigin(self):
        return [self._live().f64_field("weight_origin", n) for n in range(3)]

    def GetWeightedExcitation(self, ny, coords):
        return self._live().weighted_excitation(int(ny), [float(v) for v in coords])

    def SetFrequency(self, val):
        self._live().set_f64_field("frequency", 0, float(val))

    def GetFrequency(self):
        return self._live().f64_field("frequency", 0)

    def SetDelay(self, val):
        self._live().set_f64_field("delay", 0, float(val))

    def GetDelay(self):
        return self._live().f64_field("delay", 0)


class CSPropProbeBox(CSProperties):
    """Probe property."""

    _type_name = "ProbeBox"
    _type_masks = (PROBEBOX,)

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        probe_type = kw.pop("p_type", None)
        weight = kw.pop("weight", None)
        over_sampling = kw.pop("over_sampling", None)
        normal_dir = kw.pop("norm_dir", None)
        frequency = kw.pop("frequency", None)
        mode_function = kw.pop("mode_function", None)
        super().__init__(pset, *args, **kw)
        if probe_type is not None:
            self.SetProbeType(probe_type)
        if weight is not None:
            self.SetWeighting(weight)
        if over_sampling is not None:
            self.SetOverSampling(over_sampling)
        if normal_dir is not None:
            self.SetNormalDir(normal_dir)
        if frequency is not None:
            self.SetFrequency(frequency)
        if mode_function is not None:
            self.SetModeFunction(mode_function)

    def SetProbeType(self, val):
        self._live().set_int_field("probe_type", 0, int(val))

    def GetProbeType(self):
        return self._live().int_field("probe_type", 0)

    def SetWeighting(self, val):
        self._live().set_f64_field("weight", 0, float(val))

    def GetWeighting(self):
        return self._live().f64_field("weight", 0)

    def SetOverSampling(self, val):
        self._live().set_int_field("oversampling", 0, int(val))

    def GetOverSampling(self):
        return self._live().int_field("oversampling", 0)

    def SetNormalDir(self, val):
        self._live().set_int_field("normal_direction", 0, int(val))

    def GetNormalDir(self):
        return self._live().int_field("normal_direction", 0)

    def AddFrequency(self, freq):
        if np.isscalar(freq):
            self._live().add_frequency(float(freq))
        else:
            for value in freq:
                self._live().add_frequency(float(value))

    def ClearFrequency(self):
        self._live().set_frequencies([])

    def SetFrequency(self, freq):
        self.ClearFrequency()
        self.AddFrequency(freq)

    def GetFrequency(self):
        return np.array(self._live().frequencies())

    def GetFrequencyCount(self):
        return len(self._live().frequencies())

    def SetModeFunction(self, mode_fun):
        assert len(mode_fun) == 3, "SetModeFunction: mode_fun must be list of length 3"
        for n in range(3):
            self._live().set_string_field("mode_function", n, str(mode_fun[n]))

    def GetModeFunction(self):
        return [self._live().string_field("mode_function", n) for n in range(3)]

    def SetModeFile(self, fileName):
        assert type(fileName) is str, '"fileName" must be a str'
        self._live().set_string_field("mode_file", 0, fileName)

    def GetModeFile(self):
        return self._live().string_field("mode_file", 0)

    def SetModeOrigin(self, origin):
        assert len(origin) == 3, "origin must have 3 elements"
        for n in range(3):
            self._live().set_f64_field("mode_origin", n, float(origin[n]))

    def GetModeOrigin(self):
        return [self._live().f64_field("mode_origin", n) for n in range(3)]


class CSPropDumpBox(CSPropProbeBox):
    """Field dump property."""

    _type_name = "DumpBox"
    _type_masks = (DUMPBOX,)

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        dump_type = kw.pop("dump_type", None)
        dump_mode = kw.pop("dump_mode", None)
        file_type = kw.pop("file_type", None)
        opt_resolution = kw.pop("opt_resolution", None)
        sub_sampling = kw.pop("sub_sampling", None)
        super().__init__(pset, *args, **kw)
        if dump_type is not None:
            self.SetDumpType(dump_type)
        if dump_mode is not None:
            self.SetDumpMode(dump_mode)
        if file_type is not None:
            self.SetFileType(file_type)
        if opt_resolution is not None:
            self.SetOptResolution(opt_resolution)
        if sub_sampling is not None:
            self.SetSubSampling(sub_sampling)

    def SetDumpType(self, val):
        self._live().set_int_field("dump_type", 0, int(val))

    def GetDumpType(self):
        return self._live().int_field("dump_type", 0)

    def SetDumpMode(self, val):
        self._live().set_int_field("dump_mode", 0, int(val))

    def GetDumpMode(self):
        return self._live().int_field("dump_mode", 0)

    def SetFileType(self, val):
        self._live().set_int_field("file_type", 0, int(val))

    def GetFileType(self):
        return self._live().int_field("file_type", 0)

    def SetOptResolution(self, val):
        assert (
            len(val) == 3
        ), "SetOptResolution: value must be list or array of length 3"
        for n in range(3):
            self._live().set_f64_field("optimal_resolution", n, float(val[n]))

    def GetOptResolution(self):
        return np.array(
            [self._live().f64_field("optimal_resolution", n) for n in range(3)]
        )

    def SetSubSampling(self, val):
        assert (
            len(val) == 3
        ), "SetSubSampling: 'val' must be a list or array of length 3"
        for n in range(3):
            self._live().set_f64_field("sub_sampling", n, int(val[n]))

    def GetSubSampling(self):
        return np.array([self._live().f64_field("sub_sampling", n) for n in range(3)])


class CSPropDispersiveMaterial(CSPropMaterial):
    """Base class for dispersive materials."""

    _type_name = None

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        order = kw.pop("order", None)
        super().__init__(pset, *args, **kw)
        if order is not None:
            self.SetDispersionOrder(order)

    def GetDispersionOrder(self):
        return self._live().dispersive_order()

    def SetDispersionOrder(self, val):
        self._live().set_dispersive_order(int(val))

    def _CheckOrder(self, order):
        if order < 0 or order >= self._live().dispersive_order():
            raise IndexError("Invalid dispersive media order requested")

    def GetDispersiveMaterialProperty(self, prop_name, order):
        self._CheckOrder(order)
        if self.GetIsotropy():
            return self._live().dispersive_field(prop_name, order, 0)
        return np.array(
            [self._live().dispersive_field(prop_name, order, n) for n in range(3)]
        )

    def SetDispersiveMaterialProperty(self, order, **kw):
        self._CheckOrder(order)
        for prop_name, val in kw.items():
            if np.ndim(val) == 0:
                self._live().set_dispersive_field(prop_name, order, 0, float(val))
                continue
            assert (
                len(val) == 3
            ), 'SetDispersiveMaterialProperty: "{}" must be a list or array of length 3'.format(
                prop_name
            )
            for n in range(3):
                self._live().set_dispersive_field(prop_name, order, n, float(val[n]))

    def GetDispersiveMaterialWeight(self, prop_name, order):
        self._CheckOrder(order)
        if self.GetIsotropy():
            return self._live().dispersive_weight(prop_name, order, 0)
        return [
            self._live().dispersive_weight(prop_name, order, n) for n in range(3)
        ]

    def SetDispersiveMaterialWeight(self, order, **kw):
        self._CheckOrder(order)
        for prop_name, val in kw.items():
            if isinstance(val, str):
                self._live().set_dispersive_weight(prop_name, order, 0, val)
                continue
            assert (
                len(val) == 3
            ), 'SetDispersiveMaterialWeight: "{}" must be a list or array of length 3'.format(
                prop_name
            )
            for n in range(3):
                self._live().set_dispersive_weight(prop_name, order, n, str(val[n]))


class CSPropLorentzMaterial(CSPropDispersiveMaterial):
    """Lorentz dispersive material."""

    _type_name = "LorentzMaterial"
    _type_masks = (LORENTZMATERIAL + DISPERSIVEMATERIAL + MATERIAL,)

    _FIELDS = (
        "eps_plasma",
        "eps_pole_freq",
        "eps_relax",
        "mue_plasma",
        "mue_pole_freq",
        "mue_relax",
    )


class CSPropDebyeMaterial(CSPropDispersiveMaterial):
    """Debye dispersive material."""

    _type_name = "DebyeMaterial"
    _type_masks = (DEBYEMATERIAL + DISPERSIVEMATERIAL + MATERIAL,)

    _FIELDS = ("eps_delta", "eps_relax")


class _LiveDiscreteTransform:
    """Write transform mutations back to the owning property.

    The Rust binding returns a value copy of the transform. Native
    ``GetTransform()`` is live, so the façade forwards every call and then
    stores the updated transform on the property.
    """

    def __init__(self, owner, transform):
        object.__setattr__(self, "_owner", owner)
        object.__setattr__(self, "_transform", transform)

    def __getattr__(self, name):
        target = getattr(object.__getattribute__(self, "_transform"), name)
        if not callable(target):
            return target

        def forward(*args, **kwargs):
            result = target(*args, **kwargs)
            object.__getattribute__(self, "_owner").SetTransform(
                object.__getattribute__(self, "_transform")
            )
            return result

        return forward


class CSPropDiscMaterial(CSPropMaterial):
    """Discrete material property loaded from an HDF5 file."""

    _type_name = "DiscMaterial"
    _type_masks = (DISCRETE_MATERIAL + MATERIAL,)

    def __init__(self, pset=None, *args, no_init=False, **kw):
        if no_init:
            super().__init__(pset, no_init=True)
            return
        has_filename = "filename" in kw
        has_filetype = "filetype" in kw
        filename = kw.pop("filename", None)
        filetype = kw.pop("filetype", None)
        scale = kw.pop("scale", None)
        use_db_background = kw.pop("use_db_background", None)
        super().__init__(pset, *args, **kw)
        if has_filename:
            self.SetFilename(filename)
        if has_filetype:
            self.SetFileType(filetype)
        elif has_filename:
            self.SetFileType(0)
        if scale is not None:
            self.SetScale(scale)
        if use_db_background is not None:
            self.SetUseDataBaseForBackground(use_db_background)

    def SetFilename(self, fn):
        self._live().set_string_field("filename", 0, str(fn))

    def GetFilename(self):
        return self._live().string_field("filename", 0)

    def SetFileType(self, ftype):
        self._live().set_int_field("file_type", 0, int(ftype))

    def GetFileType(self):
        return self._live().int_field("file_type", 0)

    def GetPolyDataModel(self, transform=True):
        return self._live().poly_data_model(bool(transform))

    def SetScale(self, val):
        self._live().set_f64_field("scale", 0, float(val))

    def GetScale(self):
        return self._live().f64_field("scale", 0)

    def SetUseDataBaseForBackground(self, val):
        self._live().set_bool_field("use_database_for_background", bool(val))

    def GetUseDataBaseForBackground(self):
        return self._live().bool_field("use_database_for_background")

    def GetTransform(self):
        if self._transform is None:
            self._transform = _LiveDiscreteTransform(
                self, self._live().discrete_transform()
            )
        return self._transform

    def SetTransform(self, transform):
        if transform is None:
            self._live().set_transform(None)
            self._transform = None
            return
        if transform is self._transform:
            return
        self._live().set_transform(transform)
        self._transform = transform

    def ReadFile(self):
        return self._live().read_file()

    def GetEpsilonWeighted(self, ny, coords):
        return self._live().weighted_discrete("epsilon", ny, list(coords))

    def GetMueWeighted(self, ny, coords):
        return self._live().weighted_discrete("mue", ny, list(coords))

    def GetKappaWeighted(self, ny, coords):
        return self._live().weighted_discrete("kappa", ny, list(coords))

    def GetSigmaWeighted(self, ny, coords):
        return self._live().weighted_discrete("sigma", ny, list(coords))

    def GetDensityWeighted(self, coords):
        return self._live().weighted_discrete("density", 0, list(coords))


_CLASS_BY_TYPE_NAME = {
    "Material": CSPropMaterial,
    "Metal": CSPropMetal,
    "ConductingSheet": CSPropConductingSheet,
    "LumpedElement": CSPropLumpedElement,
    "Excitation": CSPropExcitation,
    "ProbeBox": CSPropProbeBox,
    "DumpBox": CSPropDumpBox,
    "LorentzMaterial": CSPropLorentzMaterial,
    "DebyeMaterial": CSPropDebyeMaterial,
    "AbsorbingBC": CSPropAbsorbingBC,
    "DiscMaterial": CSPropDiscMaterial,
    "Discrete-Material": CSPropDiscMaterial,
}


def property_class(type_string):
    """Native class for a property type string (as returned by ``GetTypeString``)."""
    return _CLASS_BY_TYPE_NAME.get(type_string, CSProperties)


__all__ = [
    "PropertyType",
    "ABCtype",
    "LE_PARALLEL",
    "LE_SERIES",
    "hex2color",
    "CSProperties",
    "CSPropMaterial",
    "CSPropAbsorbingBC",
    "CSPropLumpedElement",
    "CSPropMetal",
    "CSPropConductingSheet",
    "CSPropExcitation",
    "CSPropProbeBox",
    "CSPropDumpBox",
    "CSPropDispersiveMaterial",
    "CSPropLorentzMaterial",
    "CSPropDebyeMaterial",
    "CSPropDiscMaterial",
    "property_class",
] + [name for name in list(globals()) if name.isupper()]
