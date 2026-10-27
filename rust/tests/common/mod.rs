//! Shared helpers for the CSXCAD native fixture corpus.
//!
//! The fixtures under `tests/fixtures/xml` and `tests/fixtures/native` are
//! produced by the *native* CSXCAD library with
//! `tests/native_gen/generate_native_fixtures.cpp`; see
//! `tests/native_gen/README.md`. They must not be edited by hand.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Directory holding the generated fixtures (`tests/fixtures`).
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn fixture_path(relative: &str) -> PathBuf {
    fixtures_dir().join(relative)
}

/// All fixture names from the generated manifest, in generation order.
pub fn manifest() -> Vec<FixtureEntry> {
    let path = fixture_path("xml/manifest.txt");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        assert_eq!(fields.len(), 4, "malformed manifest line {line:?}");
        entries.push(FixtureEntry {
            name: fields[0].to_owned(),
            properties: fields[1].parse().expect("property count"),
            primitives: fields[2].parse().expect("primitive count"),
            bytes: fields[3].parse().expect("byte count"),
        });
    }
    assert!(!entries.is_empty(), "manifest is empty");
    entries
}

#[derive(Clone, Debug)]
pub struct FixtureEntry {
    pub name: String,
    pub properties: usize,
    pub primitives: usize,
    pub bytes: u64,
}

/// One parsed native report (`tests/fixtures/native/<name>.report`).
///
/// The format is line oriented: `key|field|field...`. Keys are unique except
/// for repeated collections (`property`, `primitive`, `detail`, `query`, ...),
/// which are kept in file order.
#[derive(Clone, Debug, Default)]
pub struct NativeReport {
    pub lines: Vec<Vec<String>>,
    pub by_key: BTreeMap<String, Vec<Vec<String>>>,
}

impl NativeReport {
    pub fn load(name: &str) -> Self {
        let path = fixture_path(&format!("native/{name}.report"));
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let mut report = Self::default();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let fields: Vec<String> = line.split('|').map(str::to_owned).collect();
            report
                .by_key
                .entry(fields[0].clone())
                .or_default()
                .push(fields.clone());
            report.lines.push(fields);
        }
        report
    }

    pub fn all(&self, key: &str) -> &[Vec<String>] {
        self.by_key.get(key).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn one(&self, key: &str) -> &[String] {
        let entries = self.all(key);
        assert_eq!(entries.len(), 1, "expected exactly one {key:?} line");
        &entries[0]
    }

    pub fn scalar(&self, key: &str) -> &str {
        &self.one(key)[1]
    }

    pub fn number(&self, key: &str) -> f64 {
        parse_number(self.scalar(key), key)
    }

    /// `detail|<property>|<key>[|<index>...]|<value>` lines for one property.
    pub fn details(&self, property: usize) -> Vec<&[String]> {
        self.all("detail")
            .iter()
            .filter(|line| line[1].parse::<usize>().expect("property index") == property)
            .map(Vec::as_slice)
            .collect()
    }
}

pub fn parse_number(value: &str, field: &str) -> f64 {
    value
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("{field}: {value:?} is not a number"))
}

pub fn parse_integer(value: &str, field: &str) -> i64 {
    value
        .parse::<i64>()
        .unwrap_or_else(|_| panic!("{field}: {value:?} is not an integer"))
}

/// Relative tolerance for values that both sides compute through the same
/// IEEE-754 double operations (native `%.17g` output round-trips exactly).
pub const REL_TOL: f64 = 1e-12;
pub const ABS_TOL: f64 = 1e-12;

pub fn assert_close(actual: f64, expected: f64, context: &str) {
    if actual == expected {
        return;
    }
    let difference = (actual - expected).abs();
    let scale = actual.abs().max(expected.abs()).max(1.0);
    assert!(
        difference <= ABS_TOL + REL_TOL * scale,
        "{context}: actual {actual:?} != native {expected:?} (difference {difference:e})"
    );
}

pub fn assert_close_slice(actual: &[f64], expected: &[f64], context: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{context}: length {} != {}",
        actual.len(),
        expected.len()
    );
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_close(*actual, *expected, &format!("{context}[{index}]"));
    }
}
