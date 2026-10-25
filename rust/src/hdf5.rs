use crate::material::{DiscreteMaterialField, DiscreteMaterialGrid, DiscreteMaterialGridError};
use std::error::Error;
use std::ffi::{c_char, c_int, c_uint, c_void, CString};
use std::fmt;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

type Hid = i64;
type Herr = c_int;
type HSize = u64;
type Hssize = i64;
type CloseFunction = unsafe extern "C" fn(Hid) -> Herr;

const H5P_DEFAULT: Hid = 0;
const H5S_ALL: Hid = 0;
const H5F_ACC_RDONLY: c_uint = 0;

#[link(name = "hdf5")]
unsafe extern "C" {
    static mut H5T_NATIVE_DOUBLE_g: Hid;
    static mut H5T_NATIVE_FLOAT_g: Hid;
    static mut H5T_NATIVE_INT_g: Hid;
    static mut H5T_NATIVE_UINT8_g: Hid;

    fn H5open() -> Herr;
    fn H5Fopen(filename: *const c_char, flags: c_uint, fapl: Hid) -> Hid;
    fn H5Fclose(file: Hid) -> Herr;
    fn H5Gopen2(location: Hid, name: *const c_char, gapl: Hid) -> Hid;
    fn H5Gclose(group: Hid) -> Herr;
    fn H5Dopen2(location: Hid, name: *const c_char, dapl: Hid) -> Hid;
    fn H5Dclose(dataset: Hid) -> Herr;
    fn H5Dget_space(dataset: Hid) -> Hid;
    fn H5Dread(
        dataset: Hid,
        memory_type: Hid,
        memory_space: Hid,
        file_space: Hid,
        transfer: Hid,
        buffer: *mut c_void,
    ) -> Herr;
    fn H5Sclose(space: Hid) -> Herr;
    fn H5Sget_simple_extent_ndims(space: Hid) -> c_int;
    fn H5Sget_simple_extent_dims(
        space: Hid,
        dimensions: *mut HSize,
        maximum_dimensions: *mut HSize,
    ) -> c_int;
    fn H5Sget_simple_extent_npoints(space: Hid) -> Hssize;
    fn H5Aexists(object: Hid, name: *const c_char) -> c_int;
    fn H5Aopen(object: Hid, name: *const c_char, access: Hid) -> Hid;
    fn H5Aclose(attribute: Hid) -> Herr;
    fn H5Aget_space(attribute: Hid) -> Hid;
    fn H5Aread(attribute: Hid, memory_type: Hid, buffer: *mut c_void) -> Herr;
}

static HDF5_ACCESS: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq)]
pub enum DiscreteMaterialLoadError {
    PathNotUnicode,
    PathContainsNul,
    MissingFilename,
    UnsupportedFileType(i32),
    Hdf5(String),
    UnsupportedVersion(f64),
    MissingAttribute {
        object: String,
        name: String,
    },
    InvalidDatabaseSize(i32),
    InvalidDatasetRank {
        path: String,
        expected: usize,
        actual: usize,
    },
    InvalidDatasetShape {
        path: String,
        detail: String,
    },
    Grid(DiscreteMaterialGridError),
}

impl fmt::Display for DiscreteMaterialLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathNotUnicode => formatter.write_str("HDF5 path is not valid Unicode"),
            Self::PathContainsNul => formatter.write_str("HDF5 path contains a NUL byte"),
            Self::MissingFilename => formatter.write_str("discrete-material filename is empty"),
            Self::UnsupportedFileType(file_type) => {
                write!(
                    formatter,
                    "unsupported discrete-material file type {file_type}"
                )
            }
            Self::Hdf5(context) => write!(formatter, "HDF5 read failed: {context}"),
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported discrete-material HDF5 version {version}"
                )
            }
            Self::MissingAttribute { object, name } => {
                write!(formatter, "missing HDF5 attribute {name:?} on {object:?}")
            }
            Self::InvalidDatabaseSize(size) => {
                write!(formatter, "invalid discrete-material database size {size}")
            }
            Self::InvalidDatasetRank {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "HDF5 dataset {path:?} must have rank {expected}, got {actual}"
            ),
            Self::InvalidDatasetShape { path, detail } => {
                write!(formatter, "invalid HDF5 dataset {path:?}: {detail}")
            }
            Self::Grid(error) => write!(formatter, "invalid discrete-material grid: {error}"),
        }
    }
}

impl Error for DiscreteMaterialLoadError {}

impl From<DiscreteMaterialGridError> for DiscreteMaterialLoadError {
    fn from(error: DiscreteMaterialGridError) -> Self {
        Self::Grid(error)
    }
}

struct Handle {
    id: Hid,
    close: CloseFunction,
}

impl Handle {
    fn open(
        id: Hid,
        close: CloseFunction,
        description: impl Into<String>,
    ) -> Result<Self, DiscreteMaterialLoadError> {
        if id < 0 {
            return Err(DiscreteMaterialLoadError::Hdf5(description.into()));
        }
        Ok(Self { id, close })
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.id);
        }
    }
}

impl DiscreteMaterialGrid {
    pub fn read_hdf5(path: impl AsRef<Path>) -> Result<Self, DiscreteMaterialLoadError> {
        let _guard = HDF5_ACCESS
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| DiscreteMaterialLoadError::Hdf5("access lock is poisoned".to_owned()))?;
        let filename_text = path
            .as_ref()
            .to_str()
            .ok_or(DiscreteMaterialLoadError::PathNotUnicode)?;
        let filename =
            CString::new(filename_text).map_err(|_| DiscreteMaterialLoadError::PathContainsNul)?;

        if unsafe { H5open() } < 0 {
            return Err(DiscreteMaterialLoadError::Hdf5(
                "failed to initialize the HDF5 library".to_owned(),
            ));
        }
        let file = Handle::open(
            unsafe { H5Fopen(filename.as_ptr(), H5F_ACC_RDONLY, H5P_DEFAULT) },
            H5Fclose,
            format!("failed to open file {filename_text:?}"),
        )?;
        let native_double = unsafe { H5T_NATIVE_DOUBLE_g };
        let native_float = unsafe { H5T_NATIVE_FLOAT_g };
        let native_int = unsafe { H5T_NATIVE_INT_g };
        let native_uint8 = unsafe { H5T_NATIVE_UINT8_g };

        let root_name = c_string("/")?;
        let root = Handle::open(
            unsafe { H5Gopen2(file.id, root_name.as_ptr(), H5P_DEFAULT) },
            H5Gclose,
            "failed to open the HDF5 root group",
        )?;
        let version =
            read_scalar_attribute::<f64>(root.id, "/", "Version", native_double)?.unwrap_or(1.0);
        if !version.is_finite() || version < 2.0 {
            return Err(DiscreteMaterialLoadError::UnsupportedVersion(version));
        }

        let disc_path = "/DiscData";
        let disc_dataset = open_dataset(file.id, disc_path)?;
        let database_size =
            read_scalar_attribute::<i32>(disc_dataset.id, disc_path, "DB_Size", native_int)?
                .ok_or_else(|| DiscreteMaterialLoadError::MissingAttribute {
                    object: disc_path.to_owned(),
                    name: "DB_Size".to_owned(),
                })?;
        let database_size = usize::try_from(database_size)
            .map_err(|_| DiscreteMaterialLoadError::InvalidDatabaseSize(database_size))?;

        let mut database_values = Vec::with_capacity(5);
        for (field, name) in [
            (DiscreteMaterialField::Epsilon, "epsR"),
            (DiscreteMaterialField::Kappa, "kappa"),
            (DiscreteMaterialField::Mue, "mueR"),
            (DiscreteMaterialField::Sigma, "sigma"),
            (DiscreteMaterialField::Density, "density"),
        ] {
            database_values.push((
                field,
                read_float_attribute(
                    disc_dataset.id,
                    disc_path,
                    name,
                    database_size,
                    native_float,
                )?,
            ));
        }
        drop(disc_dataset);

        let mut mesh = [Vec::new(), Vec::new(), Vec::new()];
        for (axis, direction) in ["x", "y", "z"].into_iter().enumerate() {
            let dataset_path = format!("/mesh/{direction}");
            let (coordinates, dimensions) =
                read_dataset::<f32>(file.id, &dataset_path, native_float)?;
            if dimensions.len() != 1 {
                return Err(DiscreteMaterialLoadError::InvalidDatasetRank {
                    path: dataset_path,
                    expected: 1,
                    actual: dimensions.len(),
                });
            }
            mesh[axis] = coordinates;
        }

        let (indices, dimensions) = read_dataset::<u8>(file.id, disc_path, native_uint8)?;
        if dimensions.len() != 3 {
            return Err(DiscreteMaterialLoadError::InvalidDatasetRank {
                path: disc_path.to_owned(),
                expected: 3,
                actual: dimensions.len(),
            });
        }
        let mut grid = Self::new(mesh, indices, database_size)?;
        for (field, values) in database_values {
            grid.set_database_values(field, values)?;
        }
        Ok(grid)
    }
}

/// Sampled 2-D mode profile for excitations and probes.
///
/// Mirrors the native `CSModeData`: the HDF5 file holds the 1-D axes `/x` and
/// `/y` plus the 2-D transverse components `/Vx` and `/Vy` (shape `nx x ny`,
/// row-major) and a scalar root attribute `Version` (>= 1.0).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModeData {
    x: Vec<f64>,
    y: Vec<f64>,
    vx: Vec<f64>,
    vy: Vec<f64>,
}

impl ModeData {
    pub fn is_loaded(&self) -> bool {
        !self.vx.is_empty()
    }

    pub fn read_hdf5(path: impl AsRef<Path>) -> Result<Self, DiscreteMaterialLoadError> {
        let _guard = HDF5_ACCESS
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| DiscreteMaterialLoadError::Hdf5("access lock is poisoned".to_owned()))?;
        let filename_text = path
            .as_ref()
            .to_str()
            .ok_or(DiscreteMaterialLoadError::PathNotUnicode)?;
        let filename =
            CString::new(filename_text).map_err(|_| DiscreteMaterialLoadError::PathContainsNul)?;

        if unsafe { H5open() } < 0 {
            return Err(DiscreteMaterialLoadError::Hdf5(
                "failed to initialize the HDF5 library".to_owned(),
            ));
        }
        let file = Handle::open(
            unsafe { H5Fopen(filename.as_ptr(), H5F_ACC_RDONLY, H5P_DEFAULT) },
            H5Fclose,
            format!("failed to open file {filename_text:?}"),
        )?;
        let native_double = unsafe { H5T_NATIVE_DOUBLE_g };

        let root_name = c_string("/")?;
        let root = Handle::open(
            unsafe { H5Gopen2(file.id, root_name.as_ptr(), H5P_DEFAULT) },
            H5Gclose,
            "failed to open the HDF5 root group",
        )?;
        let version = read_scalar_attribute::<f64>(root.id, "/", "Version", native_double)?
            .unwrap_or(0.0);
        if !version.is_finite() || version < 1.0 {
            return Err(DiscreteMaterialLoadError::UnsupportedVersion(version));
        }

        let (x, x_dimensions) = read_dataset::<f64>(file.id, "/x", native_double)?;
        let (y, y_dimensions) = read_dataset::<f64>(file.id, "/y", native_double)?;
        for (path, dimensions) in [("/x", &x_dimensions), ("/y", &y_dimensions)] {
            if dimensions.len() != 1 {
                return Err(DiscreteMaterialLoadError::InvalidDatasetRank {
                    path: path.to_owned(),
                    expected: 1,
                    actual: dimensions.len(),
                });
            }
        }
        if x.len() < 2 || y.len() < 2 {
            return Err(DiscreteMaterialLoadError::Hdf5(
                "mode axes need at least two points".to_owned(),
            ));
        }

        let (vx, vx_dimensions) = read_dataset::<f64>(file.id, "/Vx", native_double)?;
        let (vy, vy_dimensions) = read_dataset::<f64>(file.id, "/Vy", native_double)?;
        let expected = [x.len(), y.len()];
        for (path, dimensions) in [("/Vx", &vx_dimensions), ("/Vy", &vy_dimensions)] {
            if dimensions != &expected {
                return Err(DiscreteMaterialLoadError::Hdf5(format!(
                    "dataset {path} has shape {dimensions:?}, expected {expected:?}"
                )));
            }
        }

        Ok(Self { x, y, vx, vy })
    }

    /// Bilinear interpolation at `(x, y)`, clamped to the grid extent.
    pub fn interpolate(&self, x: f64, y: f64) -> [f64; 2] {
        if !self.is_loaded() {
            return [0.0, 0.0];
        }
        let x = x.clamp(self.x[0], self.x[self.x.len() - 1]);
        let y = y.clamp(self.y[0], self.y[self.y.len() - 1]);
        let nx = self.x.len();
        let ny = self.y.len();

        let i = left_index(&self.x, x).min(nx - 2);
        let j = left_index(&self.y, y).min(ny - 2);

        let (x1, x2) = (self.x[i], self.x[i + 1]);
        let (y1, y2) = (self.y[j], self.y[j + 1]);
        let d = 1.0 / ((x2 - x1) * (y2 - y1));

        let interpolate = |values: &[f64]| {
            let f11 = values[i * ny + j];
            let f21 = values[(i + 1) * ny + j];
            let f12 = values[i * ny + j + 1];
            let f22 = values[(i + 1) * ny + j + 1];
            d * (f11 * (x2 - x) * (y2 - y)
                + f21 * (x - x1) * (y2 - y)
                + f12 * (x2 - x) * (y - y1)
                + f22 * (x - x1) * (y - y1))
        };

        [interpolate(&self.vx), interpolate(&self.vy)]
    }
}

fn left_index(axis: &[f64], value: f64) -> usize {
    match axis.binary_search_by(|probe| probe.partial_cmp(&value).unwrap_or(std::cmp::Ordering::Equal)) {
        Ok(index) => index.saturating_sub(1),
        Err(index) => index.saturating_sub(1),
    }
}

fn c_string(value: &str) -> Result<CString, DiscreteMaterialLoadError> {
    CString::new(value).map_err(|_| DiscreteMaterialLoadError::PathContainsNul)
}

fn open_dataset(file: Hid, path: &str) -> Result<Handle, DiscreteMaterialLoadError> {
    let name = c_string(path)?;
    Handle::open(
        unsafe { H5Dopen2(file, name.as_ptr(), H5P_DEFAULT) },
        H5Dclose,
        format!("failed to open dataset {path:?}"),
    )
}

fn read_scalar_attribute<T: Default>(
    object: Hid,
    object_name: &str,
    attribute_name: &str,
    native_type: Hid,
) -> Result<Option<T>, DiscreteMaterialLoadError> {
    let attribute = open_attribute(object, object_name, attribute_name)?;
    let Some(attribute) = attribute else {
        return Ok(None);
    };
    let space = Handle::open(
        unsafe { H5Aget_space(attribute.id) },
        H5Sclose,
        format!("failed to inspect attribute {attribute_name:?} on {object_name:?}"),
    )?;
    let count = attribute_count(space.id, object_name, attribute_name)?;
    if count != 1 {
        return Err(DiscreteMaterialLoadError::InvalidDatasetShape {
            path: object_name.to_owned(),
            detail: format!("attribute {attribute_name:?} must contain one value"),
        });
    }
    let mut value = T::default();
    if unsafe {
        H5Aread(
            attribute.id,
            native_type,
            (&mut value as *mut T).cast::<c_void>(),
        )
    } < 0
    {
        return Err(DiscreteMaterialLoadError::Hdf5(format!(
            "failed to read attribute {attribute_name:?} on {object_name:?}"
        )));
    }
    Ok(Some(value))
}

fn read_float_attribute(
    object: Hid,
    object_name: &str,
    attribute_name: &str,
    expected_count: usize,
    native_type: Hid,
) -> Result<Option<Vec<f32>>, DiscreteMaterialLoadError> {
    let attribute = open_attribute(object, object_name, attribute_name)?;
    let Some(attribute) = attribute else {
        return Ok(None);
    };
    let space = Handle::open(
        unsafe { H5Aget_space(attribute.id) },
        H5Sclose,
        format!("failed to inspect attribute {attribute_name:?} on {object_name:?}"),
    )?;
    let count = attribute_count(space.id, object_name, attribute_name)?;
    if count != expected_count {
        return Err(DiscreteMaterialLoadError::InvalidDatasetShape {
            path: object_name.to_owned(),
            detail: format!(
                "attribute {attribute_name:?} has {count} values; expected {expected_count}"
            ),
        });
    }
    let mut values = vec![0.0; count];
    if count > 0
        && unsafe {
            H5Aread(
                attribute.id,
                native_type,
                values.as_mut_ptr().cast::<c_void>(),
            )
        } < 0
    {
        return Err(DiscreteMaterialLoadError::Hdf5(format!(
            "failed to read attribute {attribute_name:?} on {object_name:?}"
        )));
    }
    Ok(Some(values))
}

fn open_attribute(
    object: Hid,
    object_name: &str,
    attribute_name: &str,
) -> Result<Option<Handle>, DiscreteMaterialLoadError> {
    let name = c_string(attribute_name)?;
    let exists = unsafe { H5Aexists(object, name.as_ptr()) };
    if exists < 0 {
        return Err(DiscreteMaterialLoadError::Hdf5(format!(
            "failed to check attribute {attribute_name:?} on {object_name:?}"
        )));
    }
    if exists == 0 {
        return Ok(None);
    }
    Ok(Some(Handle::open(
        unsafe { H5Aopen(object, name.as_ptr(), H5P_DEFAULT) },
        H5Aclose,
        format!("failed to open attribute {attribute_name:?} on {object_name:?}"),
    )?))
}

fn attribute_count(
    space: Hid,
    object_name: &str,
    attribute_name: &str,
) -> Result<usize, DiscreteMaterialLoadError> {
    usize::try_from(unsafe { H5Sget_simple_extent_npoints(space) }).map_err(|_| {
        DiscreteMaterialLoadError::Hdf5(format!(
            "failed to read extent of attribute {attribute_name:?} on {object_name:?}"
        ))
    })
}

fn read_dataset<T: Default + Clone>(
    file: Hid,
    path: &str,
    native_type: Hid,
) -> Result<(Vec<T>, Vec<usize>), DiscreteMaterialLoadError> {
    let dataset = open_dataset(file, path)?;
    let space = Handle::open(
        unsafe { H5Dget_space(dataset.id) },
        H5Sclose,
        format!("failed to inspect dataset {path:?}"),
    )?;
    let rank = unsafe { H5Sget_simple_extent_ndims(space.id) };
    if rank < 0 {
        return Err(DiscreteMaterialLoadError::Hdf5(format!(
            "failed to read rank of dataset {path:?}"
        )));
    }
    let mut raw_dimensions = vec![0; rank as usize];
    if unsafe {
        H5Sget_simple_extent_dims(space.id, raw_dimensions.as_mut_ptr(), std::ptr::null_mut())
    } < 0
    {
        return Err(DiscreteMaterialLoadError::Hdf5(format!(
            "failed to read dimensions of dataset {path:?}"
        )));
    }
    let dimensions = raw_dimensions
        .into_iter()
        .map(|dimension| {
            usize::try_from(dimension).map_err(|_| DiscreteMaterialLoadError::InvalidDatasetShape {
                path: path.to_owned(),
                detail: "dimension exceeds addressable memory".to_owned(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count = dimensions
        .iter()
        .try_fold(1usize, |count, dimension| count.checked_mul(*dimension));
    let Some(count) = count else {
        return Err(DiscreteMaterialLoadError::InvalidDatasetShape {
            path: path.to_owned(),
            detail: "element count overflows addressable memory".to_owned(),
        });
    };
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|error| {
        DiscreteMaterialLoadError::InvalidDatasetShape {
            path: path.to_owned(),
            detail: format!("cannot allocate {count} elements: {error}"),
        }
    })?;
    values.resize(count, T::default());
    if count > 0
        && unsafe {
            H5Dread(
                dataset.id,
                native_type,
                H5S_ALL,
                H5S_ALL,
                H5P_DEFAULT,
                values.as_mut_ptr().cast::<c_void>(),
            )
        } < 0
    {
        return Err(DiscreteMaterialLoadError::Hdf5(format!(
            "failed to read dataset {path:?}"
        )));
    }
    Ok((values, dimensions))
}

#[cfg(test)]
mod tests {
    use crate::grid::CoordinateSystem;
    use crate::material::{DiscreteMaterialField, DiscreteMaterialProperties};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    struct TestFile(PathBuf);

    impl TestFile {
        fn from_fixture() -> Self {
            let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "csxcad-disc-material-{}-{sequence}.h5",
                std::process::id()
            ));
            fs::write(
                &path,
                include_bytes!("../tests/fixtures/discrete_material_v2.h5"),
            )
            .unwrap();
            Self(path)
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn reads_native_discrete_material_hdf5_layout_and_properties() {
        let file = TestFile::from_fixture();
        let mut properties = DiscreteMaterialProperties::default();
        properties.set_file_type(0);
        properties.set_file_name(file.0.to_string_lossy().into_owned());
        let grid = properties.load_grid().unwrap();
        let coordinates = [1.5, 0.5, 0.5];
        assert_eq!(
            grid.material_index(coordinates, CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            Some(1)
        );
        for (field, expected) in [
            (DiscreteMaterialField::Epsilon, 2.0),
            (DiscreteMaterialField::Kappa, 11.0),
            (DiscreteMaterialField::Mue, 21.0),
            (DiscreteMaterialField::Sigma, 31.0),
            (DiscreteMaterialField::Density, 41.0),
        ] {
            assert_eq!(
                grid.material_value(field, coordinates, CoordinateSystem::Cartesian, &properties)
                    .unwrap(),
                Some(expected)
            );
        }
    }

    #[test]
    fn reports_unsupported_discrete_material_file_configuration() {
        let properties = DiscreteMaterialProperties::default();
        assert_eq!(
            properties.load_grid().unwrap_err(),
            super::DiscreteMaterialLoadError::UnsupportedFileType(-1)
        );
        let mut properties = DiscreteMaterialProperties::default();
        properties.set_file_type(0);
        assert_eq!(
            properties.load_grid().unwrap_err(),
            super::DiscreteMaterialLoadError::MissingFilename
        );
    }

    #[test]
    fn reports_invalid_hdf5_paths() {
        let error =
            super::DiscreteMaterialGrid::read_hdf5(std::path::Path::new("invalid\0path.h5"))
                .unwrap_err();
        assert_eq!(error, super::DiscreteMaterialLoadError::PathContainsNul);
    }
}

#[cfg(test)]
mod mode_data_tests {
    use super::ModeData;

    fn sample() -> ModeData {
        // x = [0, 1, 2], y = [0, 1]; values are row-major A[i * ny + j]
        ModeData {
            x: vec![0.0, 1.0, 2.0],
            y: vec![0.0, 1.0],
            vx: vec![0.0, 0.0, 1.0, 1.0, 2.0, 2.0],
            vy: vec![0.0, 10.0, 0.0, 10.0, 0.0, 10.0],
        }
    }

    #[test]
    fn interpolates_bilinearly_inside_the_grid() {
        let data = sample();
        let [vx, vy] = data.interpolate(0.5, 0.25);
        assert!((vx - 0.5).abs() < 1e-12);
        assert!((vy - 2.5).abs() < 1e-12);
        let [vx, vy] = data.interpolate(1.5, 0.75);
        assert!((vx - 1.5).abs() < 1e-12);
        assert!((vy - 7.5).abs() < 1e-12);
    }

    #[test]
    fn clamps_coordinates_outside_the_grid() {
        let data = sample();
        assert_eq!(data.interpolate(-5.0, -5.0), [0.0, 0.0]);
        assert_eq!(data.interpolate(5.0, 5.0), [2.0, 10.0]);
    }

    #[test]
    fn empty_data_returns_zero() {
        let data = ModeData::default();
        assert!(!data.is_loaded());
        assert_eq!(data.interpolate(1.0, 1.0), [0.0, 0.0]);
    }
}
