use crate::grid::{transform_coordinates, CoordinateSystem, RectGrid};
use crate::material::{
    AbsorbingBoundaryProperties, BackgroundMaterial, ConductingSheetProperties,
    DebyeMaterialProperties, DiscreteMaterialProperties, DumpBoxProperties, ExcitationProperties,
    LorentzMaterialProperties, LumpedElementProperties, MaterialProperties, ProbeProperties,
    ResolutionBoxProperties,
};
use crate::parameter::{ParameterError, ParameterScalar, ParameterSet};
use crate::transform::{Transform, TransformError};
use fparser::Parser;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::ops::BitOr;

pub use crate::polyhedron_reader::PolyhedronReaderError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PropertyId(u64);

impl PropertyId {
    pub fn from_u64(val: u64) -> Self {
        Self(val)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrimitiveId(u64);

impl PrimitiveId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertyColor {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl PropertyColor {
    pub fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn components(self) -> [u8; 4] {
        [self.red, self.green, self.blue, self.alpha]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum PropertyType {
    Any = 0xffff,
    Unknown = 0x001,
    Material = 0x002,
    Metal = 0x004,
    Excitation = 0x008,
    ProbeBox = 0x010,
    ResBox = 0x020,
    DumpBox = 0x040,
    DispersiveMaterial = 0x100,
    LorentzMaterial = 0x200,
    DebyeMaterial = 0x400,
    DiscreteMaterial = 0x1000,
    LumpedElement = 0x2000,
    ConductingSheet = 0x4000,
    AbsorbingBoundary = 0x8000,
}

impl PropertyType {
    /// Native `CSProperties::GetTypeXMLString`: the XML element name of the
    /// property type. `Any` is not a concrete property and has no name.
    pub fn xml_string(self) -> Option<&'static str> {
        match self {
            PropertyType::Any => None,
            PropertyType::Unknown => Some("Unknown"),
            PropertyType::Material => Some("Material"),
            PropertyType::Metal => Some("Metal"),
            PropertyType::Excitation => Some("Excitation"),
            PropertyType::ProbeBox => Some("ProbeBox"),
            PropertyType::ResBox => Some("ResBox"),
            PropertyType::DumpBox => Some("DumpBox"),
            PropertyType::DispersiveMaterial => Some("DispersiveMaterial"),
            PropertyType::LorentzMaterial => Some("LorentzMaterial"),
            PropertyType::DebyeMaterial => Some("DebyeMaterial"),
            PropertyType::DiscreteMaterial => Some("DiscMaterial"),
            PropertyType::LumpedElement => Some("LumpedElement"),
            PropertyType::ConductingSheet => Some("ConductingSheet"),
            PropertyType::AbsorbingBoundary => Some("AbsorbingBC"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropertyTypeMask(u16);

impl PropertyTypeMask {
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Build a mask from the native `CSProperties::PropertyType` bit values.
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    fn matches(self, property_type: PropertyType) -> bool {
        self.0 & property_type as u16 != 0
    }
}

impl From<PropertyType> for PropertyTypeMask {
    fn from(property_type: PropertyType) -> Self {
        Self(property_type as u16)
    }
}

impl BitOr for PropertyType {
    type Output = PropertyTypeMask;

    fn bitor(self, rhs: Self) -> Self::Output {
        PropertyTypeMask(self as u16 | rhs as u16)
    }
}

impl BitOr<PropertyType> for PropertyTypeMask {
    type Output = Self;

    fn bitor(self, rhs: PropertyType) -> Self::Output {
        Self(self.0 | rhs as u16)
    }
}

impl BitOr for PropertyTypeMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveType {
    Point,
    Box,
    Sphere,
    SphericalShell,
    Cylinder,
    CylindricalShell,
    Curve,
    Wire,
    Polygon,
    LinPoly,
    RotPoly,
    Polyhedron,
    MultiBox,
    UserDefined,
    PolyhedronReader,
}

impl PrimitiveType {
    /// Native `CSPrimitives::GetTypeName`: the primitive type name shown in
    /// the structure tree and used as the XML element name.
    pub fn type_name(self) -> &'static str {
        match self {
            PrimitiveType::Point => "Point",
            PrimitiveType::Box => "Box",
            PrimitiveType::Sphere => "Sphere",
            PrimitiveType::SphericalShell => "SphericalShell",
            PrimitiveType::Cylinder => "Cylinder",
            PrimitiveType::CylindricalShell => "CylindricalShell",
            PrimitiveType::Curve => "Curve",
            PrimitiveType::Wire => "Wire",
            PrimitiveType::Polygon => "Polygon",
            PrimitiveType::LinPoly => "LinPoly",
            PrimitiveType::RotPoly => "RotPoly",
            PrimitiveType::Polyhedron => "Polyhedron",
            PrimitiveType::MultiBox => "MultiBox",
            PrimitiveType::UserDefined => "User-Defined",
            PrimitiveType::PolyhedronReader => "PolyhedronReader",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModelError {
    PropertyNotFound(PropertyId),
    PrimitiveNotFound(PrimitiveId),
    PrimitiveIdNotAssigned,
    Parameter(ParameterError),
    Transform(TransformError),
    PolyhedronReader(PolyhedronReaderError),
    UnsupportedCoordinateSystem {
        mesh_type: CoordinateSystem,
        primitive_type: CoordinateSystem,
    },
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PropertyNotFound(id) => write!(formatter, "property {} was not found", id.get()),
            Self::PrimitiveNotFound(id) => {
                write!(formatter, "primitive {} was not found", id.get())
            }
            Self::PrimitiveIdNotAssigned => {
                formatter.write_str("primitive is not registered in its structure")
            }
            Self::Parameter(error) => write!(formatter, "parameter evaluation failed: {error}"),
            Self::Transform(error) => write!(formatter, "transform failed: {error}"),
            Self::PolyhedronReader(error) => write!(formatter, "polyhedron reader failed: {error}"),
            Self::UnsupportedCoordinateSystem {
                mesh_type,
                primitive_type,
            } => write!(
                formatter,
                "coordinate systems {mesh_type:?} and {primitive_type:?} are not supported here"
            ),
        }
    }
}

impl Error for ModelError {}

impl From<ParameterError> for ModelError {
    fn from(error: ParameterError) -> Self {
        Self::Parameter(error)
    }
}

impl From<TransformError> for ModelError {
    fn from(error: TransformError) -> Self {
        Self::Transform(error)
    }
}

impl From<PolyhedronReaderError> for ModelError {
    fn from(error: PolyhedronReaderError) -> Self {
        Self::PolyhedronReader(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum StructureValidationError {
    NoProperties,
    NoPrimitives,
    InsufficientGridLines {
        direction: usize,
        minimum: usize,
        actual: usize,
    },
    NoExcitationPrimitives,
    Property(StructurePropertyValidationError),
    Primitive(StructurePrimitiveValidationError),
}

impl fmt::Display for StructureValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProperties => formatter.write_str("structure has no properties"),
            Self::NoPrimitives => formatter.write_str("structure has no primitives"),
            Self::InsufficientGridLines {
                direction,
                minimum,
                actual,
            } => write!(
                formatter,
                "grid direction {direction} has {actual} lines; at least {minimum} are required"
            ),
            Self::NoExcitationPrimitives => {
                formatter.write_str("structure has no excitation property with primitives")
            }
            Self::Property(error) => error.fmt(formatter),
            Self::Primitive(error) => error.fmt(formatter),
        }
    }
}

impl Error for StructureValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Property(error) => Some(error),
            Self::Primitive(error) => Some(error),
            Self::NoProperties
            | Self::NoPrimitives
            | Self::InsufficientGridLines { .. }
            | Self::NoExcitationPrimitives => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructurePropertyValidationError {
    property_id: Option<PropertyId>,
    source: ParameterError,
}

impl StructurePropertyValidationError {
    pub fn property_id(&self) -> Option<PropertyId> {
        self.property_id
    }
}

impl fmt::Display for StructurePropertyValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(property_id) = self.property_id {
            write!(formatter, "property {}: {}", property_id.get(), self.source)
        } else {
            write!(
                formatter,
                "property parameter evaluation failed: {}",
                self.source
            )
        }
    }
}

impl Error for StructurePropertyValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructurePrimitiveValidationError {
    primitive_id: Option<PrimitiveId>,
    source: ModelError,
}

impl StructurePrimitiveValidationError {
    pub fn primitive_id(&self) -> Option<PrimitiveId> {
        self.primitive_id
    }
}

impl fmt::Display for StructurePrimitiveValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(primitive_id) = self.primitive_id {
            write!(
                formatter,
                "primitive {}: {}",
                primitive_id.get(),
                self.source
            )
        } else {
            write!(
                formatter,
                "primitive geometry validation failed: {}",
                self.source
            )
        }
    }
}

impl Error for StructurePrimitiveValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

impl From<StructurePropertyValidationError> for StructureValidationError {
    fn from(error: StructurePropertyValidationError) -> Self {
        Self::Property(error)
    }
}

impl From<StructurePrimitiveValidationError> for StructureValidationError {
    fn from(error: StructurePrimitiveValidationError) -> Self {
        Self::Primitive(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoxPrimitive {
    coordinates: [[ParameterScalar; 3]; 2],
}

impl BoxPrimitive {
    pub fn new(start: [f64; 3], stop: [f64; 3]) -> Self {
        Self {
            coordinates: [
                start.map(ParameterScalar::new),
                stop.map(ParameterScalar::new),
            ],
        }
    }

    pub fn from_expressions(start: [&str; 3], stop: [&str; 3]) -> Result<Self, ParameterError> {
        Ok(Self {
            coordinates: [
                start
                    .into_iter()
                    .map(ParameterScalar::from_value_string)
                    .collect::<Result<Vec<_>, _>>()?
                    .try_into()
                    .expect("three start coordinates"),
                stop.into_iter()
                    .map(ParameterScalar::from_value_string)
                    .collect::<Result<Vec<_>, _>>()?
                    .try_into()
                    .expect("three stop coordinates"),
            ],
        })
    }

    pub fn set_coordinate(&mut self, index: usize, value: f64) -> bool {
        if index >= 6 {
            return false;
        }
        self.coordinates[index % 2][index / 2].set_value(value);
        true
    }

    pub fn set_coordinate_expression(
        &mut self,
        index: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        if index >= 6 {
            return Ok(false);
        }
        self.coordinates[index % 2][index / 2].set_value_string(expression)?;
        Ok(true)
    }

    /// Shift both corners by `delta` (metres), the viewport drag primitive.
    pub fn translate(&mut self, delta: [f64; 3]) -> Result<(), ParameterError> {
        for index in 0..6 {
            let axis = index / 2;
            if let Some(value) = self.coordinate(index, None)? {
                self.set_coordinate(index, value + delta[axis]);
            }
        }
        Ok(())
    }

    pub fn coordinate(
        &self,
        index: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        if index >= 6 {
            return Ok(None);
        }
        self.coordinates[index % 2][index / 2]
            .evaluate(parameters)
            .map(Some)
    }

    pub fn coordinate_expression(&self, index: usize) -> Option<&str> {
        if index >= 6 {
            return None;
        }
        self.coordinates[index % 2][index / 2].expression()
    }

    pub(crate) fn coordinate_xml_term(&self, index: usize) -> Option<String> {
        if index >= 6 {
            return None;
        }
        Some(self.coordinates[index % 2][index / 2].value_string())
    }

    pub fn evaluated_coordinates(
        &self,
        parameters: Option<&ParameterSet>,
    ) -> Result<[[f64; 3]; 2], ParameterError> {
        let mut result = [[0.0; 3]; 2];
        for (point_index, point) in self.coordinates.iter().enumerate() {
            for (axis, coordinate) in point.iter().enumerate() {
                result[point_index][axis] = coordinate.evaluate(parameters)?;
            }
        }
        Ok(result)
    }

    pub fn dimension(&self, parameters: Option<&ParameterSet>) -> Result<i32, ParameterError> {
        let coordinates = self.evaluated_coordinates(parameters)?;
        Ok((0..3)
            .filter(|axis| coordinates[0][*axis] != coordinates[1][*axis])
            .count() as i32)
    }

    pub fn bounding_box(
        &self,
        parameters: Option<&ParameterSet>,
        preserve_orientation: bool,
    ) -> Result<[f64; 6], ParameterError> {
        self.bounding_box_in_coordinate_systems(
            parameters,
            preserve_orientation,
            CoordinateSystem::Cartesian,
            CoordinateSystem::Cartesian,
        )
    }

    fn bounding_box_in_coordinate_systems(
        &self,
        parameters: Option<&ParameterSet>,
        preserve_orientation: bool,
        input_system: CoordinateSystem,
        output_system: CoordinateSystem,
    ) -> Result<[f64; 6], ParameterError> {
        let mut coordinates = self.evaluated_coordinates(parameters)?;
        for point in &mut coordinates {
            *point = transform_coordinates(*point, input_system, output_system);
        }
        let mut bounds = [0.0; 6];
        for axis in 0..3 {
            bounds[2 * axis] = coordinates[0][axis];
            bounds[2 * axis + 1] = coordinates[1][axis];
            if !preserve_orientation && bounds[2 * axis] > bounds[2 * axis + 1] {
                bounds.swap(2 * axis, 2 * axis + 1);
            }
        }
        Ok(bounds)
    }

    fn contains(
        &self,
        coordinates: [f64; 3],
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
        tolerance: f64,
    ) -> Result<bool, ParameterError> {
        let [[start_x, start_y, start_z], [stop_x, stop_y, stop_z]] =
            self.evaluated_coordinates(parameters)?;
        Ok(coordinate_in_range(
            coordinates,
            [start_x, start_y, start_z],
            [stop_x, stop_y, stop_z],
            coordinate_system,
            tolerance,
        ))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointPrimitive {
    coordinates: [ParameterScalar; 3],
}

impl PointPrimitive {
    pub fn new(coordinates: [f64; 3]) -> Self {
        Self {
            coordinates: coordinates.map(ParameterScalar::new),
        }
    }

    pub fn from_expressions(coordinates: [&str; 3]) -> Result<Self, ParameterError> {
        Ok(Self {
            coordinates: coordinates
                .into_iter()
                .map(ParameterScalar::from_value_string)
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .expect("three point coordinates"),
        })
    }

    pub fn set_coordinate(&mut self, axis: usize, value: f64) -> bool {
        let Some(coordinate) = self.coordinates.get_mut(axis) else {
            return false;
        };
        coordinate.set_value(value);
        true
    }

    pub fn set_coordinate_expression(
        &mut self,
        axis: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(coordinate) = self.coordinates.get_mut(axis) else {
            return Ok(false);
        };
        coordinate.set_value_string(expression)?;
        Ok(true)
    }

    /// Position of one coordinate axis (native `GetCoord(axis)`).
    pub fn coordinate(
        &self,
        axis: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        self.coordinates
            .get(axis)
            .map(|coordinate| coordinate.evaluate(parameters))
            .transpose()
    }

    /// Evaluate the point coordinates (renderer / exporter accessor).
    pub fn evaluated_coordinates(
        &self,
        parameters: Option<&ParameterSet>,
    ) -> Result<[f64; 3], ParameterError> {
        Ok([
            self.coordinates[0].evaluate(parameters)?,
            self.coordinates[1].evaluate(parameters)?,
            self.coordinates[2].evaluate(parameters)?,
        ])
    }

    pub(crate) fn coordinate_xml_term(&self, axis: usize) -> Option<String> {
        self.coordinates
            .get(axis)
            .map(ParameterScalar::value_string)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpherePrimitive {
    center: [ParameterScalar; 3],
    radius: ParameterScalar,
}

impl SpherePrimitive {
    pub fn new(center: [f64; 3], radius: f64) -> Self {
        Self {
            center: center.map(ParameterScalar::new),
            radius: ParameterScalar::new(radius),
        }
    }

    pub fn from_expressions(center: [&str; 3], radius: &str) -> Result<Self, ParameterError> {
        Ok(Self {
            center: center
                .into_iter()
                .map(ParameterScalar::from_value_string)
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .expect("three center coordinates"),
            radius: ParameterScalar::from_value_string(radius)?,
        })
    }

    pub fn set_center_coordinate(&mut self, axis: usize, value: f64) -> bool {
        let Some(coordinate) = self.center.get_mut(axis) else {
            return false;
        };
        coordinate.set_value(value);
        true
    }

    pub fn center_coordinate(
        &self,
        axis: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        self.center
            .get(axis)
            .map(|coordinate| coordinate.evaluate(parameters))
            .transpose()
    }

    pub fn center_expression(&self, axis: usize) -> Option<&str> {
        self.center.get(axis).and_then(ParameterScalar::expression)
    }

    pub(crate) fn center_xml_term(&self, axis: usize) -> Option<String> {
        self.center.get(axis).map(ParameterScalar::value_string)
    }

    pub fn set_center_expression(
        &mut self,
        axis: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(coordinate) = self.center.get_mut(axis) else {
            return Ok(false);
        };
        coordinate.set_value_string(expression)?;
        Ok(true)
    }

    pub fn set_radius(&mut self, radius: f64) {
        self.radius.set_value(radius);
    }

    pub fn radius_expression(&self) -> Option<&str> {
        self.radius.expression()
    }

    pub(crate) fn radius_xml_term(&self) -> String {
        self.radius.value_string()
    }

    pub fn set_radius_expression(&mut self, expression: &str) -> Result<(), ParameterError> {
        self.radius.set_value_string(expression)
    }

    /// Shift the centre by `delta` (metres).
    pub fn translate(&mut self, delta: [f64; 3]) -> Result<(), ParameterError> {
        let center = self.center(None)?;
        for axis in 0..3 {
            self.set_center_coordinate(axis, center[axis] + delta[axis]);
        }
        Ok(())
    }

    pub fn center(&self, parameters: Option<&ParameterSet>) -> Result<[f64; 3], ParameterError> {
        Ok([
            self.center[0].evaluate(parameters)?,
            self.center[1].evaluate(parameters)?,
            self.center[2].evaluate(parameters)?,
        ])
    }

    pub fn radius(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.radius.evaluate(parameters)
    }

    pub fn bounding_box(
        &self,
        parameters: Option<&ParameterSet>,
    ) -> Result<[f64; 6], ParameterError> {
        self.bounding_box_in_coordinate_system(parameters, CoordinateSystem::Cartesian)
    }

    fn bounding_box_in_coordinate_system(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Result<[f64; 6], ParameterError> {
        let center = transform_coordinates(
            self.center(parameters)?,
            coordinate_system,
            CoordinateSystem::Cartesian,
        );
        let radius = self.radius(parameters)?;
        Ok([
            center[0] - radius,
            center[0] + radius,
            center[1] - radius,
            center[1] + radius,
            center[2] - radius,
            center[2] + radius,
        ])
    }

    pub fn dimension(&self, parameters: Option<&ParameterSet>) -> Result<i32, ParameterError> {
        Ok(if self.radius(parameters)? > 0.0 { 3 } else { 0 })
    }

    fn contains(
        &self,
        coordinates: [f64; 3],
        center_coordinate_system: CoordinateSystem,
        parameters: Option<&ParameterSet>,
        tolerance: f64,
    ) -> Result<bool, ParameterError> {
        let center = transform_coordinates(
            self.center(parameters)?,
            center_coordinate_system,
            CoordinateSystem::Cartesian,
        );
        let radius = self.radius(parameters)?;
        let distance = (0..3)
            .map(|axis| (coordinates[axis] - center[axis]).powi(2))
            .sum::<f64>()
            .sqrt();
        Ok(distance <= radius + tolerance)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SphericalShellPrimitive {
    sphere: SpherePrimitive,
    shell_width: ParameterScalar,
}

impl SphericalShellPrimitive {
    pub fn new(center: [f64; 3], radius: f64, shell_width: f64) -> Self {
        Self {
            sphere: SpherePrimitive::new(center, radius),
            shell_width: ParameterScalar::new(shell_width),
        }
    }

    pub fn from_expressions(
        center: [&str; 3],
        radius: &str,
        shell_width: &str,
    ) -> Result<Self, ParameterError> {
        Ok(Self {
            sphere: SpherePrimitive::from_expressions(center, radius)?,
            shell_width: ParameterScalar::from_value_string(shell_width)?,
        })
    }

    pub fn center(&self, parameters: Option<&ParameterSet>) -> Result<[f64; 3], ParameterError> {
        self.sphere.center(parameters)
    }

    pub fn set_center_coordinate(&mut self, axis: usize, value: f64) -> bool {
        self.sphere.set_center_coordinate(axis, value)
    }

    pub fn set_center_expression(
        &mut self,
        axis: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        self.sphere.set_center_expression(axis, expression)
    }

    pub fn radius(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.sphere.radius(parameters)
    }

    pub fn set_radius(&mut self, radius: f64) {
        self.sphere.set_radius(radius);
    }

    pub fn set_radius_expression(&mut self, expression: &str) -> Result<(), ParameterError> {
        self.sphere.set_radius_expression(expression)
    }

    pub fn shell_width(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.shell_width.evaluate(parameters)
    }

    pub fn set_shell_width(&mut self, shell_width: f64) {
        self.shell_width.set_value(shell_width);
    }

    pub fn set_shell_width_expression(&mut self, expression: &str) -> Result<(), ParameterError> {
        self.shell_width.set_value_string(expression)
    }

    pub fn shell_width_expression(&self) -> Option<&str> {
        self.shell_width.expression()
    }

    pub(crate) fn center_xml_term(&self, axis: usize) -> Option<String> {
        self.sphere.center_xml_term(axis)
    }

    pub(crate) fn radius_xml_term(&self) -> String {
        self.sphere.radius_xml_term()
    }

    pub(crate) fn shell_width_xml_term(&self) -> String {
        self.shell_width.value_string()
    }

    pub fn bounding_box(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Result<[f64; 6], ParameterError> {
        let center = transform_coordinates(
            self.sphere.center(parameters)?,
            coordinate_system,
            CoordinateSystem::Cartesian,
        );
        let radius = self.sphere.radius(parameters)? + self.shell_width(parameters)? / 2.0;
        Ok([
            center[0] - radius,
            center[0] + radius,
            center[1] - radius,
            center[1] + radius,
            center[2] - radius,
            center[2] + radius,
        ])
    }

    pub fn dimension(&self, parameters: Option<&ParameterSet>) -> Result<i32, ParameterError> {
        let shell_width = self.shell_width(parameters)?;
        let radius = self.sphere.radius(parameters)?;
        Ok(if shell_width > 0.0 {
            3
        } else if radius > 0.0 {
            1
        } else {
            0
        })
    }

    fn contains(
        &self,
        coordinates: [f64; 3],
        center_coordinate_system: CoordinateSystem,
        parameters: Option<&ParameterSet>,
        tolerance: f64,
    ) -> Result<bool, ParameterError> {
        let center = transform_coordinates(
            self.sphere.center(parameters)?,
            center_coordinate_system,
            CoordinateSystem::Cartesian,
        );
        let distance = (0..3)
            .map(|axis| (coordinates[axis] - center[axis]).powi(2))
            .sum::<f64>()
            .sqrt();
        Ok((distance - self.sphere.radius(parameters)?).abs()
            <= self.shell_width(parameters)? / 2.0 + tolerance)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CylinderPrimitive {
    axis: [[ParameterScalar; 3]; 2],
    radius: ParameterScalar,
    shell_width: Option<ParameterScalar>,
}

impl CylinderPrimitive {
    pub fn new(start: [f64; 3], stop: [f64; 3], radius: f64) -> Self {
        Self {
            axis: [
                start.map(ParameterScalar::new),
                stop.map(ParameterScalar::new),
            ],
            radius: ParameterScalar::new(radius),
            shell_width: None,
        }
    }

    pub fn from_expressions(
        start: [&str; 3],
        stop: [&str; 3],
        radius: &str,
    ) -> Result<Self, ParameterError> {
        Ok(Self {
            axis: [
                start
                    .into_iter()
                    .map(ParameterScalar::from_value_string)
                    .collect::<Result<Vec<_>, _>>()?
                    .try_into()
                    .expect("three start coordinates"),
                stop.into_iter()
                    .map(ParameterScalar::from_value_string)
                    .collect::<Result<Vec<_>, _>>()?
                    .try_into()
                    .expect("three stop coordinates"),
            ],
            radius: ParameterScalar::from_value_string(radius)?,
            shell_width: None,
        })
    }

    fn with_shell_width(mut self, shell_width: ParameterScalar) -> Self {
        self.shell_width = Some(shell_width);
        self
    }

    fn is_shell(&self) -> bool {
        self.shell_width.is_some()
    }

    pub fn shell_width(
        &self,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        self.shell_width
            .as_ref()
            .map(|width| width.evaluate(parameters))
            .transpose()
    }

    pub fn set_shell_width(&mut self, shell_width: f64) -> bool {
        if let Some(width) = &mut self.shell_width {
            width.set_value(shell_width);
            true
        } else {
            false
        }
    }

    pub fn set_shell_width_expression(&mut self, expression: &str) -> Result<bool, ParameterError> {
        if let Some(width) = &mut self.shell_width {
            width.set_value_string(expression)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn shell_width_xml_term(&self) -> Option<String> {
        self.shell_width.as_ref().map(ParameterScalar::value_string)
    }

    pub fn set_coordinate(&mut self, index: usize, value: f64) -> bool {
        if index >= 6 {
            return false;
        }
        self.axis[index % 2][index / 2].set_value(value);
        true
    }

    pub fn set_coordinate_expression(
        &mut self,
        index: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        if index >= 6 {
            return Ok(false);
        }
        self.axis[index % 2][index / 2].set_value_string(expression)?;
        Ok(true)
    }

    /// Axis coordinate by native `GetCoord(index)` ordering (start 0..2, stop 3..5).
    pub fn coordinate(
        &self,
        index: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        if index >= 6 {
            return Ok(None);
        }
        self.axis[index % 2][index / 2]
            .evaluate(parameters)
            .map(Some)
    }

    pub(crate) fn coordinate_xml_term(&self, index: usize) -> Option<String> {
        if index >= 6 {
            return None;
        }
        Some(self.axis[index % 2][index / 2].value_string())
    }

    pub fn set_radius(&mut self, radius: f64) {
        self.radius.set_value(radius);
    }

    pub fn set_radius_expression(&mut self, expression: &str) -> Result<(), ParameterError> {
        self.radius.set_value_string(expression)
    }

    pub(crate) fn radius_xml_term(&self) -> String {
        self.radius.value_string()
    }

    /// Evaluate the cylinder axis endpoints in Cartesian coordinates.
    pub fn axis_coordinates(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Result<[[f64; 3]; 2], ParameterError> {
        let mut axis = [[0.0; 3]; 2];
        for (point_index, point) in self.axis.iter().enumerate() {
            let coordinates = [
                point[0].evaluate(parameters)?,
                point[1].evaluate(parameters)?,
                point[2].evaluate(parameters)?,
            ];
            axis[point_index] =
                transform_coordinates(coordinates, coordinate_system, CoordinateSystem::Cartesian);
        }
        Ok(axis)
    }

    pub fn radius(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.radius.evaluate(parameters)
    }

    fn bounding_box(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Result<[f64; 6], ParameterError> {
        let [[start_x, start_y, start_z], [stop_x, stop_y, stop_z]] =
            self.axis_coordinates(parameters, coordinate_system)?;
        let start = [start_x, start_y, start_z];
        let stop = [stop_x, stop_y, stop_z];
        let radius = self.radius(parameters)? + self.shell_width(parameters)?.unwrap_or(0.0) / 2.0;
        let direction = [stop[0] - start[0], stop[1] - start[1], stop[2] - start[2]];
        let length = direction
            .iter()
            .map(|value| value.powi(2))
            .sum::<f64>()
            .sqrt();
        if length == 0.0 {
            return Ok([start[0], start[0], start[1], start[1], start[2], start[2]]);
        }
        let mut bounds = [0.0; 6];
        for axis in 0..3 {
            let radial_extent = radius * (1.0 - (direction[axis] / length).powi(2)).max(0.0).sqrt();
            bounds[2 * axis] = start[axis].min(stop[axis]) - radial_extent;
            bounds[2 * axis + 1] = start[axis].max(stop[axis]) + radial_extent;
        }
        Ok(bounds)
    }

    fn dimension(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Result<i32, ParameterError> {
        let bounding_radius =
            self.radius(parameters)? + self.shell_width(parameters)?.unwrap_or(0.0) / 2.0;
        if bounding_radius > 0.0 {
            return Ok(3);
        }
        let [[start_x, start_y, start_z], [stop_x, stop_y, stop_z]] =
            self.axis_coordinates(parameters, coordinate_system)?;
        Ok(if [start_x, start_y, start_z] == [stop_x, stop_y, stop_z] {
            0
        } else {
            1
        })
    }

    fn contains(
        &self,
        coordinates: [f64; 3],
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
        tolerance: f64,
    ) -> Result<bool, ParameterError> {
        let [[start_x, start_y, start_z], [stop_x, stop_y, stop_z]] =
            self.axis_coordinates(parameters, coordinate_system)?;
        let start = [start_x, start_y, start_z];
        let stop = [stop_x, stop_y, stop_z];
        let axis = [stop[0] - start[0], stop[1] - start[1], stop[2] - start[2]];
        let length_squared = axis.iter().map(|value| value.powi(2)).sum::<f64>();
        if length_squared == 0.0 {
            // Native CSPrimCylinder's degenerate bounds collapse to the axis point.
            return Ok((0..3)
                .map(|axis| (coordinates[axis] - start[axis]).powi(2))
                .sum::<f64>()
                .sqrt()
                <= tolerance);
        }
        let offset = [
            coordinates[0] - start[0],
            coordinates[1] - start[1],
            coordinates[2] - start[2],
        ];
        let foot = (0..3).map(|index| offset[index] * axis[index]).sum::<f64>() / length_squared;
        // The tolerance along the axis, in parts of its length.
        let length = length_squared.sqrt();
        let foot_tolerance = if length > 0.0 {
            tolerance / length
        } else {
            0.0
        };
        if foot < -foot_tolerance || foot > 1.0 + foot_tolerance {
            return Ok(false);
        }
        let distance_squared = (0..3)
            .map(|index| (offset[index] - foot * axis[index]).powi(2))
            .sum::<f64>();
        let distance = distance_squared.sqrt();
        let radius = self.radius(parameters)?;
        if let Some(shell_width) = self.shell_width(parameters)? {
            Ok((distance - radius).abs() <= shell_width / 2.0 + tolerance)
        } else {
            Ok(distance <= radius + tolerance)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurvePrimitive {
    points: Vec<[ParameterScalar; 3]>,
    wire_radius: Option<ParameterScalar>,
}

impl CurvePrimitive {
    fn new(wire_radius: Option<f64>) -> Self {
        Self {
            points: Vec::new(),
            wire_radius: wire_radius.map(ParameterScalar::new),
        }
    }

    fn from_expressions(wire_radius: Option<&str>) -> Result<Self, ParameterError> {
        Ok(Self {
            points: Vec::new(),
            wire_radius: wire_radius
                .map(ParameterScalar::from_value_string)
                .transpose()?,
        })
    }

    pub fn add_point(&mut self, coordinates: [f64; 3]) -> usize {
        self.points.push(coordinates.map(ParameterScalar::new));
        self.points.len()
    }

    pub fn add_point_from_expressions(
        &mut self,
        coordinates: [&str; 3],
    ) -> Result<usize, ParameterError> {
        self.points.push([
            ParameterScalar::from_value_string(coordinates[0])?,
            ParameterScalar::from_value_string(coordinates[1])?,
            ParameterScalar::from_value_string(coordinates[2])?,
        ]);
        Ok(self.points.len())
    }

    pub fn clear_points(&mut self) {
        self.points.clear();
    }

    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    /// Evaluate every curve point (renderer / exporter accessor).
    pub fn evaluated_points(
        &self,
        parameters: Option<&ParameterSet>,
    ) -> Result<Vec<[f64; 3]>, ParameterError> {
        self.points
            .iter()
            .map(|point| {
                Ok([
                    point[0].evaluate(parameters)?,
                    point[1].evaluate(parameters)?,
                    point[2].evaluate(parameters)?,
                ])
            })
            .collect()
    }

    pub fn set_coordinate(&mut self, point: usize, axis: usize, value: f64) -> bool {
        let Some(coordinate) = self
            .points
            .get_mut(point)
            .and_then(|point| point.get_mut(axis))
        else {
            return false;
        };
        coordinate.set_value(value);
        true
    }

    pub fn set_coordinate_expression(
        &mut self,
        point: usize,
        axis: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(coordinate) = self
            .points
            .get_mut(point)
            .and_then(|point| point.get_mut(axis))
        else {
            return Ok(false);
        };
        coordinate.set_value_string(expression)?;
        Ok(true)
    }

    pub fn set_wire_radius(&mut self, radius: f64) -> bool {
        if let Some(wire_radius) = &mut self.wire_radius {
            wire_radius.set_value(radius);
            true
        } else {
            false
        }
    }

    pub fn set_wire_radius_expression(&mut self, expression: &str) -> Result<bool, ParameterError> {
        if let Some(wire_radius) = &mut self.wire_radius {
            wire_radius.set_value_string(expression)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn wire_radius(
        &self,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        self.wire_radius
            .as_ref()
            .map(|radius| radius.evaluate(parameters))
            .transpose()
    }

    pub(crate) fn wire_radius_xml_term(&self) -> Option<String> {
        self.wire_radius.as_ref().map(ParameterScalar::value_string)
    }

    /// Evaluated point of the curve (native `GetPoint(idx)`).
    pub fn point(
        &self,
        point: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<[f64; 3]>, ParameterError> {
        let Some(coordinates) = self.points.get(point) else {
            return Ok(None);
        };
        Ok(Some([
            coordinates[0].evaluate(parameters)?,
            coordinates[1].evaluate(parameters)?,
            coordinates[2].evaluate(parameters)?,
        ]))
    }

    pub(crate) fn point_xml_terms(&self, point: usize) -> Option<[String; 3]> {
        self.points
            .get(point)
            .map(|point| point.clone().map(|coordinate| coordinate.value_string()))
    }

    fn evaluate_points(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Result<Vec<[f64; 3]>, ParameterError> {
        self.points
            .iter()
            .map(|point| {
                let coordinates = [
                    point[0].evaluate(parameters)?,
                    point[1].evaluate(parameters)?,
                    point[2].evaluate(parameters)?,
                ];
                Ok(transform_coordinates(
                    coordinates,
                    coordinate_system,
                    CoordinateSystem::Cartesian,
                ))
            })
            .collect()
    }

    fn bounding_box(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Result<[f64; 6], ParameterError> {
        let points = self.evaluate_points(parameters, coordinate_system)?;
        let mut bounds = [0.0; 6];
        if let Some(first) = points.first() {
            for axis in 0..3 {
                bounds[2 * axis] = first[axis];
                bounds[2 * axis + 1] = first[axis];
            }
            for point in points.iter().skip(1) {
                for axis in 0..3 {
                    bounds[2 * axis] = bounds[2 * axis].min(point[axis]);
                    bounds[2 * axis + 1] = bounds[2 * axis + 1].max(point[axis]);
                }
            }
        }
        if let Some(radius) = self.wire_radius(parameters)? {
            for axis in 0..3 {
                bounds[2 * axis] -= radius;
                bounds[2 * axis + 1] += radius;
            }
        }
        Ok(bounds)
    }

    fn dimension(&self, parameters: Option<&ParameterSet>) -> Result<i32, ParameterError> {
        let dimension = if self.points.len() <= 1 { 0 } else { 1 };
        Ok(dimension
            + if self.wire_radius(parameters)?.unwrap_or(0.0) > 0.0 {
                2
            } else {
                0
            })
    }

    fn contains(
        &self,
        coordinates: [f64; 3],
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
        tolerance: f64,
    ) -> Result<bool, ParameterError> {
        let Some(radius) = self.wire_radius(parameters)? else {
            return Ok(false);
        };
        let radius = radius + tolerance;
        let points = self.evaluate_points(parameters, coordinate_system)?;
        for (index, point) in points.iter().enumerate() {
            let distance = (0..3)
                .map(|axis| (coordinates[axis] - point[axis]).powi(2))
                .sum::<f64>()
                .sqrt();
            if distance <= radius {
                return Ok(true);
            }
            if let Some(next) = points.get(index + 1) {
                let segment = [next[0] - point[0], next[1] - point[1], next[2] - point[2]];
                let offset = [
                    coordinates[0] - point[0],
                    coordinates[1] - point[1],
                    coordinates[2] - point[2],
                ];
                let length_squared = segment.iter().map(|value| value.powi(2)).sum::<f64>();
                if length_squared == 0.0 {
                    continue;
                }
                let foot = (0..3)
                    .map(|component| offset[component] * segment[component])
                    .sum::<f64>()
                    / length_squared;
                if foot > 0.0 && foot < 1.0 {
                    let distance = (0..3)
                        .map(|component| (offset[component] - foot * segment[component]).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    if distance < radius {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolygonPrimitive {
    vertices: Vec<[ParameterScalar; 2]>,
    normal_direction: usize,
    elevation: ParameterScalar,
    extrusion_length: Option<ParameterScalar>,
    rotation_axis_direction: Option<usize>,
    rotation_angles: Option<[ParameterScalar; 2]>,
}

impl PolygonPrimitive {
    fn new(elevation: f64) -> Self {
        Self {
            vertices: Vec::new(),
            normal_direction: 0,
            elevation: ParameterScalar::new(elevation),
            extrusion_length: None,
            rotation_axis_direction: None,
            rotation_angles: None,
        }
    }

    fn from_expressions(elevation: &str) -> Result<Self, ParameterError> {
        Ok(Self {
            vertices: Vec::new(),
            normal_direction: 0,
            elevation: ParameterScalar::from_value_string(elevation)?,
            extrusion_length: None,
            rotation_axis_direction: None,
            rotation_angles: None,
        })
    }

    fn with_extrusion_length(mut self, length: ParameterScalar) -> Self {
        self.extrusion_length = Some(length);
        self
    }

    fn is_linear_extrusion(&self) -> bool {
        self.extrusion_length.is_some()
    }

    fn with_rotation(
        mut self,
        axis_direction: usize,
        start_angle: ParameterScalar,
        stop_angle: ParameterScalar,
    ) -> Self {
        self.rotation_axis_direction = Some(axis_direction);
        self.rotation_angles = Some([start_angle, stop_angle]);
        self
    }

    fn is_rotational(&self) -> bool {
        self.rotation_axis_direction.is_some()
    }

    pub fn add_vertex(&mut self, coordinates: [f64; 2]) -> usize {
        self.vertices.push(coordinates.map(ParameterScalar::new));
        self.vertices.len()
    }

    pub fn add_vertex_from_expressions(
        &mut self,
        coordinates: [&str; 2],
    ) -> Result<usize, ParameterError> {
        self.vertices.push([
            ParameterScalar::from_value_string(coordinates[0])?,
            ParameterScalar::from_value_string(coordinates[1])?,
        ]);
        Ok(self.vertices.len())
    }

    pub fn remove_vertex(&mut self, index: usize) -> bool {
        if index >= self.vertices.len() {
            return false;
        }
        self.vertices.remove(index);
        true
    }

    pub fn clear_vertices(&mut self) {
        self.vertices.clear();
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn set_coordinate(&mut self, vertex: usize, axis: usize, value: f64) -> bool {
        let Some(coordinate) = self
            .vertices
            .get_mut(vertex)
            .and_then(|vertex| vertex.get_mut(axis))
        else {
            return false;
        };
        coordinate.set_value(value);
        true
    }

    pub fn set_coordinate_expression(
        &mut self,
        vertex: usize,
        axis: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(coordinate) = self
            .vertices
            .get_mut(vertex)
            .and_then(|vertex| vertex.get_mut(axis))
        else {
            return Ok(false);
        };
        coordinate.set_value_string(expression)?;
        Ok(true)
    }

    pub fn normal_direction(&self) -> usize {
        self.normal_direction
    }

    pub fn set_normal_direction(&mut self, direction: usize) -> bool {
        if direction >= 3 {
            return false;
        }
        self.normal_direction = direction;
        true
    }

    pub fn rotation_axis_direction(&self) -> Option<usize> {
        self.rotation_axis_direction
    }

    pub fn set_rotation_axis_direction(&mut self, direction: usize) -> bool {
        if direction >= 3 || self.rotation_axis_direction.is_none() {
            return false;
        }
        self.rotation_axis_direction = Some(direction);
        true
    }

    pub fn rotation_angles(
        &self,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<[f64; 2]>, ParameterError> {
        self.rotation_angles
            .as_ref()
            .map(|angles| {
                Ok([
                    angles[0].evaluate(parameters)?,
                    angles[1].evaluate(parameters)?,
                ])
            })
            .transpose()
    }

    pub fn set_rotation_angle_expression(
        &mut self,
        index: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(angle) = self
            .rotation_angles
            .as_mut()
            .and_then(|angles| angles.get_mut(index))
        else {
            return Ok(false);
        };
        angle.set_value_string(expression)?;
        Ok(true)
    }

    pub fn elevation(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.elevation.evaluate(parameters)
    }

    pub fn set_elevation(&mut self, elevation: f64) {
        self.elevation.set_value(elevation);
    }

    pub fn set_elevation_expression(&mut self, expression: &str) -> Result<(), ParameterError> {
        self.elevation.set_value_string(expression)
    }

    pub fn extrusion_length(
        &self,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        self.extrusion_length
            .as_ref()
            .map(|length| length.evaluate(parameters))
            .transpose()
    }

    pub fn set_extrusion_length(&mut self, length: f64) -> bool {
        if let Some(extrusion_length) = &mut self.extrusion_length {
            extrusion_length.set_value(length);
            true
        } else {
            false
        }
    }

    pub fn set_extrusion_length_expression(
        &mut self,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        if let Some(extrusion_length) = &mut self.extrusion_length {
            extrusion_length.set_value_string(expression)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn elevation_xml_term(&self) -> String {
        self.elevation.value_string()
    }

    pub(crate) fn extrusion_length_xml_term(&self) -> Option<String> {
        self.extrusion_length
            .as_ref()
            .map(ParameterScalar::value_string)
    }

    pub(crate) fn rotation_angles_xml_terms(&self) -> Option<[String; 2]> {
        self.rotation_angles
            .as_ref()
            .map(|angles| angles.clone().map(|angle| angle.value_string()))
    }

    pub(crate) fn vertex_xml_terms(&self, vertex: usize) -> Option<[String; 2]> {
        self.vertices
            .get(vertex)
            .map(|vertex| vertex.clone().map(|coordinate| coordinate.value_string()))
    }

    /// Evaluated polygon vertex (native `GetCoord(2*n)`/`GetCoord(2*n+1)`).
    pub fn vertex(
        &self,
        vertex: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<[f64; 2]>, ParameterError> {
        let Some(coordinates) = self.vertices.get(vertex) else {
            return Ok(None);
        };
        Ok(Some([
            coordinates[0].evaluate(parameters)?,
            coordinates[1].evaluate(parameters)?,
        ]))
    }

    /// Evaluate the two-dimensional polygon profile vertices (renderer /
    /// exporter accessor).
    pub fn evaluated_vertices(
        &self,
        parameters: Option<&ParameterSet>,
    ) -> Result<Vec<[f64; 2]>, ParameterError> {
        self.vertices
            .iter()
            .map(|vertex| {
                Ok([
                    vertex[0].evaluate(parameters)?,
                    vertex[1].evaluate(parameters)?,
                ])
            })
            .collect()
    }

    fn bounding_box(&self, parameters: Option<&ParameterSet>) -> Result<[f64; 6], ParameterError> {
        let vertices = self.evaluated_vertices(parameters)?;
        let elevation = self.elevation(parameters)?;
        if vertices.is_empty() && self.extrusion_length.is_none() {
            return Ok([0.0; 6]);
        }
        let extrusion = self.extrusion_length(parameters)?.unwrap_or(0.0);
        let mut bounds = [0.0; 6];
        if extrusion > 0.0 {
            bounds[2 * self.normal_direction] = elevation;
            bounds[2 * self.normal_direction + 1] = elevation + extrusion;
        } else {
            bounds[2 * self.normal_direction] = elevation + extrusion;
            bounds[2 * self.normal_direction + 1] = elevation;
        }
        if vertices.is_empty() {
            return Ok(bounds);
        }
        let mut min = vertices[0];
        let mut max = vertices[0];
        for vertex in vertices.iter().skip(1) {
            for axis in 0..2 {
                min[axis] = min[axis].min(vertex[axis]);
                max[axis] = max[axis].max(vertex[axis]);
            }
        }
        let first_axis = (self.normal_direction + 1) % 3;
        let second_axis = (self.normal_direction + 2) % 3;
        bounds[2 * first_axis] = min[0];
        bounds[2 * first_axis + 1] = max[0];
        bounds[2 * second_axis] = min[1];
        bounds[2 * second_axis + 1] = max[1];
        Ok(bounds)
    }

    fn dimension(&self, parameters: Option<&ParameterSet>) -> Result<i32, ParameterError> {
        let bounds = self.bounding_box(parameters)?;
        Ok((0..3)
            .filter(|axis| bounds[2 * axis] != bounds[2 * axis + 1])
            .count() as i32)
    }

    fn contains(
        &self,
        coordinates: [f64; 3],
        parameters: Option<&ParameterSet>,
        tolerance: f64,
    ) -> Result<bool, ParameterError> {
        let bounds = self.bounding_box(parameters)?;
        if self.vertices.len() < 2
            || (0..3).any(|axis| {
                coordinates[axis] < bounds[2 * axis] - tolerance
                    || coordinates[axis] > bounds[2 * axis + 1] + tolerance
            })
        {
            return Ok(false);
        }
        let vertices = self.evaluated_vertices(parameters)?;
        let first_axis = (self.normal_direction + 1) % 3;
        let second_axis = (self.normal_direction + 2) % 3;
        let (x, y) = (coordinates[first_axis], coordinates[second_axis]);
        // Native: an edge or corner counts as inside, and so does a point
        // within the drawing tolerance of it.
        if tolerance > 0.0 {
            let mut start = *vertices.last().expect("at least two vertices");
            for end in &vertices {
                let edge = [end[0] - start[0], end[1] - start[1]];
                let length_squared = edge[0].powi(2) + edge[1].powi(2);
                let foot = if length_squared > 0.0 {
                    (((x - start[0]) * edge[0] + (y - start[1]) * edge[1]) / length_squared)
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let offset = [start[0] + foot * edge[0] - x, start[1] + foot * edge[1] - y];
                if offset[0].powi(2) + offset[1].powi(2) <= tolerance.powi(2) {
                    return Ok(true);
                }
                start = *end;
            }
        }
        let mut winding_number = 0;
        let mut previous = *vertices.last().expect("at least two vertices");
        let mut start_over = previous[1] >= y;
        for current in vertices {
            if (current[0] == previous[0]
                && previous[0] == x
                && ((y < previous[1] && y > current[1]) || (y > previous[1] && y < current[1])))
                || (current[1] == previous[1]
                    && previous[1] == y
                    && ((x < previous[0] && x > current[0]) || (x > previous[0] && x < current[0])))
            {
                return Ok(true);
            }
            let end_over = current[1] >= y;
            if start_over != end_over {
                if (current[1] - y) * (current[0] - previous[0])
                    <= (current[1] - previous[1]) * (current[0] - x)
                {
                    if end_over {
                        winding_number += 1;
                    }
                } else if !end_over {
                    winding_number -= 1;
                }
            }
            start_over = end_over;
            previous = current;
        }
        Ok(winding_number != 0)
    }

    fn contains_rotational(
        &self,
        coordinates: [f64; 3],
        parameters: Option<&ParameterSet>,
        tolerance: f64,
    ) -> Result<bool, ParameterError> {
        let axis_direction = self
            .rotation_axis_direction
            .expect("rotational polygon has an axis");
        let [mut start_angle, mut stop_angle] = self
            .rotation_angles(parameters)?
            .expect("rotational polygon has angles");
        if start_angle > stop_angle {
            stop_angle += 2.0 * std::f64::consts::PI;
        }
        if start_angle > 2.0 * std::f64::consts::PI {
            start_angle -= 2.0 * std::f64::consts::PI;
            stop_angle -= 2.0 * std::f64::consts::PI;
        }
        if start_angle < 0.0 {
            start_angle += 2.0 * std::f64::consts::PI;
            stop_angle += 2.0 * std::f64::consts::PI;
        }
        let first_radial_axis = (axis_direction + 1) % 3;
        let second_radial_axis = (axis_direction + 2) % 3;
        let mut angle = coordinates[second_radial_axis].atan2(coordinates[first_radial_axis]);
        if first_radial_axis == self.normal_direction {
            angle -= std::f64::consts::FRAC_PI_2;
        }
        if angle < 0.0 {
            angle += 2.0 * std::f64::consts::PI;
        }
        let mut radial_distance = (coordinates[first_radial_axis].powi(2)
            + coordinates[second_radial_axis].powi(2))
        .sqrt();
        let mut cross_section_point = [radial_distance; 3];
        cross_section_point[self.normal_direction] = 0.0;
        cross_section_point[axis_direction] = coordinates[axis_direction];
        // The angle the tolerance covers at this distance from the axis.
        let angle_tolerance = if radial_distance.abs() > tolerance {
            tolerance / radial_distance.abs()
        } else {
            std::f64::consts::PI
        };
        let low = start_angle - angle_tolerance;
        let in_range = |mut value: f64| {
            while value < low {
                value += 2.0 * std::f64::consts::PI;
            }
            while value >= low + 2.0 * std::f64::consts::PI {
                value -= 2.0 * std::f64::consts::PI;
            }
            value <= stop_angle + angle_tolerance
        };
        if angle < low {
            angle += 2.0 * std::f64::consts::PI;
        }
        if in_range(angle) && self.contains(cross_section_point, parameters, tolerance)? {
            return Ok(true);
        }
        radial_distance *= -1.0;
        angle += std::f64::consts::PI;
        if angle > 2.0 * std::f64::consts::PI {
            angle -= 2.0 * std::f64::consts::PI;
        }
        if angle < low {
            angle += 2.0 * std::f64::consts::PI;
        }
        if !in_range(angle) {
            return Ok(false);
        }
        cross_section_point[first_radial_axis] = radial_distance;
        cross_section_point[second_radial_axis] = radial_distance;
        cross_section_point[axis_direction] = coordinates[axis_direction];
        self.contains(cross_section_point, parameters, tolerance)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum UserDefinedCoordinateSystem {
    #[default]
    Cartesian = 0,
    Cylindrical = 1,
    Spherical = 2,
}

impl UserDefinedCoordinateSystem {
    pub(crate) fn from_xml_value(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Cartesian),
            1 => Some(Self::Cylindrical),
            2 => Some(Self::Spherical),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UserDefinedPrimitive {
    function: String,
    coordinate_system: UserDefinedCoordinateSystem,
    coordinate_shift: [ParameterScalar; 3],
}

impl Default for UserDefinedPrimitive {
    fn default() -> Self {
        Self {
            function: String::new(),
            coordinate_system: UserDefinedCoordinateSystem::default(),
            coordinate_shift: std::array::from_fn(|_| ParameterScalar::new(0.0)),
        }
    }
}

impl UserDefinedPrimitive {
    pub fn function(&self) -> &str {
        &self.function
    }

    pub fn set_function(&mut self, function: impl Into<String>) {
        self.function = function.into();
    }

    pub fn coordinate_system(&self) -> UserDefinedCoordinateSystem {
        self.coordinate_system
    }

    pub fn set_coordinate_system(&mut self, coordinate_system: UserDefinedCoordinateSystem) {
        self.coordinate_system = coordinate_system;
    }

    pub fn coordinate_shift_term(&self, axis: usize) -> Option<String> {
        self.coordinate_shift
            .get(axis)
            .map(ParameterScalar::value_string)
    }

    pub fn set_coordinate_shift(&mut self, axis: usize, value: f64) -> bool {
        let Some(shift) = self.coordinate_shift.get_mut(axis) else {
            return false;
        };
        shift.set_value(value);
        true
    }

    pub fn set_coordinate_shift_expression(
        &mut self,
        axis: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(shift) = self.coordinate_shift.get_mut(axis) else {
            return Ok(false);
        };
        shift.set_value_string(expression)?;
        Ok(true)
    }

    fn validate_function(&self, parameters: Option<&ParameterSet>) -> Result<(), ParameterError> {
        let mut variable_names = parameters
            .into_iter()
            .flat_map(ParameterSet::iter)
            .map(|parameter| parameter.name().to_owned())
            .collect::<Vec<_>>();
        variable_names.extend(["x", "y", "z"].map(str::to_owned));
        match self.coordinate_system {
            UserDefinedCoordinateSystem::Cartesian => {}
            UserDefinedCoordinateSystem::Cylindrical => {
                variable_names.extend(["r", "a"].map(str::to_owned));
            }
            UserDefinedCoordinateSystem::Spherical => {
                variable_names.extend(["r", "a", "t"].map(str::to_owned));
            }
        }
        let mut parser = Parser::new();
        parser
            .parse(&self.function, &variable_names.join(","), false)
            .map_err(ParameterError::Parse)
    }

    fn validate_update(&self, parameters: Option<&ParameterSet>) -> Result<(), ParameterError> {
        self.validate_function(parameters)?;
        for shift in &self.coordinate_shift {
            shift.evaluate(parameters)?;
        }
        Ok(())
    }

    fn contains(
        &self,
        coordinates: [f64; 3],
        parameters: Option<&ParameterSet>,
    ) -> Result<bool, ParameterError> {
        let mut coordinates = coordinates;
        for (axis, shift) in self.coordinate_shift.iter().enumerate() {
            coordinates[axis] -= shift.evaluate(parameters)?;
        }

        let [x, y, z] = coordinates;
        let mut variable_names = parameters
            .into_iter()
            .flat_map(ParameterSet::iter)
            .map(|parameter| parameter.name().to_owned())
            .collect::<Vec<_>>();
        let mut values = parameters
            .into_iter()
            .flat_map(ParameterSet::iter)
            .map(|parameter| parameter.value())
            .collect::<Vec<_>>();
        variable_names.extend(["x", "y", "z"].map(str::to_owned));
        values.extend([x, y, z]);
        match self.coordinate_system {
            UserDefinedCoordinateSystem::Cartesian => {}
            UserDefinedCoordinateSystem::Cylindrical => {
                variable_names.extend(["r", "a"].map(str::to_owned));
                values.extend([(x * x + y * y).sqrt(), y.atan2(x)]);
            }
            UserDefinedCoordinateSystem::Spherical => {
                let radial_distance = (x * x + y * y).sqrt();
                variable_names.extend(["r", "a", "t"].map(str::to_owned));
                values.extend([
                    (x * x + y * y + z * z).sqrt(),
                    y.atan2(x),
                    std::f64::consts::FRAC_PI_2 - (z / radial_distance).atan(),
                ]);
            }
        }

        let mut parser = Parser::new();
        parser
            .parse(&self.function, &variable_names.join(","), false)
            .map_err(ParameterError::Parse)?;
        let value = parser.eval(&values).map_err(ParameterError::Evaluation)?;
        Ok(value == 1.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolyhedronPrimitive {
    vertices: Vec<[f64; 3]>,
    faces: Vec<Vec<i32>>,
}

impl PolyhedronPrimitive {
    pub fn add_vertex(&mut self, coordinates: [f64; 3]) -> usize {
        // Native: polyhedron vertices are double precision.
        self.vertices.push(coordinates);
        self.vertices.len() - 1
    }

    pub fn add_face(&mut self, vertices: Vec<i32>) -> usize {
        self.faces.push(vertices);
        self.faces.len() - 1
    }

    pub fn reset(&mut self) {
        self.vertices.clear();
        self.faces.clear();
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    pub fn vertex(&self, index: usize) -> Option<[f64; 3]> {
        self.vertices.get(index).copied()
    }

    pub fn face(&self, index: usize) -> Option<&[i32]> {
        self.faces.get(index).map(Vec::as_slice)
    }

    pub(crate) fn vertex_xml_text(&self, index: usize) -> Option<String> {
        self.vertices.get(index).map(|vertex| {
            vertex
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        })
    }

    pub(crate) fn face_xml_text(&self, index: usize) -> Option<String> {
        self.faces.get(index).map(|face| {
            face.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        })
    }

    fn is_valid_face(&self, face: &[i32]) -> bool {
        if face.len() < 3 {
            return false;
        }
        let mut vertex_indices = BTreeSet::new();
        face.iter().all(|index| {
            usize::try_from(*index).is_ok_and(|index| index < self.vertices.len())
                && vertex_indices.insert(*index)
        })
    }

    fn is_closed(&self) -> bool {
        if self.faces.is_empty() || self.faces.iter().any(|face| !self.is_valid_face(face)) {
            return false;
        }
        let mut edge_counts = BTreeMap::new();
        for face in &self.faces {
            for index in 0..face.len() {
                let start = face[index] as usize;
                let end = face[(index + 1) % face.len()] as usize;
                let edge = (start.min(end), start.max(end));
                *edge_counts.entry(edge).or_insert(0_u32) += 1;
            }
        }
        edge_counts.values().all(|count| *count == 2)
    }

    fn accepted_faces(&self) -> Vec<&[i32]> {
        let mut edge_counts = BTreeMap::new();
        let mut edge_directions = BTreeMap::<(usize, usize), bool>::new();
        let mut vertex_edge_counts = BTreeMap::<usize, (usize, usize)>::new();
        let mut vertex_links = BTreeMap::<usize, BTreeMap<usize, BTreeSet<usize>>>::new();
        let mut accepted = Vec::new();
        for face in &self.faces {
            if !self.is_valid_face(face) {
                continue;
            }
            let mut oriented_face = face.clone();
            if !Self::facet_orientation_is_compatible(&oriented_face, &edge_directions) {
                oriented_face.reverse();
                if !Self::facet_orientation_is_compatible(&oriented_face, &edge_directions) {
                    continue;
                }
            }
            let edges = (0..face.len())
                .map(|index| {
                    let start = face[index] as usize;
                    let end = face[(index + 1) % face.len()] as usize;
                    (start.min(end), start.max(end))
                })
                .collect::<Vec<_>>();
            if edges
                .iter()
                .any(|edge| edge_counts.get(edge).is_some_and(|count| *count >= 2))
                || face.iter().any(|index| {
                    vertex_edge_counts
                        .get(&(*index as usize))
                        .is_some_and(|(incident, closed)| incident == closed)
                })
                || (0..face.len()).any(|index| {
                    let vertex = face[index] as usize;
                    let previous = face[(index + face.len() - 1) % face.len()] as usize;
                    let next = face[(index + 1) % face.len()] as usize;
                    vertex_links.get(&vertex).is_some_and(|links| {
                        links.contains_key(&previous)
                            && links.contains_key(&next)
                            && !Self::vertex_link_is_connected(links, previous, next)
                    })
                })
            {
                continue;
            }
            for index in 0..oriented_face.len() {
                let start = oriented_face[index] as usize;
                let end = oriented_face[(index + 1) % oriented_face.len()] as usize;
                edge_directions
                    .entry((start.min(end), start.max(end)))
                    .or_insert(start < end);
            }
            for edge in edges {
                let count = edge_counts.entry(edge).or_insert(0_u32);
                if *count == 0 {
                    vertex_edge_counts.entry(edge.0).or_default().0 += 1;
                    vertex_edge_counts.entry(edge.1).or_default().0 += 1;
                }
                *count += 1;
                if *count == 2 {
                    vertex_edge_counts.entry(edge.0).or_default().1 += 1;
                    vertex_edge_counts.entry(edge.1).or_default().1 += 1;
                }
            }
            for index in 0..oriented_face.len() {
                let vertex = oriented_face[index] as usize;
                let previous =
                    oriented_face[(index + oriented_face.len() - 1) % oriented_face.len()] as usize;
                let next = oriented_face[(index + 1) % oriented_face.len()] as usize;
                let links = vertex_links.entry(vertex).or_default();
                links.entry(previous).or_default().insert(next);
                links.entry(next).or_default().insert(previous);
            }
            accepted.push(face.as_slice());
        }
        accepted
    }

    fn facet_orientation_is_compatible(
        face: &[i32],
        edge_directions: &BTreeMap<(usize, usize), bool>,
    ) -> bool {
        (0..face.len()).all(|index| {
            let start = face[index] as usize;
            let end = face[(index + 1) % face.len()] as usize;
            let edge = (start.min(end), start.max(end));
            edge_directions
                .get(&edge)
                .is_none_or(|direction| *direction != (start < end))
        })
    }

    fn vertex_link_is_connected(
        links: &BTreeMap<usize, BTreeSet<usize>>,
        start: usize,
        target: usize,
    ) -> bool {
        let mut pending = vec![start];
        let mut visited = BTreeSet::new();
        while let Some(vertex) = pending.pop() {
            if vertex == target {
                return true;
            }
            if visited.insert(vertex) {
                if let Some(neighbors) = links.get(&vertex) {
                    pending.extend(neighbors);
                }
            }
        }
        false
    }

    fn dimension(&self) -> i32 {
        if self.faces.is_empty() {
            0
        } else if self.is_closed() || self.accepted_faces().len() != self.faces.len() {
            3
        } else {
            2
        }
    }

    fn bounding_box(&self) -> [f64; 6] {
        let Some(first) = self.vertices.first() else {
            return [0.0; 6];
        };
        let mut bounds = [
            f64::from(first[0]),
            f64::from(first[0]),
            f64::from(first[1]),
            f64::from(first[1]),
            f64::from(first[2]),
            f64::from(first[2]),
        ];
        for vertex in self.vertices.iter().skip(1) {
            for axis in 0..3 {
                let coordinate = f64::from(vertex[axis]);
                bounds[2 * axis] = bounds[2 * axis].min(coordinate);
                bounds[2 * axis + 1] = bounds[2 * axis + 1].max(coordinate);
            }
        }
        bounds
    }

    fn contains(&self, coordinates: [f64; 3], tolerance: f64) -> bool {
        if self.dimension() < 3 {
            return false;
        }
        let bounds = self.bounding_box();
        if (0..3).any(|axis| {
            coordinates[axis] < bounds[2 * axis] - tolerance
                || coordinates[axis] > bounds[2 * axis + 1] + tolerance
        }) {
            return false;
        }
        // Native: a point on the surface (within the tolerance) is inside. The
        // relative epsilon also catches a point that is exactly on a face but
        // not representable in floating point, where the solid angle below is
        // only half of 4*pi.
        let diagonal = {
            let mut sum = 0.0;
            for axis in 0..3 {
                let extent = bounds[2 * axis + 1] - bounds[2 * axis];
                sum += extent * extent;
            }
            sum.sqrt()
        };
        let surface_epsilon = tolerance.max(1.0e-9 * diagonal.max(1.0));
        for face in self.accepted_faces() {
            let first = self.vertices[face[0] as usize];
            for triangle in 1..(face.len() - 1) {
                let second = self.vertices[face[triangle] as usize];
                let third = self.vertices[face[triangle + 1] as usize];
                if point_triangle_distance(coordinates, first, second, third) <= surface_epsilon {
                    return true;
                }
            }
        }
        // Solid angle: robust for points on an edge, a cavity, touching or
        // overlapping bodies; no face is dropped.
        let mut solid_angle = 0.0;
        for face in self.accepted_faces() {
            let first = self.vertices[face[0] as usize];
            for triangle in 1..(face.len() - 1) {
                let second = self.vertices[face[triangle] as usize];
                let third = self.vertices[face[triangle + 1] as usize];
                solid_angle += triangle_solid_angle(coordinates, first, second, third);
            }
        }
        solid_angle.abs() > 2.0 * std::f64::consts::PI
    }
}

/// Native `CoordInRange`: a point within `tolerance` of the range counts as
/// inside. Along a cylindrical angle the tolerance is a length too, so it is
/// divided by the radius (any angle counts close to the axis).
fn coordinate_in_range(
    coordinates: [f64; 3],
    start: [f64; 3],
    stop: [f64; 3],
    system: CoordinateSystem,
    tolerance: f64,
) -> bool {
    let mut point = coordinates;
    let mut distance = [tolerance; 3];
    if system == CoordinateSystem::Cylindrical {
        distance[1] = if point[0].abs() > tolerance {
            tolerance / point[0].abs()
        } else {
            std::f64::consts::PI
        };
        let low = start[1].min(stop[1]) - distance[1];
        let high = start[1].max(stop[1]) + distance[1];
        while point[1] < low {
            point[1] += 2.0 * std::f64::consts::PI;
        }
        while point[1] > high {
            point[1] -= 2.0 * std::f64::consts::PI;
        }
    }
    (0..3).all(|axis| {
        point[axis] >= start[axis].min(stop[axis]) - distance[axis]
            && point[axis] <= start[axis].max(stop[axis]) + distance[axis]
    })
}

fn ray_intersects_triangle(
    origin: [f64; 3],
    first: [f64; 3],
    second: [f64; 3],
    third: [f64; 3],
) -> bool {
    let direction = [1.0, 0.371_390_676_354_103_7, 0.694_278_967_891];
    let edge1 = subtract3(second, first);
    let edge2 = subtract3(third, first);
    let cross = cross3(direction, edge2);
    let determinant = dot3(edge1, cross);
    if determinant.abs() < 1e-12 {
        return false;
    }
    let inverse = 1.0 / determinant;
    let offset = subtract3(origin, first);
    let u = dot3(offset, cross) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let cross = cross3(offset, edge1);
    let v = dot3(direction, cross) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    dot3(edge2, cross) * inverse > 1e-12
}

/// Distance from `point` to the triangle `a`-`b`-`c`.
fn point_triangle_distance(point: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    // Project onto the triangle plane and clamp to its edges.
    let ab = subtract3(b, a);
    let ac = subtract3(c, a);
    let normal = cross3(ab, ac);
    let normal_length_squared = dot3(normal, normal);
    let projected = if normal_length_squared > 0.0 {
        let offset = subtract3(point, a);
        let signed = dot3(offset, normal) / normal_length_squared;
        subtract3(point, [normal[0] * signed, normal[1] * signed, normal[2] * signed])
    } else {
        point
    };
    // Inside the triangle (same side of all three edges)?
    let inside = |edge_start: [f64; 3], edge_end: [f64; 3]| {
        let edge = subtract3(edge_end, edge_start);
        let to_point = subtract3(projected, edge_start);
        dot3(cross3(edge, to_point), normal) >= 0.0
    };
    if inside(a, b) && inside(b, c) && inside(c, a) {
        return subtract3(point, projected)
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
    }
    let segment_distance = |start: [f64; 3], end: [f64; 3]| {
        let segment = subtract3(end, start);
        let length_squared = dot3(segment, segment);
        let foot = if length_squared > 0.0 {
            (dot3(subtract3(point, start), segment) / length_squared).clamp(0.0, 1.0)
        } else {
            0.0
        };
        subtract3(point, [
            start[0] + foot * segment[0],
            start[1] + foot * segment[1],
            start[2] + foot * segment[2],
        ])
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt()
    };
    segment_distance(a, b)
        .min(segment_distance(b, c))
        .min(segment_distance(c, a))
}

/// Signed solid angle of the triangle `a`-`b`-`c` seen from `point`
/// (Van Oosterom and Strackee). A closed surface sums to 4*pi inside.
fn triangle_solid_angle(point: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let a = subtract3(a, point);
    let b = subtract3(b, point);
    let c = subtract3(c, point);
    let numerator = dot3(a, cross3(b, c));
    let la = dot3(a, a).sqrt();
    let lb = dot3(b, b).sqrt();
    let lc = dot3(c, c).sqrt();
    let denominator = la * lb * lc + dot3(a, b) * lc + dot3(a, c) * lb + dot3(b, c) * la;
    2.0 * numerator.atan2(denominator)
}

fn subtract3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PolyhedronFileType {
    #[default]
    Unknown,
    Stl,
    Ply,
}

impl PolyhedronFileType {
    pub(crate) fn from_xml_value(value: &str) -> Self {
        match value {
            "STL" => Self::Stl,
            "PLY" => Self::Ply,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn xml_value(self) -> &'static str {
        match self {
            Self::Unknown => "Unkown",
            Self::Stl => "STL",
            Self::Ply => "PLY",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolyhedronReaderPrimitive {
    filename: String,
    file_type: PolyhedronFileType,
    polyhedron: PolyhedronPrimitive,
}

impl PolyhedronReaderPrimitive {
    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn set_filename(&mut self, filename: impl Into<String>) {
        self.filename = filename.into();
    }

    pub fn file_type(&self) -> PolyhedronFileType {
        self.file_type
    }

    pub fn set_file_type(&mut self, file_type: PolyhedronFileType) {
        self.file_type = file_type;
    }

    pub fn polyhedron(&self) -> &PolyhedronPrimitive {
        &self.polyhedron
    }

    pub fn read_file(&mut self) -> Result<(), PolyhedronReaderError> {
        let polyhedron = crate::polyhedron_reader::read_file(self.file_type, &self.filename)?;
        self.polyhedron = polyhedron;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MultiBoxPrimitive {
    boxes: Vec<BoxPrimitive>,
}

impl MultiBoxPrimitive {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_box(&mut self, shape: BoxPrimitive) -> usize {
        self.boxes.push(shape);
        self.boxes.len() - 1
    }

    pub fn delete_box(&mut self, index: usize) -> bool {
        if index >= self.boxes.len() {
            return false;
        }
        self.boxes.remove(index);
        true
    }

    pub fn box_at(&self, index: usize) -> Option<&BoxPrimitive> {
        self.boxes.get(index)
    }

    pub fn box_count(&self) -> usize {
        self.boxes.len()
    }

    fn bounding_box(
        &self,
        parameters: Option<&ParameterSet>,
        _preserve_orientation: bool,
        input_system: CoordinateSystem,
        output_system: CoordinateSystem,
    ) -> Result<[f64; 6], ParameterError> {
        let Some(first) = self.boxes.first() else {
            return Ok([0.0; 6]);
        };
        let mut bounds = first.bounding_box_in_coordinate_systems(
            parameters,
            false,
            input_system,
            output_system,
        )?;
        for shape in &self.boxes[1..] {
            let shape_bounds = shape.bounding_box_in_coordinate_systems(
                parameters,
                false,
                input_system,
                output_system,
            )?;
            for axis in 0..3 {
                bounds[2 * axis] = bounds[2 * axis].min(shape_bounds[2 * axis]);
                bounds[2 * axis + 1] = bounds[2 * axis + 1].max(shape_bounds[2 * axis + 1]);
            }
        }
        Ok(bounds)
    }

    fn dimension(
        &self,
        parameters: Option<&ParameterSet>,
        input_system: CoordinateSystem,
        output_system: CoordinateSystem,
    ) -> Result<i32, ParameterError> {
        let bounds = self.bounding_box(parameters, false, input_system, output_system)?;
        Ok((0..3)
            .filter(|axis| bounds[2 * axis] != bounds[2 * axis + 1])
            .count() as i32)
    }

    fn contains(
        &self,
        coordinates: [f64; 3],
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
        tolerance: f64,
    ) -> Result<bool, ParameterError> {
        for shape in &self.boxes {
            if shape.contains(coordinates, parameters, coordinate_system, tolerance)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PrimitiveShape {
    Point(PointPrimitive),
    Box(BoxPrimitive),
    Sphere(SpherePrimitive),
    SphericalShell(SphericalShellPrimitive),
    Cylinder(CylinderPrimitive),
    Curve(CurvePrimitive),
    Polygon(PolygonPrimitive),
    Polyhedron(PolyhedronPrimitive),
    PolyhedronReader(PolyhedronReaderPrimitive),
    MultiBox(MultiBoxPrimitive),
    UserDefined(UserDefinedPrimitive),
}

fn collect_scalar_update_error(
    scalar: &ParameterScalar,
    parameters: &ParameterSet,
    errors: &mut Vec<ParameterError>,
) -> bool {
    match scalar.evaluate(Some(parameters)) {
        Ok(_) => true,
        Err(error) => {
            errors.push(error);
            false
        }
    }
}

fn collect_coordinate_update_errors(
    coordinates: &[ParameterScalar; 3],
    parameters: &ParameterSet,
    errors: &mut Vec<ParameterError>,
) -> bool {
    let mut valid = true;
    for coordinate in coordinates {
        valid &= collect_scalar_update_error(coordinate, parameters, errors);
    }
    valid
}

#[derive(Clone, Debug, PartialEq)]
pub struct Primitive {
    id: Option<PrimitiveId>,
    property: Option<PropertyId>,
    shape: PrimitiveShape,
    priority: i32,
    used: bool,
    coordinate_system: CoordinateSystem,
    mesh_type: CoordinateSystem,
    transform: Option<Transform>,
}

impl Primitive {
    pub fn point(coordinates: [f64; 3]) -> Self {
        Self::with_shape(PrimitiveShape::Point(PointPrimitive::new(coordinates)))
    }

    pub fn point_from_expressions(coordinates: [&str; 3]) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::Point(
            PointPrimitive::from_expressions(coordinates)?,
        )))
    }

    pub fn rot_poly() -> Self {
        Self::with_shape(PrimitiveShape::Polygon(
            PolygonPrimitive::new(0.0).with_rotation(
                0,
                ParameterScalar::new(0.0),
                ParameterScalar::new(2.0 * std::f64::consts::PI),
            ),
        ))
    }

    pub(crate) fn rot_poly_from_expressions(
        elevation: &str,
        axis_direction: usize,
        start_angle: &str,
        stop_angle: &str,
    ) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::Polygon(
            PolygonPrimitive::from_expressions(elevation)?.with_rotation(
                axis_direction,
                ParameterScalar::from_value_string(start_angle)?,
                ParameterScalar::from_value_string(stop_angle)?,
            ),
        )))
    }

    pub fn box_primitive(start: [f64; 3], stop: [f64; 3]) -> Self {
        Self::with_shape(PrimitiveShape::Box(BoxPrimitive::new(start, stop)))
    }

    pub fn box_from_expressions(start: [&str; 3], stop: [&str; 3]) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::Box(
            BoxPrimitive::from_expressions(start, stop)?,
        )))
    }

    pub fn sphere(center: [f64; 3], radius: f64) -> Self {
        Self::with_shape(PrimitiveShape::Sphere(SpherePrimitive::new(center, radius)))
    }

    pub fn sphere_from_expressions(
        center: [&str; 3],
        radius: &str,
    ) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::Sphere(
            SpherePrimitive::from_expressions(center, radius)?,
        )))
    }

    pub fn spherical_shell(center: [f64; 3], radius: f64, shell_width: f64) -> Self {
        Self::with_shape(PrimitiveShape::SphericalShell(
            SphericalShellPrimitive::new(center, radius, shell_width),
        ))
    }

    pub fn spherical_shell_from_expressions(
        center: [&str; 3],
        radius: &str,
        shell_width: &str,
    ) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::SphericalShell(
            SphericalShellPrimitive::from_expressions(center, radius, shell_width)?,
        )))
    }

    pub fn cylinder(start: [f64; 3], stop: [f64; 3], radius: f64) -> Self {
        Self::with_shape(PrimitiveShape::Cylinder(CylinderPrimitive::new(
            start, stop, radius,
        )))
    }

    pub fn cylinder_from_expressions(
        start: [&str; 3],
        stop: [&str; 3],
        radius: &str,
    ) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::Cylinder(
            CylinderPrimitive::from_expressions(start, stop, radius)?,
        )))
    }

    pub fn cylindrical_shell(
        start: [f64; 3],
        stop: [f64; 3],
        radius: f64,
        shell_width: f64,
    ) -> Self {
        Self::with_shape(PrimitiveShape::Cylinder(
            CylinderPrimitive::new(start, stop, radius)
                .with_shell_width(ParameterScalar::new(shell_width)),
        ))
    }

    pub fn cylindrical_shell_from_expressions(
        start: [&str; 3],
        stop: [&str; 3],
        radius: &str,
        shell_width: &str,
    ) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::Cylinder(
            CylinderPrimitive::from_expressions(start, stop, radius)?
                .with_shell_width(ParameterScalar::from_value_string(shell_width)?),
        )))
    }

    pub fn curve() -> Self {
        Self::with_shape(PrimitiveShape::Curve(CurvePrimitive::new(None)))
    }

    pub fn wire(radius: f64) -> Self {
        Self::with_shape(PrimitiveShape::Curve(CurvePrimitive::new(Some(radius))))
    }

    pub fn curve_from_expressions(wire_radius: Option<&str>) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::Curve(
            CurvePrimitive::from_expressions(wire_radius)?,
        )))
    }

    pub fn polygon() -> Self {
        Self::with_shape(PrimitiveShape::Polygon(PolygonPrimitive::new(0.0)))
    }

    pub fn polygon_from_expressions(elevation: &str) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::Polygon(
            PolygonPrimitive::from_expressions(elevation)?,
        )))
    }

    pub fn lin_poly(length: f64) -> Self {
        Self::with_shape(PrimitiveShape::Polygon(
            PolygonPrimitive::new(0.0).with_extrusion_length(ParameterScalar::new(length)),
        ))
    }

    pub fn lin_poly_from_expressions(
        elevation: &str,
        length: &str,
    ) -> Result<Self, ParameterError> {
        Ok(Self::with_shape(PrimitiveShape::Polygon(
            PolygonPrimitive::from_expressions(elevation)?
                .with_extrusion_length(ParameterScalar::from_value_string(length)?),
        )))
    }

    pub fn polyhedron() -> Self {
        Self::with_shape(PrimitiveShape::Polyhedron(PolyhedronPrimitive::default()))
    }

    pub fn polyhedron_reader() -> Self {
        Self::with_shape(PrimitiveShape::PolyhedronReader(
            PolyhedronReaderPrimitive::default(),
        ))
    }

    pub fn multibox() -> Self {
        Self::with_shape(PrimitiveShape::MultiBox(MultiBoxPrimitive::new()))
    }

    pub fn user_defined() -> Self {
        Self::with_shape(PrimitiveShape::UserDefined(UserDefinedPrimitive::default()))
    }

    fn with_shape(shape: PrimitiveShape) -> Self {
        Self {
            id: None,
            property: None,
            shape,
            priority: 0,
            used: false,
            coordinate_system: CoordinateSystem::Undefined,
            mesh_type: CoordinateSystem::Cartesian,
            transform: None,
        }
    }

    pub fn id(&self) -> Option<PrimitiveId> {
        self.id
    }

    pub fn property_id(&self) -> Option<PropertyId> {
        self.property
    }

    pub fn primitive_type(&self) -> PrimitiveType {
        match &self.shape {
            PrimitiveShape::Point(_) => PrimitiveType::Point,
            PrimitiveShape::Box(_) => PrimitiveType::Box,
            PrimitiveShape::Sphere(_) => PrimitiveType::Sphere,
            PrimitiveShape::SphericalShell(_) => PrimitiveType::SphericalShell,
            PrimitiveShape::Cylinder(shape) if shape.is_shell() => PrimitiveType::CylindricalShell,
            PrimitiveShape::Cylinder(_) => PrimitiveType::Cylinder,
            PrimitiveShape::Curve(shape) if shape.wire_radius.is_some() => PrimitiveType::Wire,
            PrimitiveShape::Curve(_) => PrimitiveType::Curve,
            PrimitiveShape::Polygon(shape) if shape.is_rotational() => PrimitiveType::RotPoly,
            PrimitiveShape::Polygon(shape) if shape.is_linear_extrusion() => PrimitiveType::LinPoly,
            PrimitiveShape::Polygon(_) => PrimitiveType::Polygon,
            PrimitiveShape::Polyhedron(_) => PrimitiveType::Polyhedron,
            PrimitiveShape::PolyhedronReader(_) => PrimitiveType::PolyhedronReader,
            PrimitiveShape::MultiBox(_) => PrimitiveType::MultiBox,
            PrimitiveShape::UserDefined(_) => PrimitiveType::UserDefined,
        }
    }

    pub fn priority(&self) -> i32 {
        self.priority
    }

    pub fn set_priority(&mut self, priority: i32) {
        self.priority = priority;
    }

    /// Access the primitive shape (Box, Sphere, Cylinder, etc.).
    pub fn shape(&self) -> &PrimitiveShape {
        &self.shape
    }

    pub fn shape_mut(&mut self) -> &mut PrimitiveShape {
        &mut self.shape
    }

    pub fn is_used(&self) -> bool {
        self.used
    }

    pub fn set_used(&mut self, used: bool) {
        self.used = used;
    }

    pub fn coordinate_system(&self) -> CoordinateSystem {
        self.coordinate_system
    }

    pub fn set_coordinate_system(&mut self, coordinate_system: CoordinateSystem) {
        self.coordinate_system = coordinate_system;
    }

    pub fn mesh_type(&self) -> CoordinateSystem {
        self.mesh_type
    }

    pub fn set_mesh_type(&mut self, mesh_type: CoordinateSystem) {
        self.mesh_type = mesh_type;
    }

    pub fn transform(&self) -> Option<&Transform> {
        self.transform.as_ref()
    }

    pub fn transform_mut(&mut self) -> &mut Transform {
        self.transform.get_or_insert_with(Transform::new)
    }

    pub fn has_transform(&self) -> bool {
        self.transform
            .as_ref()
            .is_some_and(Transform::has_transform)
    }

    fn parameter_update_errors(&self, parameters: &ParameterSet) -> Vec<ParameterError> {
        let mut errors = Vec::new();
        match &self.shape {
            PrimitiveShape::Point(shape) => {
                // Native: per-component ParameterCoord messages in order x,y,z
                // Each coordinate is evaluated independently; errors collected per-axis
                for coordinate in &shape.coordinates {
                    collect_scalar_update_error(coordinate, parameters, &mut errors);
                }
            }
            PrimitiveShape::Box(shape) => {
                // Native CSPrimBox::Update:
                //   m_Coords[0].Evaluate(ErrStr) && m_Coords[1].Evaluate(ErrStr)
                // ParameterCoord::Evaluate() collects all three component errors;
                // the `&&` means a failing P1 short-circuits P2.
                collect_coordinate_update_errors(&shape.coordinates[0], parameters, &mut errors);
                if errors.is_empty() {
                    collect_coordinate_update_errors(
                        &shape.coordinates[1],
                        parameters,
                        &mut errors,
                    );
                }
            }
            PrimitiveShape::Sphere(shape) => {
                // Native: center (x,y,z) then radius; each evaluated independently
                for coordinate in &shape.center {
                    collect_scalar_update_error(coordinate, parameters, &mut errors);
                }
                collect_scalar_update_error(&shape.radius, parameters, &mut errors);
            }
            PrimitiveShape::SphericalShell(shape) => {
                // Native: center (x,y,z), radius, then shell_width
                for coordinate in &shape.sphere.center {
                    collect_scalar_update_error(coordinate, parameters, &mut errors);
                }
                collect_scalar_update_error(&shape.sphere.radius, parameters, &mut errors);
                collect_scalar_update_error(&shape.shell_width, parameters, &mut errors);
            }
            PrimitiveShape::Cylinder(shape) => {
                // Native: cylinder axis (2 coordinates) then radius then optional shell_width
                // Evaluate all coordinates and collect all errors
                collect_coordinate_update_errors(&shape.axis[0], parameters, &mut errors);
                collect_scalar_update_error(&shape.radius, parameters, &mut errors);
                if let Some(shell_width) = &shape.shell_width {
                    collect_scalar_update_error(shell_width, parameters, &mut errors);
                }
            }
            PrimitiveShape::Curve(shape) => {
                // Native: per-point coordinates; each point evaluated independently
                for point in &shape.points {
                    collect_coordinate_update_errors(point, parameters, &mut errors);
                }
                if let Some(wire_radius) = &shape.wire_radius {
                    collect_scalar_update_error(wire_radius, parameters, &mut errors);
                }
            }
            PrimitiveShape::Polygon(shape) => {
                // Native: per-vertex coordinates in order; evaluate vertices first, then elevation/extrusion/angles
                for vertex in &shape.vertices {
                    for coordinate in vertex {
                        collect_scalar_update_error(coordinate, parameters, &mut errors);
                    }
                }
                // Elevation error after vertices (native order)
                collect_scalar_update_error(&shape.elevation, parameters, &mut errors);
                // Extrusion length after elevation
                if let Some(extrusion_length) = &shape.extrusion_length {
                    collect_scalar_update_error(extrusion_length, parameters, &mut errors);
                }
                // Rotation angles last
                if let Some(angles) = &shape.rotation_angles {
                    for angle in angles {
                        collect_scalar_update_error(angle, parameters, &mut errors);
                    }
                }
            }
            PrimitiveShape::MultiBox(shape) => {
                // Native: flat insertion order for boxes - each box's 6 coordinates evaluated
                // in insertion order: start.x, start.y, start.z, stop.x, stop.y, stop.z
                // Native coordinate order: evaluate per-axis across all boxes
                // i.e., for axis 0 (x): start.x of box1, start.x of box2, ...
                // then for axis 1 (y): start.y of box1, start.y of box2, ...
                // then for axis 2 (z): start.z of box1, start.z of box2, ...
                // then for axis 3 (x): stop.x of box1, stop.x of box2, ...
                // then for axis 4 (y): stop.y of box1, stop.y of box2, ...
                // then for axis 5 (z): stop.z of box1, stop.z of box2, ...
                for axis in 0..3 {
                    for box_primitive in &shape.boxes {
                        // start coordinate for this axis
                        collect_scalar_update_error(
                            &box_primitive.coordinates[0][axis],
                            parameters,
                            &mut errors,
                        );
                        // stop coordinate for this axis
                        collect_scalar_update_error(
                            &box_primitive.coordinates[1][axis],
                            parameters,
                            &mut errors,
                        );
                    }
                }
            }
            PrimitiveShape::UserDefined(shape) => {
                if let Err(error) = shape.validate_function(Some(parameters)) {
                    errors.push(error);
                }
                for shift in &shape.coordinate_shift {
                    collect_scalar_update_error(shift, parameters, &mut errors);
                }
            }
            PrimitiveShape::Polyhedron(_) | PrimitiveShape::PolyhedronReader(_) => {}
        }
        errors
    }

    pub fn box_shape(&self) -> Option<&BoxPrimitive> {
        match &self.shape {
            PrimitiveShape::Point(_) => None,
            PrimitiveShape::Box(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn box_shape_mut(&mut self) -> Option<&mut BoxPrimitive> {
        match &mut self.shape {
            PrimitiveShape::Point(_) => None,
            PrimitiveShape::Box(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn sphere_shape(&self) -> Option<&SpherePrimitive> {
        match &self.shape {
            PrimitiveShape::Sphere(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn sphere_shape_mut(&mut self) -> Option<&mut SpherePrimitive> {
        match &mut self.shape {
            PrimitiveShape::Sphere(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn spherical_shell_shape(&self) -> Option<&SphericalShellPrimitive> {
        match &self.shape {
            PrimitiveShape::SphericalShell(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn spherical_shell_shape_mut(&mut self) -> Option<&mut SphericalShellPrimitive> {
        match &mut self.shape {
            PrimitiveShape::SphericalShell(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn point_shape(&self) -> Option<&PointPrimitive> {
        match &self.shape {
            PrimitiveShape::Point(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn point_shape_mut(&mut self) -> Option<&mut PointPrimitive> {
        match &mut self.shape {
            PrimitiveShape::Point(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn cylinder_shape(&self) -> Option<&CylinderPrimitive> {
        match &self.shape {
            PrimitiveShape::Cylinder(shape) if !shape.is_shell() => Some(shape),
            _ => None,
        }
    }

    pub fn cylinder_shape_mut(&mut self) -> Option<&mut CylinderPrimitive> {
        match &mut self.shape {
            PrimitiveShape::Cylinder(shape) if !shape.is_shell() => Some(shape),
            _ => None,
        }
    }

    pub fn cylindrical_shell_shape(&self) -> Option<&CylinderPrimitive> {
        match &self.shape {
            PrimitiveShape::Cylinder(shape) if shape.is_shell() => Some(shape),
            _ => None,
        }
    }

    pub fn cylindrical_shell_shape_mut(&mut self) -> Option<&mut CylinderPrimitive> {
        match &mut self.shape {
            PrimitiveShape::Cylinder(shape) if shape.is_shell() => Some(shape),
            _ => None,
        }
    }

    pub fn curve_shape(&self) -> Option<&CurvePrimitive> {
        match &self.shape {
            PrimitiveShape::Curve(shape) if shape.wire_radius.is_none() => Some(shape),
            _ => None,
        }
    }

    pub fn curve_shape_mut(&mut self) -> Option<&mut CurvePrimitive> {
        match &mut self.shape {
            PrimitiveShape::Curve(shape) if shape.wire_radius.is_none() => Some(shape),
            _ => None,
        }
    }

    pub fn wire_shape(&self) -> Option<&CurvePrimitive> {
        match &self.shape {
            PrimitiveShape::Curve(shape) if shape.wire_radius.is_some() => Some(shape),
            _ => None,
        }
    }

    pub fn wire_shape_mut(&mut self) -> Option<&mut CurvePrimitive> {
        match &mut self.shape {
            PrimitiveShape::Curve(shape) if shape.wire_radius.is_some() => Some(shape),
            _ => None,
        }
    }

    pub fn polygon_shape(&self) -> Option<&PolygonPrimitive> {
        match &self.shape {
            PrimitiveShape::Polygon(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn polygon_shape_mut(&mut self) -> Option<&mut PolygonPrimitive> {
        match &mut self.shape {
            PrimitiveShape::Polygon(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn polyhedron_shape(&self) -> Option<&PolyhedronPrimitive> {
        match &self.shape {
            PrimitiveShape::Polyhedron(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn polyhedron_shape_mut(&mut self) -> Option<&mut PolyhedronPrimitive> {
        match &mut self.shape {
            PrimitiveShape::Polyhedron(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn polyhedron_reader_shape(&self) -> Option<&PolyhedronReaderPrimitive> {
        match &self.shape {
            PrimitiveShape::PolyhedronReader(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn polyhedron_reader_shape_mut(&mut self) -> Option<&mut PolyhedronReaderPrimitive> {
        match &mut self.shape {
            PrimitiveShape::PolyhedronReader(shape) => Some(shape),
            _ => None,
        }
    }

    pub(crate) fn curve_or_wire_shape(&self) -> Option<&CurvePrimitive> {
        match &self.shape {
            PrimitiveShape::Curve(shape) => Some(shape),
            _ => None,
        }
    }

    pub(crate) fn curve_or_wire_shape_mut(&mut self) -> Option<&mut CurvePrimitive> {
        match &mut self.shape {
            PrimitiveShape::Curve(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn multibox_shape(&self) -> Option<&MultiBoxPrimitive> {
        match &self.shape {
            PrimitiveShape::MultiBox(shape) => Some(shape),
            PrimitiveShape::Point(_)
            | PrimitiveShape::Box(_)
            | PrimitiveShape::Sphere(_)
            | PrimitiveShape::SphericalShell(_)
            | PrimitiveShape::Cylinder(_)
            | PrimitiveShape::Curve(_)
            | PrimitiveShape::Polygon(_)
            | PrimitiveShape::Polyhedron(_)
            | PrimitiveShape::PolyhedronReader(_)
            | PrimitiveShape::UserDefined(_) => None,
        }
    }

    pub fn multibox_shape_mut(&mut self) -> Option<&mut MultiBoxPrimitive> {
        match &mut self.shape {
            PrimitiveShape::MultiBox(shape) => Some(shape),
            PrimitiveShape::Point(_)
            | PrimitiveShape::Box(_)
            | PrimitiveShape::Sphere(_)
            | PrimitiveShape::SphericalShell(_)
            | PrimitiveShape::Cylinder(_)
            | PrimitiveShape::Curve(_)
            | PrimitiveShape::Polygon(_)
            | PrimitiveShape::Polyhedron(_)
            | PrimitiveShape::PolyhedronReader(_)
            | PrimitiveShape::UserDefined(_) => None,
        }
    }

    pub fn user_defined_shape(&self) -> Option<&UserDefinedPrimitive> {
        match &self.shape {
            PrimitiveShape::UserDefined(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn user_defined_shape_mut(&mut self) -> Option<&mut UserDefinedPrimitive> {
        match &mut self.shape {
            PrimitiveShape::UserDefined(shape) => Some(shape),
            _ => None,
        }
    }

    pub fn bounding_box(
        &self,
        parameters: Option<&ParameterSet>,
        preserve_orientation: bool,
    ) -> Result<[f64; 6], ModelError> {
        let bounds = match &self.shape {
            PrimitiveShape::Point(shape) => {
                let input_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                let coordinates = transform_coordinates(
                    shape.evaluated_coordinates(parameters)?,
                    input_system,
                    self.mesh_type,
                );
                [
                    coordinates[0],
                    coordinates[0],
                    coordinates[1],
                    coordinates[1],
                    coordinates[2],
                    coordinates[2],
                ]
            }
            PrimitiveShape::Box(shape) => {
                let input_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                shape.bounding_box_in_coordinate_systems(
                    parameters,
                    preserve_orientation,
                    input_system,
                    self.mesh_type,
                )?
            }
            PrimitiveShape::Sphere(shape) => {
                let center_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                shape.bounding_box_in_coordinate_system(parameters, center_system)?
            }
            PrimitiveShape::SphericalShell(shape) => {
                let center_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                shape.bounding_box(parameters, center_system)?
            }
            PrimitiveShape::Cylinder(shape) => {
                let axis_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                shape.bounding_box(parameters, axis_system)?
            }
            PrimitiveShape::Curve(shape) => {
                let point_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                shape.bounding_box(parameters, point_system)?
            }
            PrimitiveShape::Polygon(shape) => shape.bounding_box(parameters)?,
            PrimitiveShape::Polyhedron(shape) => shape.bounding_box(),
            PrimitiveShape::PolyhedronReader(shape) => shape.polyhedron().bounding_box(),
            PrimitiveShape::MultiBox(shape) => {
                let input_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                shape.bounding_box(
                    parameters,
                    preserve_orientation,
                    input_system,
                    self.mesh_type,
                )?
            }
            PrimitiveShape::UserDefined(_) => {
                let max = f64::MAX;
                [-max, max, -max, max, -max, max]
            }
        };
        Ok(bounds)
    }

    pub fn dimension(&self, parameters: Option<&ParameterSet>) -> Result<i32, ModelError> {
        match &self.shape {
            PrimitiveShape::Point(_) => Ok(0),
            PrimitiveShape::Box(shape) => Ok(shape.dimension(parameters)?),
            PrimitiveShape::Sphere(shape) => Ok(shape.dimension(parameters)?),
            PrimitiveShape::SphericalShell(shape) => Ok(shape.dimension(parameters)?),
            PrimitiveShape::Cylinder(shape) => {
                let axis_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                Ok(shape.dimension(parameters, axis_system)?)
            }
            PrimitiveShape::Curve(shape) => Ok(shape.dimension(parameters)?),
            PrimitiveShape::Polygon(shape) => Ok(shape.dimension(parameters)?),
            PrimitiveShape::Polyhedron(shape) => Ok(shape.dimension()),
            PrimitiveShape::PolyhedronReader(shape) => Ok(shape.polyhedron().dimension()),
            PrimitiveShape::MultiBox(shape) => {
                let input_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                Ok(shape.dimension(parameters, input_system, self.mesh_type)?)
            }
            PrimitiveShape::UserDefined(_) => Ok(3),
        }
    }

    pub fn is_inside(
        &self,
        coordinates: [f64; 3],
        parameters: Option<&ParameterSet>,
    ) -> Result<bool, ModelError> {
        self.is_inside_with_tolerance(coordinates, parameters, 0.0)
    }

    /// Native `CSPrimitives::IsInside` with the drawing tolerance: the surface
    /// counts as inside, and so does a point within `tolerance` (a length in
    /// the global coordinates) of it. The tolerance is converted into the
    /// primitive's own coordinates; user-defined primitives ignore it.
    pub fn is_inside_with_tolerance(
        &self,
        coordinates: [f64; 3],
        parameters: Option<&ParameterSet>,
        tolerance: f64,
    ) -> Result<bool, ModelError> {
        // Native `LocalTolerance`: a global length in the primitive's
        // coordinates before its transform.
        let local_tolerance = match &self.transform {
            Some(transform) => tolerance * transform.inverse_scale(),
            None => tolerance,
        };
        match &self.shape {
            PrimitiveShape::Point(_) => Ok(false),
            PrimitiveShape::Box(shape) => {
                let mut local_coordinates = coordinates;
                if let Some(transform) = &self.transform {
                    local_coordinates = transform_coordinates(
                        local_coordinates,
                        self.mesh_type,
                        CoordinateSystem::Cartesian,
                    );
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                    local_coordinates = transform_coordinates(
                        local_coordinates,
                        CoordinateSystem::Cartesian,
                        self.mesh_type,
                    );
                }
                let primitive_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                let local_coordinates =
                    transform_coordinates(local_coordinates, self.mesh_type, primitive_system);
                Ok(shape.contains(local_coordinates, parameters, primitive_system, local_tolerance)?)
            }
            PrimitiveShape::Sphere(shape) => {
                let mut local_coordinates =
                    transform_coordinates(coordinates, self.mesh_type, CoordinateSystem::Cartesian);
                if let Some(transform) = &self.transform {
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                }
                let center_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                Ok(shape.contains(local_coordinates, center_system, parameters, local_tolerance)?)
            }
            PrimitiveShape::SphericalShell(shape) => {
                let mut local_coordinates =
                    transform_coordinates(coordinates, self.mesh_type, CoordinateSystem::Cartesian);
                if let Some(transform) = &self.transform {
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                }
                let center_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                Ok(shape.contains(local_coordinates, center_system, parameters, local_tolerance)?)
            }
            PrimitiveShape::Cylinder(shape) => {
                let mut local_coordinates =
                    transform_coordinates(coordinates, self.mesh_type, CoordinateSystem::Cartesian);
                if let Some(transform) = &self.transform {
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                }
                let axis_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                Ok(shape.contains(local_coordinates, parameters, axis_system, local_tolerance)?)
            }
            PrimitiveShape::Curve(shape) => {
                let mut local_coordinates =
                    transform_coordinates(coordinates, self.mesh_type, CoordinateSystem::Cartesian);
                if let Some(transform) = &self.transform {
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                }
                let point_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                Ok(shape.contains(local_coordinates, parameters, point_system, local_tolerance)?)
            }
            PrimitiveShape::Polygon(shape) => {
                let mut local_coordinates =
                    transform_coordinates(coordinates, self.mesh_type, CoordinateSystem::Cartesian);
                if let Some(transform) = &self.transform {
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                }
                if shape.is_rotational() {
                    Ok(shape.contains_rotational(
                        local_coordinates,
                        parameters,
                        local_tolerance,
                    )?)
                } else {
                    Ok(shape.contains(local_coordinates, parameters, local_tolerance)?)
                }
            }
            PrimitiveShape::Polyhedron(shape) => {
                let mut local_coordinates =
                    transform_coordinates(coordinates, self.mesh_type, CoordinateSystem::Cartesian);
                if let Some(transform) = &self.transform {
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                }
                Ok(shape.contains(local_coordinates, local_tolerance))
            }
            PrimitiveShape::PolyhedronReader(shape) => {
                let mut local_coordinates =
                    transform_coordinates(coordinates, self.mesh_type, CoordinateSystem::Cartesian);
                if let Some(transform) = &self.transform {
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                }
                Ok(shape.polyhedron().contains(local_coordinates, local_tolerance))
            }
            PrimitiveShape::MultiBox(shape) => {
                let mut local_coordinates = coordinates;
                if let Some(transform) = &self.transform {
                    local_coordinates = transform_coordinates(
                        local_coordinates,
                        self.mesh_type,
                        CoordinateSystem::Cartesian,
                    );
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                    local_coordinates = transform_coordinates(
                        local_coordinates,
                        CoordinateSystem::Cartesian,
                        self.mesh_type,
                    );
                }
                let primitive_system = if self.coordinate_system == CoordinateSystem::Undefined {
                    self.mesh_type
                } else {
                    self.coordinate_system
                };
                let local_coordinates =
                    transform_coordinates(local_coordinates, self.mesh_type, primitive_system);
                Ok(shape.contains(local_coordinates, parameters, primitive_system, local_tolerance)?)
            }
            PrimitiveShape::UserDefined(shape) => {
                let mut local_coordinates =
                    transform_coordinates(coordinates, self.mesh_type, CoordinateSystem::Cartesian);
                if let Some(transform) = &self.transform {
                    local_coordinates = transform.inverse_transform(local_coordinates)?;
                }
                Ok(shape.contains(local_coordinates, parameters)?)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    id: Option<PropertyId>,
    index: usize,
    property_type: PropertyType,
    name: String,
    coordinate_input_type: CoordinateSystem,
    visible: bool,
    fill_color: Option<PropertyColor>,
    edge_color: Option<PropertyColor>,
    attributes: Vec<(String, String)>,
    primitives: Vec<Primitive>,
    material: Option<MaterialProperties>,
    debye_material: Option<DebyeMaterialProperties>,
    lorentz_material: Option<LorentzMaterialProperties>,
    discrete_material: Option<DiscreteMaterialProperties>,
    conducting_sheet: Option<ConductingSheetProperties>,
    absorbing_boundary: Option<AbsorbingBoundaryProperties>,
    lumped_element: Option<LumpedElementProperties>,
    probe: Option<ProbeProperties>,
    dump_box: Option<DumpBoxProperties>,
    excitation: Option<ExcitationProperties>,
    resolution_box: Option<ResolutionBoxProperties>,
    unknown_property: Option<String>,
}

impl Property {
    pub fn new(name: impl Into<String>, property_type: PropertyType) -> Self {
        let material = matches!(
            property_type,
            PropertyType::Material
                | PropertyType::DispersiveMaterial
                | PropertyType::LorentzMaterial
                | PropertyType::DebyeMaterial
                | PropertyType::DiscreteMaterial
        )
        .then(MaterialProperties::new);
        let debye_material =
            (property_type == PropertyType::DebyeMaterial).then(DebyeMaterialProperties::default);
        let lorentz_material = (property_type == PropertyType::LorentzMaterial)
            .then(LorentzMaterialProperties::default);
        let discrete_material = (property_type == PropertyType::DiscreteMaterial)
            .then(DiscreteMaterialProperties::default);
        let conducting_sheet = (property_type == PropertyType::ConductingSheet)
            .then(ConductingSheetProperties::default);
        let absorbing_boundary = (property_type == PropertyType::AbsorbingBoundary)
            .then(AbsorbingBoundaryProperties::default);
        let lumped_element =
            (property_type == PropertyType::LumpedElement).then(LumpedElementProperties::default);
        let probe = matches!(
            property_type,
            PropertyType::ProbeBox | PropertyType::DumpBox
        )
        .then(ProbeProperties::default);
        let dump_box = (property_type == PropertyType::DumpBox).then(DumpBoxProperties::default);
        let excitation =
            (property_type == PropertyType::Excitation).then(ExcitationProperties::default);
        let resolution_box =
            (property_type == PropertyType::ResBox).then(ResolutionBoxProperties::default);
        Self {
            id: None,
            index: 0,
            property_type,
            name: name.into(),
            coordinate_input_type: if property_type == PropertyType::Excitation {
                CoordinateSystem::Undefined
            } else {
                CoordinateSystem::Cartesian
            },
            visible: !matches!(
                property_type,
                PropertyType::Unknown
                    | PropertyType::ProbeBox
                    | PropertyType::DumpBox
                    | PropertyType::ResBox
            ),
            fill_color: None,
            edge_color: None,
            attributes: Vec::new(),
            primitives: Vec::new(),
            material,
            debye_material,
            lorentz_material,
            discrete_material,
            conducting_sheet,
            absorbing_boundary,
            lumped_element,
            probe,
            dump_box,
            excitation,
            resolution_box,
            unknown_property: (property_type == PropertyType::Unknown).then(String::new),
        }
    }

    pub fn get_copy(&self) -> Self {
        let mut copy = self.clone();
        copy.primitives.clear();
        copy
    }

    pub fn get_copy_with_primitives(&self) -> Self {
        self.clone()
    }

    pub fn id(&self) -> Option<PropertyId> {
        self.id
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn property_type(&self) -> PropertyType {
        self.property_type
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn coordinate_input_type(&self) -> CoordinateSystem {
        self.coordinate_input_type
    }

    pub fn set_coordinate_input_type(&mut self, coordinate_system: CoordinateSystem) {
        self.coordinate_input_type = coordinate_system;
        for primitive in &mut self.primitives {
            primitive.set_mesh_type(coordinate_system);
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn fill_color(&self) -> Option<PropertyColor> {
        self.fill_color
    }

    pub fn set_fill_color(&mut self, color: Option<PropertyColor>) {
        self.fill_color = color;
    }

    pub fn edge_color(&self) -> Option<PropertyColor> {
        self.edge_color
    }

    pub fn set_edge_color(&mut self, color: Option<PropertyColor>) {
        self.edge_color = color;
    }

    pub fn material(&self) -> Option<&MaterialProperties> {
        self.material.as_ref()
    }

    pub fn material_mut(&mut self) -> Option<&mut MaterialProperties> {
        self.material.as_mut()
    }

    pub fn debye_material(&self) -> Option<&DebyeMaterialProperties> {
        self.debye_material.as_ref()
    }

    pub fn debye_material_mut(&mut self) -> Option<&mut DebyeMaterialProperties> {
        self.debye_material.as_mut()
    }

    pub fn lorentz_material(&self) -> Option<&LorentzMaterialProperties> {
        self.lorentz_material.as_ref()
    }

    pub fn lorentz_material_mut(&mut self) -> Option<&mut LorentzMaterialProperties> {
        self.lorentz_material.as_mut()
    }

    pub fn discrete_material(&self) -> Option<&DiscreteMaterialProperties> {
        self.discrete_material.as_ref()
    }

    pub fn discrete_material_mut(&mut self) -> Option<&mut DiscreteMaterialProperties> {
        self.discrete_material.as_mut()
    }

    pub fn conducting_sheet(&self) -> Option<&ConductingSheetProperties> {
        self.conducting_sheet.as_ref()
    }

    pub fn conducting_sheet_mut(&mut self) -> Option<&mut ConductingSheetProperties> {
        self.conducting_sheet.as_mut()
    }

    pub fn absorbing_boundary(&self) -> Option<&AbsorbingBoundaryProperties> {
        self.absorbing_boundary.as_ref()
    }

    pub fn absorbing_boundary_mut(&mut self) -> Option<&mut AbsorbingBoundaryProperties> {
        self.absorbing_boundary.as_mut()
    }

    pub fn lumped_element(&self) -> Option<&LumpedElementProperties> {
        self.lumped_element.as_ref()
    }

    pub fn lumped_element_mut(&mut self) -> Option<&mut LumpedElementProperties> {
        self.lumped_element.as_mut()
    }

    pub fn probe(&self) -> Option<&ProbeProperties> {
        self.probe.as_ref()
    }

    pub fn probe_mut(&mut self) -> Option<&mut ProbeProperties> {
        self.probe.as_mut()
    }

    pub fn dump_box(&self) -> Option<&DumpBoxProperties> {
        self.dump_box.as_ref()
    }

    pub fn dump_box_mut(&mut self) -> Option<&mut DumpBoxProperties> {
        self.dump_box.as_mut()
    }

    pub fn excitation(&self) -> Option<&ExcitationProperties> {
        self.excitation.as_ref()
    }

    pub fn excitation_mut(&mut self) -> Option<&mut ExcitationProperties> {
        self.excitation.as_mut()
    }

    pub fn resolution_box(&self) -> Option<&ResolutionBoxProperties> {
        self.resolution_box.as_ref()
    }

    pub fn resolution_box_mut(&mut self) -> Option<&mut ResolutionBoxProperties> {
        self.resolution_box.as_mut()
    }

    pub fn unknown_property(&self) -> Option<&str> {
        self.unknown_property.as_deref()
    }

    pub fn set_unknown_property(&mut self, value: impl Into<String>) {
        self.unknown_property = Some(value.into());
    }

    fn validate_parameter_expressions(
        &self,
        parameters: &ParameterSet,
    ) -> Result<(), ParameterError> {
        self.parameter_expression_errors(parameters)
            .into_iter()
            .next()
            .map_or(Ok(()), Err)
    }

    fn parameter_expression_errors(&self, parameters: &ParameterSet) -> Vec<ParameterError> {
        let mut errors = Vec::new();
        if let Some(material) = &self.material {
            errors.extend(material.expression_errors(Some(parameters), self.coordinate_input_type));
        }
        if let Some(material) = &self.debye_material {
            errors.extend(material.expression_errors(Some(parameters), self.coordinate_input_type));
        }
        if let Some(material) = &self.lorentz_material {
            errors.extend(material.expression_errors(Some(parameters), self.coordinate_input_type));
        }
        if let Some(sheet) = &self.conducting_sheet {
            errors.extend(sheet.conductivity(Some(parameters)).err());
            errors.extend(sheet.thickness(Some(parameters)).err());
        }
        if let Some(boundary) = &self.absorbing_boundary {
            errors.extend(boundary.phase_velocity(Some(parameters)).err());
        }
        if let Some(element) = &self.lumped_element {
            errors.extend(element.resistance(Some(parameters)).err());
            errors.extend(element.capacitance(Some(parameters)).err());
            errors.extend(element.inductance(Some(parameters)).err());
        }
        if let Some(excitation) = &self.excitation {
            for component in 0..3 {
                errors.extend(excitation.excitation(component, Some(parameters)).err());
                errors.extend(
                    excitation
                        .propagation_direction(component, Some(parameters))
                        .err(),
                );
            }
            errors.extend(excitation.frequency(Some(parameters)).err());
            errors.extend(excitation.delay(Some(parameters)).err());
        }
        errors
    }

    pub fn has_attribute(&self, name: &str) -> bool {
        self.attributes.iter().any(|(key, _)| key == name)
    }

    pub fn attribute(&self, name: &str) -> &str {
        self.attributes
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value.as_str()))
            .unwrap_or("")
    }

    pub fn set_attribute(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        if name.is_empty() {
            return;
        }
        let value = value.into();
        if let Some((_, existing)) = self
            .attributes
            .iter_mut()
            .find(|(existing, _)| existing == &name)
        {
            *existing = value;
        } else {
            self.attributes.push((name, value));
        }
    }

    pub fn remove_attribute(&mut self, name: &str) {
        if let Some(index) = self.attributes.iter().position(|(key, _)| key == name) {
            self.attributes.remove(index);
        }
    }

    pub fn attribute_names(&self) -> impl Iterator<Item = &str> {
        self.attributes.iter().map(|(name, _)| name.as_str())
    }

    pub fn primitives(&self) -> &[Primitive] {
        &self.primitives
    }

    pub fn primitive(&self, index: usize) -> Option<&Primitive> {
        self.primitives.get(index)
    }

    pub fn primitive_mut(&mut self, index: usize) -> Option<&mut Primitive> {
        self.primitives.get_mut(index)
    }

    pub fn primitive_count(&self) -> usize {
        self.primitives.len()
    }

    /// Append a primitive to this property (native `CSProperties::AddPrimitive`).
    ///
    /// A detached property (no id yet) keeps the primitive with an unassigned
    /// id; `Structure::add_property` assigns the ids when the property is added.
    pub fn add_primitive(&mut self, mut primitive: Primitive) {
        primitive.property = self.id;
        primitive.mesh_type = self.coordinate_input_type;
        self.primitives.push(primitive);
    }

    fn take_primitive(&mut self, id: PrimitiveId) -> Option<Primitive> {
        let index = self
            .primitives
            .iter()
            .position(|primitive| primitive.id == Some(id))?;
        let mut primitive = self.primitives.remove(index);
        primitive.property = None;
        Some(primitive)
    }

    fn set_index(&mut self, index: usize) {
        self.index = index;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Structure {
    parameters: ParameterSet,
    grid: RectGrid,
    /// Native `CSXCAD::SetDrawingTolerance`: a point within this distance of a
    /// primitive's surface counts as inside. The operator sets it from the
    /// smallest mesh step.
    drawing_tolerance: f64,
    background_material: BackgroundMaterial,
    coordinate_system: CoordinateSystem,
    properties: Vec<Property>,
    next_property_id: u64,
    next_primitive_id: u64,
}

impl Default for Structure {
    fn default() -> Self {
        Self::new()
    }
}

impl Structure {
    pub fn new() -> Self {
        Self {
            parameters: ParameterSet::new(),
            grid: RectGrid::new(),
            drawing_tolerance: 0.0,
            background_material: BackgroundMaterial::default(),
            coordinate_system: CoordinateSystem::Cartesian,
            properties: Vec::new(),
            next_property_id: 1,
            next_primitive_id: 1,
        }
    }

    pub fn parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    pub fn parameters_mut(&mut self) -> &mut ParameterSet {
        &mut self.parameters
    }

    pub fn grid(&self) -> &RectGrid {
        &self.grid
    }

    pub fn grid_mut(&mut self) -> &mut RectGrid {
        &mut self.grid
    }

    pub fn background_material(&self) -> &BackgroundMaterial {
        &self.background_material
    }

    pub fn background_material_mut(&mut self) -> &mut BackgroundMaterial {
        &mut self.background_material
    }

    pub fn coordinate_system(&self) -> CoordinateSystem {
        self.coordinate_system
    }

    /// Native `CSXCAD::GetDrawingTolerance`.
    pub fn drawing_tolerance(&self) -> f64 {
        self.drawing_tolerance
    }

    /// Native `CSXCAD::SetDrawingTolerance`.
    pub fn set_drawing_tolerance(&mut self, tolerance: f64) {
        self.drawing_tolerance = tolerance.max(0.0);
    }

    pub fn set_coordinate_system(&mut self, coordinate_system: CoordinateSystem) {
        self.coordinate_system = coordinate_system;
        self.grid.set_coordinate_system(coordinate_system);
        for property in &mut self.properties {
            property.set_coordinate_input_type(coordinate_system);
        }
    }

    pub fn clear(&mut self) {
        self.properties.clear();
        self.parameters.clear();
        self.grid.clear();
        self.set_coordinate_system(CoordinateSystem::Cartesian);
        self.background_material = BackgroundMaterial::default();
    }

    pub fn add_property(&mut self, mut property: Property) -> PropertyId {
        let id = PropertyId(self.next_property_id);
        self.next_property_id += 1;
        property.id = Some(id);
        property.set_index(self.properties.len());
        property.set_coordinate_input_type(self.coordinate_system);
        for primitive in &mut property.primitives {
            primitive.id = Some(PrimitiveId(self.next_primitive_id));
            self.next_primitive_id += 1;
            primitive.property = Some(id);
        }
        self.properties.push(property);
        id
    }

    pub fn replace_property(&mut self, old_id: PropertyId, mut new_property: Property) -> bool {
        let Some(index) = self.property_index(old_id) else {
            return false;
        };
        let id = PropertyId(self.next_property_id);
        self.next_property_id += 1;
        new_property.id = Some(id);
        new_property.set_index(index);
        new_property.set_coordinate_input_type(self.coordinate_system);
        for primitive in &mut new_property.primitives {
            primitive.id = Some(PrimitiveId(self.next_primitive_id));
            self.next_primitive_id += 1;
            primitive.property = Some(id);
            primitive.mesh_type = new_property.coordinate_input_type;
        }
        let old_property = &mut self.properties[index];
        for primitive in &mut old_property.primitives {
            primitive.property = Some(id);
            primitive.mesh_type = new_property.coordinate_input_type;
        }
        new_property.primitives.append(&mut old_property.primitives);
        self.properties[index] = new_property;
        true
    }

    pub fn remove_property(&mut self, id: PropertyId) -> Option<Property> {
        let index = self.property_index(id)?;
        let property = self.properties.remove(index);
        for (new_index, remaining) in self.properties.iter_mut().enumerate() {
            remaining.set_index(new_index);
        }
        Some(property)
    }

    pub fn delete_property(&mut self, id: PropertyId) -> bool {
        self.remove_property(id).is_some()
    }

    pub fn add_primitive(
        &mut self,
        property_id: PropertyId,
        mut primitive: Primitive,
    ) -> Result<PrimitiveId, ModelError> {
        let Some(index) = self.property_index(property_id) else {
            return Err(ModelError::PropertyNotFound(property_id));
        };
        let id = PrimitiveId(self.next_primitive_id);
        self.next_primitive_id += 1;
        primitive.id = Some(id);
        self.properties[index].add_primitive(primitive);
        Ok(id)
    }

    pub fn move_primitive(
        &mut self,
        primitive_id: PrimitiveId,
        target_property_id: PropertyId,
    ) -> Result<(), ModelError> {
        let Some(target_index) = self.property_index(target_property_id) else {
            return Err(ModelError::PropertyNotFound(target_property_id));
        };
        let Some(source_index) = self.properties.iter().position(|property| {
            property
                .primitives
                .iter()
                .any(|primitive| primitive.id == Some(primitive_id))
        }) else {
            return Err(ModelError::PrimitiveNotFound(primitive_id));
        };
        if source_index == target_index {
            return Ok(());
        }
        let primitive = self.properties[source_index]
            .take_primitive(primitive_id)
            .ok_or(ModelError::PrimitiveNotFound(primitive_id))?;
        self.properties[target_index].add_primitive(primitive);
        Ok(())
    }

    pub fn delete_primitive(&mut self, primitive_id: PrimitiveId) -> bool {
        let Some(property) = self.properties.iter_mut().find(|property| {
            property
                .primitives
                .iter()
                .any(|primitive| primitive.id == Some(primitive_id))
        }) else {
            return false;
        };
        property.take_primitive(primitive_id).is_some()
    }

    pub fn property(&self, id: PropertyId) -> Option<&Property> {
        self.properties
            .iter()
            .find(|property| property.id == Some(id))
    }

    pub fn property_mut(&mut self, id: PropertyId) -> Option<&mut Property> {
        self.properties
            .iter_mut()
            .find(|property| property.id == Some(id))
    }

    /// Mutable property plus the structure parameter set it resolves against.
    ///
    /// Splitting the borrow here is what lets the Python layer evaluate
    /// parameter expressions of an attached property without cloning the set.
    pub fn property_with_parameters_mut(
        &mut self,
        id: PropertyId,
    ) -> Option<(&mut Property, &ParameterSet)> {
        let property = self
            .properties
            .iter_mut()
            .find(|property| property.id == Some(id))?;
        Some((property, &self.parameters))
    }

    pub fn property_by_index(&self, index: usize) -> Option<&Property> {
        self.properties.get(index)
    }

    pub fn property_by_index_mut(&mut self, index: usize) -> Option<&mut Property> {
        self.properties.get_mut(index)
    }

    pub fn properties_by_name<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Property> {
        self.properties
            .iter()
            .filter(move |property| property.name == name)
    }

    pub fn primitive(&self, id: PrimitiveId) -> Option<&Primitive> {
        self.properties
            .iter()
            .flat_map(|property| &property.primitives)
            .find(|primitive| primitive.id == Some(id))
    }

    pub fn property_for_primitive(&self, id: PrimitiveId) -> Option<&Property> {
        self.properties.iter().find(|property| {
            property
                .primitives
                .iter()
                .any(|primitive| primitive.id == Some(id))
        })
    }

    pub fn property_count(&self) -> usize {
        self.properties.len()
    }

    pub fn property_count_by_type(&self, property_type: impl Into<PropertyTypeMask>) -> usize {
        self.properties_by_type(property_type).count()
    }

    pub fn properties_by_type(
        &self,
        property_type: impl Into<PropertyTypeMask>,
    ) -> impl Iterator<Item = &Property> {
        let filter = property_type.into();
        self.properties
            .iter()
            .filter(move |property| filter.matches(property.property_type))
    }

    pub fn property_by_coord_priority(
        &mut self,
        coordinates: [f64; 3],
        property_type: impl Into<PropertyTypeMask>,
        mark_found_as_used: bool,
    ) -> Result<Option<(PropertyId, PrimitiveId)>, ModelError> {
        let filter = property_type.into();
        let mut winner = None;
        {
            let parameters = &self.parameters;
            for property in self.properties_by_type(filter) {
                let Some(property_id) = property.id else {
                    continue;
                };
                for primitive in property.primitives() {
                    if primitive.is_inside_with_tolerance(
                coordinates,
                Some(parameters),
                self.drawing_tolerance,
            )? {
                        let primitive_id =
                            primitive.id().ok_or(ModelError::PrimitiveIdNotAssigned)?;
                        let replace_winner = match winner {
                            Some((_, _, priority)) => primitive.priority > priority,
                            None => true,
                        };
                        if replace_winner {
                            winner = Some((property_id, primitive_id, primitive.priority));
                        }
                    }
                }
            }
        }
        let winner = winner.map(|(property_id, primitive_id, _)| (property_id, primitive_id));
        if mark_found_as_used {
            if let Some((_, primitive_id)) = winner {
                let primitive = self
                    .properties
                    .iter_mut()
                    .flat_map(|property| &mut property.primitives)
                    .find(|primitive| primitive.id == Some(primitive_id))
                    .ok_or(ModelError::PrimitiveNotFound(primitive_id))?;
                primitive.set_used(true);
            }
        }
        Ok(winner)
    }

    pub fn properties_by_coords_priority(
        &mut self,
        coordinates: &[[f64; 3]],
        property_type: impl Into<PropertyTypeMask>,
        mark_found_as_used: bool,
    ) -> Result<Vec<Option<(PropertyId, PrimitiveId)>>, ModelError> {
        let property_type = property_type.into();
        coordinates
            .iter()
            .map(|coordinates| {
                self.property_by_coord_priority(*coordinates, property_type, mark_found_as_used)
            })
            .collect()
    }

    pub fn primitive_count(&self) -> usize {
        self.properties.iter().map(Property::primitive_count).sum()
    }

    fn validate_geometry_counts(&mut self) -> Result<(), StructureValidationError> {
        if self.properties.is_empty() {
            return Err(StructureValidationError::NoProperties);
        }
        if self.primitive_count() == 0 {
            return Err(StructureValidationError::NoPrimitives);
        }
        for direction in 0..3 {
            let minimum = if direction < 2 { 2 } else { 1 };
            let actual = self.grid.line_count(direction);
            if actual < minimum {
                return Err(StructureValidationError::InsufficientGridLines {
                    direction,
                    minimum,
                    actual,
                });
            }
        }
        Ok(())
    }

    fn validate_excitation_primitives(&self) -> Result<(), StructureValidationError> {
        if !self.properties.iter().any(|property| {
            property.property_type == PropertyType::Excitation && !property.primitives.is_empty()
        }) {
            return Err(StructureValidationError::NoExcitationPrimitives);
        }
        Ok(())
    }

    pub fn validate_geometry_prerequisites(&mut self) -> Result<(), StructureValidationError> {
        self.validate_geometry_counts()?;
        self.validate_excitation_primitives()
    }

    pub fn validate_property_expressions(&self) -> Result<(), StructurePropertyValidationError> {
        for property in &self.properties {
            if let Err(source) = property.validate_parameter_expressions(&self.parameters) {
                return Err(StructurePropertyValidationError {
                    property_id: property.id,
                    source,
                });
            }
        }
        Ok(())
    }

    fn validate_primitive_update(&self, primitive: &Primitive) -> Result<(), ModelError> {
        if let PrimitiveShape::UserDefined(shape) = &primitive.shape {
            shape
                .validate_update(Some(&self.parameters))
                .map_err(ModelError::from)?;
        }
        primitive.bounding_box(Some(&self.parameters), false)?;
        if let PrimitiveShape::Polygon(shape) = &primitive.shape {
            if shape.is_rotational() {
                shape
                    .rotation_angles(Some(&self.parameters))
                    .map(|_| ())
                    .map_err(ModelError::from)?;
            }
        }
        Ok(())
    }

    /// Evaluates property and primitive updates in native traversal order.
    ///
    /// Returns source-carrying errors without stopping at the first invalid item.
    /// Property and primitive scalar failures are reported individually in
    /// native update order.
    pub fn validate_updates(&self) -> Vec<StructureValidationError> {
        let mut errors = Vec::new();
        for property in &self.properties {
            for source in property.parameter_expression_errors(&self.parameters) {
                errors.push(
                    StructurePropertyValidationError {
                        property_id: property.id,
                        source,
                    }
                    .into(),
                );
            }
        }
        for primitive in self.all_primitives(false, PropertyType::Any) {
            let parameter_errors = primitive.parameter_update_errors(&self.parameters);
            if parameter_errors.is_empty() {
                if let Err(source) = self.validate_primitive_update(primitive) {
                    errors.push(
                        StructurePrimitiveValidationError {
                            primitive_id: primitive.id,
                            source,
                        }
                        .into(),
                    );
                }
            } else {
                for source in parameter_errors {
                    errors.push(
                        StructurePrimitiveValidationError {
                            primitive_id: primitive.id,
                            source: source.into(),
                        }
                        .into(),
                    );
                }
            }
        }
        errors
    }

    pub fn validate_primitive_bounds(&self) -> Result<(), StructurePrimitiveValidationError> {
        for primitive in self.all_primitives(false, PropertyType::Any) {
            if let Err(source) = self.validate_primitive_update(primitive) {
                return Err(StructurePrimitiveValidationError {
                    primitive_id: primitive.id,
                    source,
                });
            }
        }
        Ok(())
    }

    pub fn validate_geometry(&mut self) -> Result<(), StructureValidationError> {
        self.validate_geometry_counts()?;
        self.validate_primitive_bounds()?;
        self.validate_property_expressions()?;
        self.validate_excitation_primitives()
    }

    pub fn primitive_count_by_type(&self, property_type: impl Into<PropertyTypeMask>) -> usize {
        self.properties_by_type(property_type)
            .map(Property::primitive_count)
            .sum()
    }

    pub fn all_primitives(
        &self,
        sorted: bool,
        property_type: impl Into<PropertyTypeMask>,
    ) -> Vec<&Primitive> {
        let mut primitives = self
            .properties_by_type(property_type)
            .flat_map(|property| property.primitives.iter())
            .collect::<Vec<_>>();
        if sorted {
            primitives.sort_by_key(|primitive| std::cmp::Reverse(primitive.priority));
        }
        primitives
    }

    pub fn unused_primitives(&self, property_type: impl Into<PropertyTypeMask>) -> Vec<&Primitive> {
        self.all_primitives(false, property_type)
            .into_iter()
            .filter(|primitive| !primitive.is_used())
            .collect()
    }

    pub fn primitives_by_type(&self, primitive_type: PrimitiveType) -> Vec<&Primitive> {
        self.all_primitives(false, PropertyType::Any)
            .into_iter()
            .filter(|primitive| primitive.primitive_type() == primitive_type)
            .collect()
    }

    pub fn primitives_by_bounds(
        &self,
        bounds: [f64; 6],
        sorted: bool,
        property_type: impl Into<PropertyTypeMask>,
    ) -> Result<Vec<&Primitive>, ModelError> {
        let mut matching = Vec::new();
        for primitive in self.all_primitives(sorted, property_type) {
            if primitive.has_transform()
                || matches!(&primitive.shape, PrimitiveShape::UserDefined(_))
            {
                matching.push(primitive);
                continue;
            }
            let primitive_bounds = primitive.bounding_box(Some(&self.parameters), false)?;
            let disjoint = (0..3).any(|axis| {
                let lower = 2 * axis;
                let upper = lower + 1;
                (bounds[lower] < primitive_bounds[lower]
                    && bounds[upper] < primitive_bounds[lower]
                    && bounds[lower] < primitive_bounds[upper]
                    && bounds[upper] < primitive_bounds[upper])
                    || (bounds[lower] > primitive_bounds[lower]
                        && bounds[upper] > primitive_bounds[lower]
                        && bounds[lower] > primitive_bounds[upper]
                        && bounds[upper] > primitive_bounds[upper])
            });
            if !disjoint {
                matching.push(primitive);
            }
        }
        Ok(matching)
    }

    pub fn object_bounds(
        &self,
        property_type: impl Into<PropertyTypeMask>,
    ) -> Result<Option<[f64; 6]>, ModelError> {
        let mut bounds: Option<[f64; 6]> = None;
        for primitive in self.all_primitives(false, property_type) {
            if matches!(&primitive.shape, PrimitiveShape::UserDefined(_)) {
                continue;
            }
            let primitive_bounds = primitive.bounding_box(Some(&self.parameters), false)?;
            if let Some(bounds) = &mut bounds {
                for axis in 0..3 {
                    bounds[2 * axis] = bounds[2 * axis].min(primitive_bounds[2 * axis]);
                    bounds[2 * axis + 1] = bounds[2 * axis + 1].max(primitive_bounds[2 * axis + 1]);
                }
            } else {
                bounds = Some(primitive_bounds);
            }
        }
        Ok(bounds)
    }

    pub fn insert_edges_to_grid(&mut self, direction: usize) -> Result<bool, ModelError> {
        if direction > 2 {
            return Ok(false);
        }

        let mut edges = Vec::new();
        for primitive in self.all_primitives(false, PropertyType::Any) {
            if matches!(&primitive.shape, PrimitiveShape::UserDefined(_)) {
                continue;
            }
            let bounds = primitive.bounding_box(Some(&self.parameters), false)?;
            edges.extend([bounds[2 * direction], bounds[2 * direction + 1]]);
        }
        self.grid.add_lines(direction, &edges);
        Ok(true)
    }

    fn property_index(&self, id: PropertyId) -> Option<usize> {
        self.properties
            .iter()
            .position(|property| property.id == Some(id))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BoxPrimitive, ModelError, Primitive, PrimitiveShape, PrimitiveType, Property,
        PropertyColor, PropertyType, Structure, StructureValidationError,
    };
    use crate::grid::CoordinateSystem;
    use crate::material::MaterialField;

    #[test]
    fn primitive_coordinate_getters_mirror_the_native_indexing() {
        use super::{CurvePrimitive, CylinderPrimitive, PointPrimitive, PolygonPrimitive};

        let point = PointPrimitive::new([1.0, 2.0, 3.0]);
        assert_eq!(point.coordinate(1, None).unwrap(), Some(2.0));
        assert_eq!(point.coordinate(3, None).unwrap(), None);

        let mut cylinder = CylinderPrimitive::new([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5);
        assert_eq!(cylinder.coordinate(5, None).unwrap(), Some(1.0));
        assert!(cylinder.set_coordinate(4, 2.0));
        assert_eq!(cylinder.coordinate(4, None).unwrap(), Some(2.0));
        assert_eq!(cylinder.coordinate(6, None).unwrap(), None);

        let mut curve = CurvePrimitive::new(None);
        curve.add_point([0.0, 1.0, 2.0]);
        curve.add_point([3.0, 4.0, 5.0]);
        assert_eq!(curve.point(0, None).unwrap(), Some([0.0, 1.0, 2.0]));
        assert_eq!(curve.point(1, None).unwrap(), Some([3.0, 4.0, 5.0]));
        assert_eq!(curve.point(2, None).unwrap(), None);

        let mut polygon = PolygonPrimitive::new(0.0);
        polygon.add_vertex([1.0, 2.0]);
        assert_eq!(polygon.vertex(0, None).unwrap(), Some([1.0, 2.0]));
        assert_eq!(polygon.vertex(1, None).unwrap(), None);
    }

    #[test]
    fn detached_property_owns_and_edits_its_primitives() {
        let mut property = Property::new("metal", PropertyType::Metal);
        property.add_primitive(Primitive::box_primitive([0.0; 3], [1.0; 3]));
        assert_eq!(property.primitive_count(), 1);
        assert!(property.primitive_mut(0).is_some());
        assert!(property.primitive_mut(1).is_none());
        property.primitive_mut(0).unwrap().set_used(true);
        assert!(property.primitive(0).unwrap().is_used());
    }

    #[test]
    fn property_type_masks_match_the_native_bit_values() {
        use super::PropertyTypeMask;
        assert_eq!(PropertyTypeMask::from_bits(0xffff).bits(), 0xffff);
        assert_eq!(
            PropertyTypeMask::from_bits(PropertyType::Metal as u16).bits(),
            0x0004
        );
        assert_eq!(
            PropertyTypeMask::from_bits(PropertyType::AbsorbingBoundary as u16).bits(),
            0x8000
        );
    }

    #[test]
    fn structure_owns_properties_and_assigns_stable_handles_and_indices() {
        let mut structure = Structure::new();
        let first = structure.add_property(Property::new("metal", PropertyType::Metal));
        let second = structure.add_property(Property::new("dielectric", PropertyType::Material));
        assert_eq!(structure.property_count(), 2);
        assert_eq!(structure.property(first).unwrap().index(), 0);
        assert_eq!(structure.property(second).unwrap().index(), 1);
        assert_eq!(
            structure
                .properties_by_name("metal")
                .map(Property::name)
                .collect::<Vec<_>>(),
            ["metal"]
        );
        let removed = structure.remove_property(first).unwrap();
        assert_eq!(removed.name(), "metal");
        assert_eq!(removed.id(), Some(first));
        assert_eq!(structure.property(second).unwrap().index(), 0);
    }

    #[test]
    fn geometry_prerequisites_report_missing_structure_grid_and_excitation() {
        let mut structure = Structure::new();
        assert_eq!(
            structure.validate_geometry_prerequisites(),
            Err(StructureValidationError::NoProperties)
        );

        let metal = structure.add_property(Property::new("metal", PropertyType::Metal));
        assert_eq!(
            structure.validate_geometry_prerequisites(),
            Err(StructureValidationError::NoPrimitives)
        );
        structure
            .add_primitive(metal, Primitive::box_primitive([0.0; 3], [1.0; 3]))
            .unwrap();
        assert_eq!(
            structure.validate_geometry_prerequisites(),
            Err(StructureValidationError::InsufficientGridLines {
                direction: 0,
                minimum: 2,
                actual: 0
            })
        );

        structure.grid_mut().add_lines(0, &[0.0, 1.0]);
        assert_eq!(
            structure.validate_geometry_prerequisites(),
            Err(StructureValidationError::InsufficientGridLines {
                direction: 1,
                minimum: 2,
                actual: 0
            })
        );
        structure.grid_mut().add_lines(1, &[0.0, 1.0]);
        assert_eq!(
            structure.validate_geometry_prerequisites(),
            Err(StructureValidationError::InsufficientGridLines {
                direction: 2,
                minimum: 1,
                actual: 0
            })
        );
        structure.grid_mut().add_line(2, 0.0);
        assert_eq!(
            structure.validate_geometry_prerequisites(),
            Err(StructureValidationError::NoExcitationPrimitives)
        );

        let excitation = structure.add_property(Property::new("source", PropertyType::Excitation));
        structure
            .add_primitive(excitation, Primitive::box_primitive([0.0; 3], [1.0; 3]))
            .unwrap();
        assert_eq!(structure.validate_geometry_prerequisites(), Ok(()));
    }

    #[test]
    fn property_expression_validation_reports_which_property_failed() {
        let mut structure = Structure::new();
        let material_id = structure.add_property(Property::new("material", PropertyType::Material));
        let material = structure
            .property_mut(material_id)
            .unwrap()
            .material_mut()
            .unwrap();
        material
            .set_value_expression(MaterialField::Epsilon, 2, "missing_parameter")
            .unwrap();
        let error = structure.validate_property_expressions().unwrap_err();
        assert_eq!(error.property_id(), Some(material_id));
        assert!(std::error::Error::source(&error).is_some());

        let material = structure
            .property_mut(material_id)
            .unwrap()
            .material_mut()
            .unwrap();
        material
            .set_value_expression(MaterialField::Epsilon, 2, "1")
            .unwrap();
        material
            .set_weight_expression(MaterialField::Sigma, 1, "missing_coordinate")
            .unwrap();
        assert_eq!(
            structure
                .validate_property_expressions()
                .unwrap_err()
                .property_id(),
            Some(material_id)
        );

        let material = structure
            .property_mut(material_id)
            .unwrap()
            .material_mut()
            .unwrap();
        material
            .set_weight_expression(MaterialField::Sigma, 1, "1")
            .unwrap();

        let debye_id = structure.add_property(Property::new("debye", PropertyType::DebyeMaterial));
        let debye = structure
            .property_mut(debye_id)
            .unwrap()
            .debye_material_mut()
            .unwrap();
        debye.set_order(1);
        debye
            .set_epsilon_delta_expression(0, 2, "missing_pole_value")
            .unwrap();
        assert_eq!(
            structure
                .validate_property_expressions()
                .unwrap_err()
                .property_id(),
            Some(debye_id)
        );
        structure
            .property_mut(debye_id)
            .unwrap()
            .debye_material_mut()
            .unwrap()
            .set_epsilon_delta_expression(0, 2, "1")
            .unwrap();

        let lorentz_id =
            structure.add_property(Property::new("lorentz", PropertyType::LorentzMaterial));
        let lorentz = structure
            .property_mut(lorentz_id)
            .unwrap()
            .lorentz_material_mut()
            .unwrap();
        lorentz.set_order(1);
        assert!(lorentz
            .set_value_expression(
                crate::material::LorentzParameter::EpsilonRelaxTime,
                0,
                1,
                "missing_lorentz_value",
            )
            .unwrap());
        assert_eq!(
            structure
                .validate_property_expressions()
                .unwrap_err()
                .property_id(),
            Some(lorentz_id)
        );
        structure
            .property_mut(lorentz_id)
            .unwrap()
            .lorentz_material_mut()
            .unwrap()
            .set_value_expression(
                crate::material::LorentzParameter::EpsilonRelaxTime,
                0,
                1,
                "1",
            )
            .unwrap();

        let excitation_id =
            structure.add_property(Property::new("source", PropertyType::Excitation));
        structure
            .property_mut(excitation_id)
            .unwrap()
            .excitation_mut()
            .unwrap()
            .set_delay_term("missing_delay")
            .unwrap();
        assert_eq!(
            structure
                .validate_property_expressions()
                .unwrap_err()
                .property_id(),
            Some(excitation_id)
        );
        structure
            .property_mut(excitation_id)
            .unwrap()
            .excitation_mut()
            .unwrap()
            .set_delay_term("0")
            .unwrap();
        assert!(structure.validate_property_expressions().is_ok());
    }

    #[test]
    fn update_validation_reports_all_properties_before_all_primitives() {
        let mut structure = Structure::new();
        let mut property_ids = Vec::new();
        let mut primitive_ids = Vec::new();
        for (property_expression, coordinate_expression) in [
            ("missing_material_a", "missing_coordinate_a"),
            ("missing_material_b", "missing_coordinate_b"),
        ] {
            let property_id =
                structure.add_property(Property::new("material", PropertyType::Material));
            structure
                .property_mut(property_id)
                .unwrap()
                .material_mut()
                .unwrap()
                .set_value_expression(MaterialField::Epsilon, 0, property_expression)
                .unwrap();
            let primitive_id = structure
                .add_primitive(
                    property_id,
                    Primitive::box_from_expressions(
                        ["0", "0", "0"],
                        [coordinate_expression, "1", "1"],
                    )
                    .unwrap(),
                )
                .unwrap();
            property_ids.push(property_id);
            primitive_ids.push(primitive_id);
        }

        let errors = structure.validate_updates();
        assert_eq!(errors.len(), 4);
        for (error, expected_id) in errors[..2].iter().zip(property_ids) {
            let StructureValidationError::Property(error) = error else {
                panic!("property update errors must precede primitive errors");
            };
            assert_eq!(error.property_id(), Some(expected_id));
            assert!(std::error::Error::source(error).is_some());
        }
        for (error, expected_id) in errors[2..].iter().zip(primitive_ids) {
            let StructureValidationError::Primitive(error) = error else {
                panic!("primitive update errors must follow property errors");
            };
            assert_eq!(error.primitive_id(), Some(expected_id));
            assert!(std::error::Error::source(error).is_some());
        }
    }

    #[test]
    fn update_validation_collects_material_errors_in_native_component_order() {
        let mut structure = Structure::new();
        let property_id = structure.add_property(Property::new("material", PropertyType::Material));
        let material = structure
            .property_mut(property_id)
            .unwrap()
            .material_mut()
            .unwrap();
        material
            .set_value_expression(MaterialField::Epsilon, 1, "missing_epsilon")
            .unwrap();
        material
            .set_value_expression(MaterialField::Sigma, 0, "1+missing_sigma")
            .unwrap();

        let errors = structure.validate_updates();
        assert_eq!(errors.len(), 2);
        let error_texts = errors
            .iter()
            .map(|error| match error {
                StructureValidationError::Property(error) => {
                    assert_eq!(error.property_id(), Some(property_id));
                    std::error::Error::source(error).unwrap().to_string()
                }
                _ => panic!("material update failures must be property errors"),
            })
            .collect::<Vec<_>>();
        assert!(error_texts[0].contains("byte 2"), "{error_texts:?}");
        assert!(error_texts[1].contains("byte 0"), "{error_texts:?}");
    }

    #[test]
    fn update_validation_collects_debye_and_lorentz_expression_errors() {
        let mut structure = Structure::new();
        let debye_id = structure.add_property(Property::new("debye", PropertyType::DebyeMaterial));
        let debye = structure
            .property_mut(debye_id)
            .unwrap()
            .debye_material_mut()
            .unwrap();
        debye.set_order(1);
        debye
            .set_epsilon_delta_expression(0, 0, "1+missing_delta")
            .unwrap();
        debye
            .set_epsilon_delta_weight_expression(0, 0, "missing_weight")
            .unwrap();

        let lorentz_id =
            structure.add_property(Property::new("lorentz", PropertyType::LorentzMaterial));
        let lorentz = structure
            .property_mut(lorentz_id)
            .unwrap()
            .lorentz_material_mut()
            .unwrap();
        lorentz.set_order(1);
        lorentz
            .set_value_expression(
                crate::material::LorentzParameter::EpsilonPlasmaFrequency,
                0,
                0,
                "missing_value",
            )
            .unwrap();
        lorentz
            .set_weight_expression(
                crate::material::LorentzParameter::EpsilonPlasmaFrequency,
                0,
                0,
                "1+missing_weight",
            )
            .unwrap();

        let errors = structure.validate_updates();
        let property_errors = errors
            .iter()
            .map(|error| match error {
                StructureValidationError::Property(error) => (
                    error.property_id().unwrap(),
                    std::error::Error::source(error).unwrap().to_string(),
                ),
                _ => panic!("dispersion update failures must be property errors"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            property_errors,
            vec![
                (
                    debye_id,
                    "expression parse failed: Unknown identifier at byte 2".to_owned()
                ),
                (
                    debye_id,
                    "expression parse failed: Unknown identifier at byte 0".to_owned()
                ),
                (
                    lorentz_id,
                    "expression parse failed: Unknown identifier at byte 0".to_owned()
                ),
                (
                    lorentz_id,
                    "expression parse failed: Unknown identifier at byte 2".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn update_validation_collects_lumped_element_expression_errors() {
        let mut structure = Structure::new();
        let property_id =
            structure.add_property(Property::new("lumped", PropertyType::LumpedElement));
        let element = structure
            .property_mut(property_id)
            .unwrap()
            .lumped_element_mut()
            .unwrap();
        element.set_resistance_term("missing_resistance").unwrap();
        element.set_capacitance_term("missing_capacitance").unwrap();
        element.set_inductance_term("missing_inductance").unwrap();

        let errors = structure.validate_updates();
        assert_eq!(errors.len(), 3);
        assert!(errors.iter().all(|error| matches!(
            error,
            StructureValidationError::Property(error)
                if error.property_id() == Some(property_id)
        )));
    }

    #[test]
    fn update_validation_collects_primitive_scalar_errors_in_native_order() {
        let mut structure = Structure::new();
        let property_id = structure.add_property(Property::new("metal", PropertyType::Metal));
        let shell = Primitive::spherical_shell_from_expressions(
            [
                "missing_center_x",
                "1+missing_center_y",
                "1+2+missing_center_z",
            ],
            "1+missing_radius",
            "missing_shell_width",
        )
        .unwrap();
        let shell_id = structure.add_primitive(property_id, shell).unwrap();
        let cylinder = Primitive::cylindrical_shell_from_expressions(
            ["missing_axis_start", "1+missing_axis_y", "0"],
            ["missing_axis_stop", "0", "0"],
            "1+missing_cylinder_radius",
            "missing_cylinder_shell_width",
        )
        .unwrap();
        let cylinder_id = structure.add_primitive(property_id, cylinder).unwrap();

        let errors = structure.validate_updates();
        let primitive_errors = errors
            .iter()
            .map(|error| match error {
                StructureValidationError::Primitive(error) => (
                    error.primitive_id().unwrap(),
                    std::error::Error::source(error).unwrap().to_string(),
                ),
                _ => panic!("primitive update failures must be primitive errors"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            primitive_errors,
            vec![
                (
                    shell_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0"
                        .to_owned()
                ),
                (
                    shell_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2"
                        .to_owned()
                ),
                (
                    shell_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 4"
                        .to_owned()
                ),
                (
                    shell_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2"
                        .to_owned()
                ),
                (
                    shell_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0"
                        .to_owned()
                ),
                (
                    cylinder_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0"
                        .to_owned()
                ),
                (
                    cylinder_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2"
                        .to_owned()
                ),
                (
                    cylinder_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2"
                        .to_owned()
                ),
                (
                    cylinder_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0"
                        .to_owned()
                ),
            ]
        );
    }

    #[test]
    fn update_validation_collects_multibox_errors_in_native_coordinate_order() {
        let mut structure = Structure::new();
        let property_id = structure.add_property(Property::new("metal", PropertyType::Metal));
        let mut primitive = Primitive::multibox();
        primitive.multibox_shape_mut().unwrap().add_box(
            BoxPrimitive::from_expressions(
                [
                    "missing_start_x",
                    "1+missing_start_y",
                    "1+2+missing_start_z",
                ],
                ["1+2+3+missing_stop_x", "missing_stop_y", "1+missing_stop_z"],
            )
            .unwrap(),
        );
        let primitive_id = structure.add_primitive(property_id, primitive).unwrap();

        let errors = structure.validate_updates();
        let error_texts = errors
            .iter()
            .map(|error| match error {
                StructureValidationError::Primitive(error) => {
                    assert_eq!(error.primitive_id(), Some(primitive_id));
                    std::error::Error::source(error).unwrap().to_string()
                }
                _ => panic!("multi-box update failures must be primitive errors"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            error_texts,
            vec![
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0",
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 6",
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2",
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0",
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 4",
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2",
            ]
        );
    }

    #[test]
    fn update_validation_collects_wire_and_linear_polygon_errors_in_native_order() {
        let mut structure = Structure::new();
        let property_id = structure.add_property(Property::new("metal", PropertyType::Metal));
        let mut wire = Primitive::curve_from_expressions(Some("1+missing_radius")).unwrap();
        wire.wire_shape_mut()
            .unwrap()
            .add_point_from_expressions(["missing_x", "1+missing_y", "1+2+missing_z"])
            .unwrap();
        let wire_id = structure.add_primitive(property_id, wire).unwrap();

        let mut polygon =
            Primitive::lin_poly_from_expressions("missing_elevation", "1+missing_length").unwrap();
        polygon
            .polygon_shape_mut()
            .unwrap()
            .add_vertex_from_expressions(["missing_vertex_x", "1+missing_vertex_y"])
            .unwrap();
        let polygon_id = structure.add_primitive(property_id, polygon).unwrap();

        let errors = structure.validate_updates();
        let primitive_errors = errors
            .iter()
            .map(|error| match error {
                StructureValidationError::Primitive(error) => (
                    error.primitive_id().unwrap(),
                    std::error::Error::source(error).unwrap().to_string(),
                ),
                _ => panic!("geometry update failures must be primitive errors"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            primitive_errors,
            vec![
                (
                    wire_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0"
                        .to_owned()
                ),
                (
                    wire_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2"
                        .to_owned()
                ),
                (
                    wire_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 4"
                        .to_owned()
                ),
                (
                    wire_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2"
                        .to_owned()
                ),
                (
                    polygon_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0"
                        .to_owned()
                ),
                (
                    polygon_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2"
                        .to_owned()
                ),
                (
                    polygon_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0"
                        .to_owned()
                ),
                (
                    polygon_id,
                    "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2"
                        .to_owned()
                ),
            ]
        );
    }

    #[test]
    fn update_validation_collects_user_defined_function_and_shift_errors() {
        let mut structure = Structure::new();
        let property_id = structure.add_property(Property::new("metal", PropertyType::Metal));
        let mut primitive = Primitive::user_defined();
        let shape = primitive.user_defined_shape_mut().unwrap();
        shape.set_function("x+missing_function");
        shape
            .set_coordinate_shift_expression(0, "missing_shift_x")
            .unwrap();
        shape
            .set_coordinate_shift_expression(1, "1+missing_shift_y")
            .unwrap();
        shape
            .set_coordinate_shift_expression(2, "missing_shift_z")
            .unwrap();
        let primitive_id = structure.add_primitive(property_id, primitive).unwrap();

        let errors = structure.validate_updates();
        let error_texts = errors
            .iter()
            .map(|error| match error {
                StructureValidationError::Primitive(error) => {
                    assert_eq!(error.primitive_id(), Some(primitive_id));
                    std::error::Error::source(error).unwrap().to_string()
                }
                _ => panic!("user-defined update failures must be primitive errors"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            error_texts,
            vec![
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2",
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0",
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 2",
                "parameter evaluation failed: expression parse failed: Unknown identifier at byte 0",
            ]
        );
    }

    #[test]
    fn primitive_bound_validation_reports_unresolved_geometry_parameters() {
        let mut structure = Structure::new();
        let property = structure.add_property(Property::new("box", PropertyType::Metal));
        let primitive = structure
            .add_primitive(
                property,
                Primitive::box_from_expressions(["0", "0", "0"], ["width", "1", "1"]).unwrap(),
            )
            .unwrap();

        let error = structure.validate_primitive_bounds().unwrap_err();
        assert_eq!(error.primitive_id(), Some(primitive));
        assert!(std::error::Error::source(&error).is_some());

        structure.parameters_mut().add("width", 2.0);
        assert!(structure.validate_primitive_bounds().is_ok());
    }

    #[test]
    fn primitive_bound_validation_evaluates_wire_radius() {
        let mut structure = Structure::new();
        let property = structure.add_property(Property::new("wire", PropertyType::Metal));
        let mut wire = Primitive::curve_from_expressions(Some("wire_radius")).unwrap();
        let shape = wire.wire_shape_mut().unwrap();
        shape.add_point_from_expressions(["0", "0", "0"]).unwrap();
        shape.add_point_from_expressions(["1", "0", "0"]).unwrap();
        let wire_id = structure.add_primitive(property, wire).unwrap();

        let error = structure.validate_primitive_bounds().unwrap_err();
        assert_eq!(error.primitive_id(), Some(wire_id));
        assert!(std::error::Error::source(&error).is_some());

        structure.parameters_mut().add("wire_radius", 0.25);
        assert!(structure.validate_primitive_bounds().is_ok());
    }

    #[test]
    fn primitive_bound_validation_evaluates_rotational_polygon_angles() {
        let mut structure = Structure::new();
        let property = structure.add_property(Property::new("rotpoly", PropertyType::Metal));
        let mut rotational = Primitive::rot_poly();
        let shape = rotational.polygon_shape_mut().unwrap();
        shape
            .set_rotation_angle_expression(0, "start_angle")
            .unwrap();
        shape
            .set_rotation_angle_expression(1, "stop_angle")
            .unwrap();
        for vertex in [[0.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]] {
            shape.add_vertex(vertex);
        }
        let primitive_id = structure.add_primitive(property, rotational).unwrap();

        let error = structure.validate_primitive_bounds().unwrap_err();
        assert_eq!(error.primitive_id(), Some(primitive_id));
        assert!(std::error::Error::source(&error).is_some());

        structure.parameters_mut().add("start_angle", 0.0);
        let error = structure.validate_primitive_bounds().unwrap_err();
        assert_eq!(error.primitive_id(), Some(primitive_id));

        structure
            .parameters_mut()
            .add("stop_angle", std::f64::consts::PI);
        assert!(structure.validate_primitive_bounds().is_ok());
    }

    #[test]
    fn primitive_bound_validation_evaluates_shell_widths_and_extrusion_length() {
        let mut structure = Structure::new();
        let property = structure.add_property(Property::new("geometry", PropertyType::Metal));
        let spherical_shell_id = structure
            .add_primitive(
                property,
                Primitive::spherical_shell_from_expressions(
                    ["0", "0", "0"],
                    "1",
                    "spherical_width",
                )
                .unwrap(),
            )
            .unwrap();
        let cylindrical_shell_id = structure
            .add_primitive(
                property,
                Primitive::cylindrical_shell_from_expressions(
                    ["0", "0", "0"],
                    ["0", "0", "1"],
                    "1",
                    "cylindrical_width",
                )
                .unwrap(),
            )
            .unwrap();
        let extrusion_id = structure
            .add_primitive(
                property,
                Primitive::lin_poly_from_expressions("0", "extrusion_length").unwrap(),
            )
            .unwrap();

        assert_eq!(
            structure
                .validate_primitive_bounds()
                .unwrap_err()
                .primitive_id(),
            Some(spherical_shell_id)
        );

        structure.parameters_mut().add("spherical_width", 0.25);
        assert_eq!(
            structure
                .validate_primitive_bounds()
                .unwrap_err()
                .primitive_id(),
            Some(cylindrical_shell_id)
        );

        structure.parameters_mut().add("cylindrical_width", 0.25);
        assert_eq!(
            structure
                .validate_primitive_bounds()
                .unwrap_err()
                .primitive_id(),
            Some(extrusion_id)
        );

        structure.parameters_mut().add("extrusion_length", 2.0);
        assert!(structure.validate_primitive_bounds().is_ok());
    }

    #[test]
    fn primitive_bound_validation_checks_user_defined_function_and_shifts() {
        let mut structure = Structure::new();
        let property = structure.add_property(Property::new("user", PropertyType::Material));
        let mut primitive = Primitive::user_defined();
        primitive
            .user_defined_shape_mut()
            .unwrap()
            .set_function("x + y + z");
        structure.add_primitive(property, primitive).unwrap();
        assert!(structure.validate_primitive_bounds().is_ok());

        let mut invalid_function = Primitive::user_defined();
        invalid_function
            .user_defined_shape_mut()
            .unwrap()
            .set_function("undefined_variable");
        let invalid_function_id = structure.add_primitive(property, invalid_function).unwrap();
        let error = structure.validate_primitive_bounds().unwrap_err();
        assert_eq!(error.primitive_id(), Some(invalid_function_id));
        assert!(std::error::Error::source(&error).is_some());
        assert!(structure.delete_primitive(invalid_function_id));

        let mut invalid_shift = Primitive::user_defined();
        let shape = invalid_shift.user_defined_shape_mut().unwrap();
        shape.set_function("x");
        shape
            .set_coordinate_shift_expression(2, "missing_shift")
            .unwrap();
        let invalid_shift_id = structure.add_primitive(property, invalid_shift).unwrap();
        assert_eq!(
            structure
                .validate_primitive_bounds()
                .unwrap_err()
                .primitive_id(),
            Some(invalid_shift_id)
        );
        structure.parameters_mut().add("missing_shift", 0.0);
        assert!(structure.validate_primitive_bounds().is_ok());
    }

    #[test]
    fn geometry_validation_composes_prerequisite_property_and_primitive_checks() {
        fn complete_structure() -> (Structure, super::PropertyId) {
            let mut structure = Structure::new();
            structure.grid_mut().add_lines(0, &[0.0, 1.0]);
            structure.grid_mut().add_lines(1, &[0.0, 1.0]);
            structure.grid_mut().add_line(2, 0.0);
            let material =
                structure.add_property(Property::new("material", PropertyType::Material));
            structure
                .add_primitive(material, Primitive::box_primitive([0.0; 3], [1.0; 3]))
                .unwrap();
            let excitation =
                structure.add_property(Property::new("source", PropertyType::Excitation));
            structure
                .add_primitive(excitation, Primitive::box_primitive([0.0; 3], [1.0; 3]))
                .unwrap();
            (structure, material)
        }

        let (mut valid, _) = complete_structure();
        assert_eq!(valid.validate_geometry(), Ok(()));

        let (mut invalid_property, material_id) = complete_structure();
        invalid_property
            .property_mut(material_id)
            .unwrap()
            .material_mut()
            .unwrap()
            .set_value_expression(MaterialField::Epsilon, 0, "missing")
            .unwrap();
        assert!(matches!(
            invalid_property.validate_geometry(),
            Err(StructureValidationError::Property(error))
                if error.property_id() == Some(material_id)
        ));

        let (mut invalid_primitive, invalid_property_id) = complete_structure();
        let primitive_id = invalid_primitive
            .add_primitive(
                invalid_property_id,
                Primitive::box_from_expressions(["0", "0", "0"], ["missing", "1", "1"]).unwrap(),
            )
            .unwrap();
        invalid_primitive
            .property_mut(invalid_property_id)
            .unwrap()
            .material_mut()
            .unwrap()
            .set_value_expression(MaterialField::Epsilon, 0, "missing_material_value")
            .unwrap();
        assert!(matches!(
            invalid_primitive.validate_geometry(),
            Err(StructureValidationError::Primitive(error))
                if error.primitive_id() == Some(primitive_id)
        ));
    }

    #[test]
    fn clearing_structure_resets_contents_without_reusing_handles() {
        let mut structure = Structure::new();
        structure.parameters_mut().add("length", 2.0);
        structure.grid_mut().add_lines(0, &[0.0, 2.0]);
        structure.set_coordinate_system(CoordinateSystem::Cylindrical);
        structure
            .background_material_mut()
            .set_value(MaterialField::Epsilon, 4.0);
        let property = structure.add_property(Property::new("metal", PropertyType::Metal));
        let primitive = structure
            .add_primitive(property, Primitive::box_primitive([0.0; 3], [1.0; 3]))
            .unwrap();

        structure.clear();

        assert!(structure.parameters().is_empty());
        assert_eq!(structure.property_count(), 0);
        assert_eq!(structure.primitive_count(), 0);
        assert_eq!(structure.coordinate_system(), CoordinateSystem::Cartesian);
        assert_eq!(
            structure.grid().coordinate_system(),
            CoordinateSystem::Cartesian
        );
        assert_eq!(structure.grid_mut().line_count(0), 0);
        assert_eq!(
            structure.background_material(),
            &crate::material::BackgroundMaterial::default()
        );
        assert!(structure.property(property).is_none());
        assert!(structure.primitive(primitive).is_none());

        let replacement = structure.add_property(Property::new("replacement", PropertyType::Metal));
        let replacement_primitive = structure
            .add_primitive(replacement, Primitive::box_primitive([0.0; 3], [1.0; 3]))
            .unwrap();
        assert_ne!(replacement, property);
        assert_ne!(replacement_primitive, primitive);
    }

    #[test]
    fn structure_queries_property_types_and_orders_primitives_by_priority() {
        let mut structure = Structure::new();
        let metal = structure.add_property(Property::new("metal", PropertyType::Metal));
        let material = structure.add_property(Property::new("material", PropertyType::Material));
        let excitation =
            structure.add_property(Property::new("excitation", PropertyType::Excitation));

        let mut low_priority = Primitive::box_primitive([0.0; 3], [1.0; 3]);
        low_priority.set_priority(2);
        let low_priority_id = structure.add_primitive(metal, low_priority).unwrap();
        let mut high_priority = Primitive::box_primitive([1.0; 3], [2.0; 3]);
        high_priority.set_priority(8);
        let high_priority_id = structure.add_primitive(metal, high_priority).unwrap();
        let mut material_primitive = Primitive::box_primitive([2.0; 3], [3.0; 3]);
        material_primitive.set_priority(5);
        let material_primitive_id = structure
            .add_primitive(material, material_primitive)
            .unwrap();
        let excitation_primitive_id = structure
            .add_primitive(excitation, Primitive::box_primitive([3.0; 3], [4.0; 3]))
            .unwrap();

        let material_and_metal = PropertyType::Material | PropertyType::Metal;
        let combined_mask =
            material_and_metal | (PropertyType::Excitation | PropertyType::ProbeBox);
        assert_eq!(structure.property_count_by_type(PropertyType::Any), 3);
        assert_eq!(structure.property_count_by_type(material_and_metal), 2);
        assert_eq!(structure.property_count_by_type(combined_mask), 3);
        assert_eq!(
            structure
                .properties_by_type(material_and_metal)
                .map(Property::name)
                .collect::<Vec<_>>(),
            ["metal", "material"]
        );
        assert_eq!(structure.primitive_count_by_type(material_and_metal), 3);
        assert_eq!(
            structure.primitive_count_by_type(PropertyType::Excitation),
            1
        );
        assert_eq!(
            structure
                .all_primitives(false, material_and_metal)
                .iter()
                .map(|primitive| primitive.id().unwrap())
                .collect::<Vec<_>>(),
            [low_priority_id, high_priority_id, material_primitive_id]
        );
        assert_eq!(
            structure
                .all_primitives(true, material_and_metal)
                .iter()
                .map(|primitive| primitive.id().unwrap())
                .collect::<Vec<_>>(),
            [high_priority_id, material_primitive_id, low_priority_id]
        );
        assert_eq!(
            structure
                .all_primitives(false, PropertyType::Any)
                .last()
                .and_then(|primitive| primitive.id()),
            Some(excitation_primitive_id)
        );
    }

    #[test]
    fn structure_primitive_type_query_matches_native_unimplemented_stub() {
        let mut structure = Structure::new();
        let metal = structure.add_property(Property::new("metal", PropertyType::Metal));
        structure
            .add_primitive(metal, Primitive::box_primitive([0.0; 3], [1.0; 3]))
            .unwrap();
        structure
            .add_primitive(metal, Primitive::sphere([0.0; 3], 1.0))
            .unwrap();

        let boxes = structure.primitives_by_type(PrimitiveType::Box);
        assert_eq!(boxes.len(), 1);
        assert!(matches!(boxes[0].shape(), PrimitiveShape::Box(_)));

        let spheres = structure.primitives_by_type(PrimitiveType::Sphere);
        assert_eq!(spheres.len(), 1);
        assert!(matches!(spheres[0].shape(), PrimitiveShape::Sphere(_)));
    }

    #[test]
    fn structure_selects_primitives_intersecting_bounds_and_keeps_unknown_bounds() {
        let mut structure = Structure::new();
        let metal = structure.add_property(Property::new("metal", PropertyType::Metal));
        let material = structure.add_property(Property::new("material", PropertyType::Material));
        let intersecting = structure
            .add_primitive(metal, Primitive::box_primitive([1.0; 3], [2.0; 3]))
            .unwrap();
        structure
            .add_primitive(metal, Primitive::box_primitive([5.0; 3], [6.0; 3]))
            .unwrap();
        let mut transformed = Primitive::box_primitive([10.0; 3], [11.0; 3]);
        transformed
            .transform_mut()
            .translate([20.0, 0.0, 0.0], true);
        let transformed = structure.add_primitive(metal, transformed).unwrap();
        let unbounded = structure
            .add_primitive(material, Primitive::user_defined())
            .unwrap();
        structure
            .add_primitive(material, Primitive::box_primitive([3.0; 3], [4.0; 3]))
            .unwrap();

        assert_eq!(
            structure
                .primitives_by_bounds([0.0, 2.0, 0.0, 2.0, 0.0, 2.0], false, PropertyType::Any,)
                .unwrap()
                .iter()
                .map(|primitive| primitive.id().unwrap())
                .collect::<Vec<_>>(),
            [intersecting, transformed, unbounded]
        );
        assert_eq!(
            structure
                .primitives_by_bounds([0.0, 2.0, 0.0, 2.0, 0.0, 2.0], false, PropertyType::Metal,)
                .unwrap()
                .iter()
                .map(|primitive| primitive.id().unwrap())
                .collect::<Vec<_>>(),
            [intersecting, transformed]
        );
    }

    #[test]
    fn structure_inserts_finite_primitive_edges_into_grid() {
        let mut structure = Structure::new();
        let metal = structure.add_property(Property::new("metal", PropertyType::Metal));
        structure.grid_mut().add_lines(0, &[0.0, 8.0]);
        structure
            .add_primitive(
                metal,
                Primitive::box_primitive([-2.0, -1.0, 0.0], [1.0, 4.0, 5.0]),
            )
            .unwrap();
        structure
            .add_primitive(
                metal,
                Primitive::box_primitive([1.0, 0.0, 2.0], [3.0, 8.0, 7.0]),
            )
            .unwrap();
        structure
            .add_primitive(metal, Primitive::user_defined())
            .unwrap();

        assert!(structure.insert_edges_to_grid(0).unwrap());
        assert_eq!(
            structure.grid_mut().lines(0).unwrap(),
            [-2.0, 0.0, 1.0, 3.0, 8.0]
        );
        assert!(structure.insert_edges_to_grid(1).unwrap());
        assert_eq!(
            structure.grid_mut().lines(1).unwrap(),
            [-1.0, 0.0, 4.0, 8.0]
        );
        assert!(!structure.insert_edges_to_grid(3).unwrap());
    }

    #[test]
    fn structure_selects_highest_priority_containing_primitive() {
        let mut structure = Structure::new();
        let metal = structure.add_property(Property::new("metal", PropertyType::Metal));
        let material = structure.add_property(Property::new("material", PropertyType::Material));
        let excitation =
            structure.add_property(Property::new("excitation", PropertyType::Excitation));

        let mut metal_box = Primitive::box_primitive([0.0; 3], [2.0; 3]);
        metal_box.set_priority(5);
        let metal_primitive = structure.add_primitive(metal, metal_box).unwrap();
        let mut material_box = Primitive::box_primitive([0.5; 3], [2.5; 3]);
        material_box.set_priority(5);
        let material_primitive = structure.add_primitive(material, material_box).unwrap();
        let mut excitation_box = Primitive::box_primitive([0.0; 3], [3.0; 3]);
        excitation_box.set_priority(9);
        let excitation_primitive = structure.add_primitive(excitation, excitation_box).unwrap();

        assert_eq!(
            structure
                .property_by_coord_priority(
                    [1.0; 3],
                    PropertyType::Metal | PropertyType::Material,
                    true,
                )
                .unwrap(),
            Some((metal, metal_primitive))
        );
        assert!(structure.primitive(metal_primitive).unwrap().is_used());
        assert!(!structure.primitive(material_primitive).unwrap().is_used());
        assert_eq!(
            structure
                .property_by_coord_priority([1.0; 3], PropertyType::Material, false)
                .unwrap(),
            Some((material, material_primitive))
        );
        assert_eq!(
            structure
                .property_by_coord_priority([1.0; 3], PropertyType::Any, false)
                .unwrap(),
            Some((excitation, excitation_primitive))
        );
        assert_eq!(
            structure
                .property_by_coord_priority([4.0; 3], PropertyType::Any, false)
                .unwrap(),
            None
        );
        assert_eq!(
            structure
                .properties_by_coords_priority(&[[1.0; 3], [4.0; 3]], PropertyType::Any, false,)
                .unwrap(),
            [Some((excitation, excitation_primitive)), None]
        );
        assert_eq!(
            structure
                .unused_primitives(PropertyType::Any)
                .iter()
                .map(|primitive| primitive.id().unwrap())
                .collect::<Vec<_>>(),
            [material_primitive, excitation_primitive]
        );

        let mut unregistered = Structure::new();
        let unregistered_property =
            unregistered.add_property(Property::new("unregistered", PropertyType::Metal));
        unregistered
            .property_mut(unregistered_property)
            .unwrap()
            .add_primitive(Primitive::box_primitive([0.0; 3], [2.0; 3]));
        assert_eq!(
            unregistered.property_by_coord_priority([1.0; 3], PropertyType::Any, false),
            Err(ModelError::PrimitiveIdNotAssigned)
        );
    }

    #[test]
    fn structure_bounds_union_filtered_finite_primitives() {
        let mut structure = Structure::new();
        let metal = structure.add_property(Property::new("metal", PropertyType::Metal));
        let material = structure.add_property(Property::new("material", PropertyType::Material));
        let excitation =
            structure.add_property(Property::new("excitation", PropertyType::Excitation));

        structure
            .add_primitive(
                metal,
                Primitive::box_primitive([-2.0, -1.0, -4.0], [1.0, 2.0, 0.0]),
            )
            .unwrap();
        structure
            .add_primitive(
                material,
                Primitive::box_primitive([3.0, -3.0, -1.0], [5.0, 4.0, 6.0]),
            )
            .unwrap();
        structure
            .add_primitive(
                excitation,
                Primitive::box_primitive([-100.0; 3], [100.0; 3]),
            )
            .unwrap();
        structure
            .add_primitive(metal, Primitive::user_defined())
            .unwrap();

        assert_eq!(
            structure
                .object_bounds(PropertyType::Metal | PropertyType::Material)
                .unwrap(),
            Some([-2.0, 5.0, -3.0, 4.0, -4.0, 6.0])
        );
        assert_eq!(
            structure.object_bounds(PropertyType::Excitation).unwrap(),
            Some([-100.0, 100.0, -100.0, 100.0, -100.0, 100.0])
        );
        assert_eq!(
            structure.object_bounds(PropertyType::Any).unwrap(),
            Some([-100.0, 100.0, -100.0, 100.0, -100.0, 100.0])
        );
    }

    #[test]
    fn replacing_a_property_transfers_its_primitives() {
        let mut structure = Structure::new();
        let old = structure.add_property(Property::new("old", PropertyType::Metal));
        let primitive = structure
            .add_primitive(old, Primitive::box_primitive([0.0; 3], [1.0; 3]))
            .unwrap();
        let mut replacement = Property::new("new", PropertyType::Material);
        replacement.add_primitive(Primitive::box_primitive([2.0; 3], [3.0; 3]));
        assert!(structure.replace_property(old, replacement));
        let replacement = structure.property_by_index(0).unwrap();
        assert_eq!(replacement.name(), "new");
        assert_eq!(replacement.primitive_count(), 2);
        assert_eq!(
            replacement.primitive(0).unwrap().property_id(),
            replacement.id()
        );
        assert_eq!(
            structure.primitive(primitive).unwrap().id(),
            Some(primitive)
        );
    }

    #[test]
    fn property_copy_matches_native_primitive_copy_options() {
        let mut structure = Structure::new();
        let mut property = Property::new("copied", PropertyType::Material);
        property.set_visible(false);
        property.set_fill_color(Some(PropertyColor::new(1, 2, 3, 4)));
        property.set_attribute("origin", "source");
        property
            .material_mut()
            .unwrap()
            .set_value(MaterialField::Epsilon, 0, 4.0);
        let property_id = structure.add_property(property);
        structure
            .add_primitive(property_id, Primitive::box_primitive([0.0; 3], [1.0; 3]))
            .unwrap();

        let source = structure.property(property_id).unwrap();
        let copy = source.get_copy();
        assert_eq!(copy.id(), source.id());
        assert_eq!(copy.name(), source.name());
        assert_eq!(copy.property_type(), PropertyType::Material);
        assert!(!copy.is_visible());
        assert_eq!(copy.fill_color(), source.fill_color());
        assert_eq!(copy.attribute("origin"), "source");
        assert_eq!(
            copy.material()
                .unwrap()
                .value(MaterialField::Epsilon, 0, None)
                .unwrap(),
            Some(4.0)
        );
        assert_eq!(copy.primitive_count(), 0);

        let copy_with_primitives = source.get_copy_with_primitives();
        assert_eq!(copy_with_primitives.primitive_count(), 1);
        assert_eq!(copy_with_primitives.primitive(0), source.primitive(0));

        let mut copied_structure = Structure::new();
        let copied_id = copied_structure.add_property(copy_with_primitives);
        let copied_property = copied_structure.property(copied_id).unwrap();
        assert_eq!(
            copied_property.primitive(0).unwrap().property_id(),
            Some(copied_id)
        );
        let copied_primitive_id = copied_property.primitive(0).unwrap().id().unwrap();
        assert_eq!(
            copied_structure
                .primitive(copied_primitive_id)
                .unwrap()
                .id(),
            Some(copied_primitive_id)
        );
    }

    #[test]
    fn primitive_transfer_and_deletion_keep_membership_consistent() {
        let mut structure = Structure::new();
        let source = structure.add_property(Property::new("source", PropertyType::Metal));
        let target = structure.add_property(Property::new("target", PropertyType::Material));
        let primitive = structure
            .add_primitive(source, Primitive::box_primitive([0.0; 3], [1.0; 3]))
            .unwrap();
        assert_eq!(
            structure.property_for_primitive(primitive).unwrap().id(),
            Some(source)
        );

        structure.move_primitive(primitive, target).unwrap();
        assert_eq!(structure.property(source).unwrap().primitive_count(), 0);
        assert_eq!(structure.property(target).unwrap().primitive_count(), 1);
        assert_eq!(
            structure.property_for_primitive(primitive).unwrap().id(),
            Some(target)
        );
        assert_eq!(
            structure.primitive(primitive).unwrap().property_id(),
            Some(target)
        );
        assert!(structure.delete_primitive(primitive));
        assert!(structure.property_for_primitive(primitive).is_none());
        assert_eq!(structure.primitive_count(), 0);
        assert!(!structure.delete_primitive(primitive));
    }

    #[test]
    fn missing_owner_and_primitive_are_reported() {
        let mut structure = Structure::new();
        let result = structure.add_primitive(
            super::PropertyId(99),
            Primitive::box_primitive([0.0; 3], [1.0; 3]),
        );
        assert_eq!(
            result,
            Err(ModelError::PropertyNotFound(super::PropertyId(99)))
        );
        assert_eq!(
            structure.move_primitive(super::PrimitiveId(12), super::PropertyId(99)),
            Err(ModelError::PropertyNotFound(super::PropertyId(99)))
        );
    }

    #[test]
    fn box_uses_inclusive_bounds_and_supports_parameter_expressions() {
        let mut structure = Structure::new();
        structure.parameters_mut().add("length", 2.0);
        let property = structure.add_property(Property::new("box", PropertyType::Metal));
        let primitive =
            Primitive::box_from_expressions(["0", "0", "0"], ["length", "1", "1"]).unwrap();
        let primitive_id = structure.add_primitive(property, primitive).unwrap();
        let primitive = structure.primitive(primitive_id).unwrap();
        assert_eq!(
            primitive
                .bounding_box(Some(structure.parameters()), false)
                .unwrap(),
            [0.0, 2.0, 0.0, 1.0, 0.0, 1.0]
        );
        assert!(primitive
            .is_inside([2.0, 1.0, 1.0], Some(structure.parameters()))
            .unwrap());
        assert!(!primitive
            .is_inside([2.01, 1.0, 1.0], Some(structure.parameters()))
            .unwrap());
    }

    #[test]
    fn box_bounds_can_preserve_input_orientation_and_transform_inside_checks() {
        let primitive = Primitive::box_primitive([1.0, 4.0, 0.0], [-1.0, 2.0, 3.0]);
        let shape = primitive.box_shape().unwrap();
        assert_eq!(
            shape.bounding_box(None, false).unwrap(),
            [-1.0, 1.0, 2.0, 4.0, 0.0, 3.0]
        );
        assert_eq!(
            shape.bounding_box(None, true).unwrap(),
            [1.0, -1.0, 4.0, 2.0, 0.0, 3.0]
        );
        assert_eq!(shape.coordinate(6, None).unwrap(), None);

        let mut transformed = Primitive::box_primitive([0.0; 3], [1.0; 3]);
        transformed.transform_mut().translate([2.0, 0.0, 0.0], true);
        assert!(transformed.is_inside([2.5, 0.5, 0.5], None).unwrap());
        assert!(!transformed.is_inside([1.5, 0.5, 0.5], None).unwrap());
    }

    #[test]
    fn sphere_uses_parameterized_center_and_radius_with_native_boundary_semantics() {
        let mut structure = Structure::new();
        structure.parameters_mut().add("offset", 2.0);
        structure.parameters_mut().add("radius", 3.0);
        let property = structure.add_property(Property::new("sphere", PropertyType::Metal));
        let primitive = Primitive::sphere_from_expressions(["offset", "0", "0"], "radius").unwrap();
        assert_eq!(
            primitive.sphere_shape().unwrap().center_expression(0),
            Some("offset")
        );
        assert_eq!(primitive.sphere_shape().unwrap().center_expression(1), None);
        assert_eq!(
            primitive.sphere_shape().unwrap().radius_expression(),
            Some("radius")
        );
        let primitive_id = structure.add_primitive(property, primitive).unwrap();
        let primitive = structure.primitive(primitive_id).unwrap();
        assert_eq!(
            primitive
                .sphere_shape()
                .unwrap()
                .center_coordinate(0, Some(structure.parameters()))
                .unwrap(),
            Some(2.0)
        );

        assert_eq!(
            primitive
                .bounding_box(Some(structure.parameters()), false)
                .unwrap(),
            [-1.0, 5.0, -3.0, 3.0, -3.0, 3.0]
        );
        assert_eq!(
            primitive.dimension(Some(structure.parameters())).unwrap(),
            3
        );
        assert!(primitive
            .is_inside([2.0, 0.0, 0.0], Some(structure.parameters()))
            .unwrap());
        // Native: the surface counts as inside.
        assert!(primitive
            .is_inside([5.0, 0.0, 0.0], Some(structure.parameters()))
            .unwrap());

        let mut transformed = Primitive::sphere([0.0; 3], 1.0);
        transformed.transform_mut().translate([2.0, 0.0, 0.0], true);
        assert!(transformed.is_inside([2.0, 0.0, 0.0], None).unwrap());
        // The translated sphere's surface is inside too.
        assert!(transformed.is_inside([3.0, 0.0, 0.0], None).unwrap());
    }

    #[test]
    fn sphere_zero_and_negative_radii_match_native_dimension_and_bounds() {
        let sphere = Primitive::sphere([1.0, 2.0, 3.0], 0.0);
        assert_eq!(sphere.dimension(None).unwrap(), 0);
        assert_eq!(
            sphere.bounding_box(None, true).unwrap(),
            [1.0, 1.0, 2.0, 2.0, 3.0, 3.0]
        );
        // A zero-radius sphere is its centre, and the surface is inside.
        assert!(sphere.is_inside([1.0, 2.0, 3.0], None).unwrap());

        let sphere = Primitive::sphere([1.0, 2.0, 3.0], -2.0);
        assert_eq!(sphere.dimension(None).unwrap(), 0);
        assert_eq!(
            sphere.bounding_box(None, false).unwrap(),
            [3.0, -1.0, 4.0, 0.0, 5.0, 1.0]
        );
        assert!(!sphere.is_inside([1.0, 2.0, 3.0], None).unwrap());
    }

    #[test]
    fn spherical_shell_matches_native_width_bounds_and_strict_containment() {
        let shell = Primitive::spherical_shell([0.0; 3], 1.0, 0.5);
        assert_eq!(
            shell.bounding_box(None, false).unwrap(),
            [-1.25, 1.25, -1.25, 1.25, -1.25, 1.25]
        );
        assert_eq!(shell.dimension(None).unwrap(), 3);
        assert!(shell.is_inside([1.0, 0.0, 0.0], None).unwrap());
        // Native: the inner and outer surfaces count as inside.
        assert!(shell.is_inside([0.75, 0.0, 0.0], None).unwrap());
        assert!(shell.is_inside([1.25, 0.0, 0.0], None).unwrap());

        let mut parameters = crate::parameter::ParameterSet::new();
        parameters.add("width", 0.5);
        let shell =
            Primitive::spherical_shell_from_expressions(["0", "0", "0"], "1", "width").unwrap();
        assert_eq!(
            shell
                .spherical_shell_shape()
                .unwrap()
                .shell_width(Some(&parameters))
                .unwrap(),
            0.5
        );

        let mut editable = Primitive::spherical_shell([0.0; 3], 1.0, 0.5);
        let shape = editable.spherical_shell_shape_mut().unwrap();
        assert!(shape.set_center_coordinate(0, 2.0));
        shape.set_radius(3.0);
        shape.set_shell_width(0.25);
        assert_eq!(shape.center(None).unwrap(), [2.0, 0.0, 0.0]);
        assert_eq!(shape.radius(None).unwrap(), 3.0);
        assert_eq!(shape.shell_width(None).unwrap(), 0.25);
    }

    #[test]
    fn containment_converts_between_cartesian_and_cylindrical_coordinates() {
        let mut cylindrical_box =
            Primitive::box_primitive([1.0, 0.0, 0.0], [2.0, std::f64::consts::FRAC_PI_2, 1.0]);
        cylindrical_box.set_coordinate_system(CoordinateSystem::Cylindrical);
        let bounds = cylindrical_box.bounding_box(None, false).unwrap();
        let expected = [0.0, 1.0, 0.0, 2.0, 0.0, 1.0];
        assert!(bounds
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 1e-15));
        assert!(cylindrical_box.is_inside([0.0, 1.5, 0.5], None).unwrap());
        assert!(!cylindrical_box.is_inside([0.0, -1.5, 0.5], None).unwrap());

        cylindrical_box.set_mesh_type(CoordinateSystem::Cylindrical);
        assert_eq!(
            cylindrical_box.bounding_box(None, false).unwrap(),
            [1.0, 2.0, 0.0, std::f64::consts::FRAC_PI_2, 0.0, 1.0]
        );

        let mut cylindrical_sphere =
            Primitive::sphere([1.0, std::f64::consts::FRAC_PI_2, 0.0], 0.5);
        cylindrical_sphere.set_coordinate_system(CoordinateSystem::Cylindrical);
        let bounds = cylindrical_sphere.bounding_box(None, false).unwrap();
        let expected = [-0.5, 0.5, 0.5, 1.5, -0.5, 0.5];
        assert!(bounds
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 1e-15));
        assert!(cylindrical_sphere.is_inside([0.0, 1.0, 0.0], None).unwrap());
        // (0, 1.5, 0) lies on the sphere's surface in Cartesian coordinates.
        assert!(cylindrical_sphere.is_inside([0.0, 1.5, 0.0], None).unwrap());

        let mut cylindrical_mesh_sphere = Primitive::sphere([2.0, 0.0, 1.0], 0.25);
        cylindrical_mesh_sphere.set_mesh_type(CoordinateSystem::Cylindrical);
        assert!(cylindrical_mesh_sphere
            .is_inside([2.0, 0.0, 1.0], None)
            .unwrap());
    }

    #[test]
    fn cylinder_bounds_and_containment_match_finite_cylinder_geometry() {
        let cylinder = Primitive::cylinder([0.0; 3], [0.0, 0.0, 2.0], 1.0);
        assert_eq!(
            cylinder.bounding_box(None, false).unwrap(),
            [-1.0, 1.0, -1.0, 1.0, 0.0, 2.0]
        );
        assert_eq!(cylinder.dimension(None).unwrap(), 3);
        assert!(cylinder.is_inside([1.0, 0.0, 1.0], None).unwrap());
        assert!(!cylinder.is_inside([1.01, 0.0, 1.0], None).unwrap());
        assert!(!cylinder.is_inside([0.0, 0.0, 2.01], None).unwrap());

        let oblique = Primitive::cylinder([0.0; 3], [2.0, 2.0, 0.0], 1.0);
        let bounds = oblique.bounding_box(None, false).unwrap();
        let expected = [
            -std::f64::consts::FRAC_1_SQRT_2,
            2.0 + std::f64::consts::FRAC_1_SQRT_2,
            -std::f64::consts::FRAC_1_SQRT_2,
            2.0 + std::f64::consts::FRAC_1_SQRT_2,
            -1.0,
            1.0,
        ];
        assert!(bounds
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 1e-15));

        let mut transformed = Primitive::cylinder([0.0; 3], [0.0, 0.0, 2.0], 0.5);
        transformed.transform_mut().translate([2.0, 0.0, 0.0], true);
        assert!(transformed.is_inside([2.0, 0.0, 1.0], None).unwrap());
        assert!(!transformed.is_inside([0.0, 0.0, 1.0], None).unwrap());
    }

    #[test]
    fn zero_height_cylinders_and_shells_match_native_point_bounds_and_containment() {
        let cylinder = Primitive::cylinder([1.0, 2.0, 3.0], [1.0, 2.0, 3.0], 1.0);
        assert_eq!(
            cylinder.bounding_box(None, false).unwrap(),
            [1.0, 1.0, 2.0, 2.0, 3.0, 3.0]
        );
        assert_eq!(cylinder.dimension(None).unwrap(), 3);
        assert!(cylinder.is_inside([1.0, 2.0, 3.0], None).unwrap());
        assert!(!cylinder.is_inside([1.1, 2.0, 3.0], None).unwrap());

        let shell = Primitive::cylindrical_shell([1.0, 2.0, 3.0], [1.0, 2.0, 3.0], 1.0, 0.5);
        assert_eq!(
            shell.bounding_box(None, false).unwrap(),
            [1.0, 1.0, 2.0, 2.0, 3.0, 3.0]
        );
        assert!(shell.is_inside([1.0, 2.0, 3.0], None).unwrap());
        assert!(!shell.is_inside([1.1, 2.0, 3.0], None).unwrap());
    }

    #[test]
    fn cylindrical_shell_matches_native_bounds_and_inclusive_containment() {
        let shell = Primitive::cylindrical_shell([0.0; 3], [0.0, 0.0, 2.0], 1.0, 0.25);
        assert_eq!(
            shell.bounding_box(None, false).unwrap(),
            [-1.125, 1.125, -1.125, 1.125, 0.0, 2.0]
        );
        assert_eq!(shell.primitive_type(), PrimitiveType::CylindricalShell);
        assert_eq!(shell.dimension(None).unwrap(), 3);
        assert!(shell.is_inside([0.875, 0.0, 1.0], None).unwrap());
        assert!(shell.is_inside([1.125, 0.0, 1.0], None).unwrap());
        assert!(!shell.is_inside([0.75, 0.0, 1.0], None).unwrap());
        assert!(!shell.is_inside([1.25, 0.0, 1.0], None).unwrap());
    }

    #[test]
    fn curves_and_wires_match_native_bounds_dimension_and_containment() {
        let mut curve = Primitive::curve();
        {
            let shape = curve.curve_shape_mut().unwrap();
            shape.add_point([0.0, 1.0, 2.0]);
            shape.add_point([3.0, -1.0, 4.0]);
        }
        assert_eq!(
            curve.bounding_box(None, false).unwrap(),
            [0.0, 3.0, -1.0, 1.0, 2.0, 4.0]
        );
        assert_eq!(curve.dimension(None).unwrap(), 1);
        assert!(!curve.is_inside([1.5, 0.0, 3.0], None).unwrap());

        let mut wire = Primitive::wire(0.25);
        {
            let shape = wire.wire_shape_mut().unwrap();
            shape.add_point([0.0, 0.0, 0.0]);
            shape.add_point([1.0, 0.0, 0.0]);
        }
        assert_eq!(
            wire.bounding_box(None, false).unwrap(),
            [-0.25, 1.25, -0.25, 0.25, -0.25, 0.25]
        );
        assert_eq!(wire.dimension(None).unwrap(), 3);
        assert!(wire.is_inside([0.5, 0.2, 0.0], None).unwrap());
        assert!(!wire.is_inside([0.5, 0.25, 0.0], None).unwrap());
    }

    #[test]
    fn wire_geometry_evaluates_parameterized_points_and_radius() {
        let mut parameters = crate::parameter::ParameterSet::new();
        parameters.add("wire_radius", 0.5);
        parameters.add("wire_length", 2.0);
        let mut wire = Primitive::curve_from_expressions(Some("wire_radius")).unwrap();
        {
            let shape = wire.wire_shape_mut().unwrap();
            shape.add_point_from_expressions(["0", "0", "0"]).unwrap();
            shape
                .add_point_from_expressions(["wire_length", "0", "0"])
                .unwrap();
        }
        assert_eq!(
            wire.bounding_box(Some(&parameters), false).unwrap(),
            [-0.5, 2.5, -0.5, 0.5, -0.5, 0.5]
        );
        assert!(wire.is_inside([1.0, 0.25, 0.0], Some(&parameters)).unwrap());
    }

    #[test]
    fn polygon_bounds_and_inside_checks_match_native_axis_and_edge_behavior() {
        let mut polygon = Primitive::polygon();
        {
            let shape = polygon.polygon_shape_mut().unwrap();
            assert!(shape.set_normal_direction(2));
            shape.set_elevation(1.0);
            for vertex in [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]] {
                shape.add_vertex(vertex);
            }
        }
        assert_eq!(
            polygon.bounding_box(None, false).unwrap(),
            [0.0, 2.0, 0.0, 2.0, 1.0, 1.0]
        );
        assert_eq!(polygon.dimension(None).unwrap(), 2);
        assert!(polygon.is_inside([1.0, 1.0, 1.0], None).unwrap());
        assert!(polygon.is_inside([1.0, 0.0, 1.0], None).unwrap());
        assert!(!polygon.is_inside([2.1, 1.0, 1.0], None).unwrap());
        assert!(!polygon.is_inside([1.0, 1.0, 1.1], None).unwrap());
    }

    #[test]
    fn polygon_expressions_use_current_parameters_and_reject_invalid_axes() {
        let mut parameters = crate::parameter::ParameterSet::new();
        parameters.add("edge", 2.0);
        parameters.add("z", 3.0);
        let mut polygon = Primitive::polygon_from_expressions("z").unwrap();
        let shape = polygon.polygon_shape_mut().unwrap();
        assert!(shape.set_normal_direction(2));
        assert!(!shape.set_normal_direction(3));
        for vertex in [["0", "0"], ["edge", "0"], ["edge", "edge"], ["0", "edge"]] {
            shape.add_vertex_from_expressions(vertex).unwrap();
        }
        assert_eq!(
            polygon.bounding_box(Some(&parameters), false).unwrap(),
            [0.0, 2.0, 0.0, 2.0, 3.0, 3.0]
        );
        assert!(polygon
            .is_inside([1.0, 1.0, 3.0], Some(&parameters))
            .unwrap());
    }

    #[test]
    fn linear_polygon_extrusion_supports_positive_and_negative_lengths() {
        let mut positive = Primitive::lin_poly(2.0);
        {
            let shape = positive.polygon_shape_mut().unwrap();
            assert!(shape.set_normal_direction(2));
            shape.set_elevation(1.0);
            for vertex in [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]] {
                shape.add_vertex(vertex);
            }
        }
        assert_eq!(positive.primitive_type(), PrimitiveType::LinPoly);
        assert_eq!(
            positive.bounding_box(None, false).unwrap(),
            [0.0, 2.0, 0.0, 2.0, 1.0, 3.0]
        );
        assert_eq!(positive.dimension(None).unwrap(), 3);
        assert!(positive.is_inside([1.0, 1.0, 2.0], None).unwrap());
        assert!(!positive.is_inside([1.0, 1.0, 3.1], None).unwrap());

        let mut negative = Primitive::lin_poly(-2.0);
        {
            let shape = negative.polygon_shape_mut().unwrap();
            assert!(shape.set_normal_direction(2));
            shape.set_elevation(1.0);
            for vertex in [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]] {
                shape.add_vertex(vertex);
            }
        }
        assert_eq!(
            negative.bounding_box(None, false).unwrap(),
            [0.0, 2.0, 0.0, 2.0, -1.0, 1.0]
        );
        assert!(negative.is_inside([1.0, 1.0, 0.0], None).unwrap());
    }

    #[test]
    fn closed_polyhedron_bounds_and_ray_containment_match_solid_semantics() {
        let mut polyhedron = Primitive::polyhedron();
        {
            let shape = polyhedron.polyhedron_shape_mut().unwrap();
            for vertex in [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ] {
                shape.add_vertex(vertex);
            }
            for face in [
                vec![0, 1, 2, 3],
                vec![4, 7, 6, 5],
                vec![0, 4, 5, 1],
                vec![3, 2, 6, 7],
                vec![0, 3, 7, 4],
                vec![1, 5, 6, 2],
            ] {
                shape.add_face(face);
            }
        }
        assert_eq!(
            polyhedron.bounding_box(None, false).unwrap(),
            [0.0, 1.0, 0.0, 1.0, 0.0, 1.0]
        );
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(polyhedron.is_inside([0.5, 0.5, 0.5], None).unwrap());
        assert!(!polyhedron.is_inside([1.1, 0.3, 0.4], None).unwrap());
        assert!(polyhedron.is_inside([0.0, 0.5, 0.5], None).unwrap());
        assert!(polyhedron.is_inside([0.0, 0.0, 0.5], None).unwrap());
        assert!(polyhedron.is_inside([0.0, 0.0, 0.0], None).unwrap());
        assert!(polyhedron.is_inside([1e-10, 0.5, 0.5], None).unwrap());
        assert!(!polyhedron.is_inside([-1e-10, 0.5, 0.5], None).unwrap());

        polyhedron
            .polyhedron_shape_mut()
            .unwrap()
            .add_face(vec![0, 1, 99]);
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(polyhedron.polyhedron_shape().unwrap().face(6).is_some());
    }

    #[test]
    fn polyhedron_face_with_repeated_vertex_matches_native_invalid_face_dimension() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for vertex in [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            shape.add_vertex(vertex);
        }
        shape.add_face(vec![0, 1, 2, 0]);

        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(!polyhedron.is_inside([0.2, 0.2, 0.0], None).unwrap());
    }

    #[test]
    fn polyhedron_degenerate_faces_match_native_cgal_rejection() {
        for vertex_count in 0..=3 {
            let mut polyhedron = Primitive::polyhedron();
            let shape = polyhedron.polyhedron_shape_mut().unwrap();
            for vertex in [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
                shape.add_vertex(vertex);
            }
            shape.add_face((0..vertex_count).collect());

            assert_eq!(
                polyhedron.dimension(None).unwrap(),
                if vertex_count < 3 { 3 } else { 2 }
            );
            assert!(!polyhedron.is_inside([0.1, 0.1, 0.0], None).unwrap());
        }
    }

    #[test]
    fn polyhedron_duplicate_facets_match_native_cgal_acceptance() {
        for faces in [
            [vec![0, 1, 2], vec![0, 1, 2]],
            [vec![0, 1, 2], vec![0, 2, 1]],
        ] {
            let mut polyhedron = Primitive::polyhedron();
            let shape = polyhedron.polyhedron_shape_mut().unwrap();
            for vertex in [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
                shape.add_vertex(vertex);
            }
            for face in faces {
                shape.add_face(face);
            }

            assert_eq!(polyhedron.dimension(None).unwrap(), 3);
            assert!(!polyhedron.is_inside([0.1, 0.1, -0.1], None).unwrap());
            // Native: the face itself counts as inside.
            assert!(polyhedron.is_inside([0.1, 0.1, 0.0], None).unwrap());
            assert!(!polyhedron.is_inside([0.1, 0.1, 0.1], None).unwrap());
        }
    }

    #[test]
    fn polyhedron_disconnected_closed_components_match_native_cgal() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for offset in [[0.0, 0.0, 0.0], [3.0, 0.0, 0.0]] {
            for vertex in [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ] {
                shape.add_vertex([
                    vertex[0] + offset[0],
                    vertex[1] + offset[1],
                    vertex[2] + offset[2],
                ]);
            }
        }
        for base in [0, 4] {
            for face in [
                vec![base, base + 2, base + 1],
                vec![base, base + 1, base + 3],
                vec![base, base + 3, base + 2],
                vec![base + 1, base + 2, base + 3],
            ] {
                shape.add_face(face);
            }
        }

        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(polyhedron.is_inside([0.1, 0.1, 0.1], None).unwrap());
        assert!(polyhedron.is_inside([3.1, 0.1, 0.1], None).unwrap());
        assert!(!polyhedron.is_inside([1.5, 0.1, 0.1], None).unwrap());
    }

    #[test]
    fn polyhedron_nested_closed_components_match_native_cgal() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for vertex in [
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 0.0, 4.0],
            [0.2, 0.2, 0.2],
            [0.8, 0.2, 0.2],
            [0.2, 0.8, 0.2],
            [0.2, 0.2, 0.8],
        ] {
            shape.add_vertex(vertex);
        }
        for base in [0, 4] {
            for face in [
                vec![base, base + 2, base + 1],
                vec![base, base + 1, base + 3],
                vec![base, base + 3, base + 2],
                vec![base + 1, base + 2, base + 3],
            ] {
                shape.add_face(face);
            }
        }

        assert_eq!(shape.accepted_faces().len(), 8);
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        // The small tetrahedron is nested in the large one: the native
        // robust test treats the union as inside (the old ray cast counted
        // two crossings and reported outside).
        assert!(polyhedron.is_inside([0.25, 0.25, 0.25], None).unwrap());
        assert!(polyhedron.is_inside([1.0, 0.5, 0.5], None).unwrap());
        assert!(!polyhedron.is_inside([4.1, 0.2, 0.2], None).unwrap());
    }

    #[test]
    fn polyhedron_overlapping_closed_components_match_native_cgal() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for offset in [[0.0, 0.0, 0.0], [0.5, 0.0, 0.0]] {
            for vertex in [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ] {
                shape.add_vertex([
                    vertex[0] + offset[0],
                    vertex[1] + offset[1],
                    vertex[2] + offset[2],
                ]);
            }
        }
        for base in [0, 4] {
            for face in [
                vec![base, base + 2, base + 1],
                vec![base, base + 1, base + 3],
                vec![base, base + 3, base + 2],
                vec![base + 1, base + 2, base + 3],
            ] {
                shape.add_face(face);
            }
        }

        assert_eq!(shape.accepted_faces().len(), 8);
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(polyhedron.is_inside([0.2, 0.1, 0.1], None).unwrap());
        // Inside both overlapping tetrahedra: the union is inside natively.
        assert!(polyhedron.is_inside([0.6, 0.1, 0.1], None).unwrap());
        assert!(polyhedron.is_inside([1.2, 0.1, 0.1], None).unwrap());
        assert!(!polyhedron.is_inside([1.6, 0.1, 0.1], None).unwrap());
    }

    #[test]
    fn polyhedron_closed_components_sharing_a_vertex_match_native_cgal() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for vertex in [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [-1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, -1.0],
        ] {
            shape.add_vertex(vertex);
        }
        for face in [
            vec![0, 2, 1],
            vec![0, 1, 3],
            vec![0, 3, 2],
            vec![1, 2, 3],
            vec![0, 4, 5],
            vec![0, 6, 4],
            vec![0, 5, 6],
            vec![4, 6, 5],
        ] {
            shape.add_face(face);
        }

        assert_eq!(shape.accepted_faces().len(), 5);
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(polyhedron.is_inside([0.1, 0.1, 0.1], None).unwrap());
        assert!(!polyhedron.is_inside([-0.1, -0.1, -0.1], None).unwrap());
    }

    #[test]
    fn polyhedron_closed_components_sharing_an_edge_match_native_cgal() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for vertex in [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, -1.0],
        ] {
            shape.add_vertex(vertex);
        }
        for face in [
            vec![0, 2, 1],
            vec![0, 1, 3],
            vec![0, 3, 2],
            vec![1, 2, 3],
            vec![0, 4, 1],
            vec![0, 1, 5],
            vec![0, 5, 4],
            vec![1, 4, 5],
        ] {
            shape.add_face(face);
        }

        assert_eq!(shape.accepted_faces().len(), 4);
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(polyhedron.is_inside([0.1, 0.1, 0.1], None).unwrap());
        assert!(!polyhedron.is_inside([0.1, -0.1, -0.1], None).unwrap());
    }

    #[test]
    fn polyhedron_closed_components_sharing_a_face_match_native_cgal() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for vertex in [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ] {
            shape.add_vertex(vertex);
        }
        for face in [
            vec![0, 2, 1],
            vec![0, 1, 3],
            vec![0, 3, 2],
            vec![1, 2, 3],
            vec![0, 1, 2],
            vec![0, 1, 4],
            vec![1, 2, 4],
            vec![2, 0, 4],
        ] {
            shape.add_face(face);
        }

        assert_eq!(shape.accepted_faces().len(), 4);
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(polyhedron.is_inside([0.1, 0.1, 0.1], None).unwrap());
        assert!(!polyhedron.is_inside([0.1, 0.1, -0.1], None).unwrap());
    }

    #[test]
    fn polyhedron_with_overfull_edge_matches_native_dimension_and_containment() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for vertex in [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ] {
            shape.add_vertex(vertex);
        }
        for face in [
            vec![0, 1, 2, 3],
            vec![4, 7, 6, 5],
            vec![0, 4, 5, 1],
            vec![3, 2, 6, 7],
            vec![0, 3, 7, 4],
            vec![1, 5, 6, 2],
        ] {
            shape.add_face(face);
        }

        assert!(polyhedron.is_inside([0.5, 0.5, 0.5], None).unwrap());
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        shape.add_vertex([1.0, 0.9, 1.0]);
        shape.add_face(vec![0, 1, 8]);

        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(polyhedron.is_inside([0.5, 0.5, 0.5], None).unwrap());
    }

    #[test]
    fn polyhedron_rejects_faces_attached_to_a_closed_vertex_star() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for vertex in [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
        ] {
            shape.add_vertex(vertex);
        }
        for face in [
            vec![0, 2, 1],
            vec![0, 1, 3],
            vec![1, 2, 3],
            vec![2, 0, 3],
            vec![0, 4, 5],
        ] {
            shape.add_face(face);
        }

        assert_eq!(shape.accepted_faces().len(), 4);
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
        assert!(polyhedron.is_inside([0.1, 0.1, 0.1], None).unwrap());
    }

    #[test]
    fn polyhedron_rejects_non_orientable_facets_like_cgal() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for vertex in [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [2.0, 4.0, 8.0],
            [3.0, 9.0, 27.0],
            [4.0, 16.0, 64.0],
            [5.0, 25.0, 125.0],
        ] {
            shape.add_vertex(vertex);
        }
        for face in [
            vec![0, 1, 2],
            vec![0, 1, 3],
            vec![0, 2, 4],
            vec![0, 3, 5],
            vec![0, 4, 5],
            vec![1, 2, 5],
            vec![1, 3, 4],
            vec![1, 4, 5],
            vec![2, 3, 4],
            vec![2, 3, 5],
        ] {
            shape.add_face(face);
        }

        assert_eq!(
            shape.accepted_faces(),
            vec![
                &[0, 1, 2][..],
                &[0, 1, 3][..],
                &[0, 2, 4][..],
                &[0, 3, 5][..],
                &[0, 4, 5][..],
                &[1, 2, 5][..],
                &[1, 3, 4][..],
            ]
        );
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
    }

    #[test]
    fn polyhedron_rejects_facets_that_join_disconnected_vertex_fans() {
        let mut polyhedron = Primitive::polyhedron();
        let shape = polyhedron.polyhedron_shape_mut().unwrap();
        for vertex in [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
        ] {
            shape.add_vertex(vertex);
        }
        for face in [vec![0, 1, 2], vec![0, 3, 4], vec![1, 0, 3]] {
            shape.add_face(face);
        }

        assert_eq!(shape.accepted_faces(), vec![&[0, 1, 2][..], &[0, 3, 4][..]]);
        assert_eq!(polyhedron.dimension(None).unwrap(), 3);
    }

    #[test]
    fn rotational_polygon_contains_only_the_requested_angular_sector() {
        let mut rotational = Primitive::rot_poly();
        {
            let shape = rotational.polygon_shape_mut().unwrap();
            assert!(shape.set_normal_direction(1));
            assert!(shape.set_rotation_axis_direction(2));
            assert!(shape.set_rotation_angle_expression(0, "0").unwrap());
            assert!(shape
                .set_rotation_angle_expression(1, "0.7853981633974483")
                .unwrap());
            for vertex in [[0.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]] {
                shape.add_vertex(vertex);
            }
        }
        assert_eq!(rotational.primitive_type(), PrimitiveType::RotPoly);
        assert_eq!(rotational.dimension(None).unwrap(), 2);
        assert!(rotational.is_inside([1.5, 0.0, 0.5], None).unwrap());
        assert!(!rotational.is_inside([0.0, 1.5, 0.5], None).unwrap());
        assert!(!rotational.is_inside([-1.5, 0.0, 0.5], None).unwrap());
    }

    #[test]
    fn cylinder_accepts_parameterized_and_cylindrical_axis_coordinates() {
        let mut parameters = crate::parameter::ParameterSet::new();
        parameters.add("height", 2.0);
        parameters.add("radius", 0.5);
        let cylinder =
            Primitive::cylinder_from_expressions(["0", "0", "0"], ["0", "0", "height"], "radius")
                .unwrap();
        assert!(cylinder
            .is_inside([0.5, 0.0, 1.0], Some(&parameters))
            .unwrap());

        let mut cylindrical = Primitive::cylinder(
            [1.0, 0.0, 0.0],
            [1.0, std::f64::consts::FRAC_PI_2, 2.0],
            0.25,
        );
        cylindrical.set_coordinate_system(CoordinateSystem::Cylindrical);
        assert!(cylindrical.is_inside([0.5, 0.5, 1.0], None).unwrap());
    }

    #[test]
    fn multibox_uses_union_bounds_and_updates_after_box_deletion() {
        let mut primitive = Primitive::multibox();
        let shape = primitive.multibox_shape_mut().unwrap();
        assert_eq!(shape.add_box(BoxPrimitive::new([0.0; 3], [1.0; 3])), 0);
        assert_eq!(
            shape
                .add_box(BoxPrimitive::from_expressions(["3", "0", "0"], ["4", "1", "1"]).unwrap()),
            1
        );
        assert_eq!(shape.box_count(), 2);
        assert_eq!(
            primitive.bounding_box(None, false).unwrap(),
            [0.0, 4.0, 0.0, 1.0, 0.0, 1.0]
        );
        assert!(primitive.is_inside([0.5, 0.5, 0.5], None).unwrap());
        assert!(primitive.is_inside([3.5, 0.5, 0.5], None).unwrap());
        assert!(!primitive.is_inside([2.0, 0.5, 0.5], None).unwrap());

        let shape = primitive.multibox_shape_mut().unwrap();
        assert!(shape.delete_box(1));
        assert!(!shape.delete_box(1));
        assert_eq!(shape.box_count(), 1);
        assert_eq!(shape.box_at(0).unwrap().dimension(None).unwrap(), 3);
        assert_eq!(
            primitive.bounding_box(None, false).unwrap(),
            [0.0, 1.0, 0.0, 1.0, 0.0, 1.0]
        );
    }

    #[test]
    fn point_is_zero_dimensional_and_never_contains_a_query_coordinate() {
        let mut parameters = crate::parameter::ParameterSet::new();
        parameters.add("height", 3.0);
        let point = Primitive::point_from_expressions(["0", "2", "height"]).unwrap();
        assert_eq!(point.dimension(Some(&parameters)).unwrap(), 0);
        assert_eq!(
            point.bounding_box(Some(&parameters), false).unwrap(),
            [0.0, 0.0, 2.0, 2.0, 3.0, 3.0]
        );
        assert!(!point.is_inside([0.0, 2.0, 3.0], Some(&parameters)).unwrap());

        let mut converted = Primitive::point([0.0, 2.0, 3.0]);
        converted.set_coordinate_system(CoordinateSystem::Cartesian);
        converted.set_mesh_type(CoordinateSystem::Cylindrical);
        let bounds = converted.bounding_box(None, false).unwrap();
        assert!((bounds[2] - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
        assert_eq!(bounds, [2.0, 2.0, bounds[2], bounds[2], 3.0, 3.0]);
        assert!(!converted
            .is_inside([2.0, std::f64::consts::FRAC_PI_2, 3.0], None)
            .unwrap());
    }

    #[test]
    fn property_attributes_preserve_order_and_coordinate_changes_propagate() {
        let mut property = Property::new("p", PropertyType::Unknown);
        assert!(!property.is_visible());
        assert_eq!(property.unknown_property(), Some(""));
        property.set_attribute("first", "1");
        property.set_attribute("second", "2");
        property.set_attribute("first", "updated");
        assert_eq!(property.attribute("first"), "updated");
        assert_eq!(
            property.attribute_names().collect::<Vec<_>>(),
            ["first", "second"]
        );
        property.remove_attribute("first");
        assert!(!property.has_attribute("first"));

        let mut structure = Structure::new();
        structure.set_coordinate_system(CoordinateSystem::Cylindrical);
        let property_id = structure.add_property(property);
        let primitive_id = structure
            .add_primitive(property_id, Primitive::box_primitive([0.0; 3], [1.0; 3]))
            .unwrap();
        assert_eq!(
            structure.primitive(primitive_id).unwrap().mesh_type(),
            CoordinateSystem::Cylindrical
        );
    }

    #[test]
    fn material_property_types_expose_material_data() {
        let mut material = Property::new("material", PropertyType::Material);
        assert_eq!(
            material
                .material()
                .unwrap()
                .value(MaterialField::Epsilon, 0, None)
                .unwrap(),
            Some(1.0)
        );
        material
            .material_mut()
            .unwrap()
            .set_value(MaterialField::Epsilon, 0, 4.0);
        assert_eq!(
            material
                .material()
                .unwrap()
                .value(MaterialField::Epsilon, 0, None)
                .unwrap(),
            Some(4.0)
        );
        assert!(Property::new("metal", PropertyType::Metal)
            .material()
            .is_none());
    }
}

#[cfg(test)]
mod polyhedron_precision_tests {
    use super::*;

    #[test]
    fn polyhedron_vertices_keep_double_precision() {
        let mut primitive = Primitive::polyhedron();
        let vertex = primitive
            .polyhedron_shape_mut()
            .expect("polyhedron")
            .add_vertex([1.0 / 3.0, 2.0e-12, 1.23456789012345]);
        let stored = primitive
            .polyhedron_shape()
            .and_then(|shape| shape.vertex(vertex))
            .expect("vertex");
        // Native `CSPrimPolyhedron` stores double vertices; a 32-bit store
        // would round 1/3 to 0.33333334 and lose the 1e-12 coordinate.
        assert_eq!(stored, [1.0 / 3.0, 2.0e-12, 1.23456789012345]);
    }
}

#[cfg(test)]
mod drawing_tolerance_tests {
    use super::*;

    #[test]
    fn a_box_obeys_the_drawing_tolerance() {
        let primitive = Primitive::box_primitive([0.0; 3], [1.0; 3]);
        assert!(!primitive
            .is_inside_with_tolerance([1.0 + 5.0e-4, 0.5, 0.5], None, 0.0)
            .unwrap());
        assert!(primitive
            .is_inside_with_tolerance([1.0 + 5.0e-4, 0.5, 0.5], None, 1.0e-3)
            .unwrap());
        // Twice the tolerance is outside again.
        assert!(!primitive
            .is_inside_with_tolerance([1.0 + 2.0e-3, 0.5, 0.5], None, 1.0e-3)
            .unwrap());
        // The surface itself counts as inside.
        assert!(primitive
            .is_inside_with_tolerance([1.0, 0.5, 0.5], None, 0.0)
            .unwrap());
    }

    #[test]
    fn spheres_shells_and_cylinders_obey_the_drawing_tolerance() {
        let sphere = Primitive::sphere([0.0; 3], 1.0);
        assert!(!sphere
            .is_inside_with_tolerance([1.0 + 5.0e-4, 0.0, 0.0], None, 0.0)
            .unwrap());
        assert!(sphere
            .is_inside_with_tolerance([1.0 + 5.0e-4, 0.0, 0.0], None, 1.0e-3)
            .unwrap());

        let shell = Primitive::spherical_shell([0.0; 3], 1.0, 0.2);
        assert!(shell
            .is_inside_with_tolerance([1.1 + 5.0e-4, 0.0, 0.0], None, 1.0e-3)
            .unwrap());
        assert!(!shell
            .is_inside_with_tolerance([1.1 + 2.0e-3, 0.0, 0.0], None, 1.0e-3)
            .unwrap());

        let cylinder = Primitive::cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        assert!(cylinder
            .is_inside_with_tolerance([1.0 + 5.0e-4, 0.0, 0.5], None, 1.0e-3)
            .unwrap());
        // The tolerance extends the axis as well.
        assert!(cylinder
            .is_inside_with_tolerance([0.0, 0.0, 1.0 + 5.0e-4], None, 1.0e-3)
            .unwrap());
        assert!(!cylinder
            .is_inside_with_tolerance([0.0, 0.0, 1.0 + 2.0e-3], None, 1.0e-3)
            .unwrap());
    }

    #[test]
    fn a_polyhedron_surface_and_vertices_are_inside() {
        let mut primitive = Primitive::polyhedron();
        let shape = primitive.polyhedron_shape_mut().expect("polyhedron");
        let corners = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        for corner in corners {
            shape.add_vertex(corner);
        }
        for face in [
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
        ] {
            shape.add_face(face);
        }
        // A point on a face and on an edge counts as inside (native robust
        // test; the ray cast alone is ambiguous there).
        assert!(primitive
            .is_inside_with_tolerance([1.0, 0.5, 0.5], None, 0.0)
            .unwrap());
        assert!(primitive
            .is_inside_with_tolerance([1.0, 0.0, 0.5], None, 0.0)
            .unwrap());
        assert!(primitive
            .is_inside_with_tolerance([1.0, 1.0, 1.0], None, 0.0)
            .unwrap());
        // Inside and outside.
        assert!(primitive
            .is_inside_with_tolerance([0.5, 0.5, 0.5], None, 0.0)
            .unwrap());
        assert!(!primitive
            .is_inside_with_tolerance([1.5, 0.5, 0.5], None, 0.0)
            .unwrap());
        // Within the tolerance of the surface.
        assert!(primitive
            .is_inside_with_tolerance([1.0 + 5.0e-4, 0.5, 0.5], None, 1.0e-3)
            .unwrap());
        assert!(!primitive
            .is_inside_with_tolerance([1.0 + 2.0e-3, 0.5, 0.5], None, 1.0e-3)
            .unwrap());
    }

    #[test]
    fn the_structure_carries_the_drawing_tolerance() {
        let mut structure = Structure::new();
        assert_eq!(structure.drawing_tolerance(), 0.0);
        structure.set_drawing_tolerance(1.0e-3);
        assert_eq!(structure.drawing_tolerance(), 1.0e-3);
        let property = structure.add_property(Property::new("m", PropertyType::Material));
        structure
            .add_primitive(
                property,
                Primitive::box_primitive([0.0; 3], [1.0; 3]),
            )
            .expect("box");
        let primitives = structure.all_primitives(false, PropertyType::Any);
        let primitive = primitives.first().expect("primitive");
        assert!(primitive
            .is_inside_with_tolerance(
                [1.0 + 5.0e-4, 0.5, 0.5],
                Some(structure.parameters()),
                structure.drawing_tolerance()
            )
            .unwrap());
    }
}
