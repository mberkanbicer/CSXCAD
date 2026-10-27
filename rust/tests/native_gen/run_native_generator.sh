#!/usr/bin/env bash
# Regenerate every CSXCAD native fixture used by the RM-028/029/034/035 tests.
#
# Run inside WSL/Linux from anywhere:
#
#   CSXCAD/rust/tests/native_gen/run_native_generator.sh
#
# Requirements (verified on Ubuntu 24.04 + WSL2):
#   * g++ with C++17
#   * cmake (>= 3.10) and the dependencies of the pinned native CSXCAD/openEMS
#     sources: VTK 9.5, TinyXML, HDF5, Boost, CGAL, Eigen, fparser
#   * Python 3 with the VTK bindings (`import vtk`)
#   * a fparser install prefix (default: $HOME/opt/openEMS, override FPARSER_PREFIX)
#
# The script never modifies the repository sources: it copies CSXCAD/openEMS to
# $HOME/rustmig-native (excluding .git and rust/target), builds them there and
# writes the fixtures back into CSXCAD/rust/tests/fixtures. Native source files
# stay untouched.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
work="${RUSTMIG_NATIVE_DIR:-$HOME/rustmig-native}"
fparser_prefix="${FPARSER_PREFIX:-$HOME/opt/openEMS}"
fixtures="$repo_root/CSXCAD/rust/tests/fixtures"

mkdir -p "$work"

echo "== copying pinned native sources to $work"
rm -rf "$work/CSXCAD-native-src" "$work/openems-native-src"
mkdir -p "$work/CSXCAD-native-src" "$work/openems-native-src"
(cd "$repo_root/CSXCAD" && tar --exclude=./rust/target --exclude=./.git -cf - .) \
    | (cd "$work/CSXCAD-native-src" && tar -xf -)
(cd "$repo_root/openEMS" && tar --exclude=./rust/target --exclude=./.git -cf - .) \
    | (cd "$work/openems-native-src" && tar -xf -)

echo "== building native CSXCAD"
rm -rf "$work/csxcad-build" "$work/csxcad-install"
cmake -S "$work/CSXCAD-native-src" -B "$work/csxcad-build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DFPARSER_ROOT_DIR="$fparser_prefix" \
    -DCMAKE_INSTALL_PREFIX="$work/csxcad-install" >/dev/null
cmake --build "$work/csxcad-build" -j "$(nproc)" >/dev/null
cmake --install "$work/csxcad-build" >/dev/null

echo "== building native openEMS (only for the openEMS wrapper fixture)"
rm -rf "$work/openems-build"
cmake -S "$work/openems-native-src" -B "$work/openems-build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DFPARSER_ROOT_DIR="$fparser_prefix" \
    -DCSXCAD_ROOT_DIR="$work/csxcad-install" \
    -DCMAKE_INSTALL_PREFIX="$work/openems-install" >/dev/null
cmake --build "$work/openems-build" -j "$(nproc)" --target openEMS >/dev/null

echo "== building the fixture generator"
g++ -std=c++17 -O2 -o "$work/generate_native_fixtures" \
    "$repo_root/CSXCAD/rust/tests/native_gen/generate_native_fixtures.cpp" \
    -I"$work/csxcad-install/include/CSXCAD" \
    -I"$work/openems-native-src" \
    -I"$fparser_prefix/include" \
    -L"$work/csxcad-build/src" \
    -L"$work/openems-build" \
    -Wl,-rpath,"$work/csxcad-build/src" \
    -Wl,-rpath,"$work/openems-build" \
    -lCSXCAD -lopenEMS

echo "== generating XML fixtures and native model reports"
mkdir -p "$fixtures/xml" "$fixtures/native"
rm -f "$fixtures/xml"/*.xml "$fixtures/native"/*.report
"$work/generate_native_fixtures" "$fixtures"

echo "== generating the STL/PLY corpus and VTK expectations"
python3 "$repo_root/CSXCAD/rust/tests/native_gen/vtk_mesh_expectations.py" \
    "$fixtures/mesh"

echo "== done"
echo "native CSXCAD: $(grep -h LIB_VERSION "$repo_root/CSXCAD/CMakeLists.txt" | head -1)"
echo "fixtures: $(ls "$fixtures/xml"/*.xml | wc -l) XML documents,"
echo "          $(ls "$fixtures/native"/*.report | wc -l) native reports,"
echo "          $(ls "$fixtures/mesh"/*.expected | wc -l) mesh expectations"
