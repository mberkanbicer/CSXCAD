//! Contract tests for the MATLAB/Octave C ABI (RM-033).
//!
//! The MATLAB delivery path is the unchanged M-file API plus this C ABI
//! (`libcsxcad_matlab`), which a MEX shim or an embedding host calls. The tests
//! below pin the parts of the contract the native library defines and that are
//! reachable from the ABI: invalid and stale handles, ownership transfer at the
//! boundary, and the error identifiers (0 / null / empty string returns).
//!
//! Native references:
//! * `CSXCAD/src/ContinuousStructure.cpp:80-89` (AddProperty takes ownership),
//!   `:115-126` (RemoveProperty hands it back), `:137-150` (DeleteProperty walks
//!   its own vector, so an unknown pointer/id is a silent no-op),
//!   `:451-467` (clear() deletes every property),
//!   `:160` (GetQtyProperties reports the remaining count).
//! * `CSXCAD/src/CSObject.cpp:20-35` (a destroyed object notifies once; a stale
//!   handle must never be dereferenced).
//! * The C ABI maps every failure to `0`/`false`/null instead of unwinding
//!   (see the `catch_unwind` wrappers in `src/lib.rs`).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

use crate::*;

fn name(value: &str) -> CString {
    CString::new(value).expect("test name has no NUL")
}

fn xml_of(ptr: *mut c_void) -> String {
    let raw = csxcad_to_xml_string(ptr);
    assert!(!raw.is_null(), "ToXML returned null");
    // Safety: the pointer was produced by csxcad_to_xml_string and is owned by
    // the caller until csxcad_free_string.
    let text = unsafe { CStr::from_ptr(raw) }
        .to_str()
        .expect("XML is UTF-8")
        .to_string();
    csxcad_free_string(raw);
    text
}

#[test]
fn null_handle_is_rejected_everywhere() {
    let null: *mut c_void = std::ptr::null_mut();
    let value = name("mat");

    // Every entry point must survive a null handle and report failure instead
    // of dereferencing it.
    assert_eq!(csxcad_property_count(null), 0);
    assert_eq!(csxcad_add_property(null, value.as_ptr(), 1), 0);
    assert_eq!(csxcad_find_property(null, value.as_ptr()), 0);
    assert_eq!(csxcad_set_background_material(null, 1.0, 1.0, 0.0, 0.0), 0);
    assert_eq!(csxcad_set_material_property(null, 1, 0, 1.0, 0), 0);
    assert_eq!(csxcad_add_box(null, 1, 0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0), 0);
    assert_eq!(
        csxcad_add_cylinder(null, 1, 0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.5),
        0
    );
    assert_eq!(csxcad_add_sphere(null, 1, 0, 0.0, 0.0, 0.0, 1.0), 0);
    assert_eq!(csxcad_add_point(null, 1, 0, 0.0, 0.0, 0.0), 0);
    assert_eq!(csxcad_add_grid_line(null, 0, 1.0), 0);
    assert_eq!(csxcad_set_delta_unit(null, 1e-3), 0);
    assert_eq!(csxcad_set_coord_system(null, 0), 0);
    assert!(csxcad_to_xml_string(null).is_null());
    csxcad_clear(null);
    csxcad_free(null);
    csxcad_free_string(std::ptr::null_mut::<c_char>());
}

#[test]
fn property_identity_and_stale_id() {
    let structure = csxcad_new();
    assert!(!structure.is_null());
    let mat = name("mat");
    let metal = name("metal");

    let mat_id = csxcad_add_property(structure, mat.as_ptr(), 1);
    let metal_id = csxcad_add_property(structure, metal.as_ptr(), 2);
    assert_ne!(mat_id, 0);
    assert_ne!(metal_id, 0);
    assert_ne!(mat_id, metal_id);
    assert_eq!(csxcad_property_count(structure), 2);

    // Looking the same object up again gives the same id: the ABI identity
    // contract mirrors the C++ pointer identity.
    assert_eq!(csxcad_find_property(structure, mat.as_ptr()), mat_id);
    assert_eq!(csxcad_find_property(structure, metal.as_ptr()), metal_id);

    // clear() deletes every property (ContinuousStructure.cpp:451-467), so the
    // ids are stale afterwards and must not resolve.
    csxcad_clear(structure);
    assert_eq!(csxcad_property_count(structure), 0);
    assert_eq!(csxcad_find_property(structure, mat.as_ptr()), 0);
    assert_eq!(
        csxcad_set_material_property(structure, mat_id, 0, 2.0, 0),
        0
    );
    assert_eq!(
        csxcad_add_box(structure, mat_id, 0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0),
        0
    );

    csxcad_free(structure);
}

#[test]
fn unknown_type_and_field_codes_are_rejected() {
    let structure = csxcad_new();
    let mat = name("mat");
    let id = csxcad_add_property(structure, mat.as_ptr(), 1);
    assert_ne!(id, 0);

    // type code outside 1..=7 -> 0 and no property
    assert_eq!(csxcad_add_property(structure, mat.as_ptr(), 0), 0);
    assert_eq!(csxcad_add_property(structure, mat.as_ptr(), 8), 0);
    assert_eq!(csxcad_property_count(structure), 1);

    // field code outside 0..=3 -> 0
    assert_eq!(csxcad_set_material_property(structure, id, 4, 1.0, 0), 0);
    assert_eq!(csxcad_set_material_property(structure, id, 0, 1.0, 0), 1);

    // coordinate system code outside 0..=1 -> 0
    assert_eq!(csxcad_set_coord_system(structure, 2), 0);
    assert_eq!(csxcad_set_coord_system(structure, 0), 1);

    csxcad_free(structure);
}

#[test]
fn unknown_property_id_is_rejected() {
    let structure = csxcad_new();
    // PropertyId 424242 was never handed out: native DeleteProperty/add-on-
    // property paths find nothing and report failure instead of touching an
    // unrelated object.
    assert_eq!(
        csxcad_set_material_property(structure, 424242, 0, 1.0, 0),
        0
    );
    assert_eq!(
        csxcad_add_box(structure, 424242, 0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0),
        0
    );
    assert_eq!(
        csxcad_add_sphere(structure, 424242, 0, 0.0, 0.0, 0.0, 1.0),
        0
    );
    assert_eq!(csxcad_add_point(structure, 424242, 0, 0.0, 0.0, 0.0), 0);
    assert_eq!(
        csxcad_add_cylinder(structure, 424242, 0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.5),
        0
    );
    csxcad_free(structure);
}

#[test]
fn xml_string_ownership_is_transferred_once() {
    let structure = csxcad_new();
    let mat = name("mat");
    csxcad_add_property(structure, mat.as_ptr(), 1);
    csxcad_add_box(structure, 1, 3, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0);

    let xml = xml_of(structure);
    assert!(xml.contains("<ContinuousStructure"), "{}", xml);
    assert!(xml.contains("Name=\"mat\""), "{}", xml);
    assert!(xml.contains("<Box"), "{}", xml);
    assert!(xml.contains("Priority=\"3\""), "{}", xml);

    // The XML string is owned by the caller: parsing it back gives an
    // independent structure with the same content.
    let c_xml = CString::new(xml).expect("XML has no NUL");
    let restored = csxcad_from_xml_string(c_xml.as_ptr());
    assert!(!restored.is_null());
    assert_eq!(csxcad_property_count(restored), 1);
    let restored_xml = xml_of(restored);
    assert!(restored_xml.contains("Name=\"mat\""), "{}", restored_xml);

    csxcad_free(restored);
    csxcad_free(structure);
}

#[test]
fn invalid_xml_returns_a_null_structure() {
    let broken = name("<ContinuousStructure><Properties>");
    assert!(csxcad_from_xml_string(broken.as_ptr()).is_null());
    let empty = name("");
    assert!(csxcad_from_xml_string(empty.as_ptr()).is_null());
}

#[test]
fn background_and_grid_calls_report_success() {
    let structure = csxcad_new();
    assert_eq!(
        csxcad_set_background_material(structure, 2.0, 1.0, 0.5, 0.1),
        1
    );
    assert_eq!(csxcad_add_grid_line(structure, 0, 0.0), 1);
    assert_eq!(csxcad_add_grid_line(structure, 1, 0.0), 1);
    assert_eq!(csxcad_add_grid_line(structure, 2, 0.0), 1);
    assert_eq!(csxcad_add_grid_line(structure, 3, 0.0), 1);
    assert_eq!(csxcad_set_delta_unit(structure, 1e-3), 1);
    let xml = xml_of(structure);
    assert!(xml.contains("DeltaUnit"), "{}", xml);
    csxcad_free(structure);
}
