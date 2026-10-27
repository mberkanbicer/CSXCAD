#!/usr/bin/env python3
"""Re-read the Octave-written CSXCAD XML with the Rust CSXCAD Python package.

RM-032: the MATLAB/Octave delivery writes the native `ContinuousStructure` XML
through the unchanged M-files. This script closes the loop: the file that
`tests/octave/test_wrappers_octave.m` wrote is parsed by the Rust core and every
value the M-file API set is checked again, so an M-file change that the Rust
reader cannot consume fails the interface test instead of silently diverging.

Usage: python check_written_xml.py <file.xml>
Set `PYTHONPATH` to `CSXCAD/rust-py/python` (done by run_octave_tests.sh).
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

from CSXCAD.CSXCAD import ContinuousStructure
from CSXCAD.CSProperties import CSPropExcitation, CSPropMaterial, CSPropMetal, CSPropProbeBox


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    path = Path(argv[0])
    if not path.is_file():
        print("missing XML file: {}".format(path), file=sys.stderr)
        return 1

    csx = ContinuousStructure()
    message = csx.ReadFromXML(str(path))
    if message != "":
        print("ReadFromXML failed: {}".format(message), file=sys.stderr)
        return 1

    failures: list[str] = []

    def check(condition: bool, description: str) -> None:
        if not condition:
            failures.append(description)

    def close(a, b, tol=1e-12) -> bool:
        return np.allclose(np.asarray(a, dtype=float), np.asarray(b, dtype=float), atol=tol, rtol=0)

    check(csx.GetQtyProperties() == 4, "property count: {}".format(csx.GetQtyProperties()))

    by_name = {prop.GetName(): prop for prop in csx.GetAllProperties()}
    check(set(by_name) == {"substrate", "metal", "exc", "ut"}, "property names: {}".format(sorted(by_name)))

    substrate = by_name.get("substrate")
    check(isinstance(substrate, CSPropMaterial), "substrate is a CSPropMaterial")
    if isinstance(substrate, CSPropMaterial):
        check(close(substrate.GetMaterialProperty("epsilon"), 2.5), "substrate epsilon")
        check(close(substrate.GetMaterialProperty("kappa"), 0.1), "substrate kappa")

    metal = by_name.get("metal")
    check(isinstance(metal, CSPropMetal), "metal is a CSPropMetal")
    if isinstance(metal, CSPropMetal):
        check(metal.GetQtyPrimitives() == 1, "metal primitive count")
        box = metal.GetPrimitive(0)
        check(type(box).__name__ == "CSPrimBox", "metal primitive is a box")
        check(box.GetPriority() == 10, "box priority: {}".format(box.GetPriority()))
        check(close(box.GetStart(), [0, 0, 0]), "box start: {}".format(box.GetStart()))
        check(close(box.GetStop(), [3, 4, 5]), "box stop: {}".format(box.GetStop()))

    excitation = by_name.get("exc")
    check(isinstance(excitation, CSPropExcitation), "exc is a CSPropExcitation")
    if isinstance(excitation, CSPropExcitation):
        check(excitation.GetExcitType() == 0, "excitation type")
        check(close(excitation.GetExcitation(), [0, 0, 1]), "excitation vector")

    probe = by_name.get("ut")
    check(isinstance(probe, CSPropProbeBox), "ut is a CSPropProbeBox")
    if isinstance(probe, CSPropProbeBox):
        check(probe.GetProbeType() == 0, "probe type")
        check(close(probe.GetFrequency(), [1e9]), "probe frequency: {}".format(probe.GetFrequency()))

    grid = csx.GetGrid()
    check(close(grid.GetDeltaUnit(), 1e-3), "grid delta unit: {}".format(grid.GetDeltaUnit()))
    check(grid.GetQtyLines("x") == 11, "x line count: {}".format(grid.GetQtyLines("x")))
    check(grid.GetQtyLines("y") == 11, "y line count: {}".format(grid.GetQtyLines("y")))
    check(grid.GetQtyLines("z") == 6, "z line count: {}".format(grid.GetQtyLines("z")))
    check(close(grid.GetSimArea(), [[0, 0, 0], [10, 10, 5]]), "grid area: {}".format(grid.GetSimArea()))

    if failures:
        for failure in failures:
            print("FAIL: {}".format(failure), file=sys.stderr)
        return 1
    print("Octave-written XML re-read by the Rust core: 4 properties, grid and box values match")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
