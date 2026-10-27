//! RM-029: differential model harness against native CSXCAD.
//!
//! The native side is `tests/native_gen/generate_native_fixtures.cpp`, which
//! links the pinned native `libCSXCAD` and writes, next to every fixture, a
//! report of the observable model read back through the native C++ API
//! (`tests/fixtures/native/<fixture>.report`). This test parses the same
//! fixtures with `csxcad-core` and compares:
//!
//! * structure/grid/background/parameter inventory and values,
//! * every property's type, name, visibility, colour, coordinate input type,
//!   generic attributes and type-specific fields,
//! * every primitive's type, priority, coordinate system, bounds and
//!   dimension,
//! * native transform matrices,
//! * coordinate/priority containment queries (single and batch),
//! * the native object area.
//!
//! Native values that the native API cannot express in the mesh coordinate
//! system (a primitive defined in another coordinate system whose
//! `GetBoundBox` returns `false`) are not compared; the report keeps the
//! native `valid` flag so the skip is explicit.

mod common;

use common::{
    assert_close, assert_close_slice, manifest, parse_integer, parse_number, NativeReport,
};
use csxcad_core::grid::CoordinateSystem;
use csxcad_core::material::{LorentzParameter, MaterialField};
use csxcad_core::model::{Primitive, Property, PropertyType, Structure};
use std::fs;

fn load(name: &str) -> Structure {
    let path = common::fixture_path(&format!("xml/{name}.xml"));
    let xml = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    Structure::from_xml_str(&xml).unwrap_or_else(|error| panic!("{name} does not parse: {error}"))
}

fn grid_lines(structure: &Structure, direction: usize) -> Vec<f64> {
    let mut grid = structure.grid().clone();
    grid.lines(direction)
        .unwrap_or_else(|| panic!("direction {direction}"))
        .to_vec()
}

fn native_primitive_name(primitive: &Primitive) -> &'static str {
    match primitive.primitive_type() {
        csxcad_core::model::PrimitiveType::Point => "Point",
        csxcad_core::model::PrimitiveType::Box => "Box",
        csxcad_core::model::PrimitiveType::MultiBox => "MultiBox",
        csxcad_core::model::PrimitiveType::Sphere => "Sphere",
        csxcad_core::model::PrimitiveType::SphericalShell => "SphericalShell",
        csxcad_core::model::PrimitiveType::Cylinder => "Cylinder",
        csxcad_core::model::PrimitiveType::CylindricalShell => "CylindricalShell",
        csxcad_core::model::PrimitiveType::Polygon => "Polygon",
        csxcad_core::model::PrimitiveType::LinPoly => "LinPoly",
        csxcad_core::model::PrimitiveType::RotPoly => "RotPoly",
        csxcad_core::model::PrimitiveType::Polyhedron => "Polyhedron",
        csxcad_core::model::PrimitiveType::PolyhedronReader => "PolyhedronReader",
        csxcad_core::model::PrimitiveType::Curve => "Curve",
        csxcad_core::model::PrimitiveType::Wire => "Wire",
        csxcad_core::model::PrimitiveType::UserDefined => "User-Defined",
    }
}

fn compare_header(structure: &Structure, report: &NativeReport, name: &str) {
    let native_coord = parse_integer(report.scalar("coord_system"), "coord_system");
    assert_eq!(
        structure.coordinate_system() as i32,
        native_coord as i32,
        "{name}: coordinate system"
    );

    // Native CSRectGrid has no coordinate-system getter of its own; the
    // structure owns the mesh type and writes it to the XML grid element.
    assert_close(
        structure.grid().delta_unit(),
        report.number("grid_delta_unit"),
        &format!("{name}: grid delta unit"),
    );

    for direction in 0..3 {
        let line = &report.all("grid")[direction];
        let count = parse_integer(&line[2], "grid qty") as usize;
        let expected: Vec<f64> = line[3..]
            .iter()
            .map(|value| parse_number(value, "grid line"))
            .collect();
        assert_eq!(count, expected.len(), "{name}: grid count field");
        assert_eq!(
            grid_lines(structure, direction),
            expected,
            "{name}: grid lines direction {direction}"
        );
    }

    let background = report.one("background");
    let material = structure.background_material();
    assert_close(
        material.value(MaterialField::Epsilon),
        parse_number(&background[1], "background.epsilon"),
        &format!("{name}: background epsilon"),
    );
    assert_close(
        material.value(MaterialField::Mue),
        parse_number(&background[2], "background.mue"),
        &format!("{name}: background mue"),
    );
    assert_close(
        material.value(MaterialField::Kappa),
        parse_number(&background[3], "background.kappa"),
        &format!("{name}: background kappa"),
    );
    assert_close(
        material.value(MaterialField::Sigma),
        parse_number(&background[4], "background.sigma"),
        &format!("{name}: background sigma"),
    );
}

fn compare_parameters(structure: &Structure, report: &NativeReport, name: &str) {
    let lines = report.all("parameter");
    assert_eq!(
        structure.parameters().len(),
        lines.len(),
        "{name}: parameter count"
    );
    for (index, line) in lines.iter().enumerate() {
        let parameter = structure.parameters().get(index).expect("parameter");
        assert_eq!(parameter.name(), line[2], "{name}: parameter {index} name");
        let native_linear = line[3] == "Linear";
        assert_eq!(
            parameter.is_linear(),
            native_linear,
            "{name}: parameter {index} kind"
        );
        assert_close(
            parameter.value(),
            parse_number(&line[4], "parameter.value"),
            &format!("{name}: parameter {index} value"),
        );
        if native_linear {
            assert_eq!(
                parameter.get_sweep(),
                line[5] == "1",
                "{name}: parameter {index} GetSweep"
            );
            let expected = (
                parse_number(&line[6], "parameter.min"),
                parse_number(&line[7], "parameter.max"),
                parse_number(&line[8], "parameter.step"),
            );
            let actual = parameter.linear_range().expect("linear parameter");
            assert_close(
                actual.0,
                expected.0,
                &format!("{name}: parameter {index} min"),
            );
            assert_close(
                actual.1,
                expected.1,
                &format!("{name}: parameter {index} max"),
            );
            assert_close(
                actual.2,
                expected.2,
                &format!("{name}: parameter {index} step"),
            );
            assert_eq!(
                parameter.count_steps(),
                ((expected.1 - expected.0) / expected.2) as usize + 1,
                "{name}: parameter {index} count_steps"
            );
        }
    }

    for line in report.all("parameter_steps") {
        let index = parse_integer(&line[1], "parameter_steps index") as usize;
        let expected: Vec<f64> = line[2..]
            .iter()
            .map(|value| parse_number(value, "parameter step"))
            .collect();
        let mut parameter = structure
            .parameters()
            .get(index)
            .expect("parameter")
            .clone();
        parameter.init_sweep();
        let mut actual = vec![parameter.value()];
        while parameter.increase_step() {
            actual.push(parameter.value());
            assert!(
                actual.len() <= expected.len() + 1,
                "step loop does not terminate"
            );
        }
        assert_eq!(
            actual, expected,
            "{name}: parameter {index} sweep sequence differs from native IncreaseStep"
        );
    }
}

fn native_primitive_count(structure: &Structure, report: &NativeReport) -> usize {
    let read_back = report.scalar("read_back") == "1";
    let user_defined = structure
        .primitives_by_type(csxcad_core::model::PrimitiveType::UserDefined)
        .len();
    if read_back {
        // Native writes <User-Defined> but ContinuousStructure::ReadFromXML
        // only dispatches <UserDefined>, so the native re-read drops those
        // primitives. Rust reads both spellings.
        structure.primitive_count() - user_defined
    } else {
        structure.primitive_count()
    }
}

fn compare_properties(structure: &Structure, report: &NativeReport, name: &str) {
    let lines = report.all("property");
    assert_eq!(
        structure.property_count(),
        lines.len(),
        "{name}: property count"
    );
    assert_eq!(
        structure.property_count(),
        parse_integer(report.scalar("property_count"), "property_count") as usize
    );
    assert_eq!(
        native_primitive_count(structure, report),
        parse_integer(report.scalar("primitive_count"), "primitive_count") as usize,
        "{name}: primitive count"
    );

    for line in lines {
        let index = parse_integer(&line[1], "property index") as usize;
        let property = structure
            .property_by_index(index)
            .unwrap_or_else(|| panic!("{name}: missing property {index}"));
        assert_eq!(
            property_type_name(property),
            line[2],
            "{name}: property {index} type"
        );
        assert_eq!(property.name(), line[3], "{name}: property {index} name");
        assert_eq!(
            property.is_visible(),
            line[4] == "1",
            "{name}: property {index} visibility"
        );
        let fill = property.fill_color().expect("native always writes colours");
        let edge = property.edge_color().expect("native always writes colours");
        assert_eq!(
            fill.components().map(i32::from),
            [
                parse_integer(&line[5], "fill.r") as i32,
                parse_integer(&line[6], "fill.g") as i32,
                parse_integer(&line[7], "fill.b") as i32,
                parse_integer(&line[8], "fill.a") as i32,
            ],
            "{name}: property {index} fill colour"
        );
        assert_eq!(
            edge.components().map(i32::from),
            [
                parse_integer(&line[9], "edge.r") as i32,
                parse_integer(&line[10], "edge.g") as i32,
                parse_integer(&line[11], "edge.b") as i32,
                parse_integer(&line[12], "edge.a") as i32,
            ],
            "{name}: property {index} edge colour"
        );
        assert_eq!(
            property.coordinate_input_type() as i32,
            parse_integer(&line[13], "property coord input") as i32,
            "{name}: property {index} coordinate input type"
        );
        assert_eq!(
            property.primitive_count()
                - property
                    .primitives()
                    .iter()
                    .filter(|primitive| {
                        matches!(
                            primitive.primitive_type(),
                            csxcad_core::model::PrimitiveType::UserDefined
                        )
                    })
                    .count(),
            if report.scalar("read_back") == "1" {
                parse_integer(&line[14], "property primitive count") as usize
            } else {
                property.primitive_count()
            },
            "{name}: property {index} primitive count"
        );
    }

    let native_attributes = report.all("attr").len();
    let rust_attributes: usize = (0..structure.property_count())
        .map(|index| {
            structure
                .property_by_index(index)
                .map(|property| property.attribute_names().count())
                .unwrap_or(0)
        })
        .sum();
    assert_eq!(
        rust_attributes, native_attributes,
        "{name}: generic attribute count"
    );
    for line in report.all("attr") {
        let index = parse_integer(&line[1], "attr property") as usize;
        let property = structure
            .property_by_index(index)
            .expect("attribute property");
        assert!(
            property.has_attribute(&line[2]),
            "{name}: property {index} lost attribute {:?}",
            line[2]
        );
        assert_eq!(
            property.attribute(&line[2]),
            line[3],
            "{name}: property {index} attribute {:?}",
            line[2]
        );
    }
}

fn property_type_name(property: &Property) -> &'static str {
    match property.property_type() {
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

fn detail_number(line: &[String], index: usize) -> f64 {
    parse_number(&line[index], "detail value")
}

/// Native `GetTerm()`-style accessors return the *expression* only (an empty
/// string for a literal), while several Rust accessors return the value string
/// because the XML writer needs the literal. Accept either.
fn assert_term_like(actual: &str, native: &str, context: &str) {
    if native.is_empty() {
        assert!(
            actual.is_empty() || actual.parse::<f64>().is_ok(),
            "{context}: expected an empty or numeric literal, got {actual:?}"
        );
    } else {
        assert_eq!(actual, native, "{context}");
    }
}

/// Native `CSPropMaterial::GetValue`/`GetTerm` pin the component index to 0 for
/// an isotropic property. The Rust model stores raw components; consumers apply
/// the pinning (see `openEMS::operator` Debye setup).
fn effective_component(property: &Property, component: usize) -> usize {
    if property
        .material()
        .is_some_and(|material| material.is_isotropic())
    {
        0
    } else {
        component
    }
}

fn compare_details(structure: &Structure, report: &NativeReport, name: &str) {
    for line in report.all("detail") {
        let index: usize = line[1].parse().expect("detail property index");
        let property = structure
            .property_by_index(index)
            .unwrap_or_else(|| panic!("{name}: detail for missing property {index}"));
        let key = line[2].as_str();
        let parameters = Some(structure.parameters());
        let context = format!("{name}: property {index} {key}");

        match key {
            "isotropy" => {
                let material = property.material().expect("material payload");
                assert_eq!(material.is_isotropic(), line[3] == "1", "{context}");
            }
            "epsilon" | "mue" | "kappa" | "sigma" => {
                let component = parse_integer(&line[3], "component") as usize;
                let field = match key {
                    "epsilon" => MaterialField::Epsilon,
                    "mue" => MaterialField::Mue,
                    "kappa" => MaterialField::Kappa,
                    _ => MaterialField::Sigma,
                };
                let material = property.material().expect("material payload");
                // Native CSPropMaterial::GetValue() repeats component 0 for an
                // isotropic material; MaterialProperties::component() does the
                // same, so every component is directly comparable.
                let value = material
                    .value(field, component, parameters)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{context}: missing component {component}"));
                assert_close(value, detail_number(line, 4), &context);
            }
            "density" => {
                let material = property.material().expect("material payload");
                assert_close(
                    material.density(parameters).unwrap(),
                    detail_number(line, 3),
                    &context,
                );
            }
            "weight_epsilon" | "weight_kappa" | "weight_density" => {
                let material = property.material().expect("material payload");
                if key == "weight_density" {
                    assert_eq!(material.density_weight_term(), line[3], "{context}");
                } else {
                    let component = parse_integer(&line[3], "component") as usize;
                    let field = if key == "weight_epsilon" {
                        MaterialField::Epsilon
                    } else {
                        MaterialField::Kappa
                    };
                    let expected = line.last().expect("weight value");
                    assert_eq!(
                        material.weight_term(field, component).unwrap_or_default(),
                        *expected,
                        "{context}"
                    );
                }
            }
            "dispersion_order" => {
                let order = parse_integer(&line[3], "order") as usize;
                let native_order = if let Some(debye) = property.debye_material() {
                    debye.order()
                } else {
                    property
                        .lorentz_material()
                        .expect("dispersive payload")
                        .order()
                };
                assert_eq!(native_order, order, "{context}");
            }
            "debye_delta" | "debye_relax" => {
                let order = parse_integer(&line[3], "order") as usize;
                let component = parse_integer(&line[4], "component") as usize;
                let debye = property.debye_material().expect("debye payload");
                // Native GetValue3() -> CSPropMaterial::GetValue() pins the
                // component to 0 for an isotropic property. The model stores the
                // raw components and solver consumers apply the same pinning
                // (openEMS Operator::SetupDebyeExtension).
                let component = effective_component(property, component);
                let value = if key == "debye_delta" {
                    debye.epsilon_delta(order, component, parameters)
                } else {
                    debye.epsilon_relax_time(order, component, parameters)
                }
                .unwrap()
                .expect("debye component");
                assert_close(value, detail_number(line, 5), &context);
            }
            "debye_delta_weight" => {
                let order = parse_integer(&line[3], "order") as usize;
                let debye = property.debye_material().expect("debye payload");
                let expected = line.last().expect("weight value");
                let actual = debye
                    .epsilon_delta_weight_term(order, effective_component(property, 0))
                    .unwrap_or_default();
                assert_term_like(&actual, expected, &context);
            }
            key if key.starts_with("lorentz_") => {
                let order = parse_integer(&line[3], "order") as usize;
                let parameter = match key {
                    "lorentz_eps_plasma" => LorentzParameter::EpsilonPlasmaFrequency,
                    "lorentz_mue_plasma" => LorentzParameter::MuePlasmaFrequency,
                    "lorentz_eps_pole" => LorentzParameter::EpsilonLorentzPoleFrequency,
                    "lorentz_mue_pole" => LorentzParameter::MueLorentzPoleFrequency,
                    "lorentz_eps_relax" => LorentzParameter::EpsilonRelaxTime,
                    "lorentz_mue_relax" => LorentzParameter::MueRelaxTime,
                    other => panic!("{context}: unknown lorentz detail {other}"),
                };
                let lorentz = property.lorentz_material().expect("lorentz payload");
                let value = lorentz
                    .value(
                        parameter,
                        order,
                        effective_component(property, 0),
                        parameters,
                    )
                    .unwrap()
                    .expect("lorentz component");
                assert_close(value, detail_number(line, 4), &context);
            }
            "disc_file" => assert_eq!(
                property
                    .discrete_material()
                    .expect("disc payload")
                    .file_name(),
                line[3],
                "{context}"
            ),
            "disc_scale" => assert_close(
                property.discrete_material().expect("disc payload").scale(),
                detail_number(line, 3),
                &context,
            ),
            "disc_use_db_background" => assert_eq!(
                property
                    .discrete_material()
                    .expect("disc payload")
                    .use_database_for_background(),
                line[3] == "1",
                "{context}"
            ),
            "conductivity" => assert_close(
                property
                    .conducting_sheet()
                    .expect("sheet payload")
                    .conductivity(parameters)
                    .unwrap(),
                detail_number(line, 3),
                &context,
            ),
            "thickness" => assert_close(
                property
                    .conducting_sheet()
                    .expect("sheet payload")
                    .thickness(parameters)
                    .unwrap(),
                detail_number(line, 3),
                &context,
            ),
            "lumped_direction" => assert_eq!(
                property
                    .lumped_element()
                    .expect("lumped payload")
                    .direction(),
                parse_integer(&line[3], "direction") as i32,
                "{context}"
            ),
            "lumped_caps" => assert_eq!(
                property.lumped_element().expect("lumped payload").caps(),
                line[3] == "1",
                "{context}"
            ),
            "lumped_R" => assert_close(
                property
                    .lumped_element()
                    .expect("lumped payload")
                    .resistance(parameters)
                    .unwrap(),
                detail_number(line, 3),
                &context,
            ),
            "lumped_C" => assert_close(
                property
                    .lumped_element()
                    .expect("lumped payload")
                    .capacitance(parameters)
                    .unwrap(),
                detail_number(line, 3),
                &context,
            ),
            "lumped_L" => assert_close(
                property
                    .lumped_element()
                    .expect("lumped payload")
                    .inductance(parameters)
                    .unwrap(),
                detail_number(line, 3),
                &context,
            ),
            "lumped_type" => assert_eq!(
                property
                    .lumped_element()
                    .expect("lumped payload")
                    .element_type()
                    .xml_value(),
                parse_integer(&line[3], "element type") as i32,
                "{context}"
            ),
            "abc_normal_sign" => assert_eq!(
                property
                    .absorbing_boundary()
                    .expect("abc payload")
                    .normal_sign_positive(),
                line[3] == "1",
                "{context}"
            ),
            "abc_phase_velocity" => assert_close(
                property
                    .absorbing_boundary()
                    .expect("abc payload")
                    .phase_velocity(parameters)
                    .unwrap(),
                detail_number(line, 3),
                &context,
            ),
            "abc_type" => assert_eq!(
                property
                    .absorbing_boundary()
                    .expect("abc payload")
                    .boundary_type()
                    .xml_value(),
                parse_integer(&line[3], "abc type") as i32,
                "{context}"
            ),
            "excitation_number" => assert_eq!(
                property.excitation().expect("excitation payload").number(),
                parse_integer(&line[3], "excitation number") as u32,
                "{context}"
            ),
            "excitation_enabled" => assert_eq!(
                property
                    .excitation()
                    .expect("excitation payload")
                    .is_enabled(),
                line[3] == "1",
                "{context}"
            ),
            "excitation_type" => assert_eq!(
                property
                    .excitation()
                    .expect("excitation payload")
                    .excitation_type(),
                parse_integer(&line[3], "excitation type") as i32,
                "{context}"
            ),
            "excitation_frequency" => assert_close(
                property
                    .excitation()
                    .expect("excitation payload")
                    .frequency(parameters)
                    .unwrap(),
                detail_number(line, 3),
                &context,
            ),
            "excitation_delay" => assert_close(
                property
                    .excitation()
                    .expect("excitation payload")
                    .delay(parameters)
                    .unwrap(),
                detail_number(line, 3),
                &context,
            ),
            "excitation_component" => {
                let component = parse_integer(&line[3], "component") as usize;
                let value = property
                    .excitation()
                    .expect("excitation payload")
                    .excitation(component, parameters)
                    .unwrap()
                    .expect("excitation component");
                assert_close(value, detail_number(line, 4), &context);
            }
            "excitation_propdir" => {
                let component = parse_integer(&line[3], "component") as usize;
                let value = property
                    .excitation()
                    .expect("excitation payload")
                    .propagation_direction(component, parameters)
                    .unwrap()
                    .expect("propagation component");
                assert_close(value, detail_number(line, 4), &context);
            }
            "excitation_weight" => {
                let component = parse_integer(&line[3], "component") as usize;
                let actual = property
                    .excitation()
                    .expect("excitation payload")
                    .weighting_term(component)
                    .unwrap_or_default();
                assert_term_like(&actual, &line[4], &context);
            }
            "excitation_weight_file" => assert_eq!(
                property
                    .excitation()
                    .expect("excitation payload")
                    .weight_file(),
                line[3],
                "{context}"
            ),
            "excitation_origin" => {
                let component = parse_integer(&line[3], "component") as usize;
                assert_close(
                    property
                        .excitation()
                        .expect("excitation payload")
                        .weight_origin()[component],
                    detail_number(line, 4),
                    &context,
                );
            }
            "probe_number" => assert_eq!(
                property.probe().expect("probe payload").number(),
                parse_integer(&line[3], "probe number") as u32,
                "{context}"
            ),
            "probe_type" => assert_eq!(
                property.probe().expect("probe payload").probe_type(),
                parse_integer(&line[3], "probe type") as i32,
                "{context}"
            ),
            "probe_weight" => assert_close(
                property.probe().expect("probe payload").weight(),
                detail_number(line, 3),
                &context,
            ),
            "probe_normdir" => assert_eq!(
                property.probe().expect("probe payload").normal_direction(),
                parse_integer(&line[3], "normal direction") as i32,
                "{context}"
            ),
            "probe_start" => assert_close(
                property.probe().expect("probe payload").start_time(),
                detail_number(line, 3),
                &context,
            ),
            "probe_stop" => assert_close(
                property.probe().expect("probe payload").stop_time(),
                detail_number(line, 3),
                &context,
            ),
            "probe_oversampling" => assert_eq!(
                property.probe().expect("probe payload").oversampling(),
                parse_integer(&line[3], "oversampling") as i32,
                "{context}"
            ),
            "probe_mode_file" => assert_eq!(
                property.probe().expect("probe payload").mode_file(),
                line[3],
                "{context}"
            ),
            "probe_mode_function" => {
                let component = parse_integer(&line[3], "component") as usize;
                assert_eq!(
                    property
                        .probe()
                        .expect("probe payload")
                        .mode_function(component)
                        .unwrap_or(""),
                    line[4],
                    "{context}"
                );
            }
            "probe_mode_origin" => {
                let component = parse_integer(&line[3], "component") as usize;
                assert_close(
                    property.probe().expect("probe payload").mode_origin()[component],
                    detail_number(line, 4),
                    &context,
                );
            }
            "probe_fd_sample" => {
                let sample = parse_integer(&line[3], "sample") as usize;
                assert_close(
                    property.probe().expect("probe payload").fd_samples()[sample],
                    detail_number(line, 4),
                    &context,
                );
            }
            "dump_type" => assert_eq!(
                property.dump_box().expect("dump payload").dump_type(),
                parse_integer(&line[3], "dump type") as i32,
                "{context}"
            ),
            "dump_mode" => assert_eq!(
                property.dump_box().expect("dump payload").dump_mode(),
                parse_integer(&line[3], "dump mode") as i32,
                "{context}"
            ),
            "dump_file_type" => assert_eq!(
                property.dump_box().expect("dump payload").file_type(),
                parse_integer(&line[3], "dump file type") as i32,
                "{context}"
            ),
            "dump_multigrid" => assert_eq!(
                property
                    .dump_box()
                    .expect("dump payload")
                    .multi_grid_level(),
                parse_integer(&line[3], "dump multigrid") as i32,
                "{context}"
            ),
            "dump_sub_sampling_enabled" => assert_eq!(
                property
                    .dump_box()
                    .expect("dump payload")
                    .sub_sampling_enabled(),
                line[3] == "1",
                "{context}"
            ),
            "dump_opt_resolution_enabled" => assert_eq!(
                property
                    .dump_box()
                    .expect("dump payload")
                    .optimal_resolution_enabled(),
                line[3] == "1",
                "{context}"
            ),
            "dump_sub_sampling" => {
                let direction = parse_integer(&line[3], "direction") as usize;
                assert_eq!(
                    property.dump_box().expect("dump payload").sub_sampling()[direction],
                    parse_integer(&line[4], "sub sampling") as u32,
                    "{context}"
                );
            }
            "dump_opt_resolution" => {
                let direction = parse_integer(&line[3], "direction") as usize;
                assert_close(
                    property
                        .dump_box()
                        .expect("dump payload")
                        .optimal_resolution()[direction],
                    detail_number(line, 4),
                    &context,
                );
            }
            "resolution_factor" => assert_eq!(
                property.resolution_box().expect("resbox payload").factor(),
                parse_integer(&line[3], "resolution factor") as u32,
                "{context}"
            ),
            "unknown_property" => assert_eq!(
                property.unknown_property().unwrap_or(""),
                line[3],
                "{context}"
            ),
            other => panic!("{context}: unhandled native detail key {other}"),
        }
    }
}

fn compare_primitives(structure: &Structure, report: &NativeReport, name: &str) {
    let lines = report.all("primitive");
    let read_back = report.scalar("read_back") == "1";
    assert_eq!(
        native_primitive_count(structure, report),
        lines.len(),
        "{name}: primitive count"
    );
    for line in lines {
        let prop_index = parse_integer(&line[1], "primitive property") as usize;
        let prim_index = parse_integer(&line[2], "primitive index") as usize;
        let property = structure
            .property_by_index(prop_index)
            .expect("primitive property");
        // Native re-reads drop <User-Defined> primitives (it only dispatches
        // <UserDefined>), so the native index is relative to the list without
        // them.
        let primitive = property
            .primitives()
            .iter()
            .filter(|primitive| {
                !(read_back
                    && matches!(
                        primitive.primitive_type(),
                        csxcad_core::model::PrimitiveType::UserDefined
                    ))
            })
            .nth(prim_index)
            .expect("primitive");
        let context = format!("{name}: primitive {prop_index}/{prim_index}");

        assert_eq!(native_primitive_name(primitive), line[3], "{context} type");
        assert_eq!(
            primitive.priority(),
            parse_integer(&line[4], "priority") as i32,
            "{context} priority"
        );
        // Native CSPrimUserDefined::Write2XML() writes its own function
        // coordinate system into the same `CoordSystem` attribute that
        // CSPrimitives::Write2XML() uses for the mesh coordinate system, so the
        // serialized value only matches a native re-read, not the in-memory
        // object the report describes. The fixture round trip covers it.
        if !matches!(
            primitive.primitive_type(),
            csxcad_core::model::PrimitiveType::UserDefined
        ) {
            assert_eq!(
                primitive.coordinate_system() as i32,
                parse_integer(&line[5], "primitive coordinate system") as i32,
                "{context} coordinate system"
            );
        }
        let native_valid = line[6] == "1";
        let native_bounds_coordinate_system =
            parse_integer(&line[7], "bound-box coordinate system") as i32;
        let native_dimension = parse_integer(&line[8], "dimension") as i32;
        let bounds: Vec<f64> = line[9..]
            .iter()
            .map(|value| parse_number(value, "primitive bound"))
            .collect();
        assert_eq!(bounds.len(), 6, "{context} bound count");

        // Native GetBoundBox() reports `false` when the primitive is defined in
        // a different coordinate system than the mesh; those bounds are marked
        // invalid and are not comparable (the report keeps the flag).
        if native_valid && native_bounds_coordinate_system == structure.coordinate_system() as i32 {
            let actual = primitive
                .bounding_box(Some(structure.parameters()), false)
                .unwrap_or_else(|error| panic!("{context} bounding box: {error}"));
            assert_close_slice(&actual, &bounds, &format!("{context} bounds"));
        }

        // Native CSPrimUserDefined::GetDimension() stays -1 because its
        // GetBoundBox() fails; the Rust model knows a user-defined volume is
        // 3-D, so that single value is deliberately not compared.
        if !matches!(
            primitive.primitive_type(),
            csxcad_core::model::PrimitiveType::UserDefined
        ) {
            assert_eq!(
                primitive
                    .dimension(Some(structure.parameters()))
                    .expect("dimension"),
                native_dimension,
                "{context} dimension"
            );
        }
    }

    for line in report.all("matrix") {
        let prop_index = parse_integer(&line[1], "matrix property") as usize;
        let prim_index = parse_integer(&line[2], "matrix primitive") as usize;
        let read_back = report.scalar("read_back") == "1";
        let primitive = structure
            .property_by_index(prop_index)
            .expect("matrix property")
            .primitives()
            .iter()
            .filter(|primitive| {
                !(read_back
                    && matches!(
                        primitive.primitive_type(),
                        csxcad_core::model::PrimitiveType::UserDefined
                    ))
            })
            .nth(prim_index)
            .expect("matrix primitive");
        let matrix = primitive
            .transform()
            .expect("native transform matrix without a Rust transform")
            .matrix();
        let expected: Vec<f64> = line[3..]
            .iter()
            .map(|value| parse_number(value, "matrix element"))
            .collect();
        assert_close_slice(
            matrix,
            &expected,
            &format!("{name}: transform matrix {prop_index}/{prim_index}"),
        );
    }
}

fn compare_queries(structure: &mut Structure, report: &NativeReport, name: &str) {
    let mut batch_coordinates = Vec::new();
    let mut batch_expected = Vec::new();
    for line in report.all("query") {
        let coordinate = [
            parse_number(&line[2], "query x"),
            parse_number(&line[3], "query y"),
            parse_number(&line[4], "query z"),
        ];
        let expected_property = parse_integer(&line[5], "query property") as i32;
        let expected_primitive = parse_integer(&line[6], "query primitive") as i32;
        let context = format!(
            "{name}: query ({}, {}, {})",
            coordinate[0], coordinate[1], coordinate[2]
        );

        let found = structure
            .property_by_coord_priority(coordinate, PropertyType::Any, false)
            .unwrap_or_else(|error| panic!("{context}: {error}"));
        match found {
            None => {
                assert_eq!(expected_property, -1, "{context}: native found a property");
                assert_eq!(
                    expected_primitive, -1,
                    "{context}: native found a primitive"
                );
            }
            Some((property_id, primitive_id)) => {
                assert!(expected_property >= 0, "{context}: native found nothing");
                let property = structure.property(property_id).expect("found property");
                assert_eq!(
                    property.index(),
                    expected_property as usize,
                    "{context}: property"
                );
                let prim_index = property
                    .primitives()
                    .iter()
                    .filter(|primitive| {
                        !(report.scalar("read_back") == "1"
                            && matches!(
                                primitive.primitive_type(),
                                csxcad_core::model::PrimitiveType::UserDefined
                            ))
                    })
                    .position(|primitive| primitive.id() == Some(primitive_id))
                    .expect("found primitive belongs to the property");
                assert_eq!(
                    prim_index, expected_primitive as usize,
                    "{context}: primitive"
                );
            }
        }

        batch_coordinates.push(coordinate);
        batch_expected.push((expected_property, expected_primitive));
    }

    if !batch_coordinates.is_empty() {
        let batch = structure
            .properties_by_coords_priority(&batch_coordinates, PropertyType::Any, false)
            .expect("batch query");
        assert_eq!(batch.len(), batch_expected.len());
        for (index, (found, (expected_property, expected_primitive))) in
            batch.into_iter().zip(batch_expected).enumerate()
        {
            match found {
                None => {
                    assert_eq!(expected_property, -1, "{name}: batch query {index}");
                }
                Some((property_id, primitive_id)) => {
                    let property = structure.property(property_id).expect("batch property");
                    assert_eq!(
                        property.index() as i32,
                        expected_property,
                        "{name}: batch query {index} property"
                    );
                    let prim_index = property
                        .primitives()
                        .iter()
                        .filter(|primitive| {
                            !(report.scalar("read_back") == "1"
                                && matches!(
                                    primitive.primitive_type(),
                                    csxcad_core::model::PrimitiveType::UserDefined
                                ))
                        })
                        .position(|primitive| primitive.id() == Some(primitive_id))
                        .expect("batch primitive");
                    assert_eq!(
                        prim_index as i32, expected_primitive,
                        "{name}: batch query {index} primitive"
                    );
                }
            }
        }
    }
}

fn compare_object_area(structure: &Structure, report: &NativeReport, name: &str) {
    let line = report.one("object_area");
    let expected: Vec<f64> = line[1..]
        .iter()
        .map(|value| parse_number(value, "object area"))
        .collect();
    let has_user_defined = structure
        .all_primitives(false, PropertyType::Any)
        .iter()
        .any(|primitive| {
            matches!(
                primitive.shape(),
                csxcad_core::model::PrimitiveShape::UserDefined(_)
            )
        });
    let actual = structure
        .object_bounds(PropertyType::Any)
        .expect("object bounds");
    if structure.primitive_count() == 0 {
        // Native defect (source-supported, not reproduced by Rust):
        // ContinuousStructure::ObjArea is never initialised and
        // GetObjectArea() returns uninitialised doubles for an empty
        // structure (the report shows them). Rust returns None instead of
        // garbage, so nothing is compared here.
        assert!(
            actual.is_none(),
            "{name}: empty structure must have no bounds"
        );
        return;
    }
    match actual {
        Some(actual) if !has_user_defined => {
            assert_close_slice(&actual, &expected, &format!("{name}: object area"));
        }
        _ => {
            // Native treats user-defined primitives as unbounded; the Rust
            // model skips them, so no comparison is possible.
            assert!(
                has_user_defined,
                "{name}: native object area {expected:?} has no Rust counterpart"
            );
        }
    }
}

fn compare_geometry_validity(structure: &Structure, report: &NativeReport, name: &str) {
    let native_valid = report.scalar("geometry_valid") == "1";
    // Rust Structure::validate_geometry() ports the native checks: at least one
    // property and primitive, grid lines (x>1, y>1, z>0), valid primitive
    // updates/expressions and at least one excitation primitive.
    let mut candidate = structure.clone();
    let rust_valid = candidate.validate_geometry().is_ok();
    assert_eq!(rust_valid, native_valid, "{name}: geometry validity");
}

#[test]
fn native_model_matches_every_fixture() {
    for entry in manifest() {
        let name = &entry.name;
        let report = NativeReport::load(name);
        assert_eq!(
            report.scalar("fixture"),
            name,
            "{name}: report fixture name"
        );
        assert_eq!(
            report.scalar("format"),
            "csxcad-native-report",
            "{name}: report format"
        );

        let mut structure = load(name);
        compare_header(&structure, &report, name);
        compare_parameters(&structure, &report, name);
        compare_properties(&structure, &report, name);
        compare_details(&structure, &report, name);
        compare_primitives(&structure, &report, name);
        compare_queries(&mut structure, &report, name);
        compare_object_area(&structure, &report, name);
        compare_geometry_validity(&structure, &report, name);
    }
}

#[test]
fn report_parameter_values_and_expression_results_match_fparser() {
    // Every fixture report carries native `ParameterScalar` evaluations; the
    // Rust expression evaluator must agree on values and on failure codes.
    use csxcad_core::parameter::ParameterScalar;
    for entry in manifest() {
        let report = NativeReport::load(&entry.name);
        let structure = load(&entry.name);
        for line in report.all("expression") {
            let expression = &line[1];
            let native_code = parse_integer(&line[2], "expression code") as i32;
            let native_value = parse_number(&line[3], "expression value");
            let scalar = ParameterScalar::from_expression(expression).expect("expression");
            match scalar.evaluate(Some(structure.parameters())) {
                Ok(value) => {
                    assert_eq!(
                        native_code, 0,
                        "{}: native rejected {expression:?} with code {native_code} but Rust evaluated it",
                        entry.name
                    );
                    assert_close(
                        value,
                        native_value,
                        &format!("{}: expression {expression:?}", entry.name),
                    );
                }
                Err(error) => {
                    assert_eq!(
                        error.native_code(),
                        native_code,
                        "{}: expression {expression:?} native code",
                        entry.name
                    );
                }
            }
        }
    }
}

#[test]
fn coordinate_system_enum_values_match_native() {
    // Native CSXCAD_Global.h: CARTESIAN=0, CYLINDRICAL=1, UNDEFINED_CS=2.
    assert_eq!(CoordinateSystem::Cartesian as i32, 0);
    assert_eq!(CoordinateSystem::Cylindrical as i32, 1);
    assert_eq!(CoordinateSystem::Undefined as i32, 2);
}
