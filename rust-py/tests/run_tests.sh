#!/usr/bin/env bash
# RM-031/RM-033: run the CSXCAD Rust Python binding tests.
#
#   ./run_tests.sh              # surface + contract tests
#   ./run_tests.sh --check-inventory  # only verify the checked-in inventory
#
# `test_python_surface` compares the Rust `CSXCAD` package with the native
# Cython surface recorded in `native_python_inventory.json`; `test_contracts`
# pins the error/ownership/lifetime behaviour. `native_inventory.py` regenerates
# the native inventory from `CSXCAD/python/CSXCAD/*.pyx`.
#
# Set RUSTPY_VENV to use a different virtual environment (default
# ~/rustpy-venv); the in-tree `python/` directory always takes precedence on
# sys.path, so an installed wheel cannot mask a source change.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
VENV="${RUSTPY_VENV:-$HOME/rustpy-venv}"
PYTHON="${VENV}/bin/python"
if [ ! -x "$PYTHON" ]; then
  PYTHON=python3
fi

export PYTHONPATH="$REPO_ROOT/CSXCAD/rust-py/python:$REPO_ROOT/openEMS/rust-py/python${PYTHONPATH:+:$PYTHONPATH}"
cd "$SCRIPT_DIR"

if [ "${1:-}" = "--check-inventory" ]; then
  "$PYTHON" native_inventory.py --check
  exit 0
fi

"$PYTHON" native_inventory.py --check
"$PYTHON" -m unittest -v test_python_surface test_contracts
