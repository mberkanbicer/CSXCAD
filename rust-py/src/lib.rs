//! Python bindings for CSXCAD geometry and material description.
//!
//! This extension module is the Rust core of the native CSXCAD Python API. The
//! class names and argument handling live in `python/CSXCAD/`, mirroring the
//! original Cython wrappers; everything below is the data model and the
//! operations on it.

#![allow(non_snake_case)]

use pyo3::prelude::{PyModule, PyResult, Python};
use pyo3::types::PyModuleMethods;
use pyo3::{pymodule, Bound};

mod grid_py;
mod property_py;
mod structure_py;
mod transform_py;

use grid_py::PyCSRectGrid;
use property_py::PyProperty;
use structure_py::{PyParameterSet, PyStructure};
use transform_py::PyCSTransform;

/// Python module definition.
#[pymodule]
fn csxcad_py(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyStructure>()?;
    m.add_class::<PyProperty>()?;
    m.add_class::<PyCSRectGrid>()?;
    m.add_class::<PyCSTransform>()?;
    m.add_class::<PyParameterSet>()?;
    m.add("PROPERTY_TYPE_METAL", "Metal")?;
    m.add("PROPERTY_TYPE_MATERIAL", "Material")?;
    m.add("PROPERTY_TYPE_ABSORBING_BOUNDARY", "AbsorbingBC")?;
    m.add("PROPERTY_TYPE_EXCITATION", "Excitation")?;
    m.add("PROPERTY_TYPE_PROBE_BOX", "ProbeBox")?;
    m.add("PROPERTY_TYPE_RES_BOX", "ResBox")?;
    m.add("PROPERTY_TYPE_DUMP_BOX", "DumpBox")?;
    Ok(())
}
