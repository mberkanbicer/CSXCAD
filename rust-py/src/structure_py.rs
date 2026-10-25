//! Low-level Python binding for the CSXCAD `ContinuousStructure`.
//!
//! The Python layer in `python/CSXCAD/CSXCAD.py` provides the native class
//! names and argument handling; this module owns the geometry data (properties,
//! primitives, grid, background material, parameter set) and the XML I/O.

use std::path::Path;

use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::{PyResult, Python};
use pyo3::types::PyAny;
use pyo3::{pyclass, pymethods, Bound, Py, PyRefMut};

use csxcad_core::grid::CoordinateSystem;
use csxcad_core::material::MaterialField;
use csxcad_core::model::{PropertyId, PropertyTypeMask, Structure};

use crate::property_py::{property_type_from_name, PyProperty};

/// Identity token for the parameter set of a structure.
///
/// The native class only exists so that properties can resolve parameter
/// expressions against the owning structure; it exposes no Python methods.
#[pyclass(unsendable, weakref, name = "PyParameterSet")]
pub struct PyParameterSet;

#[pymethods]
impl PyParameterSet {
    #[new]
    fn py_new() -> Self {
        Self
    }
}

/// Python wrapper around the Rust `Structure` (native `ContinuousStructure`).
#[pyclass(unsendable, weakref, name = "PyStructure")]
pub struct PyStructure {
    inner: Structure,
}

impl PyStructure {
    pub(crate) fn structure(&self) -> &Structure {
        &self.inner
    }

    pub(crate) fn structure_mut(&mut self) -> &mut Structure {
        &mut self.inner
    }

    pub(crate) fn property(&self, id: PropertyId) -> Option<&csxcad_core::model::Property> {
        self.inner.property(id)
    }

    pub(crate) fn parameters(&self) -> &csxcad_core::parameter::ParameterSet {
        self.inner.parameters()
    }

    pub(crate) fn grid(&self) -> &csxcad_core::grid::RectGrid {
        self.inner.grid()
    }

    pub(crate) fn grid_mut(&mut self) -> &mut csxcad_core::grid::RectGrid {
        self.inner.grid_mut()
    }
}

fn coordinate_system(ctype: i32) -> PyResult<CoordinateSystem> {
    match ctype {
        0 => Ok(CoordinateSystem::Cartesian),
        1 => Ok(CoordinateSystem::Cylindrical),
        other => Err(PyValueError::new_err(format!(
            "Unknown coordinate system: {other}"
        ))),
    }
}

#[pymethods]
impl PyStructure {
    #[new]
    #[pyo3(signature = (CoordSystem = 0))]
    fn py_new(CoordSystem: i32) -> PyResult<Self> {
        let mut inner = Structure::new();
        inner.set_coordinate_system(coordinate_system(CoordSystem)?);
        Ok(Self { inner })
    }

    fn coord_input_type(&self) -> i32 {
        self.inner.coordinate_system() as i32
    }

    fn set_coord_input_type(&mut self, ctype: i32) -> PyResult<()> {
        self.inner.set_coordinate_system(coordinate_system(ctype)?);
        Ok(())
    }

    fn clear(&mut self) {
        self.inner.clear();
    }

    /// Serialize the structure and write it to `path`; returns the success flag
    /// of the underlying file write (native `Write2XML` returns a bool).
    fn write_xml(&self, path: &str) -> PyResult<bool> {
        let xml = self
            .inner
            .to_xml_string()
            .map_err(|error| PyValueError::new_err(format!("Write2XML: {error}")))?;
        let path = Path::new(path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.is_dir() {
                return Err(PyIOError::new_err(format!(
                    "Directory in which file is to be saved does not exist: {}",
                    parent.display()
                )));
            }
        }
        match std::fs::write(path, xml) {
            Ok(()) => Ok(true),
            Err(error) => Err(PyIOError::new_err(format!(
                "Write2XML: failed to write {}: {error}",
                path.display()
            ))),
        }
    }

    /// Read the structure from `path`; returns an empty string on success and
    /// the native error message otherwise.
    fn read_xml(&mut self, path: &str) -> String {
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) => return format!("ReadFromXML: {error}"),
        };
        match Structure::from_xml_str(&contents) {
            Ok(structure) => {
                self.inner = structure;
                String::new()
            }
            Err(error) => format!("ReadFromXML: {error}"),
        }
    }

    fn to_xml_string(&self) -> PyResult<String> {
        self.inner
            .to_xml_string()
            .map_err(|error| PyValueError::new_err(format!("ToXML: {error}")))
    }

    #[staticmethod]
    fn from_xml_string(xml: &str) -> PyResult<Self> {
        let inner = Structure::from_xml_str(xml)
            .map_err(|error| PyValueError::new_err(format!("FromXML: {error}")))?;
        Ok(Self { inner })
    }

    // -- properties --------------------------------------------------------

    fn property_count(&self) -> usize {
        self.inner.property_count()
    }

    fn property_count_by_type(&self, property_type: i32) -> PyResult<usize> {
        Ok(self
            .inner
            .property_count_by_type(property_mask(property_type)?))
    }

    fn property_ids(&self) -> Vec<u64> {
        (0..self.inner.property_count())
            .filter_map(|index| self.inner.property_by_index(index))
            .filter_map(|property| property.id())
            .map(|id| id.get())
            .collect()
    }

    fn property_ids_by_name(&self, name: &str) -> Vec<u64> {
        self.inner
            .properties_by_name(name)
            .filter_map(|property| property.id())
            .map(|id| id.get())
            .collect()
    }

    fn property_ids_by_type(&self, property_type: i32) -> PyResult<Vec<u64>> {
        Ok(self
            .inner
            .properties_by_type(property_mask(property_type)?)
            .filter_map(|property| property.id())
            .map(|id| id.get())
            .collect())
    }

    fn property_type_name(&self, id: u64) -> Option<String> {
        self.inner
            .property(PropertyId::from_u64(id))
            .map(|property| {
                crate::property_py::property_type_name(property.property_type()).to_string()
            })
    }

    /// Handle to an existing property of this structure.
    fn property_handle(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        id: u64,
    ) -> PyResult<Py<PyProperty>> {
        let property_id = PropertyId::from_u64(id);
        if slf.borrow().inner.property(property_id).is_none() {
            return Err(PyValueError::new_err("unknown property id"));
        }
        Py::new(
            py,
            PyProperty::attached_handle(slf.clone().unbind(), property_id),
        )
    }

    /// Create and add a property of the given type name; returns its id.
    fn create_property(&mut self, type_name: &str, name: &str) -> PyResult<u64> {
        let property_type = property_type_from_name(type_name).ok_or_else(|| {
            PyValueError::new_err(format!("unknown property type: {type_name}"))
        })?;
        let property = csxcad_core::model::Property::new(name, property_type);
        Ok(self.inner.add_property(property).get())
    }

    /// Add a detached property to this structure, taking ownership of it.
    fn add_property(slf: &Bound<'_, Self>, property: &Bound<'_, PyProperty>) -> PyResult<u64> {
        let mut borrowed: PyRefMut<'_, PyProperty> = property.borrow_mut();
        let local = borrowed.take_local().ok_or_else(|| {
            PyValueError::new_err("AddProperty: property already belongs to a structure")
        })?;
        drop(borrowed);
        let id = slf.borrow_mut().inner.add_property(local);
        property
            .borrow_mut()
            .attach(slf.clone().unbind(), id);
        Ok(id.get())
    }

    /// Remove a property from the structure and hand ownership back to the caller.
    fn remove_property(&mut self, py: Python<'_>, id: u64) -> PyResult<Py<PyProperty>> {
        let property = self
            .inner
            .remove_property(PropertyId::from_u64(id))
            .ok_or_else(|| PyValueError::new_err("RemoveProperty: unknown property"))?;
        Py::new(py, PyProperty::from_owned(property))
    }

    fn delete_property(&mut self, id: u64) -> bool {
        self.inner.delete_property(PropertyId::from_u64(id))
    }

    #[pyo3(signature = (coord, property_type = 0xffff, mark_found_as_used = false))]
    fn property_at_coord(
        &mut self,
        coord: [f64; 3],
        property_type: i32,
        mark_found_as_used: bool,
    ) -> PyResult<Option<u64>> {
        let found = self
            .inner
            .property_by_coord_priority(
                coord,
                property_mask(property_type)?,
                mark_found_as_used,
            )
            .map_err(|error| PyValueError::new_err(format!("GetPropertyByCoordPriority: {error}")))?;
        Ok(found.map(|(property, _primitive)| property.get()))
    }

    // -- primitives --------------------------------------------------------

    #[pyo3(signature = (sorted = false, property_type = 0xffff))]
    fn primitive_locations(
        &self,
        sorted: bool,
        property_type: i32,
    ) -> PyResult<Vec<(u64, usize)>> {
        let mask = property_mask(property_type)?;
        let mut locations = Vec::new();
        for primitive in self.inner.all_primitives(sorted, mask) {
            let Some(property_id) = primitive.property_id() else {
                continue;
            };
            let stable_id = primitive.id();
            let Some(property) = self.inner.property(property_id) else {
                continue;
            };
            let Some(index) = property
                .primitives()
                .iter()
                .position(|candidate| candidate.id() == stable_id)
            else {
                continue;
            };
            locations.push((property_id.get(), index));
        }
        Ok(locations)
    }

    fn primitive_count_by_type(&self, property_type: i32) -> PyResult<usize> {
        Ok(self
            .inner
            .primitive_count_by_type(property_mask(property_type)?))
    }

    // -- background material ----------------------------------------------

    fn background(&self, field: &str) -> PyResult<f64> {
        let field = material_field(field)?;
        Ok(self.inner.background_material().value(field))
    }

    fn set_background(&mut self, field: &str, value: f64) -> PyResult<()> {
        let field = material_field(field)?;
        if !self.inner.background_material_mut().set_value(field, value) {
            return Err(PyValueError::new_err(format!(
                "invalid background material value {value}"
            )));
        }
        Ok(())
    }

    fn reset_background(&mut self) {
        *self.inner.background_material_mut() = Default::default();
    }

    // -- validation --------------------------------------------------------

    fn update(&mut self) -> String {
        if let Err(error) = self.inner.validate_geometry_prerequisites() {
            return error.to_string();
        }
        let errors = self.inner.validate_updates();
        errors
            .iter()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn property_mask(property_type: i32) -> PyResult<PropertyTypeMask> {
    let value = u16::try_from(property_type).map_err(|_| {
        PyValueError::new_err(format!("invalid property type mask: {property_type}"))
    })?;
    Ok(PropertyTypeMask::from_bits(value))
}

fn material_field(field: &str) -> PyResult<MaterialField> {
    match field {
        "epsilon" => Ok(MaterialField::Epsilon),
        "mue" => Ok(MaterialField::Mue),
        "kappa" => Ok(MaterialField::Kappa),
        "sigma" => Ok(MaterialField::Sigma),
        other => Err(PyValueError::new_err(format!(
            "unknown background material field: {other}"
        ))),
    }
}

#[allow(dead_code)]
fn unused(_: &Bound<'_, PyAny>) {}
