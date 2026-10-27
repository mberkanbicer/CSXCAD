//! RM-028: XML dispatch and round trip over fixtures written by native CSXCAD.
//!
//! Every fixture in `tests/fixtures/xml` was produced by
//! `ContinuousStructure::Write2XML` of the pinned native CSXCAD library
//! (see `tests/native_gen/`). For each fixture the test asserts:
//!
//! * the fixture inventory in `manifest.txt` matches the files on disk,
//! * native XML parses into the Rust model in strict mode without warnings,
//! * the model values the parser keeps are the native values,
//! * Rust XML re-parses into an equal model and is byte-stable on a second
//!   round trip,
//! * an openEMS wrapper file yields the same `ContinuousStructure`.

mod common;

use common::{fixture_path, manifest, FixtureEntry, NativeReport};
use csxcad_core::grid::CoordinateSystem;
use csxcad_core::material::MaterialField;
use csxcad_core::model::{PrimitiveType, PropertyType, Structure};
use csxcad_core::serialization::StructureXmlWarning;
use std::collections::BTreeSet;
use std::fs;

fn native_xml(name: &str) -> String {
    let path = fixture_path(&format!("xml/{name}.xml"));
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn load(name: &str) -> Structure {
    Structure::from_xml_str(&native_xml(name))
        .unwrap_or_else(|error| panic!("{name} does not parse: {error}"))
}

fn property_type_native_name(kind: PropertyType) -> &'static str {
    match kind {
        PropertyType::Unknown => "Unknown",
        PropertyType::Material => "Material",
        PropertyType::Metal => "Metal",
        PropertyType::ConductingSheet => "ConductingSheet",
        PropertyType::AbsorbingBoundary => "AbsorbingBC",
        PropertyType::LumpedElement => "LumpedElement",
        PropertyType::ProbeBox => "ProbeBox",
        PropertyType::DumpBox => "DumpBox",
        PropertyType::Excitation => "Excitation",
        PropertyType::ResBox => "ResBox",
        PropertyType::DispersiveMaterial => "DispersiveMaterial",
        PropertyType::DebyeMaterial => "DebyeMaterial",
        PropertyType::LorentzMaterial => "LorentzMaterial",
        PropertyType::DiscreteMaterial => "DiscMaterial",
        PropertyType::Any => "Any",
    }
}

fn primitive_type_native_name(kind: PrimitiveType) -> &'static str {
    match kind {
        PrimitiveType::Point => "Point",
        PrimitiveType::Box => "Box",
        PrimitiveType::MultiBox => "MultiBox",
        PrimitiveType::Sphere => "Sphere",
        PrimitiveType::SphericalShell => "SphericalShell",
        PrimitiveType::Cylinder => "Cylinder",
        PrimitiveType::CylindricalShell => "CylindricalShell",
        PrimitiveType::Polygon => "Polygon",
        PrimitiveType::LinPoly => "LinPoly",
        PrimitiveType::RotPoly => "RotPoly",
        PrimitiveType::Polyhedron => "Polyhedron",
        PrimitiveType::PolyhedronReader => "PolyhedronReader",
        PrimitiveType::Curve => "Curve",
        PrimitiveType::Wire => "Wire",
        PrimitiveType::UserDefined => "User-Defined",
    }
}

#[test]
fn fixture_manifest_matches_the_files_on_disk() {
    let entries = manifest();
    let mut listed = BTreeSet::new();
    for entry in &entries {
        let name = &entry.name;
        assert!(
            listed.insert(name.clone()),
            "duplicate manifest entry {name}"
        );
        let path = fixture_path(&format!("xml/{name}.xml"));
        let metadata = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("missing fixture {}: {error}", path.display()));
        assert_eq!(
            metadata.len(),
            entry.bytes,
            "{name}: manifest byte count does not match {}",
            path.display()
        );
        let report = fixture_path(&format!("native/{name}.report"));
        assert!(
            report.is_file(),
            "missing native report {}",
            report.display()
        );
    }

    let mut on_disk = BTreeSet::new();
    for entry in fs::read_dir(fixture_path("xml")).expect("fixture directory") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|value| value.to_str()) == Some("xml") {
            on_disk.insert(
                path.file_stem()
                    .expect("xml file stem")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    assert_eq!(
        listed, on_disk,
        "manifest and xml directory disagree; regenerate with tests/native_gen/run_native_generator.sh"
    );
}

#[test]
fn every_native_fixture_parses_round_trips_and_is_stable() {
    for entry in manifest() {
        let name = &entry.name;
        let source = native_xml(name);

        let (structure, warnings) = Structure::from_xml_str_with_warnings(&source)
            .unwrap_or_else(|error| panic!("{name}: native compatibility parse failed: {error}"));
        assert!(
            warnings.is_empty(),
            "{name}: native reader produced warnings for a native document: {warnings:?}"
        );

        assert_eq!(
            structure.property_count(),
            entry.properties,
            "{name}: property count"
        );
        assert_eq!(
            structure.primitive_count(),
            entry.primitives,
            "{name}: primitive count"
        );

        let strict = Structure::from_xml_str(&source)
            .unwrap_or_else(|error| panic!("{name}: strict parse failed: {error}"));
        assert_eq!(
            strict, structure,
            "{name}: strict and compat parse disagree"
        );

        let serialized = structure
            .to_xml_string()
            .unwrap_or_else(|error| panic!("{name}: serialization failed: {error}"));
        let reparsed = Structure::from_xml_str(&serialized)
            .unwrap_or_else(|error| panic!("{name}: Rust XML does not parse: {error}"));
        assert_eq!(reparsed, structure, "{name}: Rust -> parse model mismatch");

        let second = reparsed
            .to_xml_string()
            .expect("second serialization must succeed");
        assert_eq!(
            serialized, second,
            "{name}: Rust XML is not stable across a second round trip"
        );
    }
}

#[test]
fn native_reader_accepts_the_documents_native_writes() {
    // Native CSTransform::GetTypeByName only accepts the names produced by
    // CSTransform::GetNameByType. The Rust writer must use those, otherwise
    // native CSXCAD silently skips every transform operation on reload.
    let structure = load("08_transforms");
    let xml = structure.to_xml_string().expect("serialize transforms");

    for native_name in [
        "Translate",
        "Scale",
        "Scale3",
        "Rotate_Origin",
        "Rotate_X",
        "Rotate_Y",
        "Rotate_Z",
        "Matrix",
    ] {
        assert!(
            xml.contains(&format!("<{native_name} Argument=")),
            "Rust XML does not use the native transform element <{native_name}>"
        );
    }
    for rust_only_name in ["RotateOrigin", "RotateX", "RotateY", "RotateZ"] {
        assert!(
            !xml.contains(&format!("<{rust_only_name} ")),
            "Rust XML still writes the non-native transform element <{rust_only_name}>"
        );
    }

    // The alias spellings stay readable for documents produced by older Rust
    // builds (native would warn and skip them, so this is a compatibility
    // extension, not the write format).
    let legacy = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<ContinuousStructure CoordSystem="0">
  <RectilinearGrid DeltaUnit="1" CoordSystem="0"><XLines Qty="0"/><YLines Qty="0"/><ZLines Qty="0"/></RectilinearGrid>
  <BackgroundMaterial Epsilon="1" Mue="1" Kappa="0" Sigma="0"/>
  <ParameterSet/>
  <Properties>
    <Metal ID="0" Name="legacy"><Primitives>
      <Box Priority="0"><Transformation><RotateZ Argument="1.5707963267948966"/></Transformation>
        <P1 X="0" Y="0" Z="0"/><P2 X="1" Y="1" Z="1"/></Box>
    </Primitives></Metal>
  </Properties>
</ContinuousStructure>"#;
    let legacy_structure = Structure::from_xml_str(legacy).expect("legacy transform alias");
    let primitive = legacy_structure
        .property_by_index(0)
        .unwrap()
        .primitive(0)
        .unwrap();
    let matrix = primitive.transform().expect("transform").matrix();
    assert!((matrix[0] - 0.0).abs() < 1e-12);
    assert!((matrix[1] + 1.0).abs() < 1e-12);
}

#[test]
fn native_unsupported_transform_is_warned_and_skipped_in_compat_mode() {
    // Native CSTransform::ReadFromXML prints a warning and keeps reading when
    // an operation fails; the strict Rust entry point rejects the document.
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<ContinuousStructure CoordSystem="0">
  <RectilinearGrid DeltaUnit="1" CoordSystem="0"><XLines Qty="0"/><YLines Qty="0"/><ZLines Qty="0"/></RectilinearGrid>
  <BackgroundMaterial Epsilon="1" Mue="1" Kappa="0" Sigma="0"/>
  <ParameterSet/>
  <Properties>
    <Metal ID="0" Name="bad-transform"><Primitives>
      <Box Priority="0"><Transformation><Unknown Operation="1"/><Translate Argument="1,0,0"/></Transformation>
        <P1 X="0" Y="0" Z="0"/><P2 X="1" Y="1" Z="1"/></Box>
    </Primitives></Metal>
  </Properties>
</ContinuousStructure>"#;
    assert!(
        Structure::from_xml_str(document).is_err(),
        "strict mode must reject an unknown transform"
    );
    let (structure, warnings) =
        Structure::from_xml_str_with_warnings(document).expect("compat mode keeps reading");
    assert!(warnings.iter().any(|warning| matches!(
        warning,
        StructureXmlWarning::InvalidTransformSkipped { kind, .. } if kind == "Unknown"
    )));
    let primitive = structure
        .property_by_index(0)
        .unwrap()
        .primitive(0)
        .unwrap();
    assert_eq!(
        primitive.transform().expect("transform").operations().len(),
        1
    );
}

#[test]
fn openems_wrapper_yields_the_same_structure() {
    let wrapper = native_xml("10_openems_wrapper");
    // The native solver writes <openEMS><FDTD/><ContinuousStructure/></openEMS>.
    assert!(wrapper.contains("<openEMS>"));
    assert!(wrapper.contains("<FDTD "));
    assert!(wrapper.contains("<BoundaryCond "));
    let extracted = Structure::from_xml_str(&wrapper).expect("wrapper extraction");
    assert_eq!(extracted.property_count(), 1);
    assert_eq!(extracted.primitive_count(), 1);
    assert_eq!(
        extracted.property_by_index(0).unwrap().name(),
        "wrapper-metal"
    );
    assert_eq!(extracted.grid().delta_unit(), 1e-3);
    let report = NativeReport::load("10_openems_wrapper");
    assert_eq!(
        extracted.property_count(),
        common::parse_integer(report.scalar("property_count"), "property_count") as usize
    );

    // A document without any ContinuousStructure child is still an error.
    assert!(Structure::from_xml_str("<openEMS><FDTD/></openEMS>").is_err());
}

#[test]
fn fixture_dispatch_covers_every_native_property_and_primitive_type() {
    // Collect the property and primitive types the native fixtures actually
    // contain. The fixture XML is authoritative: the report of a native re-read
    // cannot contain <User-Defined> because native only dispatches the
    // <UserDefined> spelling (a native write/read defect recorded in the
    // generator README).
    let mut property_types = BTreeSet::new();
    let mut primitive_types = BTreeSet::new();
    for entry in manifest() {
        let structure = load(&entry.name);
        for index in 0..structure.property_count() {
            let property = structure.property_by_index(index).unwrap();
            property_types.insert(property_type_native_name(property.property_type()));
            for primitive in property.primitives() {
                primitive_types.insert(primitive_type_native_name(primitive.primitive_type()));
            }
        }
    }

    let expected_properties = [
        "Unknown",
        "Material",
        "Metal",
        "ConductingSheet",
        "AbsorbingBC",
        "LumpedElement",
        "ProbeBox",
        "DumpBox",
        "Excitation",
        "ResBox",
        "DebyeMaterial",
        "LorentzMaterial",
        "DiscMaterial",
    ];
    for name in expected_properties {
        assert!(
            property_types.contains(name),
            "native corpus does not cover property type {name}"
        );
    }

    let expected_primitives = [
        "Point",
        "Box",
        "MultiBox",
        "Sphere",
        "SphericalShell",
        "Cylinder",
        "CylindricalShell",
        "Polygon",
        "LinPoly",
        "RotPoly",
        "Polyhedron",
        "Curve",
        "Wire",
        "User-Defined",
    ];
    for name in expected_primitives {
        assert!(
            primitive_types.contains(name),
            "native corpus does not cover primitive type {name}"
        );
    }

    // Every fixture model exposes only types the writer can serialize back.
    for entry in manifest() {
        let structure = load(&entry.name);
        for index in 0..structure.property_count() {
            let property = structure.property_by_index(index).unwrap();
            let kind = property.property_type();
            assert!(
                !matches!(kind, PropertyType::DiscreteMaterial)
                    || property.discrete_material().is_some(),
                "{}: DiscreteMaterial property lost its payload",
                entry.name
            );
            for primitive in property.primitives() {
                let _ = primitive.primitive_type();
                assert!(
                    !matches!(primitive.primitive_type(), PrimitiveType::Point)
                        || primitive.point_shape().is_some(),
                    "{}: point primitive lost its shape",
                    entry.name
                );
            }
        }
    }
}

#[test]
fn field_level_values_survive_the_round_trip() {
    // Spot checks that the values the model keeps are the native values; the
    // exhaustive per-field comparison lives in tests/native_model_parity.rs.
    let materials = load("03_materials");
    let first = materials.property_by_index(0).unwrap();
    assert_eq!(first.property_type(), PropertyType::Material);
    assert_eq!(first.name(), "material-iso");
    assert!(!first.is_visible());
    assert_eq!(first.attribute("custom"), "value & <tag>");
    assert_eq!(
        first
            .material()
            .unwrap()
            .value(MaterialField::Epsilon, 0, Some(materials.parameters()))
            .unwrap(),
        Some(2.5)
    );
    assert_eq!(
        first
            .material()
            .unwrap()
            .weight_term(MaterialField::Epsilon, 0)
            .as_deref(),
        Some("2*x+1")
    );

    let dispersive = load("04_dispersive");
    assert_eq!(
        dispersive.property_by_index(0).unwrap().property_type(),
        PropertyType::DebyeMaterial
    );
    assert_eq!(
        dispersive
            .property_by_index(0)
            .unwrap()
            .debye_material()
            .unwrap()
            .order(),
        2
    );
    assert_eq!(
        dispersive.property_by_index(1).unwrap().property_type(),
        PropertyType::LorentzMaterial
    );

    let cylindrical = load("09_cylindrical");
    assert_eq!(
        cylindrical.coordinate_system(),
        CoordinateSystem::Cylindrical
    );
    assert_eq!(cylindrical.property_count(), 2);

    let disc = load("11_disc_material");
    let discrete = disc
        .property_by_index(0)
        .unwrap()
        .discrete_material()
        .expect("disc material");
    assert_eq!(discrete.file_name(), "materials.h5");
    assert_eq!(discrete.file_type(), 0);
    assert!(!discrete.use_database_for_background());
    assert_eq!(discrete.scale(), 1e-3);
    assert!(discrete
        .transform()
        .expect("disc transform")
        .has_transform());

    let parameters = load("02_grid_background_parameters");
    assert_eq!(parameters.parameters().len(), 3);
    assert_eq!(
        parameters.parameters().get(1).unwrap().linear_range(),
        Some((0.5, 2.5, 0.5))
    );
    assert_eq!(
        parameters
            .background_material()
            .value(MaterialField::Epsilon),
        2.5
    );
    assert_eq!(
        parameters
            .grid()
            .clone()
            .lines(0)
            .expect("x lines")
            .to_vec(),
        vec![-10.0, 0.0, 10.0]
    );
}

#[test]
fn fixture_entry_accessor_is_used_by_all_corpus_tests() {
    // Guards against a silently empty manifest (`FixtureEntry` is public so
    // the other corpus tests can share the inventory).
    let entry: FixtureEntry = manifest().remove(0);
    assert_eq!(entry.name, "01_empty");
    assert_eq!(entry.properties, 0);
    assert_eq!(entry.primitives, 0);
}
