//! Python wrapper for the native `CSRectGrid` class.
//!
//! The Rust grid already keeps the mesh lines sorted and unique (deferred
//! sorting, increasing appends); this module mirrors the Cython API, including
//! the Python-level index rules of the native wrapper (`IndexError` for out of
//! range indices, negative indices counting from the end, `TypeError` for
//! non-integer indices).
//!
//! A grid either owns its lines (a standalone `CSRectGrid`) or is the grid of a
//! `ContinuousStructure`, which is the same object the structure serializes.

use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::{PyResult, Python};
use pyo3::types::{PyAny, PyAnyMethods, PyDict, PyTypeMethods};
use pyo3::{pyclass, pymethods, Bound, Py};

use csxcad_core::grid::{CoordinateSystem, RectGrid};

use crate::structure_py::PyStructure;

/// Translate a direction given as `0/1/2`, `'x'/'y'/'z'` or `'r'/'a'/'z'`.
pub(crate) fn check_ny_dir(ny: &Bound<'_, PyAny>) -> PyResult<usize> {
    if let Ok(index) = ny.call_method0("__index__") {
        if let Ok(value) = index.extract::<i128>() {
            if (0..3).contains(&value) {
                return Ok(value as usize);
            }
        }
    }
    if let Ok(name) = ny.extract::<String>() {
        match name.as_str() {
            "x" | "r" => return Ok(0),
            "y" | "a" => return Ok(1),
            "z" => return Ok(2),
            _ => {}
        }
    }
    Err(PyValueError::new_err(format!(
        "Invalid direction `{ny}`, valid directions are (0, 1, 2), ('x', 'y', 'z') in Cartesian \
         coordinates or ('r', 'a', 'z') in cylindrical coordinates."
    )))
}

/// `'all'` expands to every direction, anything else to a single direction.
fn check_multi_dirs(ny: &Bound<'_, PyAny>) -> PyResult<Vec<usize>> {
    if let Ok(name) = ny.extract::<String>() {
        if name == "all" || name == "xyz" || name == "arz" {
            return Ok(vec![0, 1, 2]);
        }
    }
    Ok(vec![check_ny_dir(ny)?])
}

/// Python `operator.index` semantics: ints and `__index__` objects only.
fn py_index(value: &Bound<'_, PyAny>) -> PyResult<i128> {
    let index = value.call_method0("__index__").map_err(|_| {
        PyTypeError::new_err(format!(
            "line index must be an integer, not {}",
            value
                .get_type()
                .name()
                .map(|name| name.to_string())
                .unwrap_or_else(|_| "object".to_string())
        ))
    })?;
    // Anything that does not fit into i128 is astronomically out of range for a
    // line vector, so report it the same way as any other out of range index.
    index
        .extract::<i128>()
        .map_err(|_| PyIndexError::new_err("GetLine: line index out of range".to_string()))
}

fn numpy_array<'py>(py: Python<'py>, data: Vec<f64>) -> PyResult<Bound<'py, PyAny>> {
    py.import("numpy")?.call_method1("array", (data,))
}

fn mesh_type(cs_type: i32) -> PyResult<CoordinateSystem> {
    match cs_type {
        0 => Ok(CoordinateSystem::Cartesian),
        1 => Ok(CoordinateSystem::Cylindrical),
        other => Err(PyValueError::new_err(format!(
            "Unknown coordinate system: {other}"
        ))),
    }
}

fn mesh_type_value(system: CoordinateSystem) -> i32 {
    match system {
        CoordinateSystem::Cartesian => 0,
        CoordinateSystem::Cylindrical => 1,
        CoordinateSystem::Undefined => 2,
    }
}

enum GridBacking {
    Local(RectGrid),
    Attached(Py<PyStructure>),
}

/// Python wrapper around the Rust `RectGrid` (native `CSRectGrid`).
#[pyclass(unsendable, weakref, name = "CSRectGrid")]
pub struct PyCSRectGrid {
    backing: GridBacking,
}

impl PyCSRectGrid {
    fn with_grid<R>(&self, py: Python<'_>, f: impl FnOnce(&RectGrid) -> R) -> PyResult<R> {
        match &self.backing {
            GridBacking::Local(grid) => Ok(f(grid)),
            GridBacking::Attached(owner) => {
                let structure = owner.borrow(py);
                Ok(f(structure.grid()))
            }
        }
    }

    fn with_grid_mut<R>(
        &mut self,
        py: Python<'_>,
        f: impl FnOnce(&mut RectGrid) -> R,
    ) -> PyResult<R> {
        match &mut self.backing {
            GridBacking::Local(grid) => Ok(f(grid)),
            GridBacking::Attached(owner) => {
                let mut structure = owner.borrow_mut(py);
                Ok(f(structure.grid_mut()))
            }
        }
    }
}

#[pymethods]
impl PyCSRectGrid {
    #[new]
    #[pyo3(signature = (CoordSystem = 0))]
    fn py_new(CoordSystem: i32) -> PyResult<Self> {
        let mut grid = RectGrid::new();
        grid.set_coordinate_system(mesh_type(CoordSystem)?);
        Ok(Self {
            backing: GridBacking::Local(grid),
        })
    }

    /// Grid of an existing structure (used by `ContinuousStructure.GetGrid`).
    #[staticmethod]
    fn _from_structure(structure: &Bound<'_, PyStructure>) -> Self {
        Self {
            backing: GridBacking::Attached(structure.clone().unbind()),
        }
    }

    fn _is_attached(&self) -> bool {
        matches!(self.backing, GridBacking::Attached(_))
    }

    fn SetMeshType(&mut self, py: Python<'_>, cs_type: i32) -> PyResult<()> {
        let system = mesh_type(cs_type)?;
        self.with_grid_mut(py, |grid| grid.set_coordinate_system(system))
    }

    fn GetMeshType(&self, py: Python<'_>) -> PyResult<i32> {
        self.with_grid(py, |grid| mesh_type_value(grid.coordinate_system()))
    }

    fn SetLines(&mut self, py: Python<'_>, ny: &Bound<'_, PyAny>, lines: Vec<f64>) -> PyResult<()> {
        let axis = check_ny_dir(ny)?;
        if lines.is_empty() {
            return Err(PyValueError::new_err(
                "SetLines: \"lines\" must be an array or list",
            ));
        }
        self.with_grid_mut(py, |grid| {
            grid.clear_lines(axis);
            grid.add_lines(axis, &lines);
        })
    }

    fn AddLine(
        &mut self,
        py: Python<'_>,
        ny: &Bound<'_, PyAny>,
        line: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let axis = check_ny_dir(ny)?;
        if let Ok(value) = line.extract::<f64>() {
            return self.with_grid_mut(py, |grid| grid.add_line(axis, value));
        }
        let values = line.extract::<Vec<f64>>().map_err(|_| {
            PyValueError::new_err("AddLine: \"lines\" must be a float, array or list")
        })?;
        if values.is_empty() {
            return Err(PyValueError::new_err(
                "AddLine: \"lines\" must be a float, array or list",
            ));
        }
        self.with_grid_mut(py, |grid| grid.add_lines(axis, &values))
    }

    fn GetQtyLines(&mut self, py: Python<'_>, ny: &Bound<'_, PyAny>) -> PyResult<usize> {
        let axis = check_ny_dir(ny)?;
        self.with_grid_mut(py, |grid| grid.line_count(axis))
    }

    fn GetLine(
        &mut self,
        py: Python<'_>,
        ny: &Bound<'_, PyAny>,
        idx: &Bound<'_, PyAny>,
    ) -> PyResult<f64> {
        let axis = check_ny_dir(ny)?;
        let index = py_index(idx)?;
        let (count, value) = self.with_grid_mut(py, |grid| {
            let count = grid.line_count(axis) as i128;
            let position = if index < 0 { index + count } else { index };
            if position < 0 || position >= count {
                return (count, None);
            }
            (count, Some(grid.line(axis, position as usize)))
        })?;
        value.ok_or_else(|| {
            PyIndexError::new_err(format!(
                "GetLine: line index {index} out of range for {count} lines"
            ))
        })
    }

    #[pyo3(signature = (ny, do_sort = true))]
    fn GetLines<'py>(
        &mut self,
        py: Python<'py>,
        ny: &Bound<'_, PyAny>,
        do_sort: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = do_sort; // deprecated: the lines are always sorted and unique
        let axis = check_ny_dir(ny)?;
        let lines = self.with_grid_mut(py, |grid| {
            grid.lines(axis).map(<[f64]>::to_vec).unwrap_or_default()
        })?;
        numpy_array(py, lines)
    }

    fn ClearLines(&mut self, py: Python<'_>, ny: &Bound<'_, PyAny>) -> PyResult<()> {
        let axis = check_ny_dir(ny)?;
        self.with_grid_mut(py, |grid| grid.clear_lines(axis))
    }

    #[pyo3(signature = (ny, max_res, ratio = 1.5, check_symmetry = true))]
    fn SmoothMeshLines(
        &mut self,
        py: Python<'_>,
        ny: &Bound<'_, PyAny>,
        max_res: f64,
        ratio: f64,
        check_symmetry: bool,
    ) -> PyResult<()> {
        let axes = check_multi_dirs(ny)?;
        let module = py.import("CSXCAD.SmoothMeshLines")?;
        let smooth = module.getattr("SmoothMeshLines")?;
        for axis in axes {
            let lines = self.with_grid_mut(py, |grid| {
                grid.lines(axis).map(<[f64]>::to_vec).unwrap_or_default()
            })?;
            let array = numpy_array(py, lines)?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("check_symmetry", check_symmetry)?;
            let smoothed = smooth.call((array, max_res, ratio), Some(&kwargs))?;
            let smoothed: Vec<f64> = smoothed.extract()?;
            self.with_grid_mut(py, |grid| {
                grid.clear_lines(axis);
                grid.add_lines(axis, &smoothed);
            })?;
        }
        Ok(())
    }

    fn Clear(&mut self, py: Python<'_>) -> PyResult<()> {
        self.with_grid_mut(py, |grid| grid.clear())
    }

    fn SetDeltaUnit(&mut self, py: Python<'_>, unit: f64) -> PyResult<()> {
        self.with_grid_mut(py, |grid| grid.set_delta_unit(unit))
    }

    fn GetDeltaUnit(&self, py: Python<'_>) -> PyResult<f64> {
        self.with_grid(py, |grid| grid.delta_unit())
    }

    #[pyo3(signature = (ny = None))]
    fn Sort(&mut self, py: Python<'_>, ny: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let axes = match ny {
            Some(ny) => check_multi_dirs(ny)?,
            None => vec![0, 1, 2],
        };
        self.with_grid_mut(py, |grid| {
            for axis in axes {
                grid.sort(axis);
            }
        })
    }

    fn Snap2LineNumber(
        &mut self,
        py: Python<'_>,
        ny: &Bound<'_, PyAny>,
        value: f64,
    ) -> PyResult<(i64, bool)> {
        let axis = check_ny_dir(ny)?;
        self.with_grid_mut(py, |grid| match grid.snap_to_line(axis, value) {
            (Some(index), inside) => (index as i64, inside),
            (None, _) => (-1, false),
        })
    }

    fn GetSimArea<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let area = self.with_grid_mut(py, |grid| grid.simulation_area())?;
        let rows = vec![
            vec![area[0], area[2], area[4]],
            vec![area[1], area[3], area[5]],
        ];
        py.import("numpy")?.call_method1("array", (rows,))
    }

    fn IsValid(&mut self, py: Python<'_>) -> PyResult<bool> {
        self.with_grid_mut(py, |grid| grid.is_valid())
    }
}
