//! Low-level Python binding for a CSXCAD property.
//!
//! The Python layer in `python/CSXCAD/CSProperties.py` provides the native
//! class names, argument checking and defaults; this module owns the data and
//! implements every accessor against the Rust `Property` model.
//!
//! A property is either *detached* (owns its `Property` value, used before it is
//! added to a `ContinuousStructure`) or *attached* (identified by the owning
//! structure and its `PropertyId`). Attaching moves the value into the structure,
//! exactly like the native `ContinuousStructure::AddProperty` takes ownership of
//! the C++ object, so live wrappers and their primitives keep working.

use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyValueError};
use pyo3::prelude::{PyResult, Python};
use pyo3::types::{PyAny, PyAnyMethods};
use pyo3::{pyclass, pymethods, Bound, Py};

use csxcad_core::grid::CoordinateSystem;
use csxcad_core::material::{
    AbsorbingBoundaryType, DiscreteMaterialField, LorentzParameter, LumpedElementType,
    MaterialField,
};
use csxcad_core::model::{
    Primitive, PrimitiveId, PrimitiveType, Property, PropertyColor, PropertyId, PropertyType,
};
use csxcad_core::parameter::ParameterSet;
use csxcad_core::transform::Transform;

use crate::structure_py::PyStructure;
use crate::transform_py::PyCSTransform;

fn param_error(context: &str, error: impl std::fmt::Display) -> pyo3::PyErr {
    PyValueError::new_err(format!("{context}: {error}"))
}

fn material_field(name: &str) -> Option<MaterialField> {
    match name {
        "epsilon" => Some(MaterialField::Epsilon),
        "mue" => Some(MaterialField::Mue),
        "kappa" => Some(MaterialField::Kappa),
        "sigma" => Some(MaterialField::Sigma),
        _ => None,
    }
}

/// Native `CSProperties::GetType()`: the base type plus the class hierarchy bits.
fn type_mask_value(property_type: PropertyType) -> i32 {
    const MATERIAL: i32 = 0x002;
    const METAL: i32 = 0x004;
    const DISPERSIVE: i32 = 0x100;
    const LORENTZ: i32 = 0x200;
    const DEBYE: i32 = 0x400;
    const DISCRETE: i32 = 0x1000;
    match property_type {
        PropertyType::Material => MATERIAL,
        PropertyType::Metal => METAL,
        PropertyType::ConductingSheet => 0x4000 | METAL,
        PropertyType::LorentzMaterial => LORENTZ | DISPERSIVE | MATERIAL,
        PropertyType::DebyeMaterial => DEBYE | DISPERSIVE | MATERIAL,
        PropertyType::DiscreteMaterial => DISCRETE | MATERIAL,
        PropertyType::DispersiveMaterial => DISPERSIVE | MATERIAL,
        other => other as i32,
    }
}

fn lorentz_parameter(name: &str) -> Option<LorentzParameter> {
    match name {
        "eps_plasma" => Some(LorentzParameter::EpsilonPlasmaFrequency),
        "eps_pole_freq" => Some(LorentzParameter::EpsilonLorentzPoleFrequency),
        "eps_relax" => Some(LorentzParameter::EpsilonRelaxTime),
        "mue_plasma" => Some(LorentzParameter::MuePlasmaFrequency),
        "mue_pole_freq" => Some(LorentzParameter::MueLorentzPoleFrequency),
        "mue_relax" => Some(LorentzParameter::MueRelaxTime),
        _ => None,
    }
}

/// Property type as used by the native `CSProperties.fromTypeName`.
pub(crate) fn property_type_from_name(name: &str) -> Option<PropertyType> {
    match name {
        "Material" => Some(PropertyType::Material),
        "Metal" => Some(PropertyType::Metal),
        "ConductingSheet" => Some(PropertyType::ConductingSheet),
        "LumpedElement" => Some(PropertyType::LumpedElement),
        "Excitation" => Some(PropertyType::Excitation),
        "ProbeBox" => Some(PropertyType::ProbeBox),
        "DumpBox" => Some(PropertyType::DumpBox),
        "LorentzMaterial" => Some(PropertyType::LorentzMaterial),
        "DebyeMaterial" => Some(PropertyType::DebyeMaterial),
        "AbsorbingBC" => Some(PropertyType::AbsorbingBoundary),
        "DiscMaterial" | "Discrete-Material" => Some(PropertyType::DiscreteMaterial),
        "ResBox" => Some(PropertyType::ResBox),
        "Unknown" => Some(PropertyType::Unknown),
        _ => None,
    }
}

/// Native `CSProperties::GetTypeString`.
pub(crate) fn property_type_name(property_type: PropertyType) -> &'static str {
    match property_type {
        PropertyType::Any => "Any",
        PropertyType::Unknown => "Unknown",
        PropertyType::Material => "Material",
        PropertyType::Metal => "Metal",
        PropertyType::Excitation => "Excitation",
        PropertyType::ProbeBox => "ProbeBox",
        PropertyType::ResBox => "ResBox",
        PropertyType::DumpBox => "DumpBox",
        PropertyType::DispersiveMaterial => "DispersiveMaterial",
        PropertyType::LorentzMaterial => "LorentzMaterial",
        PropertyType::DebyeMaterial => "DebyeMaterial",
        PropertyType::DiscreteMaterial => "DiscMaterial",
        PropertyType::LumpedElement => "LumpedElement",
        PropertyType::ConductingSheet => "ConductingSheet",
        PropertyType::AbsorbingBoundary => "AbsorbingBC",
    }
}

/// Native `CSPrimitives::GetType` numeric ids.
pub(crate) fn primitive_type_id(primitive_type: PrimitiveType) -> i32 {
    match primitive_type {
        PrimitiveType::Point => 0,
        PrimitiveType::Box => 1,
        PrimitiveType::MultiBox => 2,
        PrimitiveType::Sphere => 3,
        PrimitiveType::SphericalShell => 4,
        PrimitiveType::Cylinder => 5,
        PrimitiveType::CylindricalShell => 6,
        PrimitiveType::Polygon => 7,
        PrimitiveType::LinPoly => 8,
        PrimitiveType::RotPoly => 9,
        PrimitiveType::Polyhedron => 10,
        PrimitiveType::Curve => 11,
        PrimitiveType::Wire => 12,
        PrimitiveType::UserDefined => 13,
        PrimitiveType::PolyhedronReader => 14,
    }
}

/// Native `CSPrimitives::GetTypeName`.
pub(crate) fn primitive_type_name(primitive_type: PrimitiveType) -> &'static str {
    match primitive_type {
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
        PrimitiveType::Curve => "Curve",
        PrimitiveType::Wire => "Wire",
        PrimitiveType::UserDefined => "UserDefined",
        PrimitiveType::PolyhedronReader => "PolyhedronReader",
    }
}

fn direction(value: i64, context: &str) -> PyResult<usize> {
    if (0..3).contains(&value) {
        Ok(value as usize)
    } else {
        Err(PyValueError::new_err(format!(
            "{context}: direction must be 0, 1 or 2, got {value}"
        )))
    }
}

/// Python wrapper around a CSXCAD property (native `CSProperties` family).
#[pyclass(unsendable, weakref, name = "PyProperty")]
pub struct PyProperty {
    local: Option<Property>,
    owner: Option<Py<PyStructure>>,
    id: Option<PropertyId>,
    class_name: String,
}

impl PyProperty {
    /// Wrap a detached property value.
    pub(crate) fn from_owned(property: Property) -> Self {
        Self {
            local: Some(property),
            owner: None,
            id: None,
            class_name: "CSProperties".to_string(),
        }
    }

    /// Handle to a property that already lives in a structure.
    pub(crate) fn attached_handle(owner: Py<PyStructure>, id: PropertyId) -> Self {
        Self {
            local: None,
            owner: Some(owner),
            id: Some(id),
            class_name: "CSProperties".to_string(),
        }
    }

    pub(crate) fn take_local(&mut self) -> Option<Property> {
        self.local.take()
    }

    pub(crate) fn attach(&mut self, owner: Py<PyStructure>, id: PropertyId) {
        self.owner = Some(owner);
        self.id = Some(id);
        self.local = None;
    }

    fn deleted(&self) -> pyo3::PyErr {
        PyRuntimeError::new_err(format!(
            "wrapped C++ object of type {} has been deleted",
            self.class_name
        ))
    }

    pub(crate) fn with<R>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&Property, Option<&ParameterSet>) -> PyResult<R>,
    ) -> PyResult<R> {
        if let Some(local) = &self.local {
            return f(local, None);
        }
        let (Some(owner), Some(id)) = (&self.owner, self.id) else {
            return Err(self.deleted());
        };
        let structure = owner.borrow(py);
        let Some(property) = structure.property(id) else {
            return Err(self.deleted());
        };
        f(property, Some(structure.parameters()))
    }

    fn with_mut<R>(
        &mut self,
        py: Python<'_>,
        f: impl FnOnce(&mut Property, Option<&ParameterSet>) -> PyResult<R>,
    ) -> PyResult<R> {
        if let Some(local) = &mut self.local {
            return f(local, None);
        }
        let (Some(owner), Some(id)) = (&self.owner, self.id) else {
            return Err(self.deleted());
        };
        let mut structure = owner.borrow_mut(py);
        let Some((property, parameters)) =
            structure.structure_mut().property_with_parameters_mut(id)
        else {
            return Err(self.deleted());
        };
        f(property, Some(parameters))
    }

    pub(crate) fn with_primitive<R>(
        &self,
        py: Python<'_>,
        index: usize,
        f: impl FnOnce(&Primitive, Option<&ParameterSet>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.with(py, |property, parameters| {
            let primitive = property.primitive(index).ok_or_else(|| {
                PyIndexError::new_err(format!(
                    "primitive index {index} out of range for {} primitives",
                    property.primitive_count()
                ))
            })?;
            f(primitive, parameters)
        })
    }

    pub(crate) fn with_primitive_mut<R>(
        &mut self,
        py: Python<'_>,
        index: usize,
        f: impl FnOnce(&mut Primitive, Option<&ParameterSet>) -> PyResult<R>,
    ) -> PyResult<R> {
        self.with_mut(py, |property, parameters| {
            let count = property.primitive_count();
            let primitive = property.primitive_mut(index).ok_or_else(|| {
                PyIndexError::new_err(format!(
                    "primitive index {index} out of range for {count} primitives"
                ))
            })?;
            f(primitive, parameters)
        })
    }

    fn add_primitive(&mut self, py: Python<'_>, primitive: Primitive) -> PyResult<usize> {
        let mut index = 0;
        self.with_mut(py, |property, _| {
            property.add_primitive(primitive);
            index = property.primitive_count() - 1;
            Ok(())
        })?;
        Ok(index)
    }
}

#[pymethods]
impl PyProperty {
    #[new]
    fn py_new(type_name: &str) -> PyResult<Self> {
        let property_type = property_type_from_name(type_name)
            .ok_or_else(|| PyValueError::new_err(format!("unknown property type: {type_name}")))?;
        Ok(Self::from_owned(Property::new("", property_type)))
    }

    fn _set_class_name(&mut self, name: &str) {
        self.class_name = name.to_string();
    }

    fn _class_name(&self) -> String {
        self.class_name.clone()
    }

    fn _is_attached(&self) -> bool {
        self.owner.is_some()
    }

    fn _id(&self) -> Option<u64> {
        self.id.map(PropertyId::get)
    }

    fn _type_string(&self) -> String {
        self.local
            .as_ref()
            .map(|property| property_type_name(property.property_type()).to_string())
            .unwrap_or_default()
    }

    fn type_string(&self, py: Python<'_>) -> PyResult<String> {
        self.with(py, |property, _| {
            Ok(property_type_name(property.property_type()).to_string())
        })
    }

    fn type_id(&self, py: Python<'_>) -> PyResult<i32> {
        self.with(py, |property, _| {
            Ok(type_mask_value(property.property_type()))
        })
    }

    fn is_material(&self, py: Python<'_>) -> PyResult<bool> {
        self.with(py, |property, _| {
            Ok(matches!(
                property.property_type(),
                PropertyType::Material
                    | PropertyType::Metal
                    | PropertyType::ConductingSheet
                    | PropertyType::DispersiveMaterial
                    | PropertyType::LorentzMaterial
                    | PropertyType::DebyeMaterial
                    | PropertyType::DiscreteMaterial
            ))
        })
    }

    fn name(&self, py: Python<'_>) -> PyResult<String> {
        self.with(py, |property, _| Ok(property.name().to_string()))
    }

    fn set_name(&mut self, py: Python<'_>, name: &str) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            property.set_name(name.to_string());
            Ok(())
        })
    }

    fn fill_color(&self, py: Python<'_>) -> PyResult<Option<(u8, u8, u8, u8)>> {
        self.with(py, |property, _| {
            Ok(property.fill_color().map(|color| {
                let [red, green, blue, alpha] = color.components();
                (red, green, blue, alpha)
            }))
        })
    }

    fn set_fill_color(
        &mut self,
        py: Python<'_>,
        red: u8,
        green: u8,
        blue: u8,
        alpha: u8,
    ) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            property.set_fill_color(Some(PropertyColor::new(red, green, blue, alpha)));
            Ok(())
        })
    }

    fn edge_color(&self, py: Python<'_>) -> PyResult<Option<(u8, u8, u8, u8)>> {
        self.with(py, |property, _| {
            Ok(property.edge_color().map(|color| {
                let [red, green, blue, alpha] = color.components();
                (red, green, blue, alpha)
            }))
        })
    }

    fn set_edge_color(
        &mut self,
        py: Python<'_>,
        red: u8,
        green: u8,
        blue: u8,
        alpha: u8,
    ) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            property.set_edge_color(Some(PropertyColor::new(red, green, blue, alpha)));
            Ok(())
        })
    }

    fn visible(&self, py: Python<'_>) -> PyResult<bool> {
        self.with(py, |property, _| Ok(property.is_visible()))
    }

    fn set_visible(&mut self, py: Python<'_>, visible: bool) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            property.set_visible(visible);
            Ok(())
        })
    }

    fn coord_input_type(&self, py: Python<'_>) -> PyResult<i32> {
        self.with(
            py,
            |property, _| Ok(property.coordinate_input_type() as i32),
        )
    }

    fn set_coord_input_type(&mut self, py: Python<'_>, ctype: i32) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            property.set_coordinate_input_type(coordinate_system(ctype)?);
            Ok(())
        })
    }

    // -- attributes --------------------------------------------------------

    fn has_attribute(&self, py: Python<'_>, name: &str) -> PyResult<bool> {
        self.with(py, |property, _| Ok(property.has_attribute(name)))
    }

    fn attribute(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        self.with(py, |property, _| {
            if !property.has_attribute(name) {
                return Err(PyValueError::new_err(format!(
                    "attribute '{name}' does not exist"
                )));
            }
            Ok(property.attribute(name).to_string())
        })
    }

    fn set_attribute(&mut self, py: Python<'_>, name: &str, value: &str) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            property.set_attribute(name.to_string(), value.to_string());
            Ok(())
        })
    }

    fn remove_attribute(&mut self, py: Python<'_>, name: &str) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            property.remove_attribute(name);
            Ok(())
        })
    }

    fn attribute_names(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        self.with(py, |property, _| {
            Ok(property
                .attribute_names()
                .map(|name| name.to_string())
                .collect())
        })
    }

    fn copy(&self, py: Python<'_>, incl_primitives: bool) -> PyResult<Self> {
        self.with(py, |property, _| {
            let copy = if incl_primitives {
                property.get_copy_with_primitives()
            } else {
                property.get_copy()
            };
            Ok(Self::from_owned(copy))
        })
    }

    // -- scalar fields -----------------------------------------------------

    fn f64_field(&self, py: Python<'_>, field: &str, component: usize) -> PyResult<f64> {
        self.with(py, |property, parameters| {
            get_f64(property, field, component, parameters)
        })
    }

    fn set_f64_field(
        &mut self,
        py: Python<'_>,
        field: &str,
        component: usize,
        value: f64,
    ) -> PyResult<()> {
        self.with_mut(py, |property, parameters| {
            set_f64(property, field, component, value, parameters)
        })
    }

    fn int_field(&self, py: Python<'_>, field: &str, component: usize) -> PyResult<i64> {
        self.with(py, |property, _| get_int(property, field, component))
    }

    fn set_int_field(
        &mut self,
        py: Python<'_>,
        field: &str,
        component: usize,
        value: i64,
    ) -> PyResult<()> {
        self.with_mut(py, |property, _| set_int(property, field, component, value))
    }

    fn bool_field(&self, py: Python<'_>, field: &str) -> PyResult<bool> {
        self.with(py, |property, _| get_bool(property, field))
    }

    fn set_bool_field(&mut self, py: Python<'_>, field: &str, value: bool) -> PyResult<()> {
        self.with_mut(py, |property, _| set_bool(property, field, value))
    }

    fn string_field(&self, py: Python<'_>, field: &str, component: usize) -> PyResult<String> {
        self.with(py, |property, _| get_string(property, field, component))
    }

    fn set_string_field(
        &mut self,
        py: Python<'_>,
        field: &str,
        component: usize,
        value: &str,
    ) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            set_string(property, field, component, value)
        })
    }

    // -- dispersive materials ---------------------------------------------

    fn dispersive_order(&self, py: Python<'_>) -> PyResult<usize> {
        self.with(py, |property, _| {
            if let Some(debye) = property.debye_material() {
                return Ok(debye.order());
            }
            if let Some(lorentz) = property.lorentz_material() {
                return Ok(lorentz.order());
            }
            Err(PyValueError::new_err(
                "property is not a dispersive material",
            ))
        })
    }

    fn set_dispersive_order(&mut self, py: Python<'_>, order: usize) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            if let Some(debye) = property.debye_material_mut() {
                debye.set_order(order);
                return Ok(());
            }
            if let Some(lorentz) = property.lorentz_material_mut() {
                lorentz.set_order(order);
                return Ok(());
            }
            Err(PyValueError::new_err(
                "property is not a dispersive material",
            ))
        })
    }

    fn dispersive_field(
        &self,
        py: Python<'_>,
        field: &str,
        order: usize,
        component: usize,
    ) -> PyResult<f64> {
        self.with(py, |property, parameters| {
            get_dispersive(property, field, order, component, parameters)
        })
    }

    fn set_dispersive_field(
        &mut self,
        py: Python<'_>,
        field: &str,
        order: usize,
        component: usize,
        value: f64,
    ) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            set_dispersive(property, field, order, component, value)
        })
    }

    fn dispersive_weight(
        &self,
        py: Python<'_>,
        field: &str,
        order: usize,
        component: usize,
    ) -> PyResult<String> {
        self.with(py, |property, _| {
            get_dispersive_weight(property, field, order, component)
        })
    }

    fn set_dispersive_weight(
        &mut self,
        py: Python<'_>,
        field: &str,
        order: usize,
        component: usize,
        expression: &str,
    ) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            set_dispersive_weight(property, field, order, component, expression)
        })
    }

    // -- probe sampling ----------------------------------------------------

    fn frequencies(&self, py: Python<'_>) -> PyResult<Vec<f64>> {
        self.with(py, |property, _| {
            let probe = property
                .probe()
                .ok_or_else(|| PyValueError::new_err("property is not a probe"))?;
            Ok(probe.fd_samples().to_vec())
        })
    }

    fn set_frequencies(&mut self, py: Python<'_>, samples: Vec<f64>) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            let probe = property
                .probe_mut()
                .ok_or_else(|| PyValueError::new_err("property is not a probe"))?;
            probe.set_fd_samples(samples);
            Ok(())
        })
    }

    fn add_frequency(&mut self, py: Python<'_>, frequency: f64) -> PyResult<()> {
        self.with_mut(py, |property, _| {
            let probe = property
                .probe_mut()
                .ok_or_else(|| PyValueError::new_err("property is not a probe"))?;
            let mut samples = probe.fd_samples().to_vec();
            samples.push(frequency);
            probe.set_fd_samples(samples);
            Ok(())
        })
    }

    // -- discrete material -------------------------------------------------

    fn weighted_discrete(
        &self,
        py: Python<'_>,
        field: &str,
        component: usize,
        coordinates: [f64; 3],
    ) -> PyResult<f64> {
        self.with(py, |property, parameters| {
            let discrete = property
                .discrete_material()
                .ok_or_else(|| PyValueError::new_err("property is not a discrete material"))?;
            let (field, material_field) = match field {
                "epsilon" => (DiscreteMaterialField::Epsilon, Some(MaterialField::Epsilon)),
                "mue" => (DiscreteMaterialField::Mue, Some(MaterialField::Mue)),
                "kappa" => (DiscreteMaterialField::Kappa, Some(MaterialField::Kappa)),
                "sigma" => (DiscreteMaterialField::Sigma, Some(MaterialField::Sigma)),
                "density" => (DiscreteMaterialField::Density, None),
                other => {
                    return Err(PyValueError::new_err(format!(
                        "unknown discrete material field: {other}"
                    )))
                }
            };
            let grid = discrete
                .load_grid()
                .map_err(|error| PyValueError::new_err(format!("ReadFile: {error}")))?;
            let value = grid
                .material_value(
                    field,
                    coordinates,
                    property.coordinate_input_type(),
                    discrete,
                )
                .map_err(|error| param_error("GetWeighted", error))?;
            match (value, material_field) {
                (Some(value), _) => Ok(value),
                // outside of the file grid: the native property falls back to the
                // homogeneous material value of the same field
                (None, Some(field)) => property
                    .material()
                    .ok_or_else(|| PyValueError::new_err("discrete material has no material"))?
                    .value(field, component, parameters)
                    .map_err(|error| param_error("GetWeighted", error))?
                    .ok_or_else(|| PyIndexError::new_err("invalid field component")),
                (None, None) => property
                    .material()
                    .ok_or_else(|| PyValueError::new_err("discrete material has no material"))?
                    .density(parameters)
                    .map_err(|error| param_error("GetDensityWeighted", error)),
            }
        })
    }

    /// Weighted excitation amplitude (native `GetWeightedExcitation`).
    fn weighted_excitation(
        &mut self,
        py: Python<'_>,
        component: usize,
        coordinates: [f64; 3],
    ) -> PyResult<f64> {
        self.with_mut(py, |property, parameters| {
            let coordinate_system = property.coordinate_input_type();
            let excitation = property
                .excitation_mut()
                .ok_or_else(|| PyValueError::new_err("property is not an excitation"))?;
            excitation
                .weighted_excitation(component, parameters, coordinates, coordinate_system)
                .map_err(|error| param_error("GetWeightedExcitation", error))
        })
    }

    fn read_file(&mut self, py: Python<'_>) -> PyResult<bool> {
        self.with_mut(py, |property, _| {
            let discrete = property
                .discrete_material_mut()
                .ok_or_else(|| PyValueError::new_err("property is not a discrete material"))?;
            match discrete.load_grid() {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        })
    }

    /// Lazily created transform of a discrete material (native
    /// `CSPropDiscMaterial::GetTransform` creates one on first access).
    fn discrete_transform(&mut self, py: Python<'_>) -> PyResult<Py<PyCSTransform>> {
        let transform = self.with_mut(py, |property, _| {
            let discrete = property
                .discrete_material_mut()
                .ok_or_else(|| PyValueError::new_err("property is not a discrete material"))?;
            if discrete.transform().is_none() {
                discrete.set_transform(Some(Transform::new()));
            }
            let transform = discrete.transform().cloned().unwrap_or_else(Transform::new);
            Ok(transform)
        })?;
        Py::new(py, PyCSTransform::from_transform(transform))
    }

    fn transform(&self, py: Python<'_>) -> PyResult<Option<Py<PyCSTransform>>> {
        let transform = self.with(py, |property, _| {
            let discrete = property
                .discrete_material()
                .ok_or_else(|| PyValueError::new_err("property is not a discrete material"))?;
            Ok(discrete.transform().cloned())
        })?;
        transform
            .map(|transform| Py::new(py, PyCSTransform::from_transform(transform)))
            .transpose()
    }

    fn set_transform(
        &mut self,
        py: Python<'_>,
        transform: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let value = match transform {
            None => None,
            Some(value) if value.is_none() => None,
            Some(value) => Some(
                value
                    .extract::<pyo3::PyRef<'_, PyCSTransform>>()?
                    .transform()
                    .clone(),
            ),
        };
        self.with_mut(py, |property, _| {
            let discrete = property
                .discrete_material_mut()
                .ok_or_else(|| PyValueError::new_err("property is not a discrete material"))?;
            discrete.set_transform(value);
            Ok(())
        })
    }

    // -- primitives --------------------------------------------------------

    fn primitive_count(&self, py: Python<'_>) -> PyResult<usize> {
        self.with(py, |property, _| Ok(property.primitive_count()))
    }

    fn primitive_id(&self, py: Python<'_>, index: usize) -> PyResult<Option<u64>> {
        self.with_primitive(py, index, |primitive, _| {
            Ok(primitive.id().map(PrimitiveId::get))
        })
    }

    /// Detached copy of one primitive (native `CSPrimitives.GetCopy()`).
    ///
    /// The clone lives in a hidden property owned by the returned handle, so the
    /// copy keeps working independently of the property it was copied from.
    fn copy_primitive(&self, py: Python<'_>, index: usize) -> PyResult<Py<PyProperty>> {
        let primitive = self.with_primitive(py, index, |primitive, _| Ok(primitive.clone()))?;
        let mut holder = PyProperty::from_owned(Property::new("", PropertyType::Material));
        holder.with_mut(py, |property, _| {
            property.add_primitive(primitive);
            Ok(())
        })?;
        Py::new(py, holder)
    }

    fn create_point(&mut self, py: Python<'_>, coord: [f64; 3]) -> PyResult<usize> {
        self.add_primitive(py, Primitive::point(coord))
    }

    fn create_box(&mut self, py: Python<'_>, start: [f64; 3], stop: [f64; 3]) -> PyResult<usize> {
        self.add_primitive(py, Primitive::box_primitive(start, stop))
    }

    fn create_sphere(&mut self, py: Python<'_>, center: [f64; 3], radius: f64) -> PyResult<usize> {
        self.add_primitive(py, Primitive::sphere(center, radius))
    }

    fn create_spherical_shell(
        &mut self,
        py: Python<'_>,
        center: [f64; 3],
        radius: f64,
        shell_width: f64,
    ) -> PyResult<usize> {
        self.add_primitive(py, Primitive::spherical_shell(center, radius, shell_width))
    }

    fn create_cylinder(
        &mut self,
        py: Python<'_>,
        start: [f64; 3],
        stop: [f64; 3],
        radius: f64,
    ) -> PyResult<usize> {
        self.add_primitive(py, Primitive::cylinder(start, stop, radius))
    }

    fn create_cylindrical_shell(
        &mut self,
        py: Python<'_>,
        start: [f64; 3],
        stop: [f64; 3],
        radius: f64,
        shell_width: f64,
    ) -> PyResult<usize> {
        self.add_primitive(
            py,
            Primitive::cylindrical_shell(start, stop, radius, shell_width),
        )
    }

    fn create_polygon(
        &mut self,
        py: Python<'_>,
        points: (Vec<f64>, Vec<f64>),
        norm_dir: i64,
        elevation: f64,
    ) -> PyResult<usize> {
        let primitive = polygon_primitive(Primitive::polygon(), points, norm_dir, elevation)?;
        self.add_primitive(py, primitive)
    }

    fn create_linpoly(
        &mut self,
        py: Python<'_>,
        points: (Vec<f64>, Vec<f64>),
        norm_dir: i64,
        elevation: f64,
        length: f64,
    ) -> PyResult<usize> {
        let primitive =
            polygon_primitive(Primitive::lin_poly(length), points, norm_dir, elevation)?;
        self.add_primitive(py, primitive)
    }

    fn create_rotpoly(
        &mut self,
        py: Python<'_>,
        points: (Vec<f64>, Vec<f64>),
        norm_dir: i64,
        elevation: f64,
        rot_axis: i64,
        angle: (f64, f64),
    ) -> PyResult<usize> {
        let mut primitive = Primitive::rot_poly();
        let axis = direction(rot_axis, "SetRotAxisDir")?;
        {
            let shape = polygon_shape_mut(&mut primitive)?;
            if !shape.set_rotation_axis_direction(axis) {
                return Err(PyValueError::new_err("SetRotAxisDir: invalid direction"));
            }
            shape
                .set_rotation_angle_expression(0, &format!("{}", angle.0))
                .map_err(|error| param_error("SetAngle", error))?;
            shape
                .set_rotation_angle_expression(1, &format!("{}", angle.1))
                .map_err(|error| param_error("SetAngle", error))?;
        }
        let primitive = polygon_primitive(primitive, points, norm_dir, elevation)?;
        self.add_primitive(py, primitive)
    }

    fn create_curve(&mut self, py: Python<'_>, points: Vec<[f64; 3]>) -> PyResult<usize> {
        let mut curve = Primitive::curve();
        if let Some(shape) = curve.curve_shape_mut() {
            for point in points {
                shape.add_point(point);
            }
        }
        self.add_primitive(py, curve)
    }

    fn create_wire(
        &mut self,
        py: Python<'_>,
        points: Vec<[f64; 3]>,
        radius: f64,
    ) -> PyResult<usize> {
        let mut wire = Primitive::wire(radius);
        if let Some(shape) = wire.wire_shape_mut() {
            for point in points {
                shape.add_point(point);
            }
        }
        self.add_primitive(py, wire)
    }

    fn create_polyhedron(&mut self, py: Python<'_>) -> PyResult<usize> {
        self.add_primitive(py, Primitive::polyhedron())
    }

    fn create_polyhedron_reader(&mut self, py: Python<'_>, filename: &str) -> PyResult<usize> {
        let mut reader = Primitive::polyhedron_reader();
        if let Some(shape) = reader.polyhedron_reader_shape_mut() {
            shape.set_filename(filename.to_string());
            shape.set_file_type(file_type_from_name(filename));
        }
        self.add_primitive(py, reader)
    }

    fn create_multibox(
        &mut self,
        py: Python<'_>,
        boxes: Vec<([f64; 3], [f64; 3])>,
    ) -> PyResult<usize> {
        let mut multibox = Primitive::multibox();
        if let Some(shape) = multibox.multibox_shape_mut() {
            for (start, stop) in boxes {
                shape.add_box(csxcad_core::model::BoxPrimitive::new(start, stop));
            }
        }
        self.add_primitive(py, multibox)
    }

    // -- primitive base ----------------------------------------------------

    fn primitive_type_id(&self, py: Python<'_>, index: usize) -> PyResult<i32> {
        self.with_primitive(py, index, |primitive, _| {
            Ok(primitive_type_id(primitive.primitive_type()))
        })
    }

    fn primitive_type_name(&self, py: Python<'_>, index: usize) -> PyResult<String> {
        self.with_primitive(py, index, |primitive, _| {
            Ok(primitive_type_name(primitive.primitive_type()).to_string())
        })
    }

    fn primitive_id_value(&self, py: Python<'_>, index: usize) -> PyResult<u64> {
        self.with_primitive(py, index, |primitive, _| {
            Ok(primitive.id().map(PrimitiveId::get).unwrap_or(0))
        })
    }

    fn primitive_priority(&self, py: Python<'_>, index: usize) -> PyResult<i32> {
        self.with_primitive(py, index, |primitive, _| Ok(primitive.priority()))
    }

    fn set_primitive_priority(
        &mut self,
        py: Python<'_>,
        index: usize,
        priority: i32,
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            primitive.set_priority(priority);
            Ok(())
        })
    }

    fn primitive_used(&self, py: Python<'_>, index: usize) -> PyResult<bool> {
        self.with_primitive(py, index, |primitive, _| Ok(primitive.is_used()))
    }

    fn set_primitive_used(&mut self, py: Python<'_>, index: usize, used: bool) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            primitive.set_used(used);
            Ok(())
        })
    }

    fn primitive_bound_box(&self, py: Python<'_>, index: usize) -> PyResult<[f64; 6]> {
        self.with_primitive(py, index, |primitive, parameters| {
            primitive
                .bounding_box(parameters, false)
                .map_err(|error| param_error("GetBoundBox", error))
        })
    }

    fn primitive_dimension(&self, py: Python<'_>, index: usize) -> PyResult<i32> {
        self.with_primitive(py, index, |primitive, parameters| {
            primitive
                .dimension(parameters)
                .map_err(|error| param_error("GetDimension", error))
        })
    }

    #[pyo3(signature = (index, coord, tol = 0.0))]
    fn primitive_is_inside(
        &self,
        py: Python<'_>,
        index: usize,
        coord: [f64; 3],
        tol: f64,
    ) -> PyResult<bool> {
        let _ = tol; // native tolerance is applied inside the shape test
        self.with_primitive(py, index, |primitive, parameters| {
            primitive
                .is_inside(coord, parameters)
                .map_err(|error| param_error("IsInside", error))
        })
    }

    fn primitive_coordinate_system(&self, py: Python<'_>, index: usize) -> PyResult<Option<i32>> {
        self.with_primitive(py, index, |primitive, _| {
            Ok(match primitive.coordinate_system() {
                CoordinateSystem::Undefined => None,
                system => Some(system as i32),
            })
        })
    }

    fn set_primitive_coordinate_system(
        &mut self,
        py: Python<'_>,
        index: usize,
        ctype: Option<i32>,
    ) -> PyResult<()> {
        let system = match ctype {
            None => CoordinateSystem::Undefined,
            Some(ctype) => coordinate_system(ctype)?,
        };
        self.with_primitive_mut(py, index, |primitive, _| {
            primitive.set_coordinate_system(system);
            Ok(())
        })
    }

    fn primitive_update(&self, py: Python<'_>, index: usize) -> PyResult<(bool, String)> {
        self.with_primitive(py, index, |primitive, parameters| {
            if let Err(error) = primitive.bounding_box(parameters, false) {
                return Ok((false, error.to_string()));
            }
            if let Err(error) = primitive.dimension(parameters) {
                return Ok((false, error.to_string()));
            }
            Ok((true, String::new()))
        })
    }

    // -- primitive transforms ---------------------------------------------

    fn primitive_has_transform(&self, py: Python<'_>, index: usize) -> PyResult<bool> {
        self.with_primitive(py, index, |primitive, _| Ok(primitive.has_transform()))
    }

    fn primitive_transform(
        slf: Bound<'_, Self>,
        py: Python<'_>,
        index: usize,
    ) -> PyResult<Py<PyCSTransform>> {
        {
            let mut this = slf.borrow_mut();
            this.with_primitive_mut(py, index, |primitive, _| {
                primitive.transform_mut();
                Ok(())
            })?;
        }
        Py::new(py, PyCSTransform::attached(slf.unbind(), index))
    }

    // -- primitive geometry -----------------------------------------------

    fn primitive_coord(&self, py: Python<'_>, index: usize, coord: usize) -> PyResult<f64> {
        self.with_primitive(py, index, |primitive, parameters| {
            primitive_coordinate(primitive, coord, parameters)
        })
    }

    fn set_primitive_coord(
        &mut self,
        py: Python<'_>,
        index: usize,
        coord: usize,
        value: f64,
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            set_primitive_coordinate(primitive, coord, value)
        })
    }

    fn primitive_f64(&self, py: Python<'_>, index: usize, field: &str) -> PyResult<f64> {
        self.with_primitive(py, index, |primitive, parameters| {
            primitive_f64(primitive, field, parameters)
        })
    }

    fn set_primitive_f64(
        &mut self,
        py: Python<'_>,
        index: usize,
        field: &str,
        value: f64,
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            set_primitive_f64(primitive, field, value)
        })
    }

    fn primitive_int(&self, py: Python<'_>, index: usize, field: &str) -> PyResult<i64> {
        self.with_primitive(py, index, |primitive, _| primitive_int(primitive, field))
    }

    fn set_primitive_int(
        &mut self,
        py: Python<'_>,
        index: usize,
        field: &str,
        value: i64,
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            set_primitive_int(primitive, field, value)
        })
    }

    fn primitive_points(&self, py: Python<'_>, index: usize) -> PyResult<Vec<[f64; 3]>> {
        self.with_primitive(py, index, |primitive, parameters| {
            let shape = curve_shape(primitive)?;
            let mut points = Vec::with_capacity(shape.point_count());
            for point in 0..shape.point_count() {
                points.push(
                    shape
                        .point(point, parameters)
                        .map_err(|error| param_error("GetPoint", error))?
                        .ok_or_else(|| PyIndexError::new_err("GetPoint: invalid index"))?,
                );
            }
            Ok(points)
        })
    }

    fn primitive_polygon_points(
        &self,
        py: Python<'_>,
        index: usize,
    ) -> PyResult<(Vec<f64>, Vec<f64>)> {
        self.with_primitive(py, index, |primitive, parameters| {
            let shape = polygon_shape(primitive)?;
            let mut x0 = Vec::with_capacity(shape.vertex_count());
            let mut x1 = Vec::with_capacity(shape.vertex_count());
            for vertex in 0..shape.vertex_count() {
                let [a, b] = shape
                    .vertex(vertex, parameters)
                    .map_err(|error| param_error("GetCoords", error))?
                    .ok_or_else(|| PyIndexError::new_err("GetCoords: invalid vertex"))?;
                x0.push(a);
                x1.push(b);
            }
            Ok((x0, x1))
        })
    }

    fn set_primitive_polygon_points(
        &mut self,
        py: Python<'_>,
        index: usize,
        points: (Vec<f64>, Vec<f64>),
    ) -> PyResult<()> {
        let (x0, x1) = points;
        if x0.len() != x1.len() || x0.is_empty() {
            return Err(PyValueError::new_err(
                "SetCoords: x0/x1 must be non-empty and of equal length",
            ));
        }
        self.with_primitive_mut(py, index, |primitive, _| {
            let shape = polygon_shape_mut(primitive)?;
            shape.clear_vertices();
            for (a, b) in x0.iter().zip(x1.iter()) {
                shape.add_vertex([*a, *b]);
            }
            Ok(())
        })
    }

    fn primitive_clear_points(&mut self, py: Python<'_>, index: usize) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            curve_shape_mut(primitive)?.clear_points();
            Ok(())
        })
    }

    fn primitive_rotation_angles(&self, py: Python<'_>, index: usize) -> PyResult<(f64, f64)> {
        self.with_primitive(py, index, |primitive, parameters| {
            let shape = polygon_shape(primitive)?;
            let angles = shape
                .rotation_angles(parameters)
                .map_err(|error| param_error("GetAngle", error))?
                .ok_or_else(|| PyValueError::new_err("GetAngle: primitive is not a rotpoly"))?;
            Ok((angles[0], angles[1]))
        })
    }

    fn set_primitive_rotation_angles(
        &mut self,
        py: Python<'_>,
        index: usize,
        start: f64,
        stop: f64,
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            let shape = polygon_shape_mut(primitive)?;
            shape
                .set_rotation_angle_expression(0, &format!("{start}"))
                .map_err(|error| param_error("SetAngle", error))?;
            shape
                .set_rotation_angle_expression(1, &format!("{stop}"))
                .map_err(|error| param_error("SetAngle", error))?;
            Ok(())
        })
    }

    fn primitive_string(&self, py: Python<'_>, index: usize, field: &str) -> PyResult<String> {
        self.with_primitive(py, index, |primitive, _| {
            let shape = primitive
                .polyhedron_reader_shape()
                .ok_or_else(|| PyValueError::new_err("primitive is not a polyhedron reader"))?;
            match field {
                "filename" => Ok(shape.filename().to_string()),
                other => Err(PyValueError::new_err(format!(
                    "unknown primitive string field: {other}"
                ))),
            }
        })
    }

    fn set_primitive_string(
        &mut self,
        py: Python<'_>,
        index: usize,
        field: &str,
        value: &str,
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            let shape = primitive
                .polyhedron_reader_shape_mut()
                .ok_or_else(|| PyValueError::new_err("primitive is not a polyhedron reader"))?;
            match field {
                "filename" => {
                    shape.set_filename(value.to_string());
                    Ok(())
                }
                other => Err(PyValueError::new_err(format!(
                    "unknown primitive string field: {other}"
                ))),
            }
        })
    }

    fn primitive_add_point(
        &mut self,
        py: Python<'_>,
        index: usize,
        point: [f64; 3],
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            curve_shape_mut(primitive)?.add_point(point);
            Ok(())
        })
    }

    fn primitive_vertex_count(&self, py: Python<'_>, index: usize) -> PyResult<usize> {
        self.with_primitive(py, index, |primitive, _| {
            Ok(polyhedron_shape(primitive)?.vertex_count())
        })
    }

    fn primitive_face_count(&self, py: Python<'_>, index: usize) -> PyResult<usize> {
        self.with_primitive(py, index, |primitive, _| {
            Ok(polyhedron_shape(primitive)?.face_count())
        })
    }

    fn primitive_vertex(&self, py: Python<'_>, index: usize, vertex: usize) -> PyResult<[f64; 3]> {
        self.with_primitive(py, index, |primitive, _| {
            polyhedron_shape(primitive)?
                .vertex(vertex)
                .map(|[x, y, z]| [f64::from(x), f64::from(y), f64::from(z)])
                .ok_or_else(|| PyIndexError::new_err("GetVertex: invalid vertex index"))
        })
    }

    fn primitive_add_vertex(
        &mut self,
        py: Python<'_>,
        index: usize,
        coordinates: [f64; 3],
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            polyhedron_shape_mut(primitive)?.add_vertex(coordinates);
            Ok(())
        })
    }

    fn primitive_face(&self, py: Python<'_>, index: usize, face: usize) -> PyResult<Vec<i32>> {
        self.with_primitive(py, index, |primitive, _| {
            polyhedron_shape(primitive)?
                .face(face)
                .map(<[i32]>::to_vec)
                .ok_or_else(|| PyIndexError::new_err("GetFace: invalid face index"))
        })
    }

    fn primitive_add_face(
        &mut self,
        py: Python<'_>,
        index: usize,
        vertices: Vec<i32>,
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            polyhedron_shape_mut(primitive)?.add_face(vertices);
            Ok(())
        })
    }

    fn primitive_reset(&mut self, py: Python<'_>, index: usize) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            polyhedron_shape_mut(primitive)?.reset();
            Ok(())
        })
    }

    fn primitive_read_file(&mut self, py: Python<'_>, index: usize) -> PyResult<bool> {
        self.with_primitive_mut(py, index, |primitive, _| {
            let shape = primitive
                .polyhedron_reader_shape_mut()
                .ok_or_else(|| PyValueError::new_err("primitive is not a polyhedron reader"))?;
            Ok(shape.read_file().is_ok())
        })
    }

    fn primitive_multibox_count(&self, py: Python<'_>, index: usize) -> PyResult<usize> {
        self.with_primitive(py, index, |primitive, _| {
            Ok(multibox_shape(primitive)?.box_count())
        })
    }

    fn primitive_multibox_box(
        &self,
        py: Python<'_>,
        index: usize,
        box_index: usize,
    ) -> PyResult<([f64; 3], [f64; 3])> {
        self.with_primitive(py, index, |primitive, parameters| {
            let shape = multibox_shape(primitive)?;
            let boxed = shape
                .box_at(box_index)
                .ok_or_else(|| PyIndexError::new_err("GetBox: index out of range"))?;
            let coordinates = boxed
                .evaluated_coordinates(parameters)
                .map_err(|error| param_error("GetBox", error))?;
            Ok((coordinates[0], coordinates[1]))
        })
    }

    fn primitive_multibox_add(
        &mut self,
        py: Python<'_>,
        index: usize,
        start: [f64; 3],
        stop: [f64; 3],
    ) -> PyResult<()> {
        self.with_primitive_mut(py, index, |primitive, _| {
            multibox_shape_mut(primitive)?
                .add_box(csxcad_core::model::BoxPrimitive::new(start, stop));
            Ok(())
        })
    }

    fn primitive_multibox_delete(
        &mut self,
        py: Python<'_>,
        index: usize,
        box_index: usize,
    ) -> PyResult<bool> {
        self.with_primitive_mut(py, index, |primitive, _| {
            Ok(multibox_shape_mut(primitive)?.delete_box(box_index))
        })
    }
}

fn polygon_primitive(
    mut primitive: Primitive,
    points: (Vec<f64>, Vec<f64>),
    norm_dir: i64,
    elevation: f64,
) -> PyResult<Primitive> {
    let (x0, x1) = points;
    if x0.len() != x1.len() {
        return Err(PyValueError::new_err(
            "SetCoords: x0/x1 do not have the same length",
        ));
    }
    let normal = direction(norm_dir, "SetNormDir")?;
    let shape = polygon_shape_mut(&mut primitive)?;
    if !x0.is_empty() {
        shape.clear_vertices();
        for (a, b) in x0.iter().zip(x1.iter()) {
            shape.add_vertex([*a, *b]);
        }
    }
    if !shape.set_normal_direction(normal) {
        return Err(PyValueError::new_err("SetNormDir: invalid direction"));
    }
    shape.set_elevation(elevation);
    Ok(primitive)
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

fn file_type_from_name(filename: &str) -> csxcad_core::model::PolyhedronFileType {
    if filename.ends_with(".stl") {
        csxcad_core::model::PolyhedronFileType::Stl
    } else if filename.ends_with(".ply") {
        csxcad_core::model::PolyhedronFileType::Ply
    } else {
        csxcad_core::model::PolyhedronFileType::Unknown
    }
}

// -- field dispatch --------------------------------------------------------

fn get_f64(
    property: &Property,
    field: &str,
    component: usize,
    parameters: Option<&ParameterSet>,
) -> PyResult<f64> {
    if let Some(material_field) = material_field(field) {
        return require_material(property)?
            .value(material_field, component, parameters)
            .map_err(|error| param_error(field, error))?
            .ok_or_else(|| {
                PyIndexError::new_err(format!("{field}: invalid component {component}"))
            });
    }
    match field {
        "density" => require_material(property)?
            .density(parameters)
            .map_err(|error| param_error(field, error)),
        "conductivity" => require_sheet(property)?
            .conductivity(parameters)
            .map_err(|error| param_error(field, error)),
        "thickness" => require_sheet(property)?
            .thickness(parameters)
            .map_err(|error| param_error(field, error)),
        "phase_velocity" => require_absorbing(property)?
            .phase_velocity(parameters)
            .map_err(|error| param_error(field, error)),
        "resistance" => require_lumped(property)?
            .resistance(parameters)
            .map_err(|error| param_error(field, error)),
        "capacitance" => require_lumped(property)?
            .capacitance(parameters)
            .map_err(|error| param_error(field, error)),
        "inductance" => require_lumped(property)?
            .inductance(parameters)
            .map_err(|error| param_error(field, error)),
        "frequency" => require_excitation(property)?
            .frequency(parameters)
            .map_err(|error| param_error(field, error)),
        "delay" => require_excitation(property)?
            .delay(parameters)
            .map_err(|error| param_error(field, error)),
        "weight" => Ok(require_probe(property)?.weight()),
        "scale" => Ok(require_discrete(property)?.scale()),
        "excitation" => require_excitation(property)?
            .excitation(component, parameters)
            .map_err(|error| param_error(field, error))?
            .ok_or_else(|| {
                PyIndexError::new_err(format!("{field}: invalid component {component}"))
            }),
        "propagation_direction" => require_excitation(property)?
            .propagation_direction(component, parameters)
            .map_err(|error| param_error(field, error))?
            .ok_or_else(|| {
                PyIndexError::new_err(format!("{field}: invalid component {component}"))
            }),
        "weighting" => require_excitation(property)?
            .weighting(component, parameters)
            .map_err(|error| param_error(field, error))?
            .ok_or_else(|| {
                PyIndexError::new_err(format!("{field}: invalid component {component}"))
            }),
        "weight_origin" => Ok(require_excitation(property)?.weight_origin()[component]),
        "mode_origin" => Ok(require_probe(property)?.mode_origin()[component]),
        "optimal_resolution" => Ok(require_dump(property)?.optimal_resolution()[component]),
        "sub_sampling" => Ok(f64::from(require_dump(property)?.sub_sampling()[component])),
        other => Err(PyValueError::new_err(format!(
            "unknown property field: {other}"
        ))),
    }
}

fn set_f64(
    property: &mut Property,
    field: &str,
    component: usize,
    value: f64,
    parameters: Option<&ParameterSet>,
) -> PyResult<()> {
    if let Some(material_field) = material_field(field) {
        let material = require_material_mut(property)?;
        if !material.set_value(material_field, component, value) {
            return Err(PyValueError::new_err(format!(
                "SetMaterialProperty: invalid value {value} for '{field}'"
            )));
        }
        return Ok(());
    }
    match field {
        "density" => require_material_mut(property)?.set_density(value),
        "conductivity" => require_sheet_mut(property)?
            .set_conductivity_term(&format!("{value}"))
            .map_err(|error| param_error(field, error))?,
        "thickness" => require_sheet_mut(property)?
            .set_thickness_term(&format!("{value}"))
            .map_err(|error| param_error(field, error))?,
        "phase_velocity" => require_absorbing_mut(property)?.set_phase_velocity(value),
        "resistance" => require_lumped_mut(property)?
            .set_resistance_term(&format!("{value}"))
            .map_err(|error| param_error(field, error))?,
        "capacitance" => require_lumped_mut(property)?
            .set_capacitance_term(&format!("{value}"))
            .map_err(|error| param_error(field, error))?,
        "inductance" => require_lumped_mut(property)?
            .set_inductance_term(&format!("{value}"))
            .map_err(|error| param_error(field, error))?,
        "frequency" => require_excitation_mut(property)?
            .set_frequency_term(&format!("{value}"))
            .map_err(|error| param_error(field, error))?,
        "delay" => require_excitation_mut(property)?
            .set_delay_term(&format!("{value}"))
            .map_err(|error| param_error(field, error))?,
        "weight" => require_probe_mut(property)?.set_weight(value),
        "scale" => require_discrete_mut(property)?.set_scale(value),
        "excitation" => {
            if !require_excitation_mut(property)?
                .set_excitation_term(component, &format!("{value}"))
                .map_err(|error| param_error(field, error))?
            {
                return Err(PyIndexError::new_err(format!(
                    "{field}: invalid component {component}"
                )));
            }
        }
        "propagation_direction" => {
            if !require_excitation_mut(property)?
                .set_propagation_direction_term(component, &format!("{value}"))
                .map_err(|error| param_error(field, error))?
            {
                return Err(PyIndexError::new_err(format!(
                    "{field}: invalid component {component}"
                )));
            }
        }
        "weighting" => {
            if !require_excitation_mut(property)?
                .set_weighting_term(component, &format!("{value}"))
                .map_err(|error| param_error(field, error))?
            {
                return Err(PyIndexError::new_err(format!(
                    "{field}: invalid component {component}"
                )));
            }
        }
        "weight_origin" => {
            let mut origin = require_excitation(property)?.weight_origin();
            origin[component] = value;
            require_excitation_mut(property)?.set_weight_origin(origin);
        }
        "mode_origin" => {
            let mut origin = require_probe(property)?.mode_origin();
            origin[component] = value;
            require_probe_mut(property)?.set_mode_origin(origin);
        }
        "optimal_resolution" => {
            if !require_dump_mut(property)?.set_optimal_resolution(component, value) {
                return Err(PyIndexError::new_err(format!(
                    "{field}: invalid component {component}"
                )));
            }
        }
        "sub_sampling" => {
            if value < 0.0 || value.fract() != 0.0 {
                return Err(PyValueError::new_err(
                    "SetSubSampling: value must be a non-negative integer",
                ));
            }
            if !require_dump_mut(property)?.set_sub_sampling(component, value as u32) {
                return Err(PyIndexError::new_err(format!(
                    "{field}: invalid component {component}"
                )));
            }
        }
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown property field: {other}"
            )))
        }
    }
    let _ = parameters;
    Ok(())
}

fn get_int(property: &Property, field: &str, _component: usize) -> PyResult<i64> {
    Ok(match field {
        "boundary_type" => i64::from(require_absorbing(property)?.boundary_type().xml_value()),
        "direction" => i64::from(require_lumped(property)?.direction()),
        "element_type" => i64::from(require_lumped(property)?.element_type().xml_value()),
        "excitation_type" => i64::from(require_excitation(property)?.excitation_type()),
        "probe_type" => i64::from(require_probe(property)?.probe_type()),
        "normal_direction" => i64::from(require_probe(property)?.normal_direction()),
        "oversampling" => i64::from(require_probe(property)?.oversampling()),
        "dump_type" => i64::from(require_dump(property)?.dump_type()),
        "dump_mode" => i64::from(require_dump(property)?.dump_mode()),
        "multi_grid_level" => i64::from(require_dump(property)?.multi_grid_level()),
        "file_type" => match property.property_type() {
            PropertyType::DiscreteMaterial => i64::from(require_discrete(property)?.file_type()),
            _ => i64::from(require_dump(property)?.file_type()),
        },
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown property integer field: {other}"
            )))
        }
    })
}

fn set_int(property: &mut Property, field: &str, _component: usize, value: i64) -> PyResult<()> {
    let value = i32::try_from(value)
        .map_err(|_| PyValueError::new_err(format!("{field}: value out of range")))?;
    match field {
        "boundary_type" => require_absorbing_mut(property)?
            .set_boundary_type(AbsorbingBoundaryType::from_xml_value(value)),
        "direction" => require_lumped_mut(property)?.set_direction(value),
        "element_type" => {
            require_lumped_mut(property)?.set_element_type(LumpedElementType::from_xml_value(value))
        }
        "excitation_type" => require_excitation_mut(property)?.set_excitation_type(value),
        "probe_type" => require_probe_mut(property)?.set_probe_type(value),
        "normal_direction" => require_probe_mut(property)?.set_normal_direction(value),
        "oversampling" => require_probe_mut(property)?.set_oversampling(value),
        "dump_type" => require_dump_mut(property)?.set_dump_type(value),
        "dump_mode" => require_dump_mut(property)?.set_dump_mode(value),
        "multi_grid_level" => require_dump_mut(property)?.set_multi_grid_level(value),
        "file_type" => match property.property_type() {
            PropertyType::DiscreteMaterial => require_discrete_mut(property)?.set_file_type(value),
            _ => require_dump_mut(property)?.set_file_type(value),
        },
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown property integer field: {other}"
            )))
        }
    }
    Ok(())
}

fn get_bool(property: &Property, field: &str) -> PyResult<bool> {
    Ok(match field {
        "isotropy" => require_material(property)?.is_isotropic(),
        "normal_sign_positive" => require_absorbing(property)?.normal_sign_positive(),
        "caps" => require_lumped(property)?.caps(),
        "enabled" => require_excitation(property)?.is_enabled(),
        "use_database_for_background" => require_discrete(property)?.use_database_for_background(),
        "sub_sampling_enabled" => require_dump(property)?.sub_sampling_enabled(),
        "optimal_resolution_enabled" => require_dump(property)?.optimal_resolution_enabled(),
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown property boolean field: {other}"
            )))
        }
    })
}

fn set_bool(property: &mut Property, field: &str, value: bool) -> PyResult<()> {
    match field {
        "isotropy" => require_material_mut(property)?.set_isotropic(value),
        "normal_sign_positive" => require_absorbing_mut(property)?.set_normal_sign_positive(value),
        "caps" => require_lumped_mut(property)?.set_caps(value),
        "enabled" => require_excitation_mut(property)?.set_enabled(value),
        "use_database_for_background" => {
            require_discrete_mut(property)?.set_use_database_for_background(value)
        }
        "sub_sampling_enabled" => require_dump_mut(property)?.set_sub_sampling_enabled(value),
        "optimal_resolution_enabled" => {
            require_dump_mut(property)?.set_optimal_resolution_enabled(value)
        }
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown property boolean field: {other}"
            )))
        }
    }
    Ok(())
}

fn get_string(property: &Property, field: &str, component: usize) -> PyResult<String> {
    if let Some(name) = field.strip_suffix("_weight") {
        if let Some(material_field) = material_field(name) {
            return Ok(require_material(property)?
                .weight_term(material_field, component)
                .unwrap_or_default());
        }
        if name == "density" {
            return Ok(require_material(property)?.density_weight_term());
        }
    }
    Ok(match field {
        "filename" => require_discrete(property)?.file_name().to_string(),
        "weight_file" => require_excitation(property)?.weight_file().to_string(),
        "mode_file" => require_probe(property)?.mode_file().to_string(),
        "weight_function" => require_excitation(property)?
            .weighting_expression(component)
            .unwrap_or_default(),
        "mode_function" => require_probe(property)?
            .mode_function(component)
            .unwrap_or_default()
            .to_string(),
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown property string field: {other}"
            )))
        }
    })
}

fn set_string(property: &mut Property, field: &str, component: usize, value: &str) -> PyResult<()> {
    if let Some(name) = field.strip_suffix("_weight") {
        if let Some(material_field) = material_field(name) {
            if !require_material_mut(property)?
                .set_weight_expression(material_field, component, value)
                .map_err(|error| param_error(field, error))?
            {
                return Err(PyIndexError::new_err(format!(
                    "{field}: invalid component {component}"
                )));
            }
            return Ok(());
        }
        if name == "density" {
            require_material_mut(property)?
                .set_density_weight_expression(value)
                .map_err(|error| param_error(field, error))?;
            return Ok(());
        }
    }
    match field {
        "filename" => require_discrete_mut(property)?.set_file_name(value.to_string()),
        "weight_file" => require_excitation_mut(property)?.set_weight_file(value.to_string()),
        "mode_file" => require_probe_mut(property)?.set_mode_file(value.to_string()),
        "weight_function" => {
            if !require_excitation_mut(property)?
                .set_weighting_term(component, value)
                .map_err(|error| param_error(field, error))?
            {
                return Err(PyIndexError::new_err(format!(
                    "{field}: invalid component {component}"
                )));
            }
        }
        "mode_function" => {
            if !require_probe_mut(property)?.set_mode_function(component, value.to_string()) {
                return Err(PyIndexError::new_err(format!(
                    "{field}: invalid component {component}"
                )));
            }
        }
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown property string field: {other}"
            )))
        }
    }
    Ok(())
}

fn get_dispersive(
    property: &Property,
    field: &str,
    order: usize,
    component: usize,
    parameters: Option<&ParameterSet>,
) -> PyResult<f64> {
    if let Some(lorentz) = property.lorentz_material() {
        let parameter = lorentz_parameter(field).ok_or_else(|| {
            PyValueError::new_err(format!("unknown Lorentz material property: {field}"))
        })?;
        return lorentz
            .value(parameter, order, component, parameters)
            .map_err(|error| param_error(field, error))?
            .ok_or_else(|| PyIndexError::new_err("Invalid dispersive media order requested"));
    }
    if let Some(debye) = property.debye_material() {
        let value = match field {
            "eps_delta" => debye.epsilon_delta(order, component, parameters),
            "eps_relax" => debye.epsilon_relax_time(order, component, parameters),
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown Debye material property: {other}"
                )))
            }
        };
        return value
            .map_err(|error| param_error(field, error))?
            .ok_or_else(|| PyIndexError::new_err("Invalid dispersive media order requested"));
    }
    Err(PyValueError::new_err(
        "property is not a dispersive material",
    ))
}

fn set_dispersive(
    property: &mut Property,
    field: &str,
    order: usize,
    component: usize,
    value: f64,
) -> PyResult<()> {
    if let Some(lorentz) = property.lorentz_material_mut() {
        let parameter = lorentz_parameter(field).ok_or_else(|| {
            PyValueError::new_err(format!("unknown Lorentz material property: {field}"))
        })?;
        if !lorentz.set_value(parameter, order, component, value) {
            return Err(PyIndexError::new_err(
                "Invalid dispersive media order requested",
            ));
        }
        return Ok(());
    }
    if let Some(debye) = property.debye_material_mut() {
        let applied = match field {
            "eps_delta" => debye.set_epsilon_delta(order, component, value),
            "eps_relax" => debye.set_epsilon_relax_time(order, component, value),
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown Debye material property: {other}"
                )))
            }
        };
        if !applied {
            return Err(PyIndexError::new_err(
                "Invalid dispersive media order requested",
            ));
        }
        return Ok(());
    }
    Err(PyValueError::new_err(
        "property is not a dispersive material",
    ))
}

fn get_dispersive_weight(
    property: &Property,
    field: &str,
    order: usize,
    component: usize,
) -> PyResult<String> {
    if let Some(lorentz) = property.lorentz_material() {
        let parameter = lorentz_parameter(field).ok_or_else(|| {
            PyValueError::new_err(format!("unknown Lorentz material property: {field}"))
        })?;
        return lorentz
            .weight_term(parameter, order, component)
            .ok_or_else(|| PyIndexError::new_err("Invalid dispersive media order requested"));
    }
    if let Some(debye) = property.debye_material() {
        let value = match field {
            "eps_delta" => debye.epsilon_delta_weight_term(order, component),
            "eps_relax" => debye.epsilon_relax_time_weight_term(order, component),
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown Debye material property: {other}"
                )))
            }
        };
        return value
            .ok_or_else(|| PyIndexError::new_err("Invalid dispersive media order requested"));
    }
    Err(PyValueError::new_err(
        "property is not a dispersive material",
    ))
}

fn set_dispersive_weight(
    property: &mut Property,
    field: &str,
    order: usize,
    component: usize,
    expression: &str,
) -> PyResult<()> {
    if let Some(lorentz) = property.lorentz_material_mut() {
        let parameter = lorentz_parameter(field).ok_or_else(|| {
            PyValueError::new_err(format!("unknown Lorentz material property: {field}"))
        })?;
        if !lorentz
            .set_weight_expression(parameter, order, component, expression)
            .map_err(|error| param_error(field, error))?
        {
            return Err(PyIndexError::new_err(
                "Invalid dispersive media order requested",
            ));
        }
        return Ok(());
    }
    if let Some(debye) = property.debye_material_mut() {
        let applied = match field {
            "eps_delta" => debye.set_epsilon_delta_weight_expression(order, component, expression),
            "eps_relax" => {
                debye.set_epsilon_relax_time_weight_expression(order, component, expression)
            }
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown Debye material property: {other}"
                )))
            }
        }
        .map_err(|error| param_error(field, error))?;
        if !applied {
            return Err(PyIndexError::new_err(
                "Invalid dispersive media order requested",
            ));
        }
        return Ok(());
    }
    Err(PyValueError::new_err(
        "property is not a dispersive material",
    ))
}

// -- typed accessor helpers ------------------------------------------------

fn missing(field: &str, property: &Property) -> pyo3::PyErr {
    PyValueError::new_err(format!(
        "property '{}' ({}) has no field '{field}'",
        property.name(),
        property_type_name(property.property_type())
    ))
}

macro_rules! accessor {
    ($fn_name:ident, $fn_name_mut:ident, $field:ident, $field_mut:ident, $ty:ty) => {
        fn $fn_name(property: &Property) -> PyResult<&$ty> {
            property
                .$field()
                .ok_or_else(|| missing(stringify!($field), property))
        }

        fn $fn_name_mut(property: &mut Property) -> PyResult<&mut $ty> {
            let ptype = property_type_name(property.property_type()).to_string();
            let name = property.name().to_string();
            property.$field_mut().ok_or_else(|| {
                PyValueError::new_err(format!(
                    "property '{name}' ({ptype}) is not a {}",
                    stringify!($field)
                ))
            })
        }
    };
}

accessor!(
    require_material,
    require_material_mut,
    material,
    material_mut,
    csxcad_core::material::MaterialProperties
);
accessor!(
    require_sheet,
    require_sheet_mut,
    conducting_sheet,
    conducting_sheet_mut,
    csxcad_core::material::ConductingSheetProperties
);
accessor!(
    require_absorbing,
    require_absorbing_mut,
    absorbing_boundary,
    absorbing_boundary_mut,
    csxcad_core::material::AbsorbingBoundaryProperties
);
accessor!(
    require_lumped,
    require_lumped_mut,
    lumped_element,
    lumped_element_mut,
    csxcad_core::material::LumpedElementProperties
);
accessor!(
    require_excitation,
    require_excitation_mut,
    excitation,
    excitation_mut,
    csxcad_core::material::ExcitationProperties
);
accessor!(
    require_probe,
    require_probe_mut,
    probe,
    probe_mut,
    csxcad_core::material::ProbeProperties
);
accessor!(
    require_dump,
    require_dump_mut,
    dump_box,
    dump_box_mut,
    csxcad_core::material::DumpBoxProperties
);
accessor!(
    require_discrete,
    require_discrete_mut,
    discrete_material,
    discrete_material_mut,
    csxcad_core::material::DiscreteMaterialProperties
);

fn curve_shape(primitive: &Primitive) -> PyResult<&csxcad_core::model::CurvePrimitive> {
    primitive
        .curve_shape()
        .or_else(|| primitive.wire_shape())
        .ok_or_else(|| PyValueError::new_err("primitive is not a curve"))
}

fn curve_shape_mut(primitive: &mut Primitive) -> PyResult<&mut csxcad_core::model::CurvePrimitive> {
    if primitive.curve_shape().is_some() {
        return primitive
            .curve_shape_mut()
            .ok_or_else(|| PyValueError::new_err("primitive is not a curve"));
    }
    primitive
        .wire_shape_mut()
        .ok_or_else(|| PyValueError::new_err("primitive is not a curve"))
}

fn polygon_shape(primitive: &Primitive) -> PyResult<&csxcad_core::model::PolygonPrimitive> {
    primitive
        .polygon_shape()
        .ok_or_else(|| PyValueError::new_err("primitive is not a polygon"))
}

fn polygon_shape_mut(
    primitive: &mut Primitive,
) -> PyResult<&mut csxcad_core::model::PolygonPrimitive> {
    primitive
        .polygon_shape_mut()
        .ok_or_else(|| PyValueError::new_err("primitive is not a polygon"))
}

fn polyhedron_shape(primitive: &Primitive) -> PyResult<&csxcad_core::model::PolyhedronPrimitive> {
    if let Some(shape) = primitive.polyhedron_shape() {
        return Ok(shape);
    }
    if let Some(reader) = primitive.polyhedron_reader_shape() {
        return Ok(reader.polyhedron());
    }
    Err(PyValueError::new_err("primitive is not a polyhedron"))
}

fn polyhedron_shape_mut(
    primitive: &mut Primitive,
) -> PyResult<&mut csxcad_core::model::PolyhedronPrimitive> {
    primitive
        .polyhedron_shape_mut()
        .ok_or_else(|| PyValueError::new_err("primitive is not a polyhedron"))
}

fn multibox_shape(primitive: &Primitive) -> PyResult<&csxcad_core::model::MultiBoxPrimitive> {
    primitive
        .multibox_shape()
        .ok_or_else(|| PyValueError::new_err("primitive is not a multibox"))
}

fn multibox_shape_mut(
    primitive: &mut Primitive,
) -> PyResult<&mut csxcad_core::model::MultiBoxPrimitive> {
    primitive
        .multibox_shape_mut()
        .ok_or_else(|| PyValueError::new_err("primitive is not a multibox"))
}

fn primitive_coordinate(
    primitive: &Primitive,
    coord: usize,
    parameters: Option<&ParameterSet>,
) -> PyResult<f64> {
    let value = match primitive.shape() {
        csxcad_core::model::PrimitiveShape::Point(shape) => shape.coordinate(coord, parameters),
        csxcad_core::model::PrimitiveShape::Box(shape) => shape.coordinate(coord, parameters),
        csxcad_core::model::PrimitiveShape::Sphere(shape) => {
            shape.center_coordinate(coord, parameters)
        }
        csxcad_core::model::PrimitiveShape::SphericalShell(shape) => {
            shape.center(parameters).map(|center| Some(center[coord]))
        }
        csxcad_core::model::PrimitiveShape::Cylinder(shape) => shape.coordinate(coord, parameters),
        other => {
            return Err(PyValueError::new_err(format!(
                "GetCoord: primitive {:?} has no single-coordinate accessor",
                other
            )))
        }
    };
    value
        .map_err(|error| param_error("GetCoord", error))?
        .ok_or_else(|| PyIndexError::new_err(format!("GetCoord: invalid index {coord}")))
}

fn set_primitive_coordinate(primitive: &mut Primitive, coord: usize, value: f64) -> PyResult<()> {
    let applied = match primitive.shape_mut() {
        csxcad_core::model::PrimitiveShape::Point(shape) => shape.set_coordinate(coord, value),
        csxcad_core::model::PrimitiveShape::Box(shape) => shape.set_coordinate(coord, value),
        csxcad_core::model::PrimitiveShape::Sphere(shape) => {
            shape.set_center_coordinate(coord, value)
        }
        csxcad_core::model::PrimitiveShape::SphericalShell(shape) => {
            shape.set_center_coordinate(coord, value)
        }
        csxcad_core::model::PrimitiveShape::Cylinder(shape) => shape.set_coordinate(coord, value),
        other => {
            return Err(PyValueError::new_err(format!(
                "SetCoord: primitive {:?} has no single-coordinate accessor",
                other
            )))
        }
    };
    if applied {
        Ok(())
    } else {
        Err(PyIndexError::new_err(format!(
            "SetCoord: invalid index {coord}"
        )))
    }
}

fn primitive_f64(
    primitive: &Primitive,
    field: &str,
    parameters: Option<&ParameterSet>,
) -> PyResult<f64> {
    let value = match field {
        "radius" => match primitive.shape() {
            csxcad_core::model::PrimitiveShape::Sphere(shape) => shape.radius(parameters),
            csxcad_core::model::PrimitiveShape::SphericalShell(shape) => shape.radius(parameters),
            csxcad_core::model::PrimitiveShape::Cylinder(shape) => shape.radius(parameters),
            _ => return Err(PyValueError::new_err("GetRadius: primitive has no radius")),
        },
        "shell_width" => match primitive.shape() {
            csxcad_core::model::PrimitiveShape::SphericalShell(shape) => {
                shape.shell_width(parameters)
            }
            csxcad_core::model::PrimitiveShape::Cylinder(shape) => shape
                .shell_width(parameters)
                .map(|width| width.unwrap_or(0.0)),
            _ => {
                return Err(PyValueError::new_err(
                    "GetShellWidth: primitive has no shell width",
                ))
            }
        },
        "elevation" => match primitive.shape() {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => shape.elevation(parameters),
            _ => {
                return Err(PyValueError::new_err(
                    "GetElevation: primitive is not a polygon",
                ))
            }
        },
        "length" => match primitive.shape() {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => shape
                .extrusion_length(parameters)
                .map(|length| length.unwrap_or(0.0)),
            _ => {
                return Err(PyValueError::new_err(
                    "GetLength: primitive is not a linpoly",
                ))
            }
        },
        "wire_radius" => match primitive.shape() {
            csxcad_core::model::PrimitiveShape::Curve(shape) => shape
                .wire_radius(parameters)
                .map(|radius| radius.unwrap_or(0.0)),
            _ => {
                return Err(PyValueError::new_err(
                    "GetWireRadius: primitive is not a wire",
                ))
            }
        },
        "angle" => match primitive.shape() {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => shape
                .rotation_angles(parameters)
                .map(|angles| angles.map(|angles| angles[0]).unwrap_or(0.0)),
            _ => {
                return Err(PyValueError::new_err(
                    "GetAngle: primitive is not a rotpoly",
                ))
            }
        },
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown primitive float field: {other}"
            )))
        }
    };
    value.map_err(|error| param_error(field, error))
}

fn set_primitive_f64(primitive: &mut Primitive, field: &str, value: f64) -> PyResult<()> {
    let shape = primitive.shape_mut();
    match field {
        "radius" => match shape {
            csxcad_core::model::PrimitiveShape::Sphere(shape) => shape.set_radius(value),
            csxcad_core::model::PrimitiveShape::SphericalShell(shape) => shape.set_radius(value),
            csxcad_core::model::PrimitiveShape::Cylinder(shape) => shape.set_radius(value),
            _ => return Err(PyValueError::new_err("SetRadius: primitive has no radius")),
        },
        "shell_width" => match shape {
            csxcad_core::model::PrimitiveShape::SphericalShell(shape) => {
                shape.set_shell_width(value)
            }
            csxcad_core::model::PrimitiveShape::Cylinder(shape) => {
                if !shape.set_shell_width(value) {
                    return Err(PyValueError::new_err(
                        "SetShellWidth: primitive has no shell width",
                    ));
                }
            }
            _ => {
                return Err(PyValueError::new_err(
                    "SetShellWidth: primitive has no shell width",
                ))
            }
        },
        "elevation" => match shape {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => shape.set_elevation(value),
            _ => {
                return Err(PyValueError::new_err(
                    "SetElevation: primitive is not a polygon",
                ))
            }
        },
        "length" => match shape {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => {
                if !shape.set_extrusion_length(value) {
                    return Err(PyValueError::new_err(
                        "SetLength: primitive is not a linpoly",
                    ));
                }
            }
            _ => {
                return Err(PyValueError::new_err(
                    "SetLength: primitive is not a linpoly",
                ))
            }
        },
        "wire_radius" => match shape {
            csxcad_core::model::PrimitiveShape::Curve(shape) => {
                if !shape.set_wire_radius(value) {
                    return Err(PyValueError::new_err(
                        "SetWireRadius: primitive is not a wire",
                    ));
                }
            }
            _ => {
                return Err(PyValueError::new_err(
                    "SetWireRadius: primitive is not a wire",
                ))
            }
        },
        "angle" => match shape {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => {
                shape
                    .set_rotation_angle_expression(0, &format!("{value}"))
                    .map_err(|error| param_error(field, error))?;
            }
            _ => {
                return Err(PyValueError::new_err(
                    "SetAngle: primitive is not a rotpoly",
                ))
            }
        },
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown primitive float field: {other}"
            )))
        }
    }
    Ok(())
}

fn primitive_int(primitive: &Primitive, field: &str) -> PyResult<i64> {
    Ok(match field {
        "norm_dir" => match primitive.shape() {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => shape.normal_direction() as i64,
            _ => {
                return Err(PyValueError::new_err(
                    "GetNormDir: primitive is not a polygon",
                ))
            }
        },
        "rot_axis" => match primitive.shape() {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => shape
                .rotation_axis_direction()
                .map(|direction| direction as i64)
                .unwrap_or(0),
            _ => {
                return Err(PyValueError::new_err(
                    "GetRotAxisDir: primitive is not a rotpoly",
                ))
            }
        },
        "file_type" => match primitive.shape() {
            csxcad_core::model::PrimitiveShape::PolyhedronReader(shape) => {
                file_type_value(shape.file_type())
            }
            _ => {
                return Err(PyValueError::new_err(
                    "GetFileType: primitive is not a polyhedron reader",
                ))
            }
        },
        "face_valid" => {
            return Err(PyValueError::new_err(
                "GetFaceValid needs a face index, use primitive_face",
            ))
        }
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown primitive integer field: {other}"
            )))
        }
    })
}

fn set_primitive_int(primitive: &mut Primitive, field: &str, value: i64) -> PyResult<()> {
    match field {
        "norm_dir" => match primitive.shape_mut() {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => {
                if !shape.set_normal_direction(direction(value, "SetNormDir")?) {
                    return Err(PyValueError::new_err("SetNormDir: invalid direction"));
                }
            }
            _ => {
                return Err(PyValueError::new_err(
                    "SetNormDir: primitive is not a polygon",
                ))
            }
        },
        "rot_axis" => match primitive.shape_mut() {
            csxcad_core::model::PrimitiveShape::Polygon(shape) => {
                if !shape.set_rotation_axis_direction(direction(value, "SetRotAxisDir")?) {
                    return Err(PyValueError::new_err("SetRotAxisDir: invalid direction"));
                }
            }
            _ => {
                return Err(PyValueError::new_err(
                    "SetRotAxisDir: primitive is not a rotpoly",
                ))
            }
        },
        "file_type" => match primitive.shape_mut() {
            csxcad_core::model::PrimitiveShape::PolyhedronReader(shape) => {
                shape.set_file_type(file_type_from_value(value)?)
            }
            _ => {
                return Err(PyValueError::new_err(
                    "SetFileType: primitive is not a polyhedron reader",
                ))
            }
        },
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown primitive integer field: {other}"
            )))
        }
    }
    Ok(())
}

fn file_type_value(file_type: csxcad_core::model::PolyhedronFileType) -> i64 {
    match file_type {
        csxcad_core::model::PolyhedronFileType::Stl => 1,
        csxcad_core::model::PolyhedronFileType::Ply => 2,
        csxcad_core::model::PolyhedronFileType::Unknown => 0,
    }
}

fn file_type_from_value(value: i64) -> PyResult<csxcad_core::model::PolyhedronFileType> {
    match value {
        0 => Ok(csxcad_core::model::PolyhedronFileType::Unknown),
        1 => Ok(csxcad_core::model::PolyhedronFileType::Stl),
        2 => Ok(csxcad_core::model::PolyhedronFileType::Ply),
        other => Err(PyValueError::new_err(format!(
            "unknown polyhedron file type: {other}"
        ))),
    }
}

/// Contract tests for the native name/id tables (RM-031/RM-033).
///
/// The Python layer dispatches on these exact strings and numbers, so a typo
/// would silently create the wrong native type. References:
/// `CSXCAD/python/CSXCAD/CSProperties.pyx` (`fromTypeName`, the only accepted
/// spellings) and `CSXCAD/src/CSPrimitives.h:84-85` (the `Type` enum order that
/// `GetType()` returns).
#[cfg(test)]
mod type_table_contract_tests {
    use super::*;

    #[test]
    fn native_property_type_names_resolve() {
        let expected = [
            ("Material", PropertyType::Material),
            ("Metal", PropertyType::Metal),
            ("ConductingSheet", PropertyType::ConductingSheet),
            ("LumpedElement", PropertyType::LumpedElement),
            ("Excitation", PropertyType::Excitation),
            ("ProbeBox", PropertyType::ProbeBox),
            ("DumpBox", PropertyType::DumpBox),
            ("LorentzMaterial", PropertyType::LorentzMaterial),
            ("DebyeMaterial", PropertyType::DebyeMaterial),
            ("AbsorbingBC", PropertyType::AbsorbingBoundary),
            ("Discrete-Material", PropertyType::DiscreteMaterial),
            ("DiscMaterial", PropertyType::DiscreteMaterial),
        ];
        for (name, property_type) in expected {
            assert_eq!(
                property_type_from_name(name),
                Some(property_type),
                "native fromTypeName({name})"
            );
        }
        // Native fromTypeName returns None for anything else, including the
        // abstract dispersive base, which is created through a concrete class.
        for name in ["DispersiveMaterial", "Material ", "material", "", "Bogus"] {
            assert_eq!(property_type_from_name(name), None, "{name}");
        }
    }

    #[test]
    fn primitive_type_ids_match_the_native_enum_order() {
        let expected = [
            (PrimitiveType::Point, 0, "Point"),
            (PrimitiveType::Box, 1, "Box"),
            (PrimitiveType::MultiBox, 2, "MultiBox"),
            (PrimitiveType::Sphere, 3, "Sphere"),
            (PrimitiveType::SphericalShell, 4, "SphericalShell"),
            (PrimitiveType::Cylinder, 5, "Cylinder"),
            (PrimitiveType::CylindricalShell, 6, "CylindricalShell"),
            (PrimitiveType::Polygon, 7, "Polygon"),
            (PrimitiveType::LinPoly, 8, "LinPoly"),
            (PrimitiveType::RotPoly, 9, "RotPoly"),
            (PrimitiveType::Polyhedron, 10, "Polyhedron"),
            (PrimitiveType::Curve, 11, "Curve"),
            (PrimitiveType::Wire, 12, "Wire"),
            (PrimitiveType::UserDefined, 13, "UserDefined"),
            (PrimitiveType::PolyhedronReader, 14, "PolyhedronReader"),
        ];
        for (primitive_type, id, name) in expected {
            assert_eq!(primitive_type_id(primitive_type), id, "{name}");
            assert_eq!(primitive_type_name(primitive_type), name);
        }
    }
}
