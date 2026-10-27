//! Python wrapper for the native `CSTransform` class.
//!
//! Mirrors the Cython API: the angle unit is radians on the Rust side and the
//! Python layer converts degrees (the native default) before calling in, exactly
//! like `CSXCAD/CSTransform.pyx` does with `np.deg2rad`.
//!
//! A transform either owns its matrix (a standalone `CSTransform`) or is the
//! transform of a primitive, in which case every operation edits the primitive
//! in place, exactly like the native wrapper around the primitive's C++ member.

use pyo3::exceptions::{PyException, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::{PyResult, Python};
use pyo3::types::{PyAny, PyAnyMethods, PyDict, PyTuple, PyTypeMethods};
use pyo3::{pyclass, pymethods, Bound, Py};

use csxcad_core::transform::Transform;

use crate::grid_py::check_ny_dir;
use crate::property_py::PyProperty;

fn numpy_array<'py>(py: Python<'py>, data: Vec<f64>) -> PyResult<Bound<'py, PyAny>> {
    py.import("numpy")?.call_method1("array", (data,))
}

fn numpy_shaped<'py>(
    py: Python<'py>,
    data: Vec<f64>,
    shape: (usize, usize),
) -> PyResult<Bound<'py, PyAny>> {
    numpy_array(py, data)?.call_method1("reshape", (shape,))
}

// The `Local` variant holds the whole 4x4 matrix (296 bytes) while `Primitive`
// is two words. Boxing would add an allocation on every transform operation for
// a value that lives for the duration of one call, so the size difference is
// accepted deliberately.
#[allow(clippy::large_enum_variant)]
enum TransformBacking {
    Local(Transform),
    /// Transform of a primitive; every operation edits the primitive in place.
    Primitive {
        property: Py<PyProperty>,
        index: usize,
    },
}

/// Python wrapper around the Rust `Transform` (native `CSTransform`).
#[pyclass(unsendable, weakref, name = "CSTransform")]
pub struct PyCSTransform {
    backing: TransformBacking,
}

impl PyCSTransform {
    /// Wrap an already configured Rust transform.
    pub(crate) fn from_transform(inner: Transform) -> Self {
        Self {
            backing: TransformBacking::Local(inner),
        }
    }

    /// Live view of the transform of one primitive, as created by
    /// `CSPrimitives.GetTransform()`.
    pub(crate) fn attached(property: Py<PyProperty>, index: usize) -> Self {
        Self {
            backing: TransformBacking::Primitive { property, index },
        }
    }

    pub(crate) fn transform(&self) -> &Transform {
        match &self.backing {
            TransformBacking::Local(transform) => transform,
            TransformBacking::Primitive { .. } => {
                unreachable!("primitive transforms are accessed through their property")
            }
        }
    }

    fn with<R>(&self, py: Python<'_>, f: impl FnOnce(&Transform) -> R) -> PyResult<R> {
        match &self.backing {
            TransformBacking::Local(transform) => Ok(f(transform)),
            TransformBacking::Primitive { property, index } => {
                let property = property.borrow(py);
                // a transform is created lazily, so a missing one is the identity
                static IDENTITY: std::sync::OnceLock<Transform> = std::sync::OnceLock::new();
                let identity = IDENTITY.get_or_init(Transform::new);
                match property.with_primitive(py, *index, |primitive, _| {
                    Ok(f(primitive.transform().unwrap_or(identity)))
                }) {
                    Ok(value) => Ok(value),
                    Err(error) if error.is_instance_of::<PyRuntimeError>(py) => Err(deleted()),
                    Err(error) => Err(error),
                }
            }
        }
    }

    fn with_mut<R>(&mut self, py: Python<'_>, f: impl FnOnce(&mut Transform) -> R) -> PyResult<R> {
        match &mut self.backing {
            TransformBacking::Local(transform) => Ok(f(transform)),
            TransformBacking::Primitive { property, index } => {
                let mut property = property.borrow_mut(py);
                match property
                    .with_primitive_mut(py, *index, |primitive, _| Ok(f(primitive.transform_mut())))
                {
                    Ok(value) => Ok(value),
                    Err(error) if error.is_instance_of::<PyRuntimeError>(py) => Err(deleted()),
                    Err(error) => Err(error),
                }
            }
        }
    }
}

fn deleted() -> pyo3::PyErr {
    PyRuntimeError::new_err("wrapped C++ object of type CSTransform has been deleted")
}

#[pymethods]
impl PyCSTransform {
    #[new]
    #[pyo3(signature = (pset = None))]
    fn py_new(pset: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        // The native constructor takes an optional ParameterSet so that a
        // primitive's transform can resolve parameter expressions. The numeric
        // Python API never passes one; reject it instead of silently ignoring it.
        match pset {
            None => {}
            Some(value) if value.is_none() => {}
            Some(value) => {
                return Err(PyTypeError::new_err(format!(
                    "CSTransform: a parameter-set backed transform is created by its primitive, \
                     not from Python (got {})",
                    value
                        .get_type()
                        .name()
                        .map(|name| name.to_string())
                        .unwrap_or_else(|_| "object".to_string())
                )))
            }
        }
        let mut inner = Transform::new();
        inner.set_angle_radian();
        Ok(Self::from_transform(inner))
    }

    /// Return an independent copy of this transform.
    fn copy(&self, py: Python<'_>) -> PyResult<Self> {
        let inner = self.with(py, Transform::clone)?;
        Ok(Self::from_transform(inner))
    }

    fn Reset(&mut self, py: Python<'_>) -> PyResult<()> {
        self.with_mut(py, Transform::reset)
    }

    fn HasTransform(&self, py: Python<'_>) -> PyResult<bool> {
        self.with(py, Transform::has_transform)
    }

    #[pyo3(signature = (coord, invers = false))]
    fn Transform<'py>(
        &self,
        py: Python<'py>,
        coord: Vec<f64>,
        invers: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        if coord.len() != 3 {
            return Err(PyValueError::new_err(
                "Transform: coordinate must have 3 components",
            ));
        }
        let input = [coord[0], coord[1], coord[2]];
        let output = self.with(py, |transform| {
            if invers {
                transform.inverse_transform(input).map_err(|error| {
                    PyValueError::new_err(format!("Transform: inverse transform failed: {error}"))
                })
            } else {
                Ok(transform.transform(input))
            }
        })??;
        numpy_array(py, output.to_vec())
    }

    fn GetMatrix<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let matrix = self.with(py, |transform| transform.matrix().to_vec())?;
        numpy_shaped(py, matrix, (4, 4))
    }

    #[pyo3(signature = (transform, *args, **kwargs))]
    fn AddTransform(
        slf: Bound<'_, Self>,
        transform: &str,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let name = match transform {
            "RotateAxis" => "RotateAxis",
            "RotateOrigin" => "RotateOrigin",
            "Translate" => "Translate",
            "Scale" => "Scale",
            "Matrix" | "SetMatrix" => "SetMatrix",
            other => {
                return Err(PyException::new_err(format!(
                    "AddTransform: Unkown transformation \"{other}\""
                )))
            }
        };
        slf.call_method(name, args, kwargs)?;
        Ok(())
    }

    #[pyo3(signature = (vec, concatenate = true))]
    fn Translate(&mut self, py: Python<'_>, vec: Vec<f64>, concatenate: bool) -> PyResult<()> {
        if vec.len() != 3 {
            return Err(PyValueError::new_err(
                "Translate vector must be a list or array of length 3",
            ));
        }
        let translation = [vec[0], vec[1], vec[2]];
        self.with_mut(py, |transform| {
            transform.translate(translation, concatenate)
        })
    }

    #[pyo3(signature = (mat, concatenate = true))]
    fn SetMatrix(
        &mut self,
        py: Python<'_>,
        mat: &Bound<'_, PyAny>,
        concatenate: bool,
    ) -> PyResult<()> {
        let rows = mat
            .extract::<Vec<Vec<f64>>>()
            .map_err(|_| PyValueError::new_err("SetMatrix: matrix must be of shape (4,4)"))?;
        if rows.len() != 4 || rows.iter().any(|row| row.len() != 4) {
            return Err(PyValueError::new_err(
                "SetMatrix: matrix must be of shape (4,4)",
            ));
        }
        let mut matrix = [0.0f64; 16];
        for (row, values) in rows.iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                matrix[4 * row + column] = *value;
            }
        }
        self.with_mut(py, |transform| transform.set_matrix(matrix, concatenate))
    }

    #[pyo3(signature = (vec, angle, deg = true, concatenate = true))]
    fn RotateOrigin(
        &mut self,
        py: Python<'_>,
        vec: Vec<f64>,
        angle: f64,
        deg: bool,
        concatenate: bool,
    ) -> PyResult<()> {
        if vec.len() != 3 {
            return Err(PyValueError::new_err(
                "RotateOrigin: axis vector must be a list or array of length 3",
            ));
        }
        let axis = [vec[0], vec[1], vec[2]];
        let angle = if deg { angle.to_radians() } else { angle };
        self.with_mut(py, |transform| {
            transform
                .rotate_origin(axis, angle, concatenate)
                .map_err(|error| PyValueError::new_err(format!("RotateOrigin: {error}")))
        })?
    }

    #[pyo3(signature = (ny, angle, deg = true, concatenate = true))]
    fn RotateAxis(
        &mut self,
        py: Python<'_>,
        ny: &Bound<'_, PyAny>,
        angle: f64,
        deg: bool,
        concatenate: bool,
    ) -> PyResult<()> {
        let direction = check_ny_dir(ny)?;
        let angle = if deg { angle.to_radians() } else { angle };
        self.with_mut(py, |transform| {
            transform
                .rotate_axis(direction, angle, concatenate)
                .map_err(|error| PyValueError::new_err(format!("RotateAxis: {error}")))
        })?
    }

    #[pyo3(signature = (scale, concatenate = true))]
    fn Scale(
        &mut self,
        py: Python<'_>,
        scale: &Bound<'_, PyAny>,
        concatenate: bool,
    ) -> PyResult<()> {
        if let Ok(value) = scale.extract::<f64>() {
            return self.with_mut(py, |transform| transform.scale(value, concatenate));
        }
        let values = scale.extract::<Vec<f64>>().map_err(|_| {
            PyValueError::new_err("Scale: scale must be a float or array of length 3")
        })?;
        if values.len() != 3 {
            return Err(PyValueError::new_err(
                "Scale: scale must be a float or array of length 3",
            ));
        }
        let values = [values[0], values[1], values[2]];
        self.with_mut(py, |transform| transform.scale3(values, concatenate))
    }

    fn SetPreMultiply(&mut self, py: Python<'_>) -> PyResult<()> {
        self.with_mut(py, Transform::set_pre_multiply)
    }

    fn SetPostMultiply(&mut self, py: Python<'_>) -> PyResult<()> {
        self.with_mut(py, Transform::set_post_multiply)
    }
}
