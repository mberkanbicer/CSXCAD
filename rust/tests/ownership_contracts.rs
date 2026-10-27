//! RM-033: ownership, lifetime and copy contracts of the CSXCAD core model.
//!
//! These are the semantics the Python/MATLAB bindings rely on, tested once at
//! the level where they are decided (the Rust model) instead of only through a
//! binding. Every expectation cites the native implementation that defines it:
//!
//! * `CSXCAD/src/ContinuousStructure.cpp:80-89`  AddProperty takes ownership,
//!   sets the owner and a fresh unique id.
//! * `CSXCAD/src/ContinuousStructure.cpp:115-126` RemoveProperty erases the
//!   entry and hands ownership back (`SetOwner(NULL)`).
//! * `CSXCAD/src/ContinuousStructure.cpp:137-150` DeleteProperty searches its
//!   own vector by pointer: deleting an unknown or already deleted property is a
//!   silent no-op.
//! * `CSXCAD/src/ContinuousStructure.cpp:451-467` clear() deletes every
//!   property and resets the background material and the grid.
//! * `CSXCAD/src/CSProperties.cpp:40-55` the copy constructor copies the id,
//!   name, attributes and metadata, and the primitives only when asked to.
//! * `CSXCAD/src/CSPrimitives.cpp:87-101` a copied primitive gets a deep copy of
//!   its transform (`CSTransform::New(prim->m_Transform)`).
//! * `CSXCAD/src/CSObject.h:46,94-104` ownership is a plain owner pointer, so a
//!   removed object belongs to nobody until another structure adopts it.

use csxcad_core::material::MaterialField;
use csxcad_core::model::{Primitive, Property, PropertyType, PropertyTypeMask, Structure};

fn material(name: &str, epsilon: f64) -> Property {
    let mut property = Property::new(name, PropertyType::Material);
    property
        .material_mut()
        .expect("material property")
        .set_value(MaterialField::Epsilon, 0, epsilon);
    property
}

fn epsilon(property: &Property) -> f64 {
    property
        .material()
        .expect("material property")
        .value(MaterialField::Epsilon, 0, None)
        .expect("epsilon evaluates")
        .expect("epsilon is a scalar")
}

fn all_properties() -> PropertyTypeMask {
    PropertyTypeMask::from_bits(0xffff)
}

/// Native `ContinuousStructure::DeleteProperty`
/// (`src/ContinuousStructure.cpp:137-150`) plus `GetQtyProperties` (line 160).
#[test]
fn deleting_a_property_twice_is_a_no_op_and_leaves_a_stale_id() {
    let mut structure = Structure::new();
    let id = structure.add_property(Property::new("metal", PropertyType::Metal));
    assert_eq!(structure.property_count(), 1);

    assert!(structure.delete_property(id));
    assert_eq!(structure.property_count(), 0);
    // second delete: the id is not in the vector any more -> false, no panic
    assert!(!structure.delete_property(id));
    assert_eq!(structure.property_count(), 0);

    // the stale id must not resolve, and it must not resolve to a property that
    // is added later (native compares pointers, not ids)
    assert!(structure.property(id).is_none());
    let replacement = structure.add_property(Property::new("metal", PropertyType::Metal));
    assert_ne!(replacement, id);
    assert!(structure.property(id).is_none());
    assert!(structure.property(replacement).is_some());
}

/// Native `ContinuousStructure::RemoveProperty` (lines 115-126): ownership is
/// handed back, the object keeps its name, attributes and primitives, and
/// another structure can adopt it with a fresh id (AddProperty, lines 80-89).
#[test]
fn removing_a_property_hands_it_back_with_its_primitives() {
    let mut structure = Structure::new();
    let id = structure.add_property(material("mat", 2.5));
    structure
        .property_mut(id)
        .expect("property")
        .set_attribute("note", "original");
    structure
        .add_primitive(
            id,
            Primitive::box_primitive([0.0, 0.0, 0.0], [1.0, 2.0, 3.0]),
        )
        .expect("box added");

    let removed = structure.remove_property(id).expect("property was removed");
    assert_eq!(structure.property_count(), 0);
    assert!(structure.property(id).is_none());
    assert_eq!(removed.name(), "mat");
    assert_eq!(removed.attribute("note"), "original");
    assert_eq!(removed.primitives().len(), 1);
    assert_eq!(epsilon(&removed), 2.5);

    // the detached property can be adopted by another structure exactly once
    let mut other = Structure::new();
    let existing = other.add_property(Property::new("existing", PropertyType::Metal));
    let new_id = other.add_property(removed);
    assert_eq!(other.property_count(), 2);
    assert_eq!(other.property(new_id).expect("adopted").name(), "mat");
    assert_eq!(
        other.property(new_id).expect("adopted").primitives().len(),
        1
    );
    // AddProperty assigns an id from the adopting structure's own counter, so
    // the adopted id is fresh in that structure and differs from the id it had
    // in the structure that released it (both counters are per structure).
    assert_ne!(new_id, existing);
    assert_ne!(new_id, id);
}

/// Native `CSProperties::CSProperties(CSProperties*, bool copyPrim)` (lines
/// 40-55): `GetCopy()` without primitives keeps id, name, attributes and the
/// material values, and drops the primitive list.
#[test]
fn property_copy_excludes_primitives_by_default() {
    let mut structure = Structure::new();
    let id = structure.add_property(material("mat", 2.5));
    structure
        .property_mut(id)
        .expect("property")
        .set_attribute("note", "original");
    structure
        .property_mut(id)
        .expect("property")
        .add_primitive(Primitive::box_primitive([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]));

    let source = structure.property(id).expect("property");
    let copy = source.get_copy();

    assert_eq!(copy.id(), source.id(), "uiID is copied, line 42");
    assert_eq!(copy.name(), "mat");
    assert_eq!(copy.attribute("note"), "original");
    assert_eq!(epsilon(&copy), 2.5);
    assert!(copy.primitives().is_empty(), "copyPrim defaults to false");
    // the source keeps its primitive
    assert_eq!(source.primitives().len(), 1);
}

/// Native `GetCopy(true)`: primitives are copied and their transforms are deep
/// copies (`src/CSPrimitives.cpp:95-96`), so editing the copy's transform or
/// geometry must not touch the original.
#[test]
fn property_copy_with_primitives_is_independent() {
    let mut property = material("mat", 2.5);
    let mut primitive = Primitive::box_primitive([0.0, 0.0, 0.0], [1.0, 2.0, 3.0]);
    primitive.transform_mut().translate([1.0, 0.0, 0.0], true);
    property.add_primitive(primitive);

    let mut copy = property.get_copy_with_primitives();
    assert_eq!(copy.primitives().len(), 1);
    assert_eq!(
        copy.primitives()[0].transform().map(|t| t.matrix()[3]),
        property.primitives()[0].transform().map(|t| t.matrix()[3])
    );

    copy.primitive_mut(0)
        .expect("copied primitive")
        .transform_mut()
        .translate([4.0, 0.0, 0.0], true);
    copy.set_name("copy");

    assert_eq!(property.name(), "mat");
    assert_eq!(
        property.primitives()[0].transform().unwrap().matrix()[3],
        1.0
    );
    assert_eq!(copy.primitives()[0].transform().unwrap().matrix()[3], 5.0);
}

/// Native `ContinuousStructure::clear()` (lines 451-467) deletes every property
/// (and with it every primitive), resets the background material (line 456) and
/// clears the grid (line 466).
#[test]
fn clear_deletes_properties_and_resets_structure_state() {
    let mut structure = Structure::new();
    let id = structure.add_property(material("mat", 4.0));
    structure
        .add_primitive(
            id,
            Primitive::box_primitive([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        )
        .expect("box");
    structure
        .background_material_mut()
        .set_value(MaterialField::Epsilon, 4.0);
    structure.grid_mut().add_line(0, 1.0);
    assert_eq!(structure.property_count(), 1);
    assert_eq!(structure.all_primitives(false, all_properties()).len(), 1);

    structure.clear();

    assert_eq!(structure.property_count(), 0);
    assert!(structure.property(id).is_none());
    assert!(structure.all_primitives(false, all_properties()).is_empty());
    assert_eq!(
        structure
            .background_material()
            .value(MaterialField::Epsilon),
        1.0
    );
    assert_eq!(structure.grid_mut().line_count(0), 0);
}

/// Native `AddProperty` re-links the primitives of an adopted property to the
/// new owner and gives them fresh ids (`SetUniqueID`/`UpdateIDs`, lines 80-89),
/// so a property that is copied and added does not alias the source primitives.
#[test]
fn adopting_a_property_relinks_its_primitives() {
    let mut source_structure = Structure::new();
    let id = source_structure.add_property(material("mat", 2.5));
    source_structure
        .add_primitive(
            id,
            Primitive::box_primitive([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        )
        .expect("box");
    let copy = source_structure
        .property(id)
        .expect("property")
        .get_copy_with_primitives();

    let mut target_structure = Structure::new();
    // occupy the target's counters first, so reassignment of the adopted ids is
    // observable (native counters are per structure, so a fresh structure would
    // hand out the same numbers)
    let occupied = target_structure.add_property(Property::new("occupied", PropertyType::Metal));
    target_structure
        .add_primitive(
            occupied,
            Primitive::box_primitive([2.0, 2.0, 2.0], [3.0, 3.0, 3.0]),
        )
        .expect("occupying box");
    let new_id = target_structure.add_property(copy);
    let adopted = target_structure.property(new_id).expect("adopted");
    assert_eq!(adopted.primitives().len(), 1);
    assert_eq!(
        adopted.primitives()[0].property_id(),
        Some(new_id),
        "the adopted primitive points at the new property"
    );
    assert_ne!(new_id, occupied);
    assert_ne!(
        adopted.primitives()[0].id(),
        source_structure.property(id).unwrap().primitives()[0].id(),
        "primitive ids are reassigned from the adopting structure's counter"
    );
    // the source structure is untouched
    assert_eq!(source_structure.property_count(), 1);
    assert_eq!(source_structure.property(id).unwrap().primitives().len(), 1);
}
