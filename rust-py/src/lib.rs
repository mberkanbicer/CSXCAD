//! Python bindings for CSXCAD geometry and material description.
//! Mirrors the C++ CSXCAD API using PyO3.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::{PyModule, PyResult, Python};
use pyo3::types::{PyAny, PyAnyMethods, PyModuleMethods};
use pyo3::{pyclass, pymethods, pymodule, Bound, FromPyObject};

use csxcad_core::material::MaterialField;
use csxcad_core::model::{Primitive, PropertyType, Structure};

/// Helper: extract a Rust type from a Python object using FromPyObject.
fn extract<'a, 'py, T: FromPyObject<'a, 'py>>(val: &'a Bound<'py, PyAny>) -> PyResult<T> {
    val.extract::<T>().map_err(Into::into)
}

fn extract_str<'py>(val: &Bound<'py, PyAny>) -> PyResult<String> {
    extract(val)
}

fn extract_f64<'py>(val: &Bound<'py, PyAny>) -> PyResult<f64> {
    extract(val)
}

fn extract_vec_f64<'py>(val: &Bound<'py, PyAny>) -> PyResult<Vec<f64>> {
    extract(val)
}

/// Python wrapper around the Rust `Structure` (CSXCAD geometry container).
#[pyclass(unsendable)]
pub struct PyStructure {
    inner: Structure,
}

#[pymethods]
impl PyStructure {
    /// Create a new empty CSXCAD structure.
    #[new]
    fn py_new() -> Self {
        Self {
            inner: Structure::new(),
        }
    }

    /// Get the number of properties in the structure.
    fn num_properties(&self) -> usize {
        self.inner.property_count()
    }

    /// Add a property (material/metal/etc.) to the structure.
    fn add_property(&mut self, name: &str, prop_type: &str) -> PyResult<u64> {
        let ptype = match prop_type {
            "Metal" => PropertyType::Metal,
            "Material" => PropertyType::Material,
            "AbsorbingBoundary" => PropertyType::AbsorbingBoundary,
            "Excitation" => PropertyType::Excitation,
            "ProbeBox" => PropertyType::ProbeBox,
            "ResBox" => PropertyType::ResBox,
            "DumpBox" => PropertyType::DumpBox,
            _ => PropertyType::Unknown,
        };
        let prop = csxcad_core::model::Property::new(name, ptype);
        let id = self.inner.add_property(prop);
        Ok(id.get())
    }

    /// Set the background material (epsilon, mue, kappa, sigma).
    fn set_background_material(
        &mut self,
        epsilon: f64,
        mue: f64,
        kappa: f64,
        sigma: f64,
    ) -> PyResult<()> {
        let bm = self.inner.background_material_mut();
        bm.set_value(MaterialField::Epsilon, epsilon);
        bm.set_value(MaterialField::Mue, mue);
        bm.set_value(MaterialField::Kappa, kappa);
        bm.set_value(MaterialField::Sigma, sigma);
        Ok(())
    }

    /// Serialize the structure to a CSXCAD XML string.
    fn to_xml_string(&self) -> PyResult<String> {
        self.inner
            .to_xml_string()
            .map_err(|e| PyValueError::new_err(format!("ToXML: {}", e)))
    }

    /// Deserialize a structure from a CSXCAD XML string.
    #[staticmethod]
    fn from_xml_string(xml: &str) -> PyResult<Self> {
        let structure = Structure::from_xml_str(xml)
            .map_err(|e| PyValueError::new_err(format!("FromXML: {}", e)))?;
        Ok(Self { inner: structure })
    }

    /// Clear all properties and reset the structure.
    fn clear(&mut self) {
        self.inner.clear();
    }

    /// Add a box primitive to a property.
    fn add_box(&mut self, property_id: u64, start: Vec<f64>, stop: Vec<f64>) -> PyResult<u64> {
        if start.len() != 3 || stop.len() != 3 {
            return Err(PyValueError::new_err(
                "Box start and stop must each have 3 coordinates",
            ));
        }
        let prim = Primitive::box_primitive([start[0], start[1], start[2]], [stop[0], stop[1], stop[2]]);
        let id = self
            .inner
            .add_primitive(csxcad_core::model::PropertyId::from_u64(property_id), prim)
            .map_err(|e| PyValueError::new_err(format!("add_box: {}", e)))?;
        Ok(id.get())
    }

    /// Add a sphere primitive to a property.
    fn add_sphere(
        &mut self,
        property_id: u64,
        center: Vec<f64>,
        radius: f64,
    ) -> PyResult<u64> {
        if center.len() != 3 {
            return Err(PyValueError::new_err("Sphere center must have 3 coordinates"));
        }
        let prim = Primitive::sphere([center[0], center[1], center[2]], radius);
        let id = self
            .inner
            .add_primitive(csxcad_core::model::PropertyId::from_u64(property_id), prim)
            .map_err(|e| PyValueError::new_err(format!("add_sphere: {}", e)))?;
        Ok(id.get())
    }

    /// Add a point primitive to a property.
    fn add_point(
        &mut self,
        property_id: u64,
        coordinates: Vec<f64>,
    ) -> PyResult<u64> {
        if coordinates.len() != 3 {
            return Err(PyValueError::new_err("Point must have 3 coordinates"));
        }
        let prim = Primitive::point([coordinates[0], coordinates[1], coordinates[2]]);
        let id = self
            .inner
            .add_primitive(csxcad_core::model::PropertyId::from_u64(property_id), prim)
            .map_err(|e| PyValueError::new_err(format!("add_point: {}", e)))?;
        Ok(id.get())
    }
}

/// Python wrapper around a CSXCAD `Property` (material/metal definition).
#[pyclass(unsendable)]
pub struct PyProperty {
    inner: csxcad_core::model::Property,
}

#[pymethods]
impl PyProperty {
    /// Create a new property with a given name and type.
    #[new]
    fn py_new(name: &str, prop_type: &str) -> PyResult<Self> {
        let ptype = match prop_type {
            "Metal" => PropertyType::Metal,
            "Material" => PropertyType::Material,
            "AbsorbingBoundary" => PropertyType::AbsorbingBoundary,
            "Excitation" => PropertyType::Excitation,
            "ProbeBox" => PropertyType::ProbeBox,
            "ResBox" => PropertyType::ResBox,
            "DumpBox" => PropertyType::DumpBox,
            _ => PropertyType::Unknown,
        };
        Ok(Self {
            inner: csxcad_core::model::Property::new(name, ptype),
        })
    }

    /// Set the property name.
    fn set_name(&mut self, name: &str) {
        self.inner.set_name(name.to_string());
    }

    /// Get the property name.
    fn name(&self) -> String {
        self.inner.name().to_string()
    }

    /// Set material properties (epsilon, mue, kappa, sigma) on component 0.
    fn set_material(
        &mut self,
        epsilon: f64,
        mue: f64,
        kappa: f64,
        sigma: f64,
    ) -> PyResult<()> {
        let mat = self
            .inner
            .material_mut()
            .ok_or_else(|| PyValueError::new_err("Property is not a material type"))?;
        mat.set_value(MaterialField::Epsilon, 0, epsilon);
        mat.set_value(MaterialField::Mue, 0, mue);
        mat.set_value(MaterialField::Kappa, 0, kappa);
        mat.set_value(MaterialField::Sigma, 0, sigma);
        Ok(())
    }
}

/// Python module definition.
#[pymodule]
fn csxcad_py(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyStructure>()?;
    m.add_class::<PyProperty>()?;
    m.add("PROPERTY_TYPE_METAL", "Metal")?;
    m.add("PROPERTY_TYPE_MATERIAL", "Material")?;
    m.add("PROPERTY_TYPE_ABSORBING_BOUNDARY", "AbsorbingBoundary")?;
    m.add("PROPERTY_TYPE_EXCITATION", "Excitation")?;
    m.add("PROPERTY_TYPE_PROBE_BOX", "ProbeBox")?;
    m.add("PROPERTY_TYPE_RES_BOX", "ResBox")?;
    m.add("PROPERTY_TYPE_DUMP_BOX", "DumpBox")?;
    Ok(())
}
