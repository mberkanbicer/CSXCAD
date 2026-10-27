#!/usr/bin/env bash
# RM-032/RM-033: run the CSXCAD MATLAB/Octave interface tests in Octave.
#
#   ./run_octave_tests.sh
#
# 1. `test_mfile_surface` checks the 58 public + 12 private M-files and the 114
#    function declarations against the checked-in inventory (with sha256
#    provenance) and resolves every entry point through addpath.
# 2. `test_wrappers_octave` exercises the wrappers and the native error
#    identifiers, and writes a representative model.
# 3. `check_written_xml.py` re-reads that model through the Rust CSXCAD Python
#    package, so the Octave output has to stay consumable by the Rust core.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
VENV="${RUSTPY_VENV:-$HOME/rustpy-venv}"
PYTHON="${VENV}/bin/python"
if [ ! -x "$PYTHON" ]; then
  PYTHON=python3
fi

cd "$SCRIPT_DIR"
XML_OUT="$(mktemp -t csxcad-octave-XXXXXX.xml)"
rm -f "$XML_OUT"
trap 'rm -f "$XML_OUT"' EXIT

echo "== octave: test_mfile_surface"
octave --no-gui --quiet --eval "addpath('$SCRIPT_DIR'); test_mfile_surface" 2>&1 \
  | grep -v -E "libEGL|MESA|ZINK"

echo "== octave: test_wrappers_octave"
RM032_XML_OUT="$XML_OUT" octave --no-gui --quiet \
  --eval "addpath('$SCRIPT_DIR'); test_wrappers_octave" 2>&1 \
  | grep -v -E "libEGL|MESA|ZINK"

echo "== python: check_written_xml.py"
PYTHONPATH="$REPO_ROOT/CSXCAD/rust-py/python${PYTHONPATH:+:$PYTHONPATH}" \
  "$PYTHON" "$SCRIPT_DIR/check_written_xml.py" "$XML_OUT"

echo "CSXCAD MATLAB/Octave interface tests passed"
