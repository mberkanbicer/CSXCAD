#!/usr/bin/env python3
"""Deterministic inventory of the native CSXCAD Cython surface (RM-031).

Parses the *native* Cython sources (``CSXCAD/python/CSXCAD/*.pyx``) and emits a
machine-readable inventory of every Python-visible class and ``def`` declaration
with its source location.  Nothing here imports the native extension: the
inventory is a pure source scan so it works in an environment without the
compiled Cython bindings.

Rules (deliberately narrow, so the result is reproducible):

* A class is any line starting with ``cdef class <Name>`` or ``class <Name>``.
* A declaration is any line whose first token is ``def``.  ``cdef``
  declarations are C-level only (not Python-visible) and are ignored; there are
  no ``cpdef`` declarations in the native sources.
* ``def`` lines inside a class body belong to that class; column-0 ``def``
  lines are module level functions.
* Names starting with ``_`` (private helpers, ``__cinit__``/``__init__`` and the
  other special methods) are recorded as ``private`` and are excluded from the
  public-name counts, matching ``docs/migration/audits/csxcad-interfaces.md``.

Run ``python native_inventory.py --check`` to verify the checked-in JSON is
still in sync with the sources; run it without arguments to rewrite it.

Provenance: every source file's size, sha256 and declaration count is recorded
in ``sources`` so a mismatch is always explainable.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
NATIVE_DIR = REPO_ROOT / "CSXCAD" / "python" / "CSXCAD"
INVENTORY_PATH = Path(__file__).resolve().with_name("native_python_inventory.json")

CLASS_RE = re.compile(r"^(?:cdef\s+)?class\s+([A-Za-z_][A-Za-z0-9_]*)\s*[:(]")
DEF_RE = re.compile(r"^(\s*)def\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")
# ``def name(`` split over several lines still has the name on the first line.


def _source_files() -> list[Path]:
    return sorted(NATIVE_DIR.glob("*.pyx"))


def scan() -> dict:
    files = _source_files()
    if not files:
        raise SystemExit("no .pyx sources found below {}".format(NATIVE_DIR))

    sources = []
    classes: dict[str, dict] = {}
    declarations: list[dict] = []

    for path in files:
        raw = path.read_bytes()
        text = raw.decode("utf-8")
        module = path.stem
        current_class: str | None = None
        class_count = 0
        def_count = 0
        for lineno, line in enumerate(text.splitlines(), start=1):
            class_match = CLASS_RE.match(line)
            if class_match:
                name = class_match.group(1)
                current_class = name
                class_count += 1
                entry = classes.setdefault(
                    name,
                    {
                        "name": name,
                        "module": module,
                        "bases": [],
                        "line": lineno,
                        "methods": [],
                    },
                )
                # ``cdef class CSPrimCylindricalShell(CSPrimCylinder):``
                tail = line[class_match.end() - 1 :]
                bases = re.match(r"\(([^)]*)\)", tail)
                if bases:
                    entry["bases"] = [
                        base.strip().split(".")[-1]
                        for base in bases.group(1).split(",")
                        if base.strip()
                    ]
                continue

            # A column-0 statement closes the current class body.
            if line and not line[0].isspace() and not line.startswith("#"):
                current_class = None

            def_match = DEF_RE.match(line)
            if def_match:
                indent = len(def_match.group(1))
                name = def_match.group(2)
                def_count += 1
                owner = current_class if indent > 0 and current_class else None
                visibility = "private" if name.startswith("_") else "public"
                declarations.append(
                    {
                        "name": name,
                        "module": module,
                        "class": owner,
                        "kind": "method" if owner else "function",
                        "visibility": visibility,
                        "line": lineno,
                    }
                )
                if owner:
                    classes[owner]["methods"].append(name)

        sources.append(
            {
                "file": str(path.relative_to(REPO_ROOT)).replace("\\", "/"),
                "bytes": len(raw),
                "sha256": hashlib.sha256(raw).hexdigest(),
                "classes": class_count,
                "def_declarations": def_count,
            }
        )

    public = [d for d in declarations if d["visibility"] == "public"]
    public_names = sorted({d["name"] for d in public})
    return {
        "inventory_version": 1,
        "generator": "CSXCAD/rust-py/tests/native_inventory.py",
        "rules": {
            "class": "line starting with 'cdef class' or 'class'",
            "declaration": "line whose first token is 'def' (cdef excluded, no cpdef present)",
            "public_name": "declaration name not starting with '_'",
        },
        "sources": sources,
        "counts": {
            "files": len(sources),
            "classes": len(classes),
            "class_declarations": sum(s["classes"] for s in sources),
            "def_declarations": len(declarations),
            "public_declarations": len(public),
            "public_distinct_names": len(public_names),
            "private_declarations": len(declarations) - len(public),
        },
        "classes": [classes[name] for name in sorted(classes)],
        "declarations": declarations,
    }


def render(inventory: dict) -> str:
    return json.dumps(inventory, indent=2, sort_keys=False) + "\n"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="do not write; fail if the checked-in inventory differs",
    )
    args = parser.parse_args(argv)

    inventory = scan()
    text = render(inventory)
    if args.check:
        if not INVENTORY_PATH.exists():
            print("missing {}".format(INVENTORY_PATH), file=sys.stderr)
            return 1
        if INVENTORY_PATH.read_text(encoding="utf-8") != text:
            print(
                "{} is stale; re-run python native_inventory.py".format(
                    INVENTORY_PATH.name
                ),
                file=sys.stderr,
            )
            return 1
        counts = inventory["counts"]
        print(
            "inventory current: {def_declarations} declarations, "
            "{public_declarations} public, {public_distinct_names} distinct public "
            "names, {classes} classes".format(**counts)
        )
        return 0

    INVENTORY_PATH.write_text(text, encoding="utf-8")
    counts = inventory["counts"]
    print(
        "wrote {}: {def_declarations} declarations, {public_declarations} public, "
        "{public_distinct_names} distinct public names, {classes} classes".format(
            INVENTORY_PATH.name, **counts
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
