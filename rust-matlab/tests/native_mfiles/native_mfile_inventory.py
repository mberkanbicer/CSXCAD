#!/usr/bin/env python3
"""Deterministic inventory of the native CSXCAD MATLAB/Octave surface (RM-032).

Parses the *unchanged* native M-files (``CSXCAD/matlab/*.m`` and
``CSXCAD/matlab/private/*.m``) and records every function declaration with its
argument list and return values, plus per-file sha256 provenance. The result is
checked in as ``native_mfile_inventory.json``; ``tests/octave/
test_mfile_surface.m`` verifies both that the inventory is still current and
that every recorded entry point is present and resolves.

Parsing rules (MATLAB grammar, restricted to the declarations these files use):

* a declaration line is ``function ...`` at the start of a line (leading
  whitespace allowed), so comments containing the word "function" are ignored;
* supported forms are ``function name(args)``, ``function ret = name(args)``,
  ``function [a, b] = name(args)`` and ``function [a b] = name(args)``;
* the first declaration of a file is the public entry point (its name must match
  the file name); later declarations are local helper functions;
* ``args`` and ``returns`` are split on top-level commas/whitespace, keeping
  ``varargin``/``varargout`` verbatim.

Run ``python native_mfiles/native_mfile_inventory.py --check`` to verify the
checked-in JSON, or without ``--check`` to rewrite it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
MATLAB_DIR = HERE.parents[2] / "matlab"
INVENTORY_PATH = HERE / "native_mfile_inventory.json"

FUNCTION_RE = re.compile(
    r"""^[ \t]*function[ \t]+
        (?:
            \[(?P<bracket_returns>[^\]]*)\][ \t]*=[ \t]*
          | (?P<single_return>[A-Za-z_][A-Za-z0-9_]*)[ \t]*=[ \t]*
        )?
        (?P<name>[A-Za-z_][A-Za-z0-9_]*)
        (?:[ \t]*\((?P<args>[^)]*)\))?
    """,
    re.VERBOSE,
)

#: Public entry points whose *M-file* delivery additionally needs an external
#: runtime artefact. Everything else runs from the M-file alone.
REQUIRES = {
    # CSXGeomPlot resolves AppCSXCAD through searchBinary() and calls it with
    # `system`; the viewer binary is part of the Rust install (bin/AppCSXCAD).
    "CSXGeomPlot": ["binary:AppCSXCAD"],
    # CreateDiscMaterial needs Octave's h5create (hdf5 package) or MATLAB's
    # built-in; the M-file rejects Octave without it.
    "CreateDiscMaterial": ["octave-package:HDF5 (h5create)"],
}


def split_names(text: str | None) -> list[str]:
    if not text:
        return []
    return [item for item in re.split(r"[,\s]+", text.strip()) if item]


def scan() -> dict:
    files = sorted(MATLAB_DIR.glob("*.m")) + sorted(MATLAB_DIR.glob("private/*.m"))
    if not files:
        raise SystemExit("no M-files found below {}".format(MATLAB_DIR))

    sources = []
    public: list[dict] = []
    private: list[dict] = []
    declarations: list[dict] = []

    for path in files:
        raw = path.read_bytes()
        text = raw.decode("utf-8", errors="surrogateescape")
        relative = path.relative_to(MATLAB_DIR.parent).as_posix()
        is_private = path.parent.name == "private"
        entry_points = []
        count = 0
        lines = text.splitlines()
        position = 0
        while position < len(lines):
            lineno = position + 1
            line = lines[position]
            position += 1
            match = FUNCTION_RE.match(line)
            if not match:
                continue
            # A signature may be wrapped with a MATLAB continuation (`...`).
            # Join the continuation lines so the recorded argument list is the
            # real one, but keep the line number of the declaration start.
            while line.rstrip().endswith("...") and position < len(lines):
                line = line.rstrip()[:-3].rstrip() + " " + lines[position].strip()
                position += 1
            match = FUNCTION_RE.match(line)
            if not match:
                raise SystemExit(
                    "{}:{}: could not parse the joined function signature".format(
                        relative, lineno
                    )
                )
            count += 1
            return_text = match.group("bracket_returns")
            if return_text is not None:
                returns = split_names(return_text)
            else:
                returns = split_names(match.group("single_return"))
            declaration = {
                "name": match.group("name"),
                "args": split_names(match.group("args")),
                "returns": returns,
                "line": lineno,
                "local": count > 1,
            }
            declarations.append(
                {
                    **declaration,
                    "file": relative,
                    "public": not is_private and count == 1,
                }
            )
            if not is_private and count == 1:
                entry_points.append(
                    {
                        "name": declaration["name"],
                        "file": relative,
                        "args": declaration["args"],
                        "returns": declaration["returns"],
                        "delivery": "m-file",
                        "requires": list(REQUIRES.get(declaration["name"], [])),
                        "local_functions": [],
                    }
                )

        # attach the local helper names after the whole file is scanned
        locals_ = [d["name"] for d in declarations if d["file"] == relative and d["local"]]
        if entry_points:
            entry_points[0]["local_functions"] = locals_
        else:
            # private helper: its entry point is the first declaration
            first = next(
                d for d in declarations if d["file"] == relative and not d["local"]
            )
            entry_points.append(
                {
                    "name": first["name"],
                    "file": relative,
                    "args": first["args"],
                    "returns": first["returns"],
                    "delivery": "m-file",
                    "requires": list(REQUIRES.get(first["name"], [])),
                    "local_functions": locals_,
                }
            )

        if len(entry_points) != 1:
            raise SystemExit(
                "{}: expected exactly one top-level function, found {}".format(
                    relative, len(entry_points)
                )
            )
        if entry_points[0]["name"] != path.stem and not is_private:
            raise SystemExit(
                "{}: top-level function {} does not match the file name".format(
                    relative, entry_points[0]["name"]
                )
            )

        sources.append(
            {
                "file": relative,
                "kind": "private" if is_private else "public",
                "bytes": len(raw),
                "sha256": hashlib.sha256(raw).hexdigest(),
                "declarations": count,
            }
        )
        if is_private:
            private.append(entry_points[0])
        else:
            public.append(entry_points[0])

    return {
        "inventory_version": 1,
        "generator": "CSXCAD/rust-matlab/tests/native_mfiles/native_mfile_inventory.py",
        "rules": {
            "declaration": "line starting with 'function' (MATLAB signature grammar)",
            "public": "first declaration of a public M-file, name == file name",
            "private": "first declaration of a file in matlab/private",
            "local": "any later declaration in the same file",
        },
        "counts": {
            "files": len(sources),
            "public_files": len(public),
            "private_files": len(private),
            "declarations": len(declarations),
        },
        "sources": sources,
        "public": sorted(public, key=lambda item: item["name"]),
        "private": sorted(private, key=lambda item: item["name"]),
        "declarations": declarations,
        # How the Rust delivery exposes the MATLAB surface. The M-file API is the
        # native API and ships unchanged (see the per-entry `delivery`), while
        # the C ABI and the CLIs are additional paths used by embedding hosts and
        # by the viewer, not replacements for the M-files.
        "delivery": {
            "m_files": "the unchanged native M-files in CSXCAD/matlab (this inventory)",
            "c_abi": {
                "library": "csxcad-matlab / libcsxcad_matlab (CSXCAD/rust-matlab)",
                "purpose": "embedding and MEX path on top of csxcad-core; "
                "tested by src/contract_tests.rs",
                "symbols": [
                    "csxcad_new",
                    "csxcad_free",
                    "csxcad_clear",
                    "csxcad_property_count",
                    "csxcad_add_property",
                    "csxcad_find_property",
                    "csxcad_set_background_material",
                    "csxcad_set_material_property",
                    "csxcad_add_box",
                    "csxcad_add_sphere",
                    "csxcad_add_point",
                    "csxcad_add_cylinder",
                    "csxcad_to_xml_string",
                    "csxcad_from_xml_string",
                    "csxcad_free_string",
                    "csxcad_set_coord_system",
                    "csxcad_add_grid_line",
                    "csxcad_set_delta_unit",
                ],
            },
            "cli": ["openEMS", "AppCSXCAD"],
        },
        # Entry points that are intentionally not provided by the Rust delivery.
        # Empty: every native public M-file ships unchanged and resolves in
        # Octave 11.1.0; the Rust `csxcad-matlab` C ABI is an additional path for
        # embedding hosts, not a replacement for the M-file API.
        "absent": {},
        "absent_note": (
            "Empty on purpose: all 58 public and 12 private native M-files are "
            "delivered unchanged, so no native entry point is missing. A name "
            "would only be listed here with the native source that defines it and "
            "the reason it cannot be provided."
        ),
    }


def render(inventory: dict) -> str:
    return json.dumps(inventory, indent=2) + "\n"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)

    inventory = scan()
    text = render(inventory)
    if args.check:
        if not INVENTORY_PATH.exists():
            print("missing {}".format(INVENTORY_PATH), file=sys.stderr)
            return 1
        if INVENTORY_PATH.read_text(encoding="utf-8") != text:
            print(
                "{} is stale; re-run native_mfile_inventory.py".format(
                    INVENTORY_PATH.name
                ),
                file=sys.stderr,
            )
            return 1
        print(
            "inventory current: {public_files} public + {private_files} private "
            "M-files, {declarations} declarations".format(**inventory["counts"])
        )
        return 0

    INVENTORY_PATH.write_text(text, encoding="utf-8")
    print(
        "wrote {}: {public_files} public + {private_files} private M-files, "
        "{declarations} declarations".format(INVENTORY_PATH.name, **inventory["counts"])
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
