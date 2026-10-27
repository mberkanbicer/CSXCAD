# -*- coding: utf-8 -*-
"""RM-033: error, ownership and lifetime contracts for the Rust Python binding.

Every expectation below is derived from the native implementation and the
native Cython wrappers; the comment above each test names the native function
and file/line range that defines it. The tests assert the specific native
outcome (identity, exception type, message fragment, remaining state), not just
"does not crash".

Run it directly (``python tests/test_contracts.py``) or through ``unittest``.
"""

from __future__ import annotations

import gc
import os
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
for path in (str(HERE), str(HERE.parent / "python")):
    if path not in sys.path:
        sys.path.insert(0, path)

import numpy as np  # noqa: E402

from CSXCAD import CSProperties as CSPropertiesModule  # noqa: E402
from CSXCAD.CSXCAD import CSBackgroundMaterial, ContinuousStructure  # noqa: E402
from CSXCAD.CSProperties import CSProperties, CSPropMaterial  # noqa: E402
from CSXCAD.CSPrimitives import CSPrimitives  # noqa: E402
from CSXCAD.CSTransform import CSTransform  # noqa: E402
from CSXCAD.Utilities import CheckNyDir  # noqa: E402


class _XMLFixture(unittest.TestCase):
    def setUp(self):
        self._dir = tempfile.mkdtemp()
        self.xml = os.path.join(self._dir, "structure.xml")

    def tearDown(self):
        for name in os.listdir(self._dir):
            os.remove(os.path.join(self._dir, name))
        os.rmdir(self._dir)


class Test_StaleHandles(_XMLFixture):
    """Native invalidates wrappers through the destruction callback.

    CSObject::~CSObject calls NotifyDestruction() (CSXCAD/src/CSObject.cpp:20-35),
    the Cython callback only clears the wrapper's pointer slot
    (CSXCAD/python/CSXCAD/CSObject.pxd:50-66), and every wrapper accessor raises
    RuntimeError afterwards (CSProperties.pyx:206-215, CSPrimitives.pyx:165-170).
    """

    def _assert_deleted(self, wrapper, call):
        with self.assertRaises(RuntimeError) as raised:
            call()
        message = str(raised.exception)
        self.assertIn("wrapped C++ object of type", message)
        self.assertIn("has been deleted", message)
        self.assertIn(type(wrapper).__name__, message)

    def test_property_primitive_and_transform_are_invalid_after_delete(self):
        # ContinuousStructure::DeleteProperty deletes the property and with it
        # every primitive (src/ContinuousStructure.cpp:137-150).
        csx = ContinuousStructure()
        prop = csx.AddMaterial("mat", epsilon=2.5)
        prim = prop.AddBox([0, 0, 0], [1, 1, 1])
        transform = prim.GetTransform()
        transform.Translate([1, 0, 0])

        csx.DeleteProperty(prop)

        self.assertEqual(csx.GetQtyProperties(), 0)
        self._assert_deleted(prop, lambda: prop.GetName())
        self._assert_deleted(prop, lambda: prop.GetMaterialProperty("epsilon"))
        self._assert_deleted(prop, prop.GetCopy)
        self._assert_deleted(prim, lambda: prim.GetStart())
        self._assert_deleted(prim, prim.GetBoundBox)
        self._assert_deleted(transform, transform.GetMatrix)

    def test_properties_are_invalid_after_structure_clear(self):
        # ContinuousStructure::clear() deletes every property
        # (src/ContinuousStructure.cpp:451-467).
        csx = ContinuousStructure()
        prop = csx.AddMetal("metal")
        prop.AddBox([0, 0, 0], [1, 1, 1])

        csx.Clear()

        self.assertEqual(csx.GetQtyProperties(), 0)
        self._assert_deleted(prop, lambda: prop.GetName())

    def test_properties_are_invalid_after_read_from_xml(self):
        # ReadFromXML replaces the structure, i.e. the C++ properties the
        # wrappers point at are gone; native ReadFromXML clears and rebuilds.
        self.assertTrue(ContinuousStructure().Write2XML(self.xml))
        csx = ContinuousStructure()
        prop = csx.AddMaterial("mat", epsilon=2.5)
        prim = prop.AddBox([0, 0, 0], [1, 1, 1])
        transform = prim.GetTransform()

        self.assertEqual(csx.ReadFromXML(self.xml), "")

        self.assertEqual(csx.GetQtyProperties(), 0)
        self._assert_deleted(prop, lambda: prop.GetMaterialProperty("epsilon"))
        self._assert_deleted(prim, prim.GetStart)
        self._assert_deleted(transform, transform.GetMatrix)

    def test_structure_owned_parts_outlive_a_property_purge(self):
        # The grid, parameter set and background material are members of the
        # ContinuousStructure (src/ContinuousStructure.cpp:56-60, 74-78); only
        # clear() resets them (line 456-466), deleting a property does not.
        csx = ContinuousStructure()
        grid = csx.GetGrid()
        background = csx.GetBackgroundMaterial()
        parameters = csx.GetParameterSet()
        for index in range(3):
            csx.AddMetal("metal_%d" % index).AddBox([0, 0, 0], [1, 1, 1])
        for prop in csx.GetAllProperties():
            csx.DeleteProperty(prop)
        gc.collect()

        self.assertEqual(csx.GetQtyProperties(), 0)
        grid.AddLine("x", [0, 1, 2])
        self.assertEqual(grid.GetQtyLines("x"), 3)
        self.assertEqual(background.GetEpsilon(), 1.0)
        self.assertIs(csx.GetGrid(), grid)
        self.assertIs(csx.GetBackgroundMaterial(), background)
        self.assertIs(csx.GetParameterSet(), parameters)

    def test_clear_resets_grid_and_background_material(self):
        # Same native clear(): m_BG_Mat.Reset() and clGrid.clear()
        # (src/ContinuousStructure.cpp:456,466).
        csx = ContinuousStructure()
        csx.GetGrid().AddLine("x", [0, 1, 2])
        csx.GetBackgroundMaterial().SetEpsilon(4.0)

        csx.Clear()

        self.assertEqual(csx.GetGrid().GetQtyLines("x"), 0)
        self.assertEqual(csx.GetBackgroundMaterial().GetEpsilon(), 1.0)


class Test_WrapperCache(unittest.TestCase):
    """One wrapper per live native object.

    Native caches wrappers in a weak-valued dict keyed by the C++ address
    (CSProperties.pyx:146-160, CSPrimitives.pyx:112-123, CSTransform.pyx:41-52,
    CSXCAD.pyx:81-92 for the background material, ParameterObjects.pyx:36-48 for
    the parameter set), so the same object has to come back as the same Python
    object.
    """

    def test_lookup_identity(self):
        csx = ContinuousStructure()
        prop = csx.AddMaterial("mat", epsilon=2.0)
        prim = prop.AddBox([0, 0, 0], [1, 1, 1])
        transform = prim.GetTransform()

        self.assertIs(csx.GetProperty(0), prop)
        self.assertIs(csx.GetPropertiesByName("mat")[0], prop)
        self.assertIs(csx.GetPropertyByType(CSPropertiesModule.MATERIAL)[0], prop)
        self.assertIs(csx.GetAllProperties()[0], prop)
        self.assertIs(prop.GetPrimitive(0), prim)
        self.assertIs(csx.GetAllPrimitives()[0], prim)
        self.assertIs(prim.GetTransform(), transform)

    def test_structure_member_identity(self):
        csx = ContinuousStructure()
        self.assertIs(csx.GetGrid(), csx.GetGrid())
        self.assertIs(csx.grid, csx.GetGrid())
        self.assertIs(csx.GetBackgroundMaterial(), csx.GetBackgroundMaterial())
        self.assertIs(csx.GetParameterSet(), csx.GetParameterSet())
        # the grid is per structure, not a module level singleton
        self.assertIsNot(csx.GetGrid(), ContinuousStructure().GetGrid())

    def test_cache_is_evicted_after_delete(self):
        # Native test_WrapperCache::test_wrapper_cache_is_evicted_exactly asserts
        # the caches return to their baseline size; the weak dictionaries drop
        # entries as soon as the wrapper dies.
        gc.collect()
        properties_before = len(CSProperties._instances)
        primitives_before = len(CSPrimitives._instances)

        csx = ContinuousStructure()
        props = [csx.AddMaterial("churn_%d" % n, epsilon=2.0) for n in range(10)]
        for prop in props:
            prop.AddBox([0, 0, 0], [1, 1, 1])
        for prop in props:
            csx.DeleteProperty(prop)
        del props, csx, prop
        gc.collect()

        self.assertEqual(len(CSProperties._instances), properties_before)
        self.assertEqual(len(CSPrimitives._instances), primitives_before)


class Test_CopySemantics(unittest.TestCase):
    """Native GetCopy copies metadata and, on request, the primitives.

    CSProperties::CSProperties(CSProperties*, bool copyPrim) copies uiID, the
    coordinate input type, colours, visibility, name and the attribute maps, and
    copies the primitives only when copyPrim is true
    (CSXCAD/src/CSProperties.cpp:40-55). CSPrimitives::CSPrimitives(CSPrimitives*,
    CSProperties*) re-points the copy at the requested property and deep-copies
    the transform (src/CSPrimitives.cpp:87-101).
    """

    def test_property_copy_without_primitives(self):
        csx = ContinuousStructure()
        prop = csx.AddMaterial("mat", epsilon=2.5)
        prop.SetAttributeValue("note", "original")
        prop.AddBox([0, 0, 0], [1, 1, 1])

        copy = prop.GetCopy()

        self.assertIsInstance(copy, CSPropMaterial)
        self.assertIsNot(copy, prop)
        self.assertEqual(copy.GetName(), "mat")
        self.assertEqual(copy.GetTypeString(), "Material")
        self.assertEqual(copy.GetMaterialProperty("epsilon"), 2.5)
        self.assertEqual(copy.GetAttributeValue("note"), "original")
        # copyPrim defaults to false
        self.assertEqual(copy.GetQtyPrimitives(), 0)
        # uiID is copied by the native copy constructor
        self.assertEqual(copy.GetID(), prop.GetID())
        # the copy is not registered in any structure
        self.assertIsNone(copy._csx)

    def test_property_copy_is_independent(self):
        csx = ContinuousStructure()
        prop = csx.AddMaterial("mat", epsilon=2.5)
        prim = prop.AddBox([0, 0, 0], [1, 1, 1])
        prim.GetTransform().Translate([1, 0, 0])

        copy = prop.GetCopy(True)
        self.assertEqual(copy.GetQtyPrimitives(), 1)
        copied_prim = copy.GetPrimitive(0)
        self.assertIsNot(copied_prim, prim)
        # the transform is deep-copied (src/CSPrimitives.cpp:95-96)
        np.testing.assert_allclose(copied_prim.GetTransform().GetMatrix()[0][3], 1.0)
        np.testing.assert_allclose(prim.GetTransform().GetMatrix()[0][3], 1.0)

        copy.SetName("copy")
        copy.SetMaterialProperty(epsilon=9.0)
        copied_prim.SetStart([5, 5, 5])
        copied_prim.GetTransform().Translate([4, 0, 0])

        self.assertEqual(prop.GetName(), "mat")
        self.assertEqual(prop.GetMaterialProperty("epsilon"), 2.5)
        np.testing.assert_allclose(prim.GetStart(), [0, 0, 0])
        np.testing.assert_allclose(prim.GetTransform().GetMatrix()[0][3], 1.0)
        np.testing.assert_allclose(copy.GetPrimitive(0).GetStart(), [5, 5, 5])

    def test_primitive_copy_keeps_the_source_property(self):
        csx = ContinuousStructure()
        prop = csx.AddMetal("metal")
        box = prop.AddBox([0, 0, 0], [1, 2, 3])

        # GetCopy(prop=None) keeps prim->clProperty (CSPrimitives.cpp:90-93)
        copy = box.GetCopy()

        self.assertIs(type(copy), type(box))
        self.assertIs(copy.GetProperty(), prop)
        np.testing.assert_allclose(copy.GetStart(), box.GetStart())
        np.testing.assert_allclose(copy.GetStop(), box.GetStop())
        self.assertIsNot(copy, box)

    def test_primitive_copy_into_another_property(self):
        csx = ContinuousStructure()
        source = csx.AddMetal("source")
        target = csx.AddMetal("target")
        box = source.AddBox([0, 0, 0], [1, 2, 3])

        copy = box.GetCopy(target)

        self.assertIs(copy.GetProperty(), target)
        np.testing.assert_allclose(copy.GetStart(), [0, 0, 0])
        np.testing.assert_allclose(copy.GetStop(), [1, 2, 3])
        # GetCopy() never calls CSProperties::AddPrimitive
        # (src/CSProperties.cpp:262-272), so the target list is untouched
        self.assertEqual(target.GetQtyPrimitives(), 0)

    def test_transform_copy(self):
        # CSTransform.pyx:99-105 returns a distinct native transform with the
        # same state.
        transform = CSTransform()
        transform.Translate([2, 0, 0])

        copy = transform.copy()
        self.assertIsNot(copy, transform)
        np.testing.assert_allclose(copy.GetMatrix()[0][3], 2.0)

        copy.Translate([1, 0, 0])
        np.testing.assert_allclose(copy.GetMatrix()[0][3], 3.0)
        np.testing.assert_allclose(transform.GetMatrix()[0][3], 2.0)


class Test_OwnershipTransfer(_XMLFixture):
    """Ownership follows the native pointer bookkeeping.

    CSObject::SetOwner/IsOwned document ownership (CSXCAD/src/CSObject.h:46,94-104).
    ContinuousStructure::AddProperty takes ownership (src/ContinuousStructure.cpp:
    80-89), RemoveProperty hands it back by clearing the owner (115-126) and
    DeleteProperty destroys the object (137-150).
    """

    def test_remove_property_hands_ownership_back(self):
        csx = ContinuousStructure()
        prop = csx.AddMaterial("mat", epsilon=2.0)
        prim = prop.AddBox([0, 0, 0], [1, 1, 1])

        csx.RemoveProperty(prop)

        self.assertEqual(csx.GetQtyProperties(), 0)
        # the wrapper and its primitives stay alive and usable
        self.assertEqual(prop.GetName(), "mat")
        self.assertEqual(prop.GetQtyPrimitives(), 1)
        self.assertIs(prop.GetPrimitive(0), prim)
        self.assertIsNone(prop._csx)

        # ownership can be taken by another structure exactly like native
        other = ContinuousStructure()
        other.AddProperty(prop)
        self.assertEqual(other.GetQtyProperties(), 1)
        self.assertIs(other.GetProperty(0), prop)
        self.assertIs(other.GetProperty(0).GetPrimitive(0), prim)
        self.assertEqual(other.GetProperty(0).GetMaterialProperty("epsilon"), 2.0)

    def test_removed_property_can_be_re_added_to_the_same_structure(self):
        csx = ContinuousStructure()
        prop = csx.AddMetal("metal")
        prop.AddBox([0, 0, 0], [1, 1, 1])
        csx.RemoveProperty(prop)
        csx.AddProperty(prop)
        self.assertEqual(csx.GetQtyProperties(), 1)
        self.assertIs(csx.GetProperty(0), prop)

    def test_delete_is_idempotent_and_never_deletes_a_foreign_property(self):
        # DeleteProperty walks its own vector by pointer and returns when the
        # pointer is not found (src/ContinuousStructure.cpp:137-150).
        csx = ContinuousStructure()
        prop = csx.AddMetal("metal")
        csx.DeleteProperty(prop)
        self.assertEqual(csx.GetQtyProperties(), 0)

        # second delete: nothing to delete, no error, the wrapper stays invalid
        self.assertIsNone(csx.DeleteProperty(prop))
        with self.assertRaises(RuntimeError):
            prop.GetName()

        # a property owned by another structure must not be deleted by pid
        # collision; the structure keeps it
        other = ContinuousStructure()
        kept = other.AddMetal("kept")
        csx.DeleteProperty(kept)
        self.assertEqual(other.GetQtyProperties(), 1)
        self.assertEqual(other.GetProperty(0).GetName(), "kept")

        # a property that was removed (owner NULL) is not deleted either
        other.RemoveProperty(kept)
        csx.DeleteProperty(kept)
        self.assertEqual(kept.GetName(), "kept")

    def test_add_property_twice_is_rejected(self):
        # Native AddProperty pushes the pointer unconditionally
        # (src/ContinuousStructure.cpp:80-89), which double-owns the property.
        # The Rust binding refuses instead of reproducing the double free; the
        # target structure must be left untouched.
        csx = ContinuousStructure()
        prop = csx.AddMetal("metal")
        other = ContinuousStructure()

        with self.assertRaises(ValueError) as raised:
            other.AddProperty(prop)
        self.assertIn("already belongs to a structure", str(raised.exception))
        self.assertEqual(other.GetQtyProperties(), 0)
        self.assertEqual(csx.GetQtyProperties(), 1)
        self.assertIs(csx.GetProperty(0), prop)
        # the property is still fully usable after the rejected transfer
        self.assertEqual(prop.GetName(), "metal")

    def test_discrete_material_transform_ownership(self):
        # SetTransform transfers the transform to the property; GetTransform
        # returns the identical wrapper (native caches per C++ pointer,
        # CSTransform.pyx:41-52) and None removes it again.
        csx = ContinuousStructure()
        disc = csx.AddDiscMaterial("disc")
        transform = CSTransform()
        transform.Translate([2, 0, 0])

        disc.SetTransform(transform)
        self.assertIs(disc.GetTransform(), transform)
        np.testing.assert_allclose(disc.GetTransform().GetMatrix()[0][3], 2.0)

        # assigning the same object again is a no-op, not a double free
        disc.SetTransform(transform)
        self.assertIs(disc.GetTransform(), transform)

        disc.SetTransform(None)
        # native SetTransform(NULL) deletes the transform, and GetTransform()
        # lazily creates a fresh identity transform afterwards
        # (src/CSPropDiscMaterial.cpp:294-311), so it is a new object, not None.
        replacement = disc.GetTransform()
        self.assertIsNot(replacement, transform)
        self.assertFalse(replacement.HasTransform())
        np.testing.assert_allclose(replacement.GetMatrix(), np.eye(4))


class Test_ErrorIdentifiers(unittest.TestCase):
    """Exception types/messages match the native wrappers."""

    def test_get_property_index_error(self):
        # CSXCAD.pyx:476-487 raises IndexError('Index is out of range') for a
        # negative or too large index.
        csx = ContinuousStructure()
        csx.AddMetal("metal")
        for index in (-1, 1):
            with self.assertRaises(IndexError) as raised:
                csx.GetProperty(index)
            self.assertIn("Index is out of range", str(raised.exception))

    def test_write2xml_missing_directory(self):
        # CSXCAD.pyx:259-267 checks path.parent.is_dir() and raises
        # FileNotFoundError before touching the native writer.
        csx = ContinuousStructure()
        missing = os.path.join(tempfile.mkdtemp(), "absent", "out.xml")
        with self.assertRaises(FileNotFoundError):
            csx.Write2XML(missing)

    def test_add_material_requires_str(self):
        # CSXCAD.pyx:346-356 raises TypeError for a non-str name.
        csx = ContinuousStructure()
        with self.assertRaises(TypeError) as raised:
            csx.AddMaterial(17)
        self.assertIn("must be a str", str(raised.exception))
        self.assertEqual(csx.GetQtyProperties(), 0)

    def test_invalid_direction_raises_value_error(self):
        # Utilities.pyx:20-38 raises ValueError for an unknown direction.
        with self.assertRaises(ValueError):
            CheckNyDir("q")
        with self.assertRaises(ValueError):
            CheckNyDir(3)

    def test_unknown_keyword_argument(self):
        # CSXCAD.pyx:192-210 rejects unknown constructor keywords.
        with self.assertRaises(Exception) as raised:
            ContinuousStructure(bogus=1)
        self.assertIn("Unknown keyword arguments", str(raised.exception))

    def test_protected_classes_cannot_be_instantiated(self):
        # CSProperties.pyx:162-167 raises for the abstract base, and
        # CSXCAD.pyx:124-125 for the background material.
        with self.assertRaises(Exception) as raised:
            CSProperties()
        self.assertIn("protected", str(raised.exception))
        with self.assertRaises(TypeError) as raised:
            CSBackgroundMaterial()
        self.assertIn("cannot be instantiated directly", str(raised.exception))

    def test_stale_handle_error_identifiers(self):
        csx = ContinuousStructure()
        prop = csx.AddMetal("metal")
        csx.DeleteProperty(prop)
        with self.assertRaises(RuntimeError) as raised:
            prop.GetName()
        self.assertEqual(
            str(raised.exception),
            "wrapped C++ object of type CSPropMetal has been deleted",
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
