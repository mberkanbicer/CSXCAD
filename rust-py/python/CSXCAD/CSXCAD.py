# -*- coding: utf-8 -*-
"""CSXCAD structure container.

The native class is :class:`ContinuousStructure`; this module keeps the native
name and behaviour on top of the Rust ``PyStructure`` core.
"""

import weakref
from pathlib import Path

import numpy as np

from csxcad_py import PyStructure as _RustStructure

from CSXCAD import CSProperties
from CSXCAD import CSPrimitives
from CSXCAD import CSRectGrid as _grid_module
from CSXCAD.CSRectGrid import CSRectGrid
from CSXCAD.SmoothMeshLines import SmoothMeshLines


class ParameterSet:
    """Parameter set of a structure (native ``ParameterObjects.ParameterSet``).

    The parameters themselves live in the Rust structure; this object exists so
    that properties can be created from the set of the structure they belong to.
    """

    def __init__(self, csx=None):
        self._csx = csx

    def __repr__(self):
        return "<ParameterSet of {}>".format(self._csx)


class CSBackgroundMaterial:
    """Background material of a structure (epsilon, mue, kappa, sigma)."""

    _instances = weakref.WeakValueDictionary()

    def __init__(self, csx=None):
        if csx is None:
            raise TypeError(
                "CSBackgroundMaterial cannot be instantiated directly; use "
                "ContinuousStructure.GetBackgroundMaterial()"
            )
        self._csx = csx

    def _structure(self):
        return self._csx._live()

    def GetEpsilon(self):
        return self._structure().background("epsilon")

    def SetEpsilon(self, val):
        self._structure().set_background("epsilon", float(val))

    def GetMue(self):
        return self._structure().background("mue")

    def SetMue(self, val):
        self._structure().set_background("mue", float(val))

    def GetKappa(self):
        return self._structure().background("kappa")

    def SetKappa(self, val):
        self._structure().set_background("kappa", float(val))

    def GetSigma(self):
        return self._structure().background("sigma")

    def SetSigma(self, val):
        self._structure().set_background("sigma", float(val))

    def Reset(self):
        self._structure().reset_background()


_grid_instances = weakref.WeakValueDictionary()


class ContinuousStructure:
    """ContinuousStructure

    With this class different properties and primitives can be created and a
    rectilinear grid assigned.
    """

    _instances = weakref.WeakValueDictionary()
    AddressMode = _grid_module.CoordinateSystem

    def __init__(self, *args, no_init=False, **kw):
        if no_init:
            self._py = None
            self._generation = 0
            self._pset = None
            return

        coord_system = 0
        if "CoordSystem" in kw:
            coord_system = kw.pop("CoordSystem")
        elif "cs_type" in kw:
            coord_system = kw.pop("cs_type")
        if kw:
            raise Exception('Unknown keyword arguments: "{}"'.format(kw))

        self._py = _RustStructure(int(coord_system))
        self._generation = 0
        self._pset = ParameterSet(self)
        ContinuousStructure._instances[id(self)] = self

    # -- lifecycle ---------------------------------------------------------
    def _live(self):
        if self._py is None:
            raise RuntimeError(
                "wrapped C++ object of type ContinuousStructure has been deleted"
            )
        return self._py

    def _invalidate(self):
        """Drop every wrapper that referred to the replaced property set."""
        self._generation += 1
        for key in [key for key in CSProperties.CSProperties._instances if key[0] == id(self)]:
            wrapper = CSProperties.CSProperties._instances.pop(key, None)
            if wrapper is not None:
                wrapper._py = None
                wrapper._csx = None
                wrapper._pid = None
                wrapper._generation = None
        for key in [
            key for key in CSPrimitives.CSPrimitives._instances if key[0] == id(self)
        ]:
            CSPrimitives.CSPrimitives._instances.pop(key, None)

    def Update(self):
        return self._live().update()

    def Clear(self):
        self._live().clear()
        self._invalidate()

    Reset = Clear

    def Write2XML(self, file):
        file = Path(file)
        if not file.parent.is_dir():
            raise FileNotFoundError(
                "Directory in which file is to be saved does not exist. "
            )
        return self._live().write_xml(str(file))

    def ReadFromXML(self, fn):
        message = self._live().read_xml(str(fn))
        if message == "":
            self._invalidate()
        return message

    def GetParameterSet(self):
        return self._pset

    def GetGrid(self):
        key = id(self)
        grid = _grid_instances.get(key)
        if grid is None:
            grid = CSRectGrid._from_structure(self._live())
            _grid_instances[key] = grid
        return grid

    @property
    def grid(self):
        return self.GetGrid()

    def GetBackgroundMaterial(self):
        key = id(self)
        material = CSBackgroundMaterial._instances.get(key)
        if material is None:
            material = CSBackgroundMaterial(self)
            CSBackgroundMaterial._instances[key] = material
        return material

    def SetMeshType(self, cs_type):
        self.GetGrid().SetMeshType(cs_type)
        self._live().set_coord_input_type(int(cs_type))

    def GetCoordInputType(self):
        return self._live().coord_input_type()

    def DefineGrid(self, mesh, unit, smooth_mesh_res=None):
        grid = self.GetGrid()
        grid.Clear()
        if smooth_mesh_res is not None:
            for k in mesh:
                mesh[k] = SmoothMeshLines(mesh[k], smooth_mesh_res)
        for k in mesh:
            grid.SetLines(k, mesh[k])
        grid.SetDeltaUnit(unit)
        return grid

    # -- properties --------------------------------------------------------
    def GetQtyProperties(self):
        return self._live().property_count()

    def GetQtyPrimitives(self, prop_type=CSProperties.ANY):
        return self._live().primitive_count_by_type(int(prop_type))

    def AddMaterial(self, name, **kw):
        if not isinstance(name, str):
            raise TypeError(
                "`name` must be a str, received object of type {}. ".format(type(name))
            )
        return self.__CreateProperty("Material", name, **kw)

    def AddDiscMaterial(self, name, **kw):
        return self.__CreateProperty("DiscMaterial", name, **kw)

    def AddAbsorbingBC(self, name, **kw):
        return self.__CreateProperty("AbsorbingBC", name, **kw)

    def AddLumpedElement(self, name, **kw):
        return self.__CreateProperty("LumpedElement", name, **kw)

    def AddMetal(self, name, **kw):
        return self.__CreateProperty("Metal", name, **kw)

    def AddConductingSheet(self, name, **kw):
        return self.__CreateProperty("ConductingSheet", name, **kw)

    def AddExcitation(self, name, exc_type, exc_val, **kw):
        return self.__CreateProperty(
            "Excitation", name, exc_type=exc_type, exc_val=exc_val, **kw
        )

    def AddProbe(self, name, p_type, **kw):
        return self.__CreateProperty("ProbeBox", name, p_type=p_type, **kw)

    def AddDump(self, name, **kw):
        return self.__CreateProperty("DumpBox", name, **kw)

    def __CreateProperty(self, type_str, name, **kw):
        pset = self.GetParameterSet()
        prop = CSProperties.CSProperties.fromTypeName(type_str, pset, **kw)
        if prop is None:
            raise RuntimeError("Unknown property type: {}".format(type_str))
        prop.SetName(name)
        self.AddProperty(prop)
        return prop

    def AddProperty(self, prop):
        pid = self._live().add_property(prop._live())
        prop._attach(self, pid)

    def RemoveProperty(self, prop):
        # Native ContinuousStructure::RemoveProperty searches its own vector by
        # pointer (src/ContinuousStructure.cpp:115-126) and silently does
        # nothing when the property belongs to another structure or to nobody.
        if prop._csx is not self:
            return
        local = self._live().remove_property(prop._pid)
        prop._detach(local)

    def DeleteProperty(self, prop):
        # Native ContinuousStructure::DeleteProperty (src/ContinuousStructure.cpp:
        # 137-150) also searches by pointer: a property that is not in this
        # structure -- including one already removed, deleted or invalidated --
        # is left untouched instead of deleting some other property with a
        # colliding id.
        if prop._csx is not self:
            return
        self._live().delete_property(prop._pid)
        prop._mark_deleted()

    # -- property lookups --------------------------------------------------
    def _wrap_property(self, pid):
        key = (id(self), pid)
        wrapper = CSProperties.CSProperties._instances.get(key)
        if wrapper is not None:
            return wrapper
        type_name = self._live().property_type_name(pid)
        if type_name is None:
            return None
        cls = CSProperties.property_class(type_name)
        obj = cls.__new__(cls)
        obj._py = self._live().property_handle(pid)
        obj._py._set_class_name(cls.__name__)
        obj._csx = self
        obj._pid = pid
        obj._pset = self.GetParameterSet()
        obj._generation = self._generation
        obj._fallback_id = None
        obj._register()
        return obj

    def GetProperty(self, index):
        if index < 0 or index >= self.GetQtyProperties():
            raise IndexError("Index is out of range")
        pid = self._live().property_ids()[index]
        return self._wrap_property(pid)

    def GetAllProperties(self):
        return [self.GetProperty(n) for n in range(self.GetQtyProperties())]

    def GetPropertiesByName(self, name):
        return [self._wrap_property(pid) for pid in self._live().property_ids_by_name(name)]

    def GetPropertyByType(self, prop_type):
        return [
            self._wrap_property(pid)
            for pid in self._live().property_ids_by_type(int(prop_type))
        ]

    def GetPropertyByCoordPriority(
        self, coord, prop_type=CSProperties.ANY, markFoundAsUsed=False
    ):
        pid = self._live().property_at_coord(
            list(coord), int(prop_type), bool(markFoundAsUsed)
        )
        if pid is None:
            return None
        return self._wrap_property(pid)

    # -- primitives --------------------------------------------------------
    def GetAllPrimitives(self, sort=False, prop_type=CSProperties.ANY):
        locations = self._live().primitive_locations(bool(sort), int(prop_type))
        primitives = []
        for pid, index in locations:
            prop = self._wrap_property(pid)
            if prop is None:
                continue
            primitives.append(CSPrimitives.CSPrimitives._wrap(prop, index))
        return primitives


__all__ = ["ContinuousStructure", "CSBackgroundMaterial", "ParameterSet"]
