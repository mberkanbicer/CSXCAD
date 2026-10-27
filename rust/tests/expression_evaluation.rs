//! RM-027: integration tests through CSXCAD expression evaluation.
//!
//! Parameters, coordinate expressions and transform expressions are evaluated
//! end to end: native-style XML is parsed, geometry is updated and bounding
//! boxes, containment and priority queries are checked against the expected
//! values. Failure paths are checked against the native CSXCAD
//! `ParameterScalar` error codes (see `ParameterError::native_code` and
//! `tests/fixtures/native/08_transforms.report`, which records the native codes
//! for the same expressions).

mod common;

use csxcad_core::model::{ModelError, Primitive, Property, PropertyType, Structure};
use csxcad_core::parameter::{ParameterError, ParameterScalar};
use csxcad_core::serialization::{StructureXmlError, StructureXmlWarning};
use csxcad_core::transform::TransformError;

const EXPRESSION_DOCUMENT: &str = include_str!("expression_evaluation_doc.xml");

fn document() -> Structure {
    Structure::from_xml_str(EXPRESSION_DOCUMENT).expect("expression document parses")
}

#[test]
fn coordinate_and_transform_expressions_drive_geometry() {
    let structure = document();
    let property = structure.property_by_index(0).unwrap();
    let expression_box = property.primitive(0).unwrap();
    let parameters = Some(structure.parameters());

    // Bounds are the evaluated corner coordinates (native GetBoundBox does not
    // apply the primitive transform).
    assert_eq!(
        expression_box.bounding_box(parameters, false).unwrap(),
        [1.0, 2.0, 0.0, 4.0, 0.0, 3.0]
    );
    assert_eq!(expression_box.dimension(parameters).unwrap(), 3);

    // The transform argument `offset` is evaluated against the structure
    // parameter set: origin moves by (+1, 0, 0).
    let transform = expression_box.transform().expect("translate transform");
    assert_eq!(transform.transform([0.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
    assert_eq!(
        transform.inverse_transform([1.0, 0.0, 0.0]).unwrap(),
        [0.0, 0.0, 0.0]
    );

    // Containment applies the inverse transform before the coordinate test.
    assert!(expression_box
        .is_inside([2.5, 1.0, 1.0], parameters)
        .unwrap());
    assert!(!expression_box
        .is_inside([1.5, 1.0, 1.0], parameters)
        .unwrap());
    assert!(!expression_box
        .is_inside([3.5, 1.0, 1.0], parameters)
        .unwrap());
}

#[test]
fn material_expressions_evaluate_against_parameters_and_coordinates() {
    let structure = document();
    let property = structure.property_by_index(0).unwrap();
    let material = property.material().expect("material");
    let parameters = Some(structure.parameters());

    assert_eq!(
        material
            .value(csxcad_core::material::MaterialField::Epsilon, 0, parameters)
            .unwrap(),
        Some(4.0)
    );
    // Weight functions see the native coordinate variables (x, y, z, rho, ...).
    assert_eq!(
        material
            .weighted_value(
                csxcad_core::material::MaterialField::Epsilon,
                0,
                parameters,
                [1.0, 2.0, 0.5],
                csxcad_core::grid::CoordinateSystem::Cartesian,
            )
            .unwrap(),
        Some(12.0)
    );
}

#[test]
fn parameter_changes_are_visible_on_the_next_evaluation() {
    let mut structure = document();
    structure
        .parameters_mut()
        .get_mut(1)
        .expect("width parameter")
        .set_value(3.0);
    let property = structure.property_by_index(0).unwrap();
    let expression_box = property.primitive(0).unwrap();
    assert_eq!(
        expression_box
            .bounding_box(Some(structure.parameters()), false)
            .unwrap(),
        [1.0, 3.0, 0.0, 6.0, 0.0, 3.0]
    );
    assert!(!expression_box
        .is_inside([4.5, 1.0, 1.0], Some(structure.parameters()))
        .unwrap());
}

#[test]
fn priority_queries_resolve_expressions_and_overlaps() {
    let mut structure = document();
    // (0.5, 0.5, 0.5) is inside both boxes; the higher priority wins.
    let winner = structure
        .property_by_coord_priority([0.5, 0.5, 0.5], PropertyType::Any, false)
        .unwrap()
        .expect("a property contains the point");
    let property = structure.property(winner.0).expect("winner property");
    let primitive = property
        .primitives()
        .iter()
        .find(|primitive| primitive.id() == Some(winner.1))
        .expect("winner primitive");
    assert_eq!(primitive.priority(), 4);
    // (2.5, 1.5, 1.5) is only inside the translated expression box.
    let winner = structure
        .property_by_coord_priority([2.5, 1.5, 1.5], PropertyType::Any, false)
        .unwrap()
        .expect("expression box contains the point");
    let property = structure.property(winner.0).expect("winner property");
    let primitive = property
        .primitives()
        .iter()
        .find(|primitive| primitive.id() == Some(winner.1))
        .expect("winner primitive");
    assert_eq!(primitive.priority(), 1);

    // Marking as used is observable on the model.
    structure
        .property_by_coord_priority([2.5, 1.5, 1.5], PropertyType::Any, true)
        .unwrap();
    let used = structure
        .all_primitives(false, PropertyType::Any)
        .into_iter()
        .filter(|primitive| primitive.is_used())
        .count();
    assert_eq!(used, 1);
}

fn native_code_of(error: &ModelError) -> Option<i32> {
    match error {
        ModelError::Parameter(error) => Some(error.native_code()),
        _ => None,
    }
}

fn add_expression_primitive(structure: &mut Structure, start: [&str; 3], stop: [&str; 3]) {
    let property_id = structure
        .properties_by_name("expression-material")
        .next()
        .and_then(Property::id)
        .expect("expression property");
    structure
        .add_primitive(
            property_id,
            Primitive::box_from_expressions(start, stop).expect("box expressions"),
        )
        .expect("add primitive");
}

#[test]
fn evaluation_errors_carry_the_native_csxcad_error_codes() {
    // Native codes are recorded in tests/fixtures/native/08_transforms.report
    // (expression|... lines): 1 division by zero, 2 sqrt, 3 log, 4 asin,
    // 100 syntax error, 102 missing ')', 111 unknown identifier (no
    // PSErrorCode2Msg entry).
    let cases: [(&str, i32); 7] = [
        ("1/0", 1),
        ("sqrt(-1)", 2),
        ("log(-1)", 3),
        ("asin(2)", 4),
        ("2*", 100),
        ("(1+2", 102),
        ("unknown_parameter+1", 111),
    ];
    for (expression, native_code) in cases {
        let scalar = ParameterScalar::from_expression(expression).expect("expression");
        let error = scalar
            .evaluate(None)
            .expect_err("expression must fail for both native and Rust");
        assert_eq!(
            error.native_code(),
            native_code,
            "expression {expression:?} native error code"
        );
    }

    // The empty expression matches native SetValue("") returning -1 without
    // touching the value.
    assert_eq!(ParameterError::EmptyExpression.native_code(), -1);

    // A failing coordinate expression surfaces through the geometry API with
    // the same code instead of being silently replaced by a value.
    let mut structure = document();
    add_expression_primitive(&mut structure, ["0", "0", "0"], ["2*", "1", "1"]);
    let primitive = structure
        .property_by_index(0)
        .unwrap()
        .primitive(2)
        .expect("expression primitive");
    let error = primitive
        .bounding_box(Some(structure.parameters()), false)
        .expect_err("syntax error must surface");
    assert_eq!(native_code_of(&error), Some(100));

    add_expression_primitive(&mut structure, ["0", "0", "0"], ["1/0", "1", "1"]);
    let primitive = structure
        .property_by_index(0)
        .unwrap()
        .primitive(3)
        .expect("expression primitive");
    let error = primitive
        .bounding_box(Some(structure.parameters()), false)
        .expect_err("division by zero must surface");
    assert_eq!(native_code_of(&error), Some(1));

    // Unknown identifiers have no native message; native still reports 111.
    add_expression_primitive(
        &mut structure,
        ["unknown_parameter", "0", "0"],
        ["1", "1", "1"],
    );
    let primitive = structure
        .property_by_index(0)
        .unwrap()
        .primitive(4)
        .expect("expression primitive");
    let error = primitive
        .bounding_box(Some(structure.parameters()), false)
        .expect_err("unknown identifier must surface");
    assert_eq!(native_code_of(&error), Some(111));
}

#[test]
fn update_validation_reports_every_bad_expression_in_native_order() {
    let mut structure = document();
    add_expression_primitive(&mut structure, ["0", "0", "0"], ["2*", "1", "1"]);
    add_expression_primitive(
        &mut structure,
        ["unknown_parameter", "0", "0"],
        ["1", "1", "1"],
    );
    let errors = structure.validate_updates();
    assert_eq!(errors.len(), 2, "expected one error per bad coordinate");
    // Errors reference the primitive that failed, in structure traversal order.
    let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert!(messages[0].contains("primitive"), "{messages:?}");
    assert!(messages[0].contains("Syntax error"), "{messages:?}");
    assert!(messages[1].contains("Unknown identifier"), "{messages:?}");
}

#[test]
fn malformed_transform_argument_matches_native_warn_and_skip() {
    let document = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<ContinuousStructure CoordSystem="0">
  <RectilinearGrid DeltaUnit="1" CoordSystem="0"><XLines Qty="0"/><YLines Qty="0"/><ZLines Qty="0"/></RectilinearGrid>
  <BackgroundMaterial Epsilon="1" Mue="1" Kappa="0" Sigma="0"/>
  <ParameterSet/>
  <Properties>
    <Metal ID="0" Name="bad-argument"><Primitives>
      <Box Priority="0">
        <Transformation><Translate Argument="1/0,0,0"/><Translate Argument="1,0,0"/></Transformation>
        <P1 X="0" Y="0" Z="0"/><P2 X="1" Y="1" Z="1"/>
      </Box>
    </Primitives></Metal>
  </Properties>
</ContinuousStructure>"#;
    let error = Structure::from_xml_str(document).expect_err("strict mode rejects the division");
    match error {
        StructureXmlError::Transform(TransformError::Parameter(parameter)) => {
            assert_eq!(parameter.native_code(), 1)
        }
        other => panic!("unexpected error {other:?}"),
    }

    // Native prints a warning and applies the following operations only.
    let (structure, warnings) =
        Structure::from_xml_str_with_warnings(document).expect("compat mode keeps reading");
    assert!(warnings.iter().any(|warning| matches!(
        warning,
        StructureXmlWarning::InvalidTransformSkipped { kind, .. } if kind == "Translate"
    )));
    let primitive = structure
        .property_by_index(0)
        .unwrap()
        .primitive(0)
        .unwrap();
    let matrix = primitive
        .transform()
        .expect("second translate applied")
        .matrix();
    assert_eq!(matrix[3], 1.0);
}

#[test]
fn normalised_expression_forms_round_trip_through_rust_xml() {
    // Expressions are written back verbatim (native parameterised write), so
    // the values survive a Rust -> native-format -> Rust round trip.
    let structure = document();
    let xml = structure.to_xml_string().expect("serialize");
    assert!(xml.contains("X=\"offset\" Y=\"0\" Z=\"0\"/>"), "{xml}");
    assert!(xml.contains("Argument=\"offset,0,0\""), "{xml}");
    assert!(xml.contains("Epsilon=\"width*2,1,1\""), "{xml}");
    let reparsed = Structure::from_xml_str(&xml).expect("reparse");
    assert_eq!(reparsed, structure);
}
