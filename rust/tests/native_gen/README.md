# CSXCAD native fixture generator

This directory holds the tools that produced the fixture corpora used by the
CSXCAD Rust migration tests:

| Path | Purpose |
| --- | --- |
| `generate_native_fixtures.cpp` | Links the pinned **native** `libCSXCAD`/`libopenEMS`, builds a document per XML feature family, writes it with `ContinuousStructure::Write2XML`, re-reads it with native `ReadFromXML`, and records the observable model in `tests/fixtures/native/<name>.report`. |
| `run_native_generator.sh` | Reproducible end-to-end regeneration (copy sources, build native CSXCAD + openEMS, build the generator, run it, run the VTK mesh pass). |
| `vtk_mesh_expectations.py` | Generates the STL/PLY corpus and its VTK-verified point/face expectations. |
| `../../tests/fixtures/xml/` | Native XML documents + `manifest.txt`. |
| `../../tests/fixtures/native/` | Native model reports (the differential reference). |
| `../../tests/fixtures/mesh/` | STL/PLY files + `.expected` arrays. |

Nothing in these directories may be edited by hand. The Rust tests fail if the
manifest, the fixture files and the reports disagree.

## Regenerating

Prerequisites (validated on Ubuntu 24.04/WSL2): `g++` (C++17), `cmake`,
VTK 9.5, TinyXML, HDF5, Boost, CGAL, Eigen, `numpy`, a fparser install and
Python 3 with VTK bindings.

```bash
CSXCAD/rust/tests/native_gen/run_native_generator.sh
# optional overrides:
#   FPARSER_PREFIX=$HOME/opt/openEMS   (fparser.hh + libfparser)
#   RUSTMIG_NATIVE_DIR=$HOME/rustmig-native
```

The script copies `CSXCAD/` and `openEMS/` to `$RUSTMIG_NATIVE_DIR` (excluding
`.git` and `rust/target`), builds them there and writes the fixtures back into
the repository. The native sources in the repository are **not** modified.

Reference environment used for the committed fixtures:

* native CSXCAD `v0.7.0` built from this worktree (`ContinuousStructure::GetInfoLine()` reports `CSXCAD -- Version: v0.7.0`),
* native openEMS `0.37.0` from this worktree (wrapper fixture only),
* fparser 4.5.1 from `~/opt/openEMS`,
* VTK 9.5.2 (STL/PLY readers and writers), TinyXML 2.6, HDF5 1.14,
  GCC 15.2, Ubuntu 24.04 under WSL2.

## XML corpus

| Fixture | Covers |
| --- | --- |
| `01_empty.xml` | Empty structure, default grid/background/parameter set. |
| `02_grid_background_parameters.xml` | Grid lines + `DeltaUnit`, non-default background material, `Const`/`Linear`/sweep-disabled parameters, expression coordinates. |
| `03_materials.xml` | Isotropic and anisotropic `Material`, values and weight expressions, `Metal`, `Unknown` (legacy property tag), `ResBox`, generic `<Attributes>`, visibility. |
| `04_dispersive.xml` | `DebyeMaterial` and `LorentzMaterial` with two poles, per-pole values and weight expressions. |
| `05_sheets_lumped_abc.xml` | `ConductingSheet`, `LumpedElement` (series, caps, R/C/L), `AbsorbingBC` (Mur 1st order, negative normal sign). |
| `06_excitation_probe_dump.xml` | `Excitation` (components, `PropDir`, weight functions, weight origin, weight file, disabled), `ProbeBox` (mode functions/origin, FD samples, oversampling), second probe with `ModeFile`, `DumpBox` (type/mode/file type/multi-grid/sub-sampling/opt resolution/FD samples). |
| `07_primitives.xml` | Every primitive type: `Point`, `Box` (priority 3), `MultiBox` (2 boxes), `Sphere`, `SphericalShell`, `Cylinder`, `CylindricalShell`, `Polygon`, `LinPoly`, `RotPoly`, `Polyhedron`, `Curve`, `Wire`, `User-Defined`, plus a primitive with its own `CoordSystem` (cylindrical box). |
| `08_transforms.xml` | Every transform operation (`Translate` with an expression, `Scale`, `Scale3`, `Rotate_Origin`, `Rotate_X/Y/Z`, `Matrix`), pre/post multiply, degree mode, matrix+scale composition. Also records native operation-order matrices (`tscenario` lines) and native expression error codes. |
| `09_cylindrical.xml` | Structure `CoordSystem=1`, cylindrical mesh, material box and cylinder in cylindrical coordinates × azimuth. |
| `10_openems_wrapper.xml` | Full `<openEMS><FDTD>…<BoundaryCond>…<ContinuousStructure>` wrapper written by native `openEMS::Write2XML`. |
| `11_disc_material.xml` | `DiscMaterial` with `<DiscFile>` (file name, scale, background flag) and `<Transformation>`. |

`manifest.txt` lists every fixture with the property/primitive counts of the
written document and its byte size.

The openEMS wrapper: the parser extracts the `ContinuousStructure` child of
`<openEMS>`; the solver settings (`FDTD`, `BoundaryCond`, `Excitation`) are out
of scope for the CSXCAD model and are preserved only by the AppCSXCAD project
layer (`qcsxcad_core::ProjectFile`).

Every native report is regenerated from a **native re-read** of the document
that was written (`read_back|1`), so it describes what native CSXCAD sees in the
file, not the in-memory object that produced it. `11_disc_material` is the one
exception (`read_back|0`): the native reader cannot load a `DiscMaterial`
without its HDF5 file, so its report is generated from the in-memory model.

## Report format

Line oriented, `key|field…`:

```text
format|csxcad-native-report|1
library|CSXCAD -- Version: v0.7.0
fixture|07_primitives
read_back|1
coord_system|0
grid_delta_unit|1
grid|<axis>|<qty>|<line>...
background|<epsilon>|<mue>|<kappa>|<sigma>
parameter|<index>|<name>|<Const|Linear>|<value>|<sweep or na>|<min>|<max>|<step>
parameter_steps|<index>|<value>...          # native LinearParameter::IncreaseStep walk
property_count|<n>
primitive_count|<n>
property|<index>|<xml type>|<name>|<visible>|<fill rgba>|<edge rgba>|<coord input>|<primitive count>
attr|<property>|<name>|<value>
detail|<property>|<field>[|<index>…]|<value>
primitive|<property>|<index>|<type>|<priority>|<coord system>|<bounds valid>|<bounds coord system>|<dimension>|<x0>|<x1>|<y0>|<y1>|<z0>|<z1>
matrix|<property>|<index>|<16 row-major values>
query|<index>|<x>|<y>|<z>|<property index>|<primitive index>    # -1/-1 when nothing contains the point
expression|<expression>|<native code>|<value>
tscenario|<name>|<16 row-major values>
object_area|<x0>|<x1>|<y0>|<y1>|<z0>|<z1>
geometry_valid|<0|1>
update_warning|<text>
read_warning|<text>
```

`ReadFromXML` values are collected through the native C++ API
(`GetEpsilon`, `GetBoundBox`, `GetPropertyByCoordPriority`,
`GetObjectArea`, `isGeometryValid`, …), not by re-parsing the XML.

## Native quirks and defects recorded by the corpus

These are native behaviours the Rust implementation mirrors or reports
explicitly. They must not be "fixed" silently on either side.

1. **`<User-Defined>` write/read asymmetry.** Native primitives write
   `<User-Defined>` (`CSPrimUserDefined::PrimTypeName`) but
   `ContinuousStructure::ReadFromXML` only dispatches `<UserDefined>`. A native
   reload therefore drops the primitive. Rust reads both spellings; the
   differential test accounts for the missing native entry.
2. **`CoordSystem` attribute collision on `User-Defined`.** The base class
   writes the mesh coordinate system into `CoordSystem`, then
   `CSPrimUserDefined::Write2XML` overwrites it with its *function* coordinate
   system (Cartesian=0). The report (native re-read) and the Rust parser both
   see the overwritten value; the in-memory native value differs.
3. **Float probe times.** `CSPropProbeBox` stores `startTime`/`stopTime` in
   `float` and writes them with `SetDoubleAttribute`; native reload shows the
   double in the file. The Rust model keeps the parsed double and mirrors the
   float setter (`set_start_time(f32)`).
4. **Transform argument precision.** `CSTransform::Write2XML` writes each
   argument with `ParameterScalar::GetValueString()` (6 significant digits), so
   a `Rotate_Origin` written from π/2 reloads as `1.5708` and the matrix changes
   by ~3.7e-6. The corpus stores the post-reload matrix; the Rust parser uses the
   argument in the file, so both agree.
5. **Pre-multiply flag not serialized.** `CSTransform` writes only the operation
   list; the `m_PostMultiply` flag is lost on save. Native (and Rust) therefore
   reload a pre-multiplied transform as post-multiplied. In-memory pre/post
   behaviour is covered by the `tscenario` lines.
6. **Uninitialised object area.** `ContinuousStructure::ObjArea` is never
   initialised; `GetObjectArea()` on an empty structure returns garbage doubles
   (visible in `01_empty.report`). Rust returns `None`; the test does not compare
   the empty case.
7. **`GetDimension() == -1` for `User-Defined`.** Native cannot compute its
   bounding box, so the dimension stays invalid; the Rust model reports 3-D.
   The differential test skips that single value.
8. **`CSPropUnknown::ReadFromXML` writes but does not read the `Property`
   attribute.** The corpus records the written value; Rust *does* read it (a
   superiority kept deliberately, since it preserves the user's tag).
9. **Isotropic component pinning.** `CSPropMaterial::GetValue` forces
   component 0 for isotropic materials, including Debye/Lorentz pole values.
   The Rust model stores raw components and consumers (e.g. the openEMS Debye
   operator) apply the same pinning; the differential test compares through the
   pinning.
10. **`Scale` with two arguments** is rejected by both sides; four or more
    arguments are accepted with a warning and the first three are used (native
    `CSTransform::Scale(std::string)`), which Rust now mirrors. `Translate`,
    `Rotate_Origin` and `Matrix` likewise ignore trailing arguments.
11. **`LinearParameter::IncreaseStep()` with step == 0** returns `true` forever
    in native (infinite loop). Rust returns `false`; the difference is recorded
    and asserted.
12. **`Discrete-Material` and `ChargeBox`** are legacy read aliases
    (`ContinuousStructure.cpp`); the canonical writer tags are `DiscMaterial`
    and `ProbeBox`.
13. **`DispersiveMaterial`** has no native read dispatch entry even though the
    class exists (its constructor is protected and only `DebyeMaterial` /
    `LorentzMaterial` are concrete), so the corpus covers the two concrete
    types. Rust still accepts the tag.

## Mesh corpus

`vtk_mesh_expectations.py` writes the files and the expectations. The STL/PLY
readers in native CSXCAD are VTK's (`vtkSTLReader` with `SetMerging(1)`,
`vtkPLYReader`), so VTK's point/face arrays are the reference:

* `sphere_native_stl.stl` / `sphere_native_ply.ply` – the native CSXCAD Python
  test meshes (binary STL, binary little-endian PLY, 50 points / 96 triangles),
* `cube_ascii.stl`, `cube_binary.stl` – VTK-written cube (8 merged points,
  12 triangles, triangle order/winding preserved),
* `quads_ascii.ply`, `quads_binary.ply` – quad polygons (4 vertices per face),
* `two_solids_ascii.stl` – two ASCII solids sharing vertices (VTK merges across
  solids and keeps the face order),
* malformed: `truncated_binary.stl`, `no_solid_ascii.stl` (VTK parse error),
  `unknown_format.ply` (unsupported PLY format), `points_only.ply` (VTK reads
  points but zero polygon cells; native rejects "empty" meshes).

`.expected` files store `format|…`, `vertices|n`, `vertex|x|y|z`,
`faces|n`, `face|i0,i1,…` in VTK order, or `error|…` for malformed input.
A PLY that ends before `end_header` makes VTK's PLY reader segfault, so that
case is deliberately not part of the corpus.

Note: native `CSPrimPolyhedron` stores vertices as `float`
(`struct vertex { float coord[3]; }`), so the Rust reader returns `[f32; 3]`;
the test widens them to compare with VTK's doubles. ASCII STL parsing keeps
double precision in Rust (VTK truncates to float); the ASCII fixtures use
exactly representable values so the comparison is exact.

## Performance baseline (RM-036)

Command:

```bash
cargo test -p csxcad-core --test perf_baseline --release -- --nocapture
cargo test -p csxcad-core --test perf_baseline -- --nocapture   # debug
```

Document: 500 grid lines per axis, 250 properties (Material, Metal,
one-pole Debye, ProbeBox), 4 primitives each (box, sphere, cylinder,
expression box) = 1000 primitives, 2 parameters; XML 158 337 bytes.
Best of three runs, measured 2026-10-27 on:

* Windows 11 (10.0.26100), x86_64, Intel-based workstation (WSL2 host),
* Rust 1.9x stable, `csxcad-core` as built by this workspace.

| Phase | release | debug |
| --- | --- | --- |
| build model | 1.38 ms | 8.95 ms |
| serialize XML | 3.03 ms (49.8 MB/s) | 10.20 ms (14.8 MB/s) |
| parse XML | 4.04 ms (37.3 MB/s) | 16.03 ms (9.4 MB/s) |
| bounding boxes (1000) | 1.50 ms (1.50 µs each) | 8.08 ms (8.08 µs each) |
| containment queries (100, all primitives) | 183.2 ms (1.83 ms each) | 820.4 ms (8.20 ms each) |

Measured limitation: containment queries dominate because every query
re-evaluates primitive expressions through fparser; native `ParameterScalar`
caches the evaluated value until the parameter set changes. A query over 1000
primitives (250 of them expression-based) therefore costs ~1.8 ms in release.
This is recorded as a baseline, not a target; caching belongs to a separate
performance task.
