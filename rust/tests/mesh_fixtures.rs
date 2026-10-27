//! RM-035: STL/PLY/XML mesh fixture corpus with VTK-verified expectations.
//!
//! `tests/fixtures/mesh` is produced by `tests/native_gen/vtk_mesh_expectations.py`.
//! It contains the two native CSXCAD test meshes (binary STL and binary little
//! endian PLY from `CSXCAD/python/tests`), VTK-written ASCII/binary STL and PLY
//! files (triangles and quads), a hand written multi-solid ASCII STL and
//! malformed files. For every valid mesh the `.expected` file stores the point
//! and polygon arrays that VTK's STL/PLY readers return - the same readers
//! native `CSPrimPolyhedronReader::ReadFile()` uses - in VTK order and winding.
//!
//! The test asserts that the Rust reader produces the same points and faces for
//! every fixture and that every malformed fixture is rejected instead of being
//! silently accepted.

mod common;

use csxcad_core::model::{PolyhedronFileType, PolyhedronPrimitive, Primitive, Structure};
use std::fs;
use std::path::{Path, PathBuf};

struct Expectation {
    file_format: String,
    points: Vec<[f64; 3]>,
    faces: Vec<Vec<i32>>,
    error: Option<String>,
}

fn mesh_dir() -> PathBuf {
    common::fixture_path("mesh")
}

fn parse_expectation(path: &Path) -> Expectation {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut expectation = Expectation {
        file_format: String::new(),
        points: Vec::new(),
        faces: Vec::new(),
        error: None,
    };
    let mut declared_vertices = None;
    let mut declared_faces = None;
    for line in text.lines() {
        let fields: Vec<&str> = line.split('|').collect();
        match fields[0] {
            "format" => expectation.file_format = fields[1].to_owned(),
            "vertex" => expectation.points.push([
                common::parse_number(fields[1], "vertex x"),
                common::parse_number(fields[2], "vertex y"),
                common::parse_number(fields[3], "vertex z"),
            ]),
            "face" => expectation.faces.push(
                fields[1]
                    .split(',')
                    .map(|index| common::parse_integer(index, "face index") as i32)
                    .collect(),
            ),
            "error" => expectation.error = Some(fields[1].to_owned()),
            "vertices" => declared_vertices = Some(fields[1].parse::<usize>().expect("count")),
            "faces" => declared_faces = Some(fields[1].parse::<usize>().expect("count")),
            other => panic!("{}: unknown expectation key {other}", path.display()),
        }
    }
    if expectation.error.is_none() {
        assert_eq!(
            declared_vertices,
            Some(expectation.points.len()),
            "{}: declared vertex count",
            path.display()
        );
        assert_eq!(
            declared_faces,
            Some(expectation.faces.len()),
            "{}: declared face count",
            path.display()
        );
    }
    expectation
}

fn mesh_files() -> Vec<(String, PathBuf)> {
    let mut files: Vec<(String, PathBuf)> = fs::read_dir(mesh_dir())
        .expect("mesh fixture directory")
        .filter_map(|entry| {
            let path = entry.expect("entry").path();
            (path.extension().and_then(|value| value.to_str()) == Some("expected")).then(|| {
                (
                    path.file_stem().unwrap().to_string_lossy().into_owned(),
                    path,
                )
            })
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "mesh fixture corpus is empty");
    files
}

fn file_type_for(source: &Path) -> PolyhedronFileType {
    match source.extension().and_then(|value| value.to_str()) {
        Some("stl") => PolyhedronFileType::Stl,
        Some("ply") => PolyhedronFileType::Ply,
        other => panic!("unexpected mesh extension {other:?}"),
    }
}

fn read_mesh(
    source: &Path,
) -> Result<PolyhedronPrimitive, csxcad_core::model::PolyhedronReaderError> {
    let mut primitive = Primitive::polyhedron_reader();
    let reader = primitive
        .polyhedron_reader_shape_mut()
        .expect("polyhedron reader");
    reader.set_filename(source.to_string_lossy());
    reader.set_file_type(file_type_for(source));
    reader.read_file()?;
    Ok(reader.polyhedron().clone())
}

fn vertices(shape: &PolyhedronPrimitive) -> Vec<[f64; 3]> {
    // Native CSPrimPolyhedron stores vertices as `float` (CSPrimPolyhedron.h
    // `struct vertex { float coord[3]; }`), so the reader returns f32 values;
    // widen them for the comparison with VTK's double points.
    (0..shape.vertex_count())
        .map(|index| {
            let vertex = shape.vertex(index).expect("vertex");
            [
                f64::from(vertex[0]),
                f64::from(vertex[1]),
                f64::from(vertex[2]),
            ]
        })
        .collect()
}

fn faces(shape: &PolyhedronPrimitive) -> Vec<Vec<i32>> {
    (0..shape.face_count())
        .map(|index| shape.face(index).expect("face").to_vec())
        .collect()
}

#[test]
fn vtk_expectations_match_every_valid_mesh_fixture() {
    let mut valid = 0;
    for (name, expectation_path) in mesh_files() {
        let expectation = parse_expectation(&expectation_path);
        if expectation.error.is_some() {
            continue;
        }
        let source = {
            // The expectation file is `<base>.expected`; the format field names
            // the mesh extension.
            let extension = expectation
                .file_format
                .split('|')
                .next()
                .expect("format kind");
            assert!(
                extension == "stl" || extension == "ply",
                "{name}: unexpected format {:?}",
                expectation.file_format
            );
            mesh_dir().join(format!("{name}.{extension}"))
        };
        assert!(source.is_file(), "missing mesh {}", source.display());

        let shape =
            read_mesh(&source).unwrap_or_else(|error| panic!("{name}: reader failed: {error}"));
        assert_eq!(
            vertices(&shape),
            expectation.points,
            "{name}: VTK point order differs"
        );
        assert_eq!(
            faces(&shape),
            expectation.faces,
            "{name}: VTK face order/winding differs"
        );
        // Native keeps one vertex per VTK point and one face per VTK polygon.
        assert_eq!(shape.vertex_count(), expectation.points.len());
        assert_eq!(shape.face_count(), expectation.faces.len());
        valid += 1;
    }
    assert!(
        valid >= 7,
        "expected the full valid mesh corpus, got {valid}"
    );
}

#[test]
fn point_and_face_ordering_is_preserved_for_known_shapes() {
    // Cube: VTK writes 36 vertices; merging (native SetMerging(1)) collapses
    // them to the 8 shared corners in first-seen order, and the 12 triangles
    // keep their order and winding.
    let shape = read_mesh(&mesh_dir().join("cube_binary.stl")).expect("cube");
    assert_eq!(
        vertices(&shape),
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ]
    );
    assert_eq!(shape.face_count(), 12);
    assert_eq!(shape.face(0), Some(&[0, 1, 2][..]));

    // Multi-solid ASCII STL: VTK merges the shared vertices of both solids and
    // keeps the second solid's faces pointing at the merged indices.
    let shape = read_mesh(&mesh_dir().join("two_solids_ascii.stl")).expect("two solids");
    assert_eq!(shape.vertex_count(), 4);
    assert_eq!(shape.face_count(), 2);
    assert_eq!(shape.face(0), Some(&[0, 1, 2][..]));
    assert_eq!(shape.face(1), Some(&[3, 1, 2][..]));

    // Quad PLY: four-vertex faces are preserved (native keeps the VTK polygon
    // arity instead of triangulating).
    let shape = read_mesh(&mesh_dir().join("quads_ascii.ply")).expect("quads");
    assert_eq!(shape.vertex_count(), 6);
    assert_eq!(shape.face(0), Some(&[0, 1, 4, 3][..]));
    assert_eq!(shape.face(1), Some(&[1, 2, 5, 4][..]));

    // The native binary PLY fixture (50 vertices / 96 triangles) keeps VTK's
    // point order; index 0 is the first vertex in the file.
    let shape = read_mesh(&mesh_dir().join("sphere_native_ply.ply")).expect("sphere ply");
    assert_eq!(shape.vertex_count(), 50);
    assert_eq!(shape.face_count(), 96);
    assert_eq!(shape.vertex(0), Some([0.0, 0.0, 0.5]));
    assert_eq!(shape.face(0), Some(&[2, 8, 0][..]));
}

#[test]
fn malformed_mesh_fixtures_are_rejected() {
    for (name, expectation_path) in mesh_files() {
        let expectation = parse_expectation(&expectation_path);
        if expectation.error.is_none() {
            continue;
        }
        let source = ["stl", "ply"]
            .iter()
            .map(|candidate| mesh_dir().join(format!("{name}.{candidate}")))
            .find(|path| path.is_file())
            .unwrap_or_else(|| panic!("missing malformed fixture {name}"));
        let result = read_mesh(&source);
        assert!(
            result.is_err(),
            "{name}: malformed mesh must be rejected, got {result:?}"
        );
    }
}

#[test]
fn polyhedron_reader_xml_round_trip_uses_the_stl_corpus() {
    // The reader primitive is what CSXCAD XML carries; build one from the
    // fixture, serialize, reload and compare the geometry to the VTK result.
    let source = mesh_dir().join("sphere_native_stl.stl");
    let mut primitive = Primitive::polyhedron_reader();
    {
        let reader = primitive.polyhedron_reader_shape_mut().expect("reader");
        reader.set_filename(source.to_string_lossy());
        reader.set_file_type(PolyhedronFileType::Stl);
        reader.read_file().expect("read native STL");
    }
    let expected = {
        let reader = primitive.polyhedron_reader_shape().expect("reader");
        vertices(reader.polyhedron())
    };

    let mut structure = Structure::new();
    let property_id = structure.add_property(csxcad_core::model::Property::new(
        "stl",
        csxcad_core::model::PropertyType::Material,
    ));
    structure
        .add_primitive(property_id, primitive)
        .expect("add");
    let xml = structure.to_xml_string().expect("serialize");
    assert!(xml.contains("<PolyhedronReader"), "{xml}");
    assert!(xml.contains("FileType=\"STL\""), "{xml}");
    let reloaded = Structure::from_xml_str(&xml).expect("reload");
    let reader = reloaded
        .property_by_index(0)
        .unwrap()
        .primitive(0)
        .unwrap()
        .polyhedron_reader_shape()
        .expect("reader");
    assert_eq!(vertices(reader.polyhedron()), expected);
    assert_eq!(reader.polyhedron().face_count(), 96);
}

#[test]
fn unknown_file_types_are_rejected() {
    let mut primitive = Primitive::polyhedron_reader();
    let reader = primitive.polyhedron_reader_shape_mut().expect("reader");
    reader.set_filename(mesh_dir().join("cube_ascii.stl").to_string_lossy());
    reader.set_file_type(PolyhedronFileType::Unknown);
    assert_eq!(
        reader.read_file(),
        Err(csxcad_core::model::PolyhedronReaderError::UnsupportedFileType)
    );
}
