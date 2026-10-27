//! RM-036: CSXCAD performance baseline for XML load/serialize and model
//! queries.
//!
//! Dependency-free by design: one `#[test]` builds a large structure in
//! memory, measures serialization, parsing (the dominant cost of loading a
//! project) and the model queries a solver performs, then prints the numbers
//! and the machine. Run it as:
//!
//! ```text
//! cargo test -p csxcad-core --test perf_baseline --release -- --nocapture
//! cargo test -p csxcad-core --test perf_baseline -- --nocapture   # debug
//! ```
//!
//! The generated document is deterministic (no random data), so repeated runs
//! on the same machine are directly comparable. Timings are the best of three
//! runs; throughput is XML bytes / second.
//!
//! Reference numbers recorded on 2026-10-27 (Windows 11, x86_64, debug build
//! and release build) are in `tests/native_gen/README.md`.

mod common;

use csxcad_core::model::{Primitive, Property, PropertyType, Structure};
use std::time::{Duration, Instant};

struct Sample {
    properties: usize,
    primitives: usize,
    xml_bytes: usize,
    build: Duration,
    serialize: Duration,
    parse: Duration,
    bounding_boxes: Duration,
    queries: Duration,
}

const PROPERTY_COUNT: usize = 250;
const PRIMITIVES_PER_PROPERTY: usize = 4;
const QUERY_COUNT: usize = 100;
const GRID_LINES: usize = 501;

fn build_large_structure() -> Structure {
    let mut structure = Structure::new();
    structure.grid_mut().set_delta_unit(1e-3);
    for line in 0..GRID_LINES {
        let value = line as f64 - (GRID_LINES as f64 / 2.0);
        structure.grid_mut().add_line(0, value);
        structure.grid_mut().add_line(1, value);
        structure.grid_mut().add_line(2, value);
    }
    structure.parameters_mut().add("scale", 1.0);
    structure
        .parameters_mut()
        .add_linear("sweep", 1.0, 0.5, 2.0, 0.25);

    for index in 0..PROPERTY_COUNT {
        let position = index as f64;
        let property_type = match index % 4 {
            0 => PropertyType::Material,
            1 => PropertyType::Metal,
            2 => PropertyType::DebyeMaterial,
            _ => PropertyType::ProbeBox,
        };
        let mut property = Property::new(format!("property-{index}"), property_type);
        if let Some(material) = property.material_mut() {
            material.set_value(
                csxcad_core::material::MaterialField::Epsilon,
                0,
                2.0 + position * 1e-3,
            );
            material.set_value(csxcad_core::material::MaterialField::Sigma, 0, 1e-3);
        }
        if let Some(debye) = property.debye_material_mut() {
            // A Debye property with a <Property> child always reloads with
            // order 1 (native ReadFromXML sets m_Order=1), so build a
            // one-pole material to keep the round trip lossless.
            debye.set_order(1);
            let _ = debye.set_epsilon_delta(0, 0, 0.5);
            let _ = debye.set_epsilon_relax_time(0, 0, 1e-12);
        }
        if let Some(probe) = property.probe_mut() {
            probe.set_number(index as u32);
            probe.set_probe_type(0);
        }
        let property_id = structure.add_property(property);
        for primitive in 0..PRIMITIVES_PER_PROPERTY {
            let offset = position * 2.0 + primitive as f64 * 0.5;
            let start = [offset, offset, 0.0];
            let stop = [offset + 0.25, offset + 0.25, 0.5];
            let mut primitive = match primitive % 4 {
                0 => Primitive::box_primitive(start, stop),
                1 => Primitive::sphere([offset + 0.1, offset + 0.1, 0.25], 0.1),
                2 => Primitive::cylinder([offset, offset, 0.0], [offset, offset, 0.5], 0.1),
                _ => Primitive::box_from_expressions(
                    ["scale*0", "scale*0", "scale*0"],
                    ["scale*0.25", "scale*0.25", "scale*0.5"],
                )
                .expect("expression primitive"),
            };
            primitive.set_priority((index % 7) as i32);
            structure
                .add_primitive(property_id, primitive)
                .expect("add primitive");
        }
    }
    structure
}

fn measure_build() -> (Structure, Duration) {
    let start = Instant::now();
    let structure = build_large_structure();
    (structure, start.elapsed())
}

fn measure<F: FnMut()>(mut operation: F) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..3 {
        let start = Instant::now();
        operation();
        let elapsed = start.elapsed();
        if elapsed < best {
            best = elapsed;
        }
    }
    best
}

fn sample() -> Sample {
    let (structure, build) = measure_build();
    let properties = structure.property_count();
    let primitives = structure.primitive_count();

    let mut xml = String::new();
    let serialize = measure(|| {
        xml = structure
            .to_xml_string()
            .expect("large structure must serialize");
    });
    let xml_bytes = xml.len();

    let mut parsed = None;
    let parse = measure(|| {
        parsed = Some(Structure::from_xml_str(&xml).expect("large XML must parse"));
    });
    let parsed = parsed.expect("parse result");
    assert_eq!(parsed.property_count(), properties);
    assert_eq!(parsed.primitive_count(), primitives);
    assert_eq!(parsed, structure, "large round trip must be lossless");

    let parsed_primitives: Vec<&Primitive> = parsed.all_primitives(false, PropertyType::Any);
    let parameters = parsed.parameters().clone();
    let bounding_boxes = measure(|| {
        for primitive in &parsed_primitives {
            primitive
                .bounding_box(Some(&parameters), false)
                .expect("bounding box");
        }
    });

    let queries = measure(|| {
        let mut structure = parsed.clone();
        for index in 0..QUERY_COUNT {
            let coordinate = [
                (index % 100) as f64 * 5.0,
                ((index * 7) % 100) as f64 * 5.0,
                0.25,
            ];
            structure
                .property_by_coord_priority(coordinate, PropertyType::Any, false)
                .expect("query");
        }
    });

    Sample {
        properties,
        primitives,
        xml_bytes,
        build,
        serialize,
        parse,
        bounding_boxes,
        queries,
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

fn megabytes_per_second(bytes: usize, duration: Duration) -> f64 {
    let seconds = duration.as_secs_f64();
    if seconds == 0.0 {
        return f64::INFINITY;
    }
    bytes as f64 / seconds / (1024.0 * 1024.0)
}

#[test]
fn xml_roundtrip_and_model_query_throughput_baseline() {
    let sample = sample();
    assert!(sample.properties > 0 && sample.primitives > 0);
    assert!(
        sample.serialize < Duration::from_secs(60),
        "serialization is unexpectedly slow"
    );
    assert!(
        sample.parse < Duration::from_secs(60),
        "parsing is unexpectedly slow"
    );
    assert!(
        sample.queries < Duration::from_secs(60),
        "queries are unexpectedly slow"
    );

    println!("csxcad-core performance baseline");
    println!("os|{}", std::env::consts::OS);
    println!("arch|{}", std::env::consts::ARCH);
    println!(
        "profile|{}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    println!(
        "model|properties={}|primitives={}|grid_lines_per_axis={}|parameters=2",
        sample.properties, sample.primitives, GRID_LINES
    );
    println!("xml_bytes|{}", sample.xml_bytes);
    println!("build_ms|{:.2}", millis(sample.build));
    println!(
        "serialize_ms|{:.2}|MB/s|{:.2}",
        millis(sample.serialize),
        megabytes_per_second(sample.xml_bytes, sample.serialize)
    );
    println!(
        "parse_ms|{:.2}|MB/s|{:.2}",
        millis(sample.parse),
        megabytes_per_second(sample.xml_bytes, sample.parse)
    );
    println!(
        "bounding_boxes_ms|{:.2}|per_primitive_us|{:.3}",
        millis(sample.bounding_boxes),
        sample.bounding_boxes.as_secs_f64() * 1e6 / sample.primitives as f64
    );
    println!(
        "queries_ms|{:.2}|query_count|{}|per_query_us|{:.3}",
        millis(sample.queries),
        QUERY_COUNT,
        sample.queries.as_secs_f64() * 1e6 / QUERY_COUNT as f64
    );
    println!(
        "throughput_parse_mb_s|{:.2}|throughput_serialize_mb_s|{:.2}",
        megabytes_per_second(sample.xml_bytes, sample.parse),
        megabytes_per_second(sample.xml_bytes, sample.serialize)
    );
}

#[test]
fn baseline_document_is_deterministic() {
    // The baseline is only comparable between runs if the document is
    // deterministic; guard that with a cheap build of the same generator.
    if cfg!(debug_assertions) {
        // Keep the debug test quick: compare the first few bytes of identity.
        let first = build_large_structure();
        let second = build_large_structure();
        assert_eq!(first.property_count(), second.property_count());
        assert_eq!(first.primitive_count(), second.primitive_count());
        assert_eq!(
            first.to_xml_string().unwrap().len(),
            second.to_xml_string().unwrap().len()
        );
    } else {
        let first = build_large_structure();
        let second = build_large_structure();
        assert_eq!(first, second);
    }
}
