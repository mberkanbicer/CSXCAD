# csxcad-core

This standalone Rust crate is the beginning of the CSXCAD migration. Its
default build uses only the local `fparser-rs` crate for parameter and transform
expressions. An optional HDF5 feature uses the system HDF5 C library directly
through a narrow FFI; it adds no Rust package dependency.

The current implementation covers ordered parameter sets and linear sweep
parameters, expression-backed scalars, rectilinear-grid line operations, and
4×4 transforms with composition and inverse transforms. Its initial owned
model layer supports base properties, primitive transfer, parameterized points,
boxes, multi-box unions, spheres, finite cylinders, curves, and wires, plus
isotropic/anisotropic material terms and coordinate weight functions.
Cartesian/cylindrical conversion is implemented for these primitive geometry
operations. XML read/write currently covers parameters, grids, base
Unknown/Material/Metal/ConductingSheet/AbsorbingBC/LumpedElement/ProbeBox/
DumpBox/Excitation/ResBox/DispersiveMaterial/DebyeMaterial/LorentzMaterial/
DiscMaterial properties, and
Point/Box/MultiBox/Sphere/SphericalShell/Cylinder/CylindricalShell/Curve/Wire/
Polygon/LinPoly/RotPoly/Polyhedron/PolyhedronReader/User-Defined primitives.
PolyhedronReader imports standard ASCII/binary STL and ASCII/little-endian or
big-endian binary PLY files without extra dependencies. VTK-specific format
extensions are not part of the native reader, which selects STL or PLY only;
ASCII STL validates complete facets while matching VTK's tolerance for normal,
structural-line, and trailing vertex tokens; PLY retains face lists from zero
through three indices as VTK does. Binary STL imports use complete available
facet records rather than the declared triangle count, matching VTK; incomplete
trailing records are ignored.
Polyhedron facet acceptance models CGAL's edge-incidence, vertex-fan, and
orientation constraints, but does not yet reproduce full CGAL surface-tree
behavior.
Other edge-case parity with VTK readers, property/primitive families, and
broader native differential validation remain outstanding. Property IDs follow
native list-index behavior. `Property::get_copy()` copies property data without
primitives, matching the native default; `get_copy_with_primitives()` also
clones its primitives. Adding either copy to a `Structure` assigns fresh
structure-owned handles. Structure queries filter properties and primitives by
property type, support combined type masks, and optionally sort primitives by
descending priority. `primitives_by_type()` matches the native
`GetPrimitivesByType` stub: it emits the native diagnostic and returns no
primitives.
Bounding-box queries use the structure coordinate system, match intersecting
primitives, and conservatively include transformed or unbounded geometry
whose overlap cannot be determined.
Zero-height cylinders and cylindrical shells retain the native point-sized
bounds and axis-point containment behavior.
`insert_edges_to_grid()` inserts finite primitive bounds on a selected mesh
axis and excludes unbounded User-Defined geometry.
`Structure::clear()` resets parameters, geometry, grid lines, and background
material while keeping issued IDs from being reused.
`validate_geometry_prerequisites()` checks the native structural gates for a
simulation setup: properties, primitives, minimum grid-line counts, and an
excitation property with primitives. It does not replace native `Update()`
validation for primitive or property contents.
`validate_property_expressions()` evaluates parameter-backed material,
dispersive-material, conducting-sheet, absorbing-boundary, lumped-element, and
excitation terms, returning the owning property ID with evaluation failures.
`validate_primitive_bounds()` evaluates registered primitive bounds and
reports the primitive ID on parameter or transform failures, including wire
radii and rotational-polygon angles. For User-Defined geometry it parses the
function and evaluates coordinate shifts, but does not evaluate the function
at a sample coordinate. These APIs do not replace all native shape-specific
`Update()` checks.
`validate_geometry()` runs the prerequisite, property-expression, and primitive-
bounds checks in sequence and preserves the source-specific error details.
`validate_updates()` checks property expressions first and primitive updates
second, collecting source-carrying diagnostics instead of stopping at the first
invalid item. Property and primitive scalar failures are reported individually
in native field order, including shape-specific short-circuit behavior. Other
primitive failures still contribute the first source-carrying error. Exact
native diagnostic text is not yet reproduced.
Single- and multi-coordinate selection chooses the highest-priority containing
primitive and can mark it used; primitive ownership can be
queried by its structure ID, and unused primitives can be listed by property
type. Filtered structure bounds include finite geometry
and exclude unbounded User-Defined primitives.
The User-Defined primitive reader accepts both the native writer's
`User-Defined` tag and the `UserDefined` spelling used by the native C++ XML
dispatch; the Rust writer emits the native writer's spelling.
`Structure::from_xml_str()` remains strict; `from_xml_str_with_warnings()`
provides a compatibility path that skips unsupported or malformed parameter
entries, skips unsupported property tags, retains recognized properties with
invalid specialized data as `Unknown` when their base metadata is valid, and
skips unsupported or invalid primitive nodes. It also warns when a property has
no primitive list or the `Properties` node is absent, and parses property
visibility with native integer semantics.
It reports recoveries and skipped nodes through warnings.
Scripting interfaces and integration with the native CSXCAD model are also not
implemented; this crate is not a replacement for CSXCAD.

Discrete-material XML preserves database file settings and coordinate
transforms. In-memory voxel-index/value lookup supports mesh coordinates,
coordinate conversion, inverse transforms, scale, native cell ordering, and
background/invalid-index fallback. With the `hdf5` feature enabled,
`DiscreteMaterialProperties::load_grid()` reads version-2 HDF5 datasets and
optional material attributes. The feature requires system HDF5 development
libraries discoverable through `pkg-config` (`hdf5-serial` or `hdf5`), or an
`HDF5_LIB_DIR` environment variable; default builds do not link HDF5.

Run its focused tests with:

```sh
cargo test --manifest-path CSXCAD/rust/Cargo.toml
cargo test --manifest-path CSXCAD/rust/Cargo.toml --features hdf5
```
