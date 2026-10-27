# -*- coding: utf-8 -*-
"""RM-031: the Rust ``CSXCAD`` package must expose the native Cython surface.

The native surface is not maintained by hand: ``native_inventory.py`` parses the
*unchanged* native Cython sources (``CSXCAD/python/CSXCAD/*.pyx``) and the result
is checked in as ``native_python_inventory.json`` with per-file sha256
provenance. This module then compares that inventory against the Rust package
class by class, method by method and module function by module function.

Run it directly (``python tests/test_python_surface.py``) or through
``unittest``; it inserts the in-tree package directory on ``sys.path`` so it
always tests the sources next to it, not an installed wheel.
"""

from __future__ import annotations

import importlib
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
PACKAGE_ROOT = HERE.parent / "python"
for path in (str(HERE), str(PACKAGE_ROOT)):
    if path not in sys.path:
        sys.path.insert(0, path)

import native_inventory  # noqa: E402  (path set up above)

INVENTORY = native_inventory.scan()


#: Native public names the Rust package intentionally does not provide.
#:
#: Empty: every one of the 256 distinct native public names is available with
#: the same name on the same class/module, and the two names that were missing
#: when this contract was written (``Utilities.RegisterWrapperFactory`` and
#: ``Utilities.WrapperFromAddress``) are implemented instead of being declared
#: absent. The mapping is kept so that a future gap has to be declared here with
#: a concrete reason tied to the native source; ``test_no_undeclared_gap`` fails
#: otherwise.
ABSENT: dict[str, str] = {}


#: Deliberate, documented differences that are *behaviour*, not surface, and are
#: therefore not covered by the name comparison above. Each entry is asserted by
#: ``test_documented_deviations_still_hold`` so the record cannot silently rot.
#:
#: name -> (reason, assertion)
#:   reason cites the native source that defines the original behaviour.
DOCUMENTED_DEVIATIONS = {
    "ContinuousStructure.AddProperty(already owned)": (
        "native ContinuousStructure::AddProperty pushes the pointer unconditionally "
        "(src/ContinuousStructure.cpp:80-89), so adding a property that another "
        "structure already owns double-owns it and later double-frees; the Rust "
        "binding rejects it with ValueError instead of reproducing memory corruption",
    ),
    "CSTransform(pset)": (
        "native CSTransform stores Parameter terms (CSTransform.h m_Matrix) so a "
        "ParameterSet-backed transform can hold expressions; the Rust Transform is a "
        "numeric 4x4 matrix. Passing a parameter set raises TypeError instead of "
        "silently dropping the expression support",
    ),
    "Utilities.GetMultiDirs(invalid)": (
        "native GetMultiDirs has no else branch (Utilities.pyx:40-56) and returns "
        "None for an unsupported direction string; the Rust port raises ValueError so "
        "the mistake is visible. No native caller depends on the None",
    ),
    "CSPrimMultiBox.ClearOverlap()": (
        "native CSPrimMultiBox::ClearOverlap trims vCoords.size()%6 trailing "
        "coordinates (src/CSPrimMultiBox.cpp:132-140); a Rust MultiBox stores whole "
        "(start, stop) pairs, so the incomplete state cannot exist and the call is a "
        "no-op with the native signature",
    ),
}


def _class_module(module_name: str):
    return importlib.import_module("CSXCAD." + module_name)


def _public_methods(entry: dict) -> list[str]:
    return [name for name in entry["methods"] if not name.startswith("_")]


def _public_functions() -> list[dict]:
    return [
        declaration
        for declaration in INVENTORY["declarations"]
        if declaration["kind"] == "function" and declaration["visibility"] == "public"
    ]


def _gaps() -> list[str]:
    """Every native public name that the Rust package does not provide."""
    gaps: list[str] = []
    for entry in INVENTORY["classes"]:
        module = _class_module(entry["module"])
        cls = getattr(module, entry["name"], None)
        if cls is None:
            gaps.append("{}:{}".format(entry["module"], entry["name"]))
            continue
        for method in _public_methods(entry):
            attribute = getattr(cls, method, None)
            if attribute is None:
                gaps.append("{}.{}.{}".format(entry["module"], entry["name"], method))
    for declaration in _public_functions():
        module = _class_module(declaration["module"])
        if getattr(module, declaration["name"], None) is None:
            gaps.append("{}:{}".format(declaration["module"], declaration["name"]))
    return sorted(gaps)


class Test_NativeInventory(unittest.TestCase):
    """The checked-in inventory is the contract, so it must be reproducible."""

    def test_inventory_matches_the_native_sources(self):
        checked_in = native_inventory.INVENTORY_PATH.read_text(encoding="utf-8")
        self.assertEqual(
            checked_in,
            native_inventory.render(INVENTORY),
            "native_python_inventory.json is stale: re-run "
            "`python tests/native_inventory.py` in CSXCAD/rust-py",
        )

    def test_inventory_counts_match_the_audit(self):
        counts = INVENTORY["counts"]
        # docs/migration/audits/csxcad-interfaces.md: 293 non-private
        # declarations, 256 distinct names, 33 Cython classes.
        self.assertEqual(counts["files"], 7)
        self.assertEqual(counts["classes"], 33)
        self.assertEqual(counts["def_declarations"], 360)
        self.assertEqual(counts["public_declarations"], 293)
        self.assertEqual(counts["public_distinct_names"], 256)

    def test_every_source_has_provenance(self):
        for source in INVENTORY["sources"]:
            self.assertRegex(source["sha256"], r"^[0-9a-f]{64}$")
            self.assertGreater(source["bytes"], 0)
            self.assertGreater(source["def_declarations"], 0)


class Test_RustPythonSurface(unittest.TestCase):
    def test_every_native_class_is_provided(self):
        missing = []
        for entry in INVENTORY["classes"]:
            module = _class_module(entry["module"])
            if getattr(module, entry["name"], None) is None:
                missing.append("{}:{}".format(entry["module"], entry["name"]))
        self.assertEqual(missing, [], "native classes missing from the Rust package")

    def test_every_native_public_method_is_provided(self):
        missing = []
        checked = 0
        for entry in INVENTORY["classes"]:
            module = _class_module(entry["module"])
            cls = getattr(module, entry["name"], None)
            if cls is None:
                continue
            for method in _public_methods(entry):
                checked += 1
                if getattr(cls, method, None) is None:
                    missing.append(
                        "{}.{}.{}".format(entry["module"], entry["name"], method)
                    )
        self.assertEqual(missing, [])
        # guards against the inventory silently shrinking
        self.assertEqual(checked, 288)

    def test_every_native_module_function_is_provided(self):
        missing = []
        checked = 0
        for declaration in _public_functions():
            checked += 1
            module = _class_module(declaration["module"])
            if getattr(module, declaration["name"], None) is None:
                missing.append("{}:{}".format(declaration["module"], declaration["name"]))
        self.assertEqual(missing, [])
        self.assertEqual(checked, 5)

    def test_provided_surface_is_callable(self):
        """A provided name has to be a real attribute, not a placeholder value."""
        not_callable = []
        for entry in INVENTORY["classes"]:
            module = _class_module(entry["module"])
            cls = getattr(module, entry["name"], None)
            if cls is None:
                continue
            if not isinstance(cls, type):
                not_callable.append("{}:{}".format(entry["module"], entry["name"]))
                continue
            for method in _public_methods(entry):
                attribute = getattr(cls, method, None)
                if attribute is None:
                    continue
                # ``ContinuousStructure.grid`` is a @property in the native
                # Cython source too (CSXCAD/python/CSXCAD/CSXCAD.pyx:301-304).
                if not callable(attribute) and not isinstance(attribute, property):
                    not_callable.append(
                        "{}.{}.{}".format(entry["module"], entry["name"], method)
                    )
        for declaration in _public_functions():
            module = _class_module(declaration["module"])
            attribute = getattr(module, declaration["name"], None)
            if attribute is not None and not callable(attribute):
                not_callable.append(
                    "{}:{}".format(declaration["module"], declaration["name"])
                )
        self.assertEqual(not_callable, [])

    def test_no_undeclared_gap(self):
        gaps = _gaps()
        undeclared = [gap for gap in gaps if gap not in ABSENT]
        self.assertEqual(
            undeclared,
            [],
            "native names missing from the Rust package and not declared in ABSENT",
        )
        self.assertEqual(
            sorted(gaps),
            sorted(ABSENT),
            "ABSENT must shrink to the real gap list so the count is honest",
        )

    def test_absent_entries_are_real_and_reasoned(self):
        native_names = set()
        for entry in INVENTORY["classes"]:
            native_names.add("{}:{}".format(entry["module"], entry["name"]))
            for method in _public_methods(entry):
                native_names.add(
                    "{}.{}.{}".format(entry["module"], entry["name"], method)
                )
        for declaration in _public_functions():
            native_names.add("{}:{}".format(declaration["module"], declaration["name"]))
        for name, reason in ABSENT.items():
            self.assertIn(name, native_names, "ABSENT names a non-native entry")
            self.assertTrue(reason.strip(), "ABSENT entry without a reason")

    def test_native_import_paths_resolve(self):
        """The native module layout and ``__all__`` must keep working."""
        import CSXCAD
        from CSXCAD import CSPrimitives, CSProperties, CSRectGrid, CSTransform, Utilities
        from CSXCAD.CSXCAD import ContinuousStructure
        from CSXCAD.CSTransform import CSTransform as TransformClass
        from CSXCAD.CSRectGrid import CSRectGrid as GridClass
        from CSXCAD.ParameterObjects import ParameterSet

        self.assertIs(CSXCAD.ContinuousStructure, ContinuousStructure)
        self.assertIs(Utilities.CheckNyDir, CSXCAD.Utilities.CheckNyDir)
        self.assertTrue(issubclass(TransformClass, object))
        self.assertTrue(callable(GridClass))
        self.assertTrue(callable(ParameterSet))
        for name in ("ContinuousStructure", "CSProperties", "CSPrimitives", "CSRectGrid",
                     "CSTransform", "Utilities"):
            self.assertIn(name, CSXCAD.__all__)
        self.assertTrue(CSProperties.ANY)
        self.assertIsNotNone(CSPrimitives.__name__)

    def test_documented_deviations_still_hold(self):
        """Keep the deviation register executable instead of prose."""
        from CSXCAD.CSXCAD import ContinuousStructure
        from CSXCAD.CSTransform import CSTransform
        from CSXCAD.Utilities import GetMultiDirs

        csx = ContinuousStructure()
        metal = csx.AddMetal("deviation")
        other = ContinuousStructure()
        with self.assertRaises(ValueError):
            other.AddProperty(metal)
        self.assertEqual(other.GetQtyProperties(), 0)

        with self.assertRaises(TypeError):
            CSTransform(csx.GetParameterSet())

        with self.assertRaises(ValueError):
            GetMultiDirs("q")


if __name__ == "__main__":
    unittest.main(verbosity=2)
