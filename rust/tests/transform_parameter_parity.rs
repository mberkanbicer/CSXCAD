//! RM-034: transform and parameter edge-case parity with native CSXCAD.
//!
//! The native reference values come from
//! `tests/fixtures/native/08_transforms.report`, which records the matrices the
//! native `CSTransform` produces for the same operation sequences
//! (`tscenario` lines) and the matrices of every primitive transform after a
//! native XML round trip (`matrix` lines). The parameter sweep sequence is
//! recorded for every fixture (`parameter_steps` lines, native `IncreaseStep`).

mod common;

use common::{assert_close, assert_close_slice, manifest, NativeReport};
use csxcad_core::parameter::{Parameter, ParameterError, ParameterSet};
use csxcad_core::transform::{Transform, TransformError, TransformType};

fn native_scenario(name: &str) -> Vec<f64> {
    let report = NativeReport::load("08_transforms");
    let line = report
        .all("tscenario")
        .iter()
        .find(|line| line[1] == name)
        .unwrap_or_else(|| panic!("missing native transform scenario {name}"));
    line[2..]
        .iter()
        .map(|value| common::parse_number(value, "scenario matrix element"))
        .collect()
}

fn matrix_values(transform: &Transform) -> Vec<f64> {
    transform.matrix().to_vec()
}

fn assert_scenario(transform: &Transform, name: &str) {
    assert_close_slice(
        &matrix_values(transform),
        &native_scenario(name),
        &format!("native transform scenario {name}"),
    );
}

#[test]
fn transform_operation_order_matches_native_pre_and_post_multiply() {
    // Native tscenario|post_translate_rotate
    let mut post = Transform::new();
    post.translate_expressions("1,0,0", None, true).unwrap();
    post.rotate_axis_expression(2, "1.5707963267948966", None, true)
        .unwrap();
    assert_scenario(&post, "post_translate_rotate");

    // Native tscenario|pre_translate_rotate: SetPreMultiply before the
    // rotation makes the rotation happen before the earlier translation.
    let mut pre = Transform::new();
    pre.translate_expressions("1,0,0", None, true).unwrap();
    pre.set_pre_multiply();
    pre.rotate_axis_expression(2, "1.5707963267948966", None, true)
        .unwrap();
    assert_scenario(&pre, "pre_translate_rotate");

    // The two orders really differ: post maps (1,0,0) to (0,2,0), pre to
    // (1,1,0).
    assert_close(post.transform([1.0, 0.0, 0.0])[1], 2.0, "post y");
    assert_close(pre.transform([1.0, 0.0, 0.0])[1], 1.0, "pre y");
}

#[test]
fn rotate_origin_axis_rotation_and_degree_mode_match_native() {
    let mut origin = Transform::new();
    origin
        .transform_by_string("Rotate_Origin", "0,1,1,1.5707963267948966", None, true)
        .unwrap();
    assert_scenario(&origin, "rotate_origin");

    let mut degrees = Transform::new();
    degrees.set_angle_degree();
    degrees
        .transform_by_string("Rotate_X", "90", None, true)
        .unwrap();
    assert_scenario(&degrees, "degree_rotate_x");

    // Native aliases (written by older Rust builds) stay readable.
    let mut alias = Transform::new();
    alias
        .transform_by_string("RotateOrigin", "0,1,1,1.5707963267948966", None, true)
        .unwrap();
    assert_scenario(&alias, "rotate_origin");
}

#[test]
fn matrix_and_scale3_operations_match_native() {
    let mut matrix = Transform::new();
    matrix
        .transform_by_string(
            "Matrix",
            "1,0,0,0,0,1,0,0,0,0,1,0,0.5,0.5,0.5,1",
            None,
            true,
        )
        .unwrap();
    matrix
        .transform_by_string("Scale", "1,2,3", None, true)
        .unwrap();
    assert_scenario(&matrix, "matrix_then_scale3");
    assert_eq!(matrix.operations().len(), 2);
    assert_eq!(matrix.operations()[0].kind, TransformType::Matrix);
    assert_eq!(matrix.operations()[1].kind, TransformType::Scale3);

    // Native writes Scale3 as <Scale3 Argument="..."/>, which must map back to
    // a three component scale (not a uniform scale).
    let mut scale3 = Transform::new();
    scale3
        .transform_by_string("Scale3", "2,3,4", None, true)
        .unwrap();
    let point = scale3.transform([1.0, 1.0, 1.0]);
    assert_eq!(point, [2.0, 3.0, 4.0]);
}

#[test]
fn invert_swaps_the_matrix_and_keeps_the_operation_log_like_native() {
    let mut inverted = Transform::new();
    inverted.translate_expressions("1,2,3", None, true).unwrap();
    inverted
        .rotate_axis_expression(2, "1.5707963267948966", None, true)
        .unwrap();
    inverted.invert().unwrap();
    assert_scenario(&inverted, "invert");

    // Round trip: a forward transform followed by its inverse restores the
    // point. Native swaps the stored matrices, so the log is unchanged.
    let mut forward = Transform::new();
    forward.translate_expressions("1,2,3", None, true).unwrap();
    forward
        .rotate_axis_expression(2, "1.5707963267948966", None, true)
        .unwrap();
    let point = [0.25, -0.5, 1.5];
    let transformed = forward.transform(point);
    let restored = forward.inverse_transform(transformed).unwrap();
    assert_close_slice(&restored, &point, "forward/inverse round trip");
}

#[test]
fn extra_transform_arguments_are_truncated_like_native() {
    // Native CSTransform::Translate warns about "larger than expected" and
    // applies the first three arguments.
    let mut translate = Transform::new();
    translate
        .transform_by_string("Translate", "1,2,3,4,5", None, true)
        .unwrap();
    assert_eq!(translate.transform([0.0, 0.0, 0.0]), [1.0, 2.0, 3.0]);
    assert_eq!(translate.operations()[0].arguments.len(), 3);

    let mut rotate = Transform::new();
    rotate
        .transform_by_string("Rotate_Origin", "0,1,0,1.5707963267948966,9", None, true)
        .unwrap();
    assert_eq!(rotate.operations()[0].arguments.len(), 4);

    let mut matrix = Transform::new();
    let mut arguments = vec!["1"; 18];
    arguments[0] = "2";
    matrix
        .transform_by_string("Matrix", &arguments.join(","), None, true)
        .unwrap();
    assert_eq!(matrix.operations()[0].arguments.len(), 16);
    assert_eq!(matrix.matrix()[0], 2.0);

    // Native Scale() with four arguments uses the first three.
    let mut scale = Transform::new();
    scale
        .transform_by_string("Scale", "1,2,3,4", None, true)
        .unwrap();
    assert_eq!(scale.transform([1.0, 1.0, 1.0]), [1.0, 2.0, 3.0]);
}

#[test]
fn rejected_transform_arguments_match_native_failure_modes() {
    let mut transform = Transform::new();
    assert_eq!(
        transform.transform_by_string("Translate", "1,2", None, true),
        Err(TransformError::InvalidArgumentCount {
            expected: "3".to_owned(),
            actual: 2
        })
    );
    assert_eq!(
        transform.transform_by_string("Scale", "2,3", None, true),
        Err(TransformError::InvalidArgumentCount {
            expected: "1 or 3".to_owned(),
            actual: 2
        })
    );
    assert_eq!(
        transform.transform_by_string("Rotate_Origin", "0,1,0", None, true),
        Err(TransformError::InvalidArgumentCount {
            expected: "4".to_owned(),
            actual: 3
        })
    );
    // A zero rotation axis is rejected (native warns and skips it).
    assert_eq!(
        transform.transform_by_string("Rotate_Origin", "0,0,0,1", None, true),
        Err(TransformError::ZeroRotationAxis)
    );
    assert_eq!(
        transform.rotate_axis(3, 1.0, true),
        Err(TransformError::InvalidDirection)
    );
    assert_eq!(
        transform.transform_by_string("Shear", "1", None, true),
        Err(TransformError::UnknownOperation("Shear".to_owned()))
    );
    // A singular matrix has no inverse; Rust fails loudly (native vtk returns a
    // zero matrix and silently produces garbage coordinates).
    transform.scale3([0.0, 1.0, 1.0], false);
    assert_eq!(
        transform.inverse_transform([0.0; 3]),
        Err(TransformError::SingularMatrix)
    );
    assert!(transform.operations().len() == 1);
}

#[test]
fn reading_native_transform_xml_recovers_the_recorded_matrices() {
    // Every matrix line in the native report is the transform of a primitive
    // after a native XML round trip; the Rust parser must reproduce it from the
    // same document.
    let report = NativeReport::load("08_transforms");
    let path = common::fixture_path("xml/08_transforms.xml");
    let xml = std::fs::read_to_string(&path).expect("transform fixture");
    let structure = csxcad_core::model::Structure::from_xml_str(&xml).expect("transform fixture");
    let mut compared = 0;
    for line in report.all("matrix") {
        let prop_index: usize = line[1].parse().expect("property index");
        let prim_index: usize = line[2].parse().expect("primitive index");
        let primitive = structure
            .property_by_index(prop_index)
            .expect("property")
            .primitive(prim_index)
            .expect("primitive");
        let transform = primitive.transform().expect("transform");
        let expected: Vec<f64> = line[3..]
            .iter()
            .map(|value| common::parse_number(value, "matrix element"))
            .collect();
        assert_close_slice(
            &matrix_values(transform),
            &expected,
            &format!("native transform matrix {prop_index}/{prim_index}"),
        );
        compared += 1;
    }
    assert!(compared >= 10, "expected the full native transform corpus");

    // The operation kinds native wrote are preserved: Scale3 and the
    // Rotate_* spellings, not a collapsed uniform scale.
    let property = structure.property_by_index(0).expect("property");
    let kinds: Vec<TransformType> = property
        .primitives()
        .iter()
        .filter_map(|primitive| primitive.transform())
        .map(|transform| transform.operations()[0].kind)
        .collect();
    assert!(kinds.contains(&TransformType::Scale3));
    assert!(kinds.contains(&TransformType::RotateOrigin));
    assert!(kinds.contains(&TransformType::RotateX));
    assert!(kinds.contains(&TransformType::RotateY));
    assert!(kinds.contains(&TransformType::RotateZ));
    assert!(kinds.contains(&TransformType::Matrix));
}

#[test]
fn parameter_sweep_matches_every_native_fixture() {
    for entry in manifest() {
        let report = NativeReport::load(&entry.name);
        let path = common::fixture_path(&format!("xml/{}.xml", entry.name));
        let xml = std::fs::read_to_string(&path).expect("fixture");
        let structure = csxcad_core::model::Structure::from_xml_str(&xml).expect("fixture");
        for line in report.all("parameter_steps") {
            let index: usize = line[1].parse().expect("parameter index");
            let expected: Vec<f64> = line[2..]
                .iter()
                .map(|value| common::parse_number(value, "step"))
                .collect();
            let mut parameter = structure
                .parameters()
                .get(index)
                .expect("parameter")
                .clone();
            parameter.init_sweep();
            let mut values = vec![parameter.value()];
            while parameter.increase_step() {
                values.push(parameter.value());
            }
            assert_eq!(values, expected, "{}: native sweep sequence", entry.name);
        }
    }
}

#[test]
fn linear_parameter_edge_cases_match_native_semantics() {
    // Native LinearParameter constructor clamps max<min to min, negative steps
    // to zero, and snaps the initial value to the step grid.
    let parameter = Parameter::linear("p", 2.6, 0.0, 5.0, 2.0);
    assert_eq!(parameter.value(), 2.0);
    assert_eq!(parameter.count_steps(), 3);
    assert_eq!(parameter.linear_range(), Some((0.0, 5.0, 2.0)));

    let clamped = Parameter::linear("p", 1.0, 2.0, 1.0, -1.0);
    assert_eq!(clamped.linear_range(), Some((2.0, 2.0, 0.0)));

    // A value above the last full step snaps down (native SetValue).
    let mut parameter = Parameter::linear("p", 0.0, 0.0, 5.0, 2.0);
    parameter.set_value(5.0);
    assert_eq!(parameter.value(), 4.0);
    parameter.set_value(-3.0);
    assert_eq!(parameter.value(), 0.0);

    // Sweep flag: a constant parameter never sweeps (native GetSweep), a linear
    // parameter honours the flag.
    let constant = Parameter::new("c", 3.0);
    assert!(!constant.get_sweep());
    let mut linear = Parameter::linear("l", 1.0, 0.0, 2.0, 1.0);
    assert!(linear.get_sweep());
    linear.set_sweep(false);
    assert!(!linear.get_sweep());
    assert!(!linear.sweep_enabled());
    assert!(linear.is_linear());

    // set_linear_range re-clamps the current value into the new range.
    let mut parameter = Parameter::linear("p", 1.0, 0.0, 4.0, 1.0);
    assert!(parameter.set_linear_range(2.0, 3.0, 0.5));
    assert_eq!(parameter.value(), 2.0);
    assert_eq!(parameter.count_steps(), 3);

    // Empty expressions map to native SetValue("") -> -1.
    let parameter = Parameter::new("p", 1.0);
    assert_eq!(parameter.name(), "p");
    let error = ParameterScalarEmpty::empty();
    assert_eq!(error.native_code(), -1);
}

/// Helper so the empty-expression path is exercised through the public API.
struct ParameterScalarEmpty;

impl ParameterScalarEmpty {
    fn empty() -> ParameterError {
        let mut scalar = csxcad_core::parameter::ParameterScalar::new(1.0);
        scalar.set_value_string("").expect_err("empty expression")
    }
}

#[test]
fn zero_step_increase_step_terminates_instead_of_looping_forever() {
    // Native LinearParameter::IncreaseStep() is `if (dValue+dStep>dMax) return
    // false; SetValue(dValue+dStep); return true;` - with dStep == 0 it returns
    // true forever. Rust guards against that; the observable difference is
    // recorded here (Rust never hangs).
    let mut parameter = Parameter::linear("p", 1.0, 1.0, 1.0, 0.0);
    assert_eq!(parameter.count_steps(), 1);
    assert!(!parameter.increase_step());
    parameter.init_sweep();
    assert_eq!(parameter.value(), 1.0);
    assert!(!parameter.increase_step());
}

#[test]
fn parameter_set_keeps_native_ordering_and_value_array() {
    let mut set = ParameterSet::new();
    set.add("a", 1.0);
    set.add_linear("b", 2.0, 0.0, 4.0, 2.0);
    set.add("c", 3.0);
    assert_eq!(set.parameter_string(","), "a,b,c");
    assert_eq!(set.value_array(), vec![1.0, 2.0, 3.0]);
    assert!(set.remove(1));
    assert!(!set.remove(7));
    assert_eq!(set.parameter_string(","), "a,c");
    assert_eq!(set.len(), 2);
}
