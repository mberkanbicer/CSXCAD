//! C ABI wrapper around csxcad-core for MATLAB/Octave MEX interop.
//!
//! All functions are `extern "C"` and `#[no_mangle]`. Opaque Structure pointers
//! are passed as `*mut c_void` and cast to/from `*mut csxcad_core::model::Structure`.
//! Strings use `std::ffi::CStr`/`CString`; callers must free strings returned by
//! `csxcad_to_xml_string` with `csxcad_free_string`.
//!
//! Panics are caught at the FFI boundary with `std::panic::catch_unwind` so they
//! never unwind into C. Failures return 0 / false / null as appropriate.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int, c_uint, c_void};
use std::panic;

use csxcad_core::grid::CoordinateSystem;
use csxcad_core::material::MaterialField;
use csxcad_core::model::{Primitive, Property, PropertyId, PropertyType, Structure};
use csxcad_core::serialization::StructureXmlError;

/// Convert a raw void pointer into a mutable reference to a Structure.
///
/// Returns `None` if the pointer is null. The `'static` lifetime is an FFI
/// convention: the caller owns the pointer's lifetime.
fn structure_mut(ptr: *mut c_void) -> Option<&'static mut Structure> {
    if ptr.is_null() {
        return None;
    }
    // Safety: the caller guarantees `ptr` is a valid, non-null pointer to a
    // Structure allocated by `csxcad_new` (or `csxcad_from_xml_string`).
    Some(unsafe { &mut *(ptr as *mut Structure) })
}

/// Run a closure that may panic, returning `None` if it panics.
///
/// Note: this returns `Option<R>` (not `Result`), so callers match on
/// `Some(Some(value))` when the closure itself returns an `Option`.
fn catch_unwind<F: FnOnce() -> R + std::panic::UnwindSafe, R>(f: F) -> Option<R> {
    panic::catch_unwind(f).ok()
}

/// Map MATLAB/Octave type codes to csxcad-core PropertyType.
///
/// 1=Material, 2=Metal, 3=AbsorbingBoundary, 4=Excitation,
/// 5=ProbeBox, 6=ResBox, 7=DumpBox
fn property_type_from_code(code: c_uint) -> Option<PropertyType> {
    match code {
        1 => Some(PropertyType::Material),
        2 => Some(PropertyType::Metal),
        3 => Some(PropertyType::AbsorbingBoundary),
        4 => Some(PropertyType::Excitation),
        5 => Some(PropertyType::ProbeBox),
        6 => Some(PropertyType::ResBox),
        7 => Some(PropertyType::DumpBox),
        _ => None,
    }
}

/// Map field codes to csxcad-core MaterialField.
///
/// 0=Epsilon, 1=Mue, 2=Kappa, 3=Sigma
fn material_field_from_code(code: c_uint) -> Option<MaterialField> {
    match code {
        0 => Some(MaterialField::Epsilon),
        1 => Some(MaterialField::Mue),
        2 => Some(MaterialField::Kappa),
        3 => Some(MaterialField::Sigma),
        _ => None,
    }
}

/// Map coordinate system codes to csxcad-core CoordinateSystem.
///
/// 0=Cartesian, 1=Cylindrical
fn coordinate_system_from_code(code: c_int) -> Option<CoordinateSystem> {
    match code {
        0 => Some(CoordinateSystem::Cartesian),
        1 => Some(CoordinateSystem::Cylindrical),
        _ => None,
    }
}

/// Add a primitive to a property, returning the new PrimitiveId as u64 (0 on failure).
fn add_primitive<F>(
    ptr: *mut c_void,
    property_id: u64,
    priority: c_int,
    make: F,
) -> u64
where
    F: FnOnce() -> Primitive + std::panic::UnwindSafe,
{
    let result = catch_unwind(move || {
        let structure = structure_mut(ptr)?;
        let mut primitive = make();
        primitive.set_priority(priority);
        let id = structure
            .add_primitive(PropertyId::from_u64(property_id), primitive)
            .ok()?;
        Some(id.get() as u64)
    });
    match result {
        Some(Some(id)) => id,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// Create a new empty CSXCAD Structure. Returns an opaque pointer to the caller.
#[no_mangle]
pub extern "C" fn csxcad_new() -> *mut c_void {
    let result = catch_unwind(|| {
        let structure = Box::new(Structure::new());
        Some(Box::into_raw(structure) as *mut c_void)
    });
    match result {
        Some(Some(ptr)) => ptr,
        _ => std::ptr::null_mut(),
    }
}

/// Free a Structure previously created by `csxcad_new` or `csxcad_from_xml_string`.
#[no_mangle]
pub extern "C" fn csxcad_free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let _ = catch_unwind(|| {
        // Safety: the pointer was allocated by `csxcad_new`/`csxcad_from_xml_string`
        // as a Box<Structure>.
        unsafe { drop(Box::from_raw(ptr as *mut Structure)) };
    });
}

/// Clear all properties (and grid/parameters/background material) of a Structure.
#[no_mangle]
pub extern "C" fn csxcad_clear(ptr: *mut c_void) {
    let _ = catch_unwind(|| {
        if let Some(structure) = structure_mut(ptr) {
            structure.clear();
        }
    });
}

/// Get the number of properties in the Structure.
#[no_mangle]
pub extern "C" fn csxcad_property_count(ptr: *mut c_void) -> c_uint {
    let result = catch_unwind(|| {
        let structure = structure_mut(ptr)?;
        Some(structure.property_count() as c_uint)
    });
    match result {
        Some(Some(count)) => count,
        _ => 0,
    }
}

/// Add a property (Material/Metal/etc.) to the Structure. Returns the new
/// PropertyId as u64, or 0 if the name/type is invalid.
#[no_mangle]
pub extern "C" fn csxcad_add_property(
    ptr: *mut c_void,
    name: *const c_char,
    type_code: c_uint,
) -> u64 {
    let result = catch_unwind(|| {
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
        let property_type = property_type_from_code(type_code)?;
        let structure = structure_mut(ptr)?;
        let property = Property::new(name, property_type);
        let id = structure.add_property(property);
        Some(id.get() as u64)
    });
    match result {
        Some(Some(id)) => id,
        _ => 0,
    }
}

/// Find a property by name. Returns its PropertyId as u64, or 0 if not found.
#[no_mangle]
pub extern "C" fn csxcad_find_property(ptr: *mut c_void, name: *const c_char) -> u64 {
    let result = catch_unwind(|| {
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
        let structure = structure_mut(ptr)?;
        let id = structure
            .properties_by_name(&name)
            .next()
            .and_then(|property| property.id())
            .map(|id| id.get() as u64)
            .unwrap_or(0);
        Some(id)
    });
    match result {
        Some(Some(id)) => id,
        _ => 0,
    }
}

/// Set the background material values (epsilon, mue, kappa, sigma).
/// Returns true (1) on success, false (0) on failure (e.g. null pointer or invalid value).
#[no_mangle]
pub extern "C" fn csxcad_set_background_material(
    ptr: *mut c_void,
    epsilon: c_double,
    mue: c_double,
    kappa: c_double,
    sigma: c_double,
) -> u8 {
    let result = catch_unwind(|| {
        let structure = structure_mut(ptr)?;
        let background = structure.background_material_mut();
        let ok = background.set_value(MaterialField::Epsilon, epsilon)
            && background.set_value(MaterialField::Mue, mue)
            && background.set_value(MaterialField::Kappa, kappa)
            && background.set_value(MaterialField::Sigma, sigma);
        Some(ok as u8)
    });
    match result {
        Some(Some(ok)) => ok,
        _ => 0,
    }
}

/// Set a material field value on a property's material at a component index.
/// Returns true (1) on success, false (0) if the property/material/field is invalid.
#[no_mangle]
pub extern "C" fn csxcad_set_material_property(
    ptr: *mut c_void,
    property_id: u64,
    field_code: c_uint,
    value: c_double,
    component: c_uint,
) -> u8 {
    let result = catch_unwind(|| {
        let field = material_field_from_code(field_code)?;
        let structure = structure_mut(ptr)?;
        let property = structure.property_mut(PropertyId::from_u64(property_id))?;
        let material = property.material_mut()?;
        let ok = material.set_value(field, component as usize, value);
        Some(ok as u8)
    });
    match result {
        Some(Some(ok)) => ok,
        _ => 0,
    }
}

/// Add a box primitive to a property. Returns the new PrimitiveId as u64, or 0 on failure.
#[no_mangle]
pub extern "C" fn csxcad_add_box(
    ptr: *mut c_void,
    property_id: u64,
    priority: c_int,
    start_x: c_double,
    start_y: c_double,
    start_z: c_double,
    stop_x: c_double,
    stop_y: c_double,
    stop_z: c_double,
) -> u64 {
    add_primitive(
        ptr,
        property_id,
        priority,
        move || Primitive::box_primitive([start_x, start_y, start_z], [stop_x, stop_y, stop_z]),
    )
}

/// Add a sphere primitive to a property. Returns the new PrimitiveId as u64, or 0 on failure.
#[no_mangle]
pub extern "C" fn csxcad_add_sphere(
    ptr: *mut c_void,
    property_id: u64,
    priority: c_int,
    center_x: c_double,
    center_y: c_double,
    center_z: c_double,
    radius: c_double,
) -> u64 {
    add_primitive(
        ptr,
        property_id,
        priority,
        move || Primitive::sphere([center_x, center_y, center_z], radius),
    )
}

/// Add a point primitive to a property. Returns the new PrimitiveId as u64, or 0 on failure.
#[no_mangle]
pub extern "C" fn csxcad_add_point(
    ptr: *mut c_void,
    property_id: u64,
    priority: c_int,
    x: c_double,
    y: c_double,
    z: c_double,
) -> u64 {
    add_primitive(ptr, property_id, priority, move || Primitive::point([x, y, z]))
}

/// Add a cylinder primitive to a property. Returns the new PrimitiveId as u64, or 0 on failure.
#[no_mangle]
pub extern "C" fn csxcad_add_cylinder(
    ptr: *mut c_void,
    property_id: u64,
    priority: c_int,
    start_x: c_double,
    start_y: c_double,
    start_z: c_double,
    stop_x: c_double,
    stop_y: c_double,
    stop_z: c_double,
    radius: c_double,
) -> u64 {
    add_primitive(
        ptr,
        property_id,
        priority,
        move || {
            Primitive::cylinder(
                [start_x, start_y, start_z],
                [stop_x, stop_y, stop_z],
                radius,
            )
        },
    )
}

/// Serialize the Structure to an XML string.
///
/// Returns a pointer to a NUL-terminated C string; the caller must free it with
/// `csxcad_free_string`. Returns null on failure.
#[no_mangle]
pub extern "C" fn csxcad_to_xml_string(ptr: *mut c_void) -> *mut c_char {
    let result = catch_unwind(|| {
        let structure = structure_mut(ptr)?;
        let xml = structure.to_xml_string().ok()?;
        let c_string = CString::new(xml).ok()?;
        Some(c_string.into_raw())
    });
    match result {
        Some(Some(ptr)) => ptr,
        _ => std::ptr::null_mut(),
    }
}

/// Deserialize a Structure from an XML string. Returns a new opaque Structure
/// pointer, or null if parsing fails.
#[no_mangle]
pub extern "C" fn csxcad_from_xml_string(xml: *const c_char) -> *mut c_void {
    let result = catch_unwind(|| {
        let xml_str = unsafe { CStr::from_ptr(xml) }.to_string_lossy().into_owned();
        let structure = Structure::from_xml_str(&xml_str).ok()?;
        Some(Box::into_raw(Box::new(structure)) as *mut c_void)
    });
    match result {
        Some(Some(ptr)) => ptr,
        _ => std::ptr::null_mut(),
    }
}

/// Free a string allocated by `csxcad_to_xml_string`.
#[no_mangle]
pub extern "C" fn csxcad_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    let _ = catch_unwind(|| {
        // Safety: the pointer was produced by `CString::into_raw` in
        // `csxcad_to_xml_string`.
        unsafe { drop(CString::from_raw(ptr)) };
    });
}

/// Set the coordinate system (0=Cartesian, 1=Cylindrical).
/// Returns true (1) on success, false (0) on failure (null pointer or invalid code).
#[no_mangle]
pub extern "C" fn csxcad_set_coord_system(ptr: *mut c_void, coord_system: c_int) -> u8 {
    let result = catch_unwind(|| {
        let system = coordinate_system_from_code(coord_system)?;
        let structure = structure_mut(ptr)?;
        structure.set_coordinate_system(system);
        Some(1u8)
    });
    match result {
        Some(Some(ok)) => ok,
        _ => 0,
    }
}

/// Add a grid line in the given direction at the given value.
/// Returns true (1) on success, false (0) on failure (null pointer).
#[no_mangle]
pub extern "C" fn csxcad_add_grid_line(ptr: *mut c_void, direction: c_uint, value: c_double) -> u8 {
    let result = catch_unwind(|| {
        let structure = structure_mut(ptr)?;
        structure.grid_mut().add_line(direction as usize, value);
        Some(1u8)
    });
    match result {
        Some(Some(ok)) => ok,
        _ => 0,
    }
}

/// Set the grid delta unit.
/// Returns true (1) on success, false (0) on failure (null pointer).
#[no_mangle]
pub extern "C" fn csxcad_set_delta_unit(ptr: *mut c_void, value: c_double) -> u8 {
    let result = catch_unwind(|| {
        let structure = structure_mut(ptr)?;
        structure.grid_mut().set_delta_unit(value);
        Some(1u8)
    });
    match result {
        Some(Some(ok)) => ok,
        _ => 0,
    }
}

// The error types are imported to document the intended API surface and allow
// future callers to match on them; they are converted to C-friendly return
// values within this crate.
#[allow(dead_code)]
fn _error_type_exports() {
    fn _assert_xml_error<T: std::error::Error + Send + Sync>() {}
    _assert_xml_error::<StructureXmlError>();
}
