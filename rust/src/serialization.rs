use crate::grid::{CoordinateSystem, RectGrid};
use crate::material::{BackgroundMaterial, MaterialField, MaterialProperties};
use crate::model::{
    BoxPrimitive, ModelError, PolyhedronFileType, Primitive, Property, PropertyColor, PropertyType,
    Structure, UserDefinedCoordinateSystem,
};
use crate::parameter::{Parameter, ParameterError, ParameterSet};
use crate::transform::{TransformError, TransformType};
use crate::xml::{self, XmlNode};
use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum StructureXmlError {
    Parse(String),
    MissingAttribute { node: String, attribute: String },
    InvalidValue { field: String, value: String },
    MissingChild { node: String, child: String },
    UnsupportedPropertyType(String),
    UnsupportedPrimitiveType(String),
    Parameter(ParameterError),
    Transform(TransformError),
    Model(ModelError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureXmlWarning {
    InvalidParameterSkipped {
        kind: Option<String>,
        name: Option<String>,
        reason: String,
    },
    MissingPrimitives(String),
    PropertiesMissing,
    UnsupportedPropertySkipped(String),
    UnsupportedPrimitiveSkipped(String),
    InvalidPropertyRetainedAsUnknown {
        kind: String,
        reason: String,
    },
    InvalidPrimitiveSkipped {
        kind: String,
        reason: String,
    },
    InvalidTransformSkipped {
        kind: String,
        reason: String,
    },
}

impl fmt::Display for StructureXmlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "XML parse failed: {message}"),
            Self::MissingAttribute { node, attribute } => {
                write!(formatter, "<{node}> is missing attribute {attribute:?}")
            }
            Self::InvalidValue { field, value } => {
                write!(formatter, "invalid {field} value {value:?}")
            }
            Self::MissingChild { node, child } => {
                write!(formatter, "<{node}> is missing child <{child}>")
            }
            Self::UnsupportedPropertyType(kind) => {
                write!(formatter, "unsupported CSXCAD property type {kind:?}")
            }
            Self::UnsupportedPrimitiveType(kind) => {
                write!(formatter, "unsupported CSXCAD primitive type {kind:?}")
            }
            Self::Parameter(error) => write!(formatter, "parameter value failed: {error}"),
            Self::Transform(error) => write!(formatter, "transform failed: {error}"),
            Self::Model(error) => write!(formatter, "model construction failed: {error}"),
        }
    }
}

impl Error for StructureXmlError {}

impl From<ParameterError> for StructureXmlError {
    fn from(error: ParameterError) -> Self {
        Self::Parameter(error)
    }
}

impl From<TransformError> for StructureXmlError {
    fn from(error: TransformError) -> Self {
        Self::Transform(error)
    }
}

impl From<ModelError> for StructureXmlError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

impl Structure {
    pub fn to_xml_string(&self) -> Result<String, StructureXmlError> {
        let mut root = XmlNode::new("ContinuousStructure");
        root.set_attribute("CoordSystem", (self.coordinate_system() as i32).to_string());

        let mut grid = self.grid().clone();
        root.children.push(write_grid(&mut grid));
        root.children
            .push(write_background_material(self.background_material()));
        root.children.push(write_parameters(self.parameters()));

        let mut properties = XmlNode::new("Properties");
        for index in 0..self.property_count() {
            let property = self
                .property_by_index(index)
                .expect("property index is valid");
            properties.children.push(write_property(property)?);
        }
        root.children.push(properties);

        let mut output =
            String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>");
        root.to_xml(&mut output);
        Ok(output)
    }

    pub fn from_xml_str(input: &str) -> Result<Self, StructureXmlError> {
        Self::from_xml_str_with_policy(input, false).map(|(structure, _)| structure)
    }

    pub fn from_xml_str_with_warnings(
        input: &str,
    ) -> Result<(Self, Vec<StructureXmlWarning>), StructureXmlError> {
        Self::from_xml_str_with_policy(input, true)
    }

    fn from_xml_str_with_policy(
        input: &str,
        native_compatibility: bool,
    ) -> Result<(Self, Vec<StructureXmlWarning>), StructureXmlError> {
        let document = xml::parse(input).map_err(StructureXmlError::Parse)?;
        let root = if document.name == "ContinuousStructure" {
            &document
        } else {
            document.child("ContinuousStructure").ok_or_else(|| {
                StructureXmlError::MissingChild {
                    node: document.name.clone(),
                    child: "ContinuousStructure".to_owned(),
                }
            })?
        };

        let mut structure = Self::new();
        if let Some(value) = root.attribute("CoordSystem") {
            structure.set_coordinate_system(parse_coordinate_system(value, "CoordSystem")?);
        }
        if let Some(grid) = root.child("RectilinearGrid") {
            read_grid(grid, structure.grid_mut())?;
        }
        if let Some(background) = root.child("BackgroundMaterial") {
            read_background_material(background, structure.background_material_mut())?;
        }
        let mut warnings = Vec::new();
        if let Some(parameters) = root.child("ParameterSet") {
            read_parameters(
                parameters,
                structure.parameters_mut(),
                native_compatibility,
                &mut warnings,
            )?;
        }
        let Some(properties) = root.child("Properties") else {
            if native_compatibility {
                warnings.push(StructureXmlWarning::PropertiesMissing);
                return Ok((structure, warnings));
            }
            return Err(StructureXmlError::MissingChild {
                node: "ContinuousStructure".to_owned(),
                child: "Properties".to_owned(),
            });
        };
        for property_node in &properties.children {
            let property = match read_property(
                property_node,
                structure.parameters(),
                native_compatibility,
                &mut warnings,
            ) {
                Ok(property) => property,
                Err(StructureXmlError::UnsupportedPropertyType(kind)) if native_compatibility => {
                    warnings.push(StructureXmlWarning::UnsupportedPropertySkipped(kind));
                    continue;
                }
                Err(error) if native_compatibility => {
                    warnings.push(StructureXmlWarning::InvalidPropertyRetainedAsUnknown {
                        kind: property_node.name.clone(),
                        reason: error.to_string(),
                    });
                    read_unknown_property(property_node, native_compatibility)?
                }
                Err(error) => return Err(error),
            };
            let property_id = structure.add_property(property);
            if let Some(primitives) = property_node.child("Primitives") {
                for primitive_node in &primitives.children {
                    let primitive = match read_primitive(
                        primitive_node,
                        structure.parameters(),
                        native_compatibility,
                        &mut warnings,
                    ) {
                        Ok(primitive) => primitive,
                        Err(StructureXmlError::UnsupportedPrimitiveType(kind))
                            if native_compatibility =>
                        {
                            warnings.push(StructureXmlWarning::UnsupportedPrimitiveSkipped(kind));
                            continue;
                        }
                        Err(error) if native_compatibility => {
                            warnings.push(StructureXmlWarning::InvalidPrimitiveSkipped {
                                kind: primitive_node.name.clone(),
                                reason: error.to_string(),
                            });
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    structure.add_primitive(property_id, primitive)?;
                }
            } else if native_compatibility {
                warnings.push(StructureXmlWarning::MissingPrimitives(
                    property_node.attribute("Name").unwrap_or("").to_owned(),
                ));
            }
        }
        Ok((structure, warnings))
    }
}

fn write_grid(grid: &mut RectGrid) -> XmlNode {
    let mut node = XmlNode::new("RectilinearGrid");
    node.set_attribute("DeltaUnit", grid.delta_unit().to_string());
    node.set_attribute("CoordSystem", (grid.coordinate_system() as i32).to_string());
    for (direction, name) in ["XLines", "YLines", "ZLines"].into_iter().enumerate() {
        let mut lines = XmlNode::new(name);
        let values = grid
            .lines(direction)
            .expect("valid grid direction")
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        lines.set_attribute("Qty", grid.line_count(direction).to_string());
        lines.text = values;
        node.children.push(lines);
    }
    node
}

fn write_parameters(parameters: &ParameterSet) -> XmlNode {
    let mut node = XmlNode::new("ParameterSet");
    for parameter in parameters.iter() {
        let mut child = XmlNode::new("Parameter");
        child.set_attribute(
            "Type",
            if parameter.is_linear() {
                "Linear"
            } else {
                "Const"
            },
        );
        child.set_attribute("name", parameter.name());
        child.set_attribute("Sweep", if parameter.sweep_enabled() { "1" } else { "0" });
        child.set_attribute("value", parameter.value().to_string());
        if let Some((min, max, step)) = parameter.linear_range() {
            child.set_attribute("min", min.to_string());
            child.set_attribute("max", max.to_string());
            child.set_attribute("step", step.to_string());
        }
        node.children.push(child);
    }
    node
}

fn write_background_material(material: &BackgroundMaterial) -> XmlNode {
    let mut node = XmlNode::new("BackgroundMaterial");
    for (field, name) in material_fields() {
        node.set_attribute(name, material.value(field).to_string());
    }
    node
}

fn read_background_material(
    node: &XmlNode,
    material: &mut BackgroundMaterial,
) -> Result<(), StructureXmlError> {
    for (field, name) in material_fields() {
        if let Some(value) = node.attribute(name) {
            let value = parse_number(value, &format!("BackgroundMaterial.{name}"))?;
            if !material.set_value(field, value) {
                return Err(StructureXmlError::InvalidValue {
                    field: format!("BackgroundMaterial.{name}"),
                    value: value.to_string(),
                });
            }
        }
    }
    Ok(())
}

fn write_property(property: &Property) -> Result<XmlNode, StructureXmlError> {
    let kind = property_xml_name(property.property_type())?;
    let mut node = XmlNode::new(kind);
    node.set_attribute("ID", property.index().to_string());
    node.set_attribute("Name", property.name());
    if !property.is_visible() {
        node.set_attribute("Visible", "0");
    }
    if let Some(color) = property.fill_color() {
        node.children.push(write_color("FillColor", color));
    }
    if let Some(color) = property.edge_color() {
        node.children.push(write_color("EdgeColor", color));
    }
    if property.attribute_names().next().is_some() {
        let mut attributes = XmlNode::new("Attributes");
        for name in property.attribute_names() {
            attributes.set_attribute(name, property.attribute(name));
        }
        node.children.push(attributes);
    }
    let mut primitives = XmlNode::new("Primitives");
    for index in 0..property.primitive_count() {
        primitives.children.push(write_primitive(
            property.primitive(index).expect("primitive index is valid"),
        )?);
    }
    node.children.push(primitives);

    if let Some(material) = property.material() {
        node.set_attribute("Isotropy", if material.is_isotropic() { "1" } else { "0" });
        let mut values = write_material_values(material);
        let mut weights = write_material_weights(material);
        if let Some(debye) = property.debye_material() {
            for order in 0..debye.order() {
                let suffix = order + 1;
                for (name, term) in [
                    (
                        format!("EpsilonDelta_{suffix}"),
                        (0..3)
                            .map(|component| debye.epsilon_delta_term(order, component).unwrap())
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                    (
                        format!("EpsilonRelaxTime_{suffix}"),
                        (0..3)
                            .map(|component| {
                                debye.epsilon_relax_time_term(order, component).unwrap()
                            })
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                ] {
                    values.set_attribute(name, term);
                }
                for (name, term) in [
                    (
                        format!("EpsilonDelta_{suffix}"),
                        (0..3)
                            .map(|component| {
                                debye.epsilon_delta_weight_term(order, component).unwrap()
                            })
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                    (
                        format!("EpsilonRelaxTime_{suffix}"),
                        (0..3)
                            .map(|component| {
                                debye
                                    .epsilon_relax_time_weight_term(order, component)
                                    .unwrap()
                            })
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                ] {
                    weights.set_attribute(name, term);
                }
            }
        }
        if let Some(lorentz) = property.lorentz_material() {
            for order in 0..lorentz.order() {
                let suffix = order + 1;
                for parameter in crate::material::LorentzParameter::ALL {
                    let name = format!("{}_{suffix}", parameter.xml_name());
                    values.set_attribute(
                        name.clone(),
                        (0..3)
                            .map(|component| lorentz.term(parameter, order, component).unwrap())
                            .collect::<Vec<_>>()
                            .join(","),
                    );
                    weights.set_attribute(
                        name,
                        (0..3)
                            .map(|component| {
                                lorentz.weight_term(parameter, order, component).unwrap()
                            })
                            .collect::<Vec<_>>()
                            .join(","),
                    );
                }
            }
        }
        node.children.push(values);
        node.children.push(weights);
    }
    if let Some(discrete) = property.discrete_material() {
        if let Some(transform) = discrete.transform() {
            let mut transformation = XmlNode::new("Transformation");
            for operation in transform.operations() {
                let mut child = XmlNode::new(transform_xml_name(operation.kind));
                child.set_attribute(
                    "Argument",
                    operation
                        .arguments
                        .iter()
                        .map(|argument| argument.value_string())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                transformation.children.push(child);
            }
            node.children.push(transformation);
        }
        let mut file = XmlNode::new("DiscFile");
        file.set_attribute("Type", discrete.file_type().to_string());
        file.set_attribute("File", discrete.file_name());
        file.set_attribute(
            "UseDBBackground",
            if discrete.use_database_for_background() {
                "1"
            } else {
                "0"
            },
        );
        file.set_attribute("Scale", discrete.scale().to_string());
        node.children.push(file);
    }
    if let Some(sheet) = property.conducting_sheet() {
        node.set_attribute("Conductivity", sheet.conductivity_term());
        node.set_attribute("Thickness", sheet.thickness_term());
    }
    if let Some(absorbing_boundary) = property.absorbing_boundary() {
        node.set_attribute(
            "NormalSignPositive",
            if absorbing_boundary.normal_sign_positive() {
                "1"
            } else {
                "0"
            },
        );
        node.set_attribute("PhaseVelocity", absorbing_boundary.phase_velocity_term());
        node.set_attribute(
            "AbsorbingBoundaryType",
            absorbing_boundary.boundary_type().xml_value().to_string(),
        );
    }
    if let Some(lumped_element) = property.lumped_element() {
        node.set_attribute("Direction", lumped_element.direction().to_string());
        node.set_attribute("Caps", if lumped_element.caps() { "1" } else { "0" });
        for (name, term) in [
            ("R", lumped_element.resistance_term()),
            ("C", lumped_element.capacitance_term()),
            ("L", lumped_element.inductance_term()),
        ] {
            if !term.parse::<f64>().is_ok_and(f64::is_nan) {
                node.set_attribute(name, term);
            }
        }
        node.set_attribute(
            "LEtype",
            lumped_element.element_type().xml_value().to_string(),
        );
    }
    if let Some(probe) = property.probe() {
        node.set_attribute("Number", (probe.number() as i32).to_string());
        node.set_attribute("Type", probe.probe_type().to_string());
        node.set_attribute("Weight", probe.weight().to_string());
        node.set_attribute("NormDir", probe.normal_direction().to_string());
        node.set_attribute("StartTime", probe.start_time().to_string());
        node.set_attribute("StopTime", probe.stop_time().to_string());
        if probe.oversampling() >= 0 {
            node.set_attribute("OverSampling", probe.oversampling().to_string());
        }
        if !probe.fd_samples().is_empty() {
            let mut samples = XmlNode::new("FD_Samples");
            samples.text = probe
                .fd_samples()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            node.children.push(samples);
        }
        if !probe.mode_file().is_empty() {
            node.set_attribute("ModeFile", probe.mode_file());
        }
        let mode_functions: Vec<_> = (0..3)
            .map(|direction| probe.mode_function(direction).unwrap_or(""))
            .collect();
        if mode_functions.iter().any(|function| !function.is_empty()) {
            let mut functions = XmlNode::new("ModeFunction");
            for (axis, function) in ["X", "Y", "Z"].into_iter().zip(mode_functions) {
                functions.set_attribute(axis, function);
            }
            node.children.push(functions);
        }
        let origin = probe.mode_origin();
        if origin != [0.0; 3] {
            let mut mode_origin = XmlNode::new("ModeOrigin");
            for (axis, coordinate) in ["X", "Y", "Z"].into_iter().zip(origin) {
                mode_origin.set_attribute(axis, coordinate.to_string());
            }
            node.children.push(mode_origin);
        }
    }
    if let Some(dump_box) = property.dump_box() {
        node.set_attribute("DumpType", dump_box.dump_type().to_string());
        node.set_attribute("DumpMode", dump_box.dump_mode().to_string());
        node.set_attribute("FileType", dump_box.file_type().to_string());
        node.set_attribute("MultiGridLevel", dump_box.multi_grid_level().to_string());
        if dump_box.sub_sampling_enabled() {
            node.set_attribute(
                "SubSampling",
                dump_box
                    .sub_sampling()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        if dump_box.optimal_resolution_enabled() {
            node.set_attribute(
                "OptResolution",
                dump_box
                    .optimal_resolution()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        if !dump_box.fd_samples().is_empty() {
            let mut samples = XmlNode::new("FD_Samples");
            samples.text = dump_box
                .fd_samples()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            node.children.push(samples);
        }
        if !dump_box.sar_method().is_empty() {
            node.set_attribute("SAR_Method", dump_box.sar_method().to_string());
        }
    }
    if let Some(excitation) = property.excitation() {
        node.set_attribute("Number", (excitation.number() as i32).to_string());
        node.set_attribute("Enabled", if excitation.is_enabled() { "1" } else { "0" });
        node.set_attribute("Frequency", excitation.frequency_term());
        node.set_attribute("Delay", excitation.delay_term());
        node.set_attribute("Type", excitation.excitation_type().to_string());
        node.set_attribute(
            "Excite",
            (0..3)
                .map(|component| excitation.excitation_term(component).unwrap())
                .collect::<Vec<_>>()
                .join(","),
        );
        if !excitation.weight_file().is_empty() {
            node.set_attribute("WeightFile", excitation.weight_file());
        }
        let origin = excitation.weight_origin();
        if origin != [0.0; 3] {
            let mut weight_origin = XmlNode::new("WeightOrigin");
            for (axis, coordinate) in ["X", "Y", "Z"].into_iter().zip(origin) {
                weight_origin.set_attribute(axis, coordinate.to_string());
            }
            node.children.push(weight_origin);
        }
        let mut weight = XmlNode::new("Weight");
        for (axis, component) in ["X", "Y", "Z"].into_iter().zip(0..3) {
            weight.set_attribute(axis, excitation.weighting_term(component).unwrap());
        }
        node.children.push(weight);
        node.set_attribute(
            "PropDir",
            (0..3)
                .map(|component| excitation.propagation_direction_term(component).unwrap())
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if let Some(resolution_box) = property.resolution_box() {
        node.set_attribute("Factor", (resolution_box.factor() as i32).to_string());
    }
    if let Some(value) = property.unknown_property() {
        node.set_attribute("Property", value);
    }
    Ok(node)
}

fn write_material_values(material: &MaterialProperties) -> XmlNode {
    let mut node = XmlNode::new("Property");
    for (field, name) in material_fields() {
        node.set_attribute(
            name,
            (0..3)
                .map(|component| material.value_xml_term(field, component).unwrap())
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    node.set_attribute("Density", material.density_xml_term());
    node
}

fn write_color(name: &str, color: PropertyColor) -> XmlNode {
    let mut node = XmlNode::new(name);
    for (attribute, component) in ["R", "G", "B", "a"].into_iter().zip(color.components()) {
        node.set_attribute(attribute, component.to_string());
    }
    node
}

fn write_material_weights(material: &MaterialProperties) -> XmlNode {
    let mut node = XmlNode::new("Weight");
    for (field, name) in material_fields() {
        node.set_attribute(
            name,
            (0..3)
                .map(|component| material.weight_xml_term(field, component).unwrap())
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    node.set_attribute("Density", material.density_weight_xml_term());
    node
}

fn write_primitive(primitive: &Primitive) -> Result<XmlNode, StructureXmlError> {
    let mut node = XmlNode::new(primitive_xml_name(primitive)?);
    node.set_attribute("Priority", primitive.priority().to_string());
    if primitive.coordinate_system() != CoordinateSystem::Undefined {
        node.set_attribute(
            "CoordSystem",
            (primitive.coordinate_system() as i32).to_string(),
        );
    }
    if let Some(transform) = primitive.transform() {
        let mut transformation = XmlNode::new("Transformation");
        for operation in transform.operations() {
            let mut child = XmlNode::new(transform_xml_name(operation.kind));
            child.set_attribute(
                "Argument",
                operation
                    .arguments
                    .iter()
                    .map(|argument| argument.value_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
            transformation.children.push(child);
        }
        node.children.push(transformation);
    }

    if let Some(point) = primitive.point_shape() {
        for axis in 0..3 {
            node.set_attribute(
                ["X", "Y", "Z"][axis],
                point
                    .coordinate_xml_term(axis)
                    .expect("point axis is valid"),
            );
        }
    } else if let Some(shape) = primitive.box_shape() {
        node.children.push(write_box_endpoint(shape, 0, "P1"));
        node.children.push(write_box_endpoint(shape, 1, "P2"));
    } else if let Some(shape) = primitive.sphere_shape() {
        node.set_attribute("Radius", shape.radius_xml_term());
        node.children.push(write_coordinate(
            "Center",
            (0..3).map(|axis| shape.center_xml_term(axis).unwrap()),
        ));
    } else if let Some(shape) = primitive.spherical_shell_shape() {
        node.set_attribute("Radius", shape.radius_xml_term());
        node.set_attribute("ShellWidth", shape.shell_width_xml_term());
        node.children.push(write_coordinate(
            "Center",
            (0..3).map(|axis| shape.center_xml_term(axis).unwrap()),
        ));
    } else if let Some(shape) = primitive.cylindrical_shell_shape() {
        node.set_attribute("Radius", shape.radius_xml_term());
        node.set_attribute(
            "ShellWidth",
            shape.shell_width_xml_term().expect("shell has a width"),
        );
        node.children
            .push(write_box_endpoint_from_cylinder(shape, 0, "P1"));
        node.children
            .push(write_box_endpoint_from_cylinder(shape, 1, "P2"));
    } else if let Some(shape) = primitive.cylinder_shape() {
        node.set_attribute("Radius", shape.radius_xml_term());
        node.children
            .push(write_box_endpoint_from_cylinder(shape, 0, "P1"));
        node.children
            .push(write_box_endpoint_from_cylinder(shape, 1, "P2"));
    } else if let Some(shape) = primitive.polygon_shape() {
        node.set_attribute("Elevation", shape.elevation_xml_term());
        node.set_attribute("NormDir", shape.normal_direction().to_string());
        node.set_attribute("QtyVertices", shape.vertex_count().to_string());
        if let Some(length) = shape.extrusion_length_xml_term() {
            node.set_attribute("Length", length);
        }
        if let Some(axis_direction) = shape.rotation_axis_direction() {
            node.set_attribute("RotAxisDir", axis_direction.to_string());
            let angles = shape
                .rotation_angles_xml_terms()
                .expect("rotational polygon has angles");
            let mut angle_node = XmlNode::new("Angles");
            angle_node.set_attribute("Start", angles[0].clone());
            angle_node.set_attribute("Stop", angles[1].clone());
            node.children.push(angle_node);
        }
        for index in 0..shape.vertex_count() {
            let terms = shape
                .vertex_xml_terms(index)
                .expect("polygon vertex exists");
            let mut vertex = XmlNode::new("Vertex");
            vertex.set_attribute("X1", terms[0].clone());
            vertex.set_attribute("X2", terms[1].clone());
            node.children.push(vertex);
        }
    } else if let Some(shape) = primitive.polyhedron_reader_shape() {
        node.set_attribute("FileName", shape.filename());
        node.set_attribute("FileType", shape.file_type().xml_value());
    } else if let Some(shape) = primitive.polyhedron_shape() {
        for index in 0..shape.vertex_count() {
            let mut vertex = XmlNode::new("Vertex");
            vertex.text = shape
                .vertex_xml_text(index)
                .expect("polyhedron vertex exists");
            node.children.push(vertex);
        }
        for index in 0..shape.face_count() {
            let mut face = XmlNode::new("Face");
            face.text = shape.face_xml_text(index).expect("polyhedron face exists");
            node.children.push(face);
        }
    } else if let Some(shape) = primitive.curve_or_wire_shape() {
        if let Some(radius) = shape.wire_radius_xml_term() {
            node.set_attribute("WireRadius", radius);
        }
        for index in 0..shape.point_count() {
            let terms = shape.point_xml_terms(index).expect("curve point exists");
            let mut vertex = XmlNode::new("Vertex");
            for (axis, term) in ["X", "Y", "Z"].into_iter().zip(terms) {
                vertex.set_attribute(axis, term);
            }
            node.children.push(vertex);
        }
    } else if let Some(shape) = primitive.multibox_shape() {
        node.set_attribute("QtyBox", shape.box_count().to_string());
        for index in 0..shape.box_count() {
            let box_shape = shape.box_at(index).expect("box index is valid");
            node.children
                .push(write_box_endpoint(box_shape, 0, "StartP"));
            node.children.push(write_box_endpoint(box_shape, 1, "EndP"));
        }
    } else if let Some(shape) = primitive.user_defined_shape() {
        node.set_attribute(
            "CoordSystem",
            (shape.coordinate_system() as i32).to_string(),
        );
        let mut coordinate_shift = XmlNode::new("CoordShift");
        for (axis, attribute) in ["X", "Y", "Z"].into_iter().enumerate() {
            coordinate_shift.set_attribute(
                attribute,
                shape
                    .coordinate_shift_term(axis)
                    .expect("coordinate shift axis is valid"),
            );
        }
        node.children.push(coordinate_shift);
        let mut function = XmlNode::new("Function");
        function.text = shape.function().to_owned();
        node.children.push(function);
    }
    Ok(node)
}

fn write_box_endpoint(shape: &BoxPrimitive, point: usize, name: &str) -> XmlNode {
    write_coordinate(
        name,
        (0..3).map(|axis| shape.coordinate_xml_term(point + axis * 2).unwrap()),
    )
}

fn write_box_endpoint_from_cylinder(
    shape: &crate::model::CylinderPrimitive,
    point: usize,
    name: &str,
) -> XmlNode {
    write_coordinate(
        name,
        (0..3).map(|axis| shape.coordinate_xml_term(point + axis * 2).unwrap()),
    )
}

fn write_coordinate(name: &str, terms: impl Iterator<Item = String>) -> XmlNode {
    let mut node = XmlNode::new(name);
    for (name, value) in ["X", "Y", "Z"].into_iter().zip(terms) {
        node.set_attribute(name, value);
    }
    node
}

fn read_grid(node: &XmlNode, grid: &mut RectGrid) -> Result<(), StructureXmlError> {
    if let Some(value) = node.attribute("DeltaUnit") {
        grid.set_delta_unit(parse_number(value, "DeltaUnit")?);
    }
    if let Some(value) = node.attribute("CoordSystem") {
        grid.set_coordinate_system(parse_coordinate_system(
            value,
            "RectilinearGrid.CoordSystem",
        )?);
    }
    for (direction, name) in ["XLines", "YLines", "ZLines"].into_iter().enumerate() {
        let lines = node
            .child(name)
            .ok_or_else(|| StructureXmlError::MissingChild {
                node: "RectilinearGrid".to_owned(),
                child: name.to_owned(),
            })?;
        for value in lines
            .text
            .trim()
            .split(',')
            .filter(|value| !value.is_empty())
        {
            grid.add_line(direction, parse_number(value.trim(), name)?);
        }
    }
    Ok(())
}

fn read_parameters(
    node: &XmlNode,
    parameters: &mut ParameterSet,
    native_compatibility: bool,
    warnings: &mut Vec<StructureXmlWarning>,
) -> Result<(), StructureXmlError> {
    for child in node.children_named("Parameter") {
        let kind = child.attribute("Type");
        let name = child.attribute("name");
        match read_parameter(child, native_compatibility) {
            Ok(parameter) => {
                parameters.insert(parameter);
            }
            Err(error) if native_compatibility => {
                warnings.push(StructureXmlWarning::InvalidParameterSkipped {
                    kind: kind.map(str::to_owned),
                    name: name.map(str::to_owned),
                    reason: error.to_string(),
                });
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn read_parameter(
    child: &XmlNode,
    native_compatibility: bool,
) -> Result<Parameter, StructureXmlError> {
    let kind = required_attribute(child, "Type")?;
    let name = child.attribute("name").unwrap_or("");
    let value = parse_number(required_attribute(child, "value")?, "Parameter.value")?;
    let mut parameter = match kind {
        "Const" => Parameter::new(name, value),
        "Linear" => Parameter::linear(
            name,
            value,
            parse_number(required_attribute(child, "min")?, "Parameter.min")?,
            parse_number(required_attribute(child, "max")?, "Parameter.max")?,
            parse_number(required_attribute(child, "step")?, "Parameter.step")?,
        ),
        _ => {
            return Err(StructureXmlError::InvalidValue {
                field: "Parameter.Type".to_owned(),
                value: kind.to_owned(),
            });
        }
    };
    if let Some(sweep) = child.attribute("Sweep") {
        if native_compatibility {
            if let Ok(value) = sweep.trim().parse::<i32>() {
                parameter.set_sweep(value > 0);
            }
        } else {
            parameter.set_sweep(parse_bool(sweep, "Parameter.Sweep")?);
        }
    }
    Ok(parameter)
}

fn read_property_base(
    node: &XmlNode,
    property: &mut Property,
    native_compatibility: bool,
) -> Result<(), StructureXmlError> {
    if let Some(visible) = node.attribute("Visible") {
        if native_compatibility {
            if let Ok(value) = visible.trim().parse::<i32>() {
                property.set_visible(value != 0);
            }
        } else {
            property.set_visible(parse_bool(visible, "Property.Visible")?);
        }
    }
    if let Some(color) = node.child("FillColor") {
        property.set_fill_color(Some(read_color(color)?));
    }
    if let Some(color) = node.child("EdgeColor") {
        property.set_edge_color(Some(read_color(color)?));
    }
    if let Some(attributes) = node.child("Attributes") {
        for (name, value) in &attributes.attributes {
            property.set_attribute(name, value);
        }
    }
    Ok(())
}

fn read_property(
    node: &XmlNode,
    parameters: &ParameterSet,
    native_compatibility: bool,
    warnings: &mut Vec<StructureXmlWarning>,
) -> Result<Property, StructureXmlError> {
    let kind = property_type(&node.name)?;
    let mut property = Property::new(node.attribute("Name").unwrap_or(""), kind);
    read_property_base(node, &mut property, native_compatibility)?;
    if let Some(material) = property.material_mut() {
        let explicit_isotropy = node.attribute("Isotropy").is_some();
        if let Some(isotropy) = node.attribute("Isotropy") {
            material.set_isotropic(parse_bool(isotropy, "Isotropy")?);
        }
        if let Some(values) = node.child("Property") {
            read_material_values(values, material, false, explicit_isotropy)?;
        }
        if let Some(weights) = node.child("Weight") {
            read_material_values(weights, material, true, explicit_isotropy)?;
        }
    }
    if let Some(debye) = property.debye_material_mut() {
        let values = node.child("Property");
        let mut order = usize::from(values.is_some());
        while values.is_some_and(|values| {
            values
                .attribute(&format!("EpsilonDelta_{}", order + 1))
                .is_some()
                || values
                    .attribute(&format!("MueDelta_{}", order + 1))
                    .is_some()
        }) {
            order += 1;
        }
        debye.set_order(order);
        for pole in 0..order {
            let suffix = pole + 1;
            if let Some(values) = values {
                read_debye_vector(
                    values,
                    &format!("EpsilonDelta_{suffix}"),
                    (pole == 0).then_some("EpsilonDelta"),
                    "EpsilonDelta",
                    "0.0",
                    |component, term| debye.set_epsilon_delta_expression(pole, component, term),
                )?;
                read_debye_vector(
                    values,
                    &format!("EpsilonRelaxTime_{suffix}"),
                    (pole == 0).then_some("EpsilonRelaxTime"),
                    "EpsilonRelaxTime",
                    "0.0",
                    |component, term| {
                        debye.set_epsilon_relax_time_expression(pole, component, term)
                    },
                )?;
            }
            if let Some(weights) = node.child("Weight") {
                read_debye_vector(
                    weights,
                    &format!("EpsilonDelta_{suffix}"),
                    (pole == 0).then_some("EpsilonDelta"),
                    "Weight.EpsilonDelta",
                    "1.0",
                    |component, term| {
                        debye.set_epsilon_delta_weight_expression(pole, component, term)
                    },
                )?;
                read_debye_vector(
                    weights,
                    &format!("EpsilonRelaxTime_{suffix}"),
                    (pole == 0).then_some("EpsilonRelaxTime"),
                    "Weight.EpsilonRelaxTime",
                    "1.0",
                    |component, term| {
                        debye.set_epsilon_relax_time_weight_expression(pole, component, term)
                    },
                )?;
            }
        }
    }
    if let Some(lorentz) = property.lorentz_material_mut() {
        let values = node.child("Property");
        let mut order = usize::from(values.is_some());
        while values.is_some_and(|values| {
            values
                .attribute(&format!("EpsilonPlasmaFrequency_{}", order + 1))
                .is_some()
                || values
                    .attribute(&format!("MuePlasmaFrequency_{}", order + 1))
                    .is_some()
        }) {
            order += 1;
        }
        lorentz.set_order(order);
        for pole in 0..order {
            let suffix = pole + 1;
            for parameter in crate::material::LorentzParameter::ALL {
                let name = format!("{}_{suffix}", parameter.xml_name());
                let legacy_name = (pole == 0).then_some(parameter.xml_name());
                if let Some(values) = values {
                    read_debye_vector(
                        values,
                        &name,
                        legacy_name,
                        parameter.xml_name(),
                        "0.0",
                        |component, term| {
                            lorentz.set_value_expression(parameter, pole, component, term)
                        },
                    )?;
                }
                if let Some(weights) = node.child("Weight") {
                    read_debye_vector(
                        weights,
                        &name,
                        legacy_name,
                        &format!("Weight.{}", parameter.xml_name()),
                        "1.0",
                        |component, term| {
                            lorentz.set_weight_expression(parameter, pole, component, term)
                        },
                    )?;
                }
            }
        }
    }
    if let Some(discrete) = property.discrete_material_mut() {
        let file = node.child("DiscFile");
        let (file_type, file_name, use_database, scale) = if let Some(file) = file {
            (
                file.attribute("Type").or(Some("0")),
                file.attribute("File"),
                file.attribute("UseDBBackground"),
                file.attribute("Scale"),
            )
        } else {
            (
                node.attribute("Type").or(Some("0")),
                node.attribute("File"),
                node.attribute("UseDBBackground"),
                node.attribute("Scale"),
            )
        };
        if let Some(value) = file_type {
            discrete.set_file_type(parse_integer(value, "DiscMaterial.FileType")?);
        }
        discrete.set_file_name(file_name.unwrap_or(""));
        if let Some(value) = use_database {
            discrete.set_use_database_for_background(
                parse_integer(value, "DiscMaterial.UseDBBackground")? != 0,
            );
        }
        if let Some(value) = scale {
            discrete.set_scale(parse_number(value, "DiscMaterial.Scale")?);
        }
        if let Some(transformation) = node.child("Transformation") {
            let mut transform = crate::transform::Transform::new();
            apply_transform_operations(
                transformation,
                &mut transform,
                parameters,
                native_compatibility,
                warnings,
            )?;
            discrete.set_transform(Some(transform));
        }
    }
    if let Some(sheet) = property.conducting_sheet_mut() {
        if let Some(conductivity) = node.attribute("Conductivity") {
            sheet.set_conductivity_term(conductivity)?;
        }
        if let Some(thickness) = node.attribute("Thickness") {
            sheet.set_thickness_term(thickness)?;
        }
    }
    if let Some(absorbing_boundary) = property.absorbing_boundary_mut() {
        if let Some(value) = node.attribute("NormalSignPositive") {
            absorbing_boundary.set_normal_sign_positive(parse_bool(value, "NormalSignPositive")?);
        }
        if let Some(value) = node.attribute("PhaseVelocity") {
            absorbing_boundary.set_phase_velocity_term(value)?;
        }
        if let Some(value) = node.attribute("AbsorbingBoundaryType") {
            let value = value
                .parse::<i32>()
                .map_err(|_| StructureXmlError::InvalidValue {
                    field: "AbsorbingBoundaryType".to_owned(),
                    value: value.to_owned(),
                })?;
            absorbing_boundary.set_boundary_type(
                crate::material::AbsorbingBoundaryType::from_xml_value(value),
            );
        }
    }
    if let Some(lumped_element) = property.lumped_element_mut() {
        if let Some(value) = node.attribute("Direction") {
            let direction = value
                .parse::<i32>()
                .map_err(|_| StructureXmlError::InvalidValue {
                    field: "LumpedElement.Direction".to_owned(),
                    value: value.to_owned(),
                })?;
            lumped_element.set_direction_from_xml(direction);
        }
        if let Some(value) = node.attribute("Caps") {
            let caps = value
                .parse::<i32>()
                .map_err(|_| StructureXmlError::InvalidValue {
                    field: "LumpedElement.Caps".to_owned(),
                    value: value.to_owned(),
                })?;
            lumped_element.set_caps(caps != 0);
        }
        if let Some(value) = node.attribute("R") {
            lumped_element.set_resistance_term(value)?;
        }
        if let Some(value) = node.attribute("C") {
            lumped_element.set_capacitance_term(value)?;
        }
        if let Some(value) = node.attribute("L") {
            lumped_element.set_inductance_term(value)?;
        }
        if let Some(value) = node.attribute("LEtype") {
            // Native writes LEtype through WriteTerm(), which formats the
            // integer as a parameter value ("1.000000e+00" for LE_SERIES).
            let value = parse_number(value, "LumpedElement.LEtype")? as i32;
            lumped_element
                .set_element_type(crate::material::LumpedElementType::from_xml_value(value));
        }
    }
    let legacy_mode_functions = ["ModeFunctionX", "ModeFunctionY", "ModeFunctionZ"].map(|name| {
        property
            .has_attribute(name)
            .then(|| property.attribute(name).to_owned())
    });
    if let Some(probe) = property.probe_mut() {
        if let Some(value) = node.attribute("Number") {
            probe.set_number(parse_integer(value, "ProbeBox.Number")? as u32);
        }
        if let Some(value) = node.attribute("Type") {
            probe.set_probe_type(parse_integer(value, "ProbeBox.Type")?);
        }
        if let Some(value) = node.attribute("Weight") {
            probe.set_weight(parse_number(value, "ProbeBox.Weight")?);
        }
        if let Some(value) = node.attribute("NormDir") {
            probe.set_normal_direction(parse_integer(value, "ProbeBox.NormDir")?);
        }
        if let Some(value) = node.attribute("StartTime") {
            probe.set_start_time_from_xml(parse_number(value, "ProbeBox.StartTime")?);
        }
        if let Some(value) = node.attribute("StopTime") {
            probe.set_stop_time_from_xml(parse_number(value, "ProbeBox.StopTime")?);
        }
        if let Some(value) = node.attribute("OverSampling") {
            probe.set_oversampling(parse_integer(value, "ProbeBox.OverSampling")?);
        }
        if let Some(value) = node.attribute("ModeFile") {
            probe.set_mode_file(value);
        }
        if let Some(samples) = node.child("FD_Samples") {
            let text = samples.text.trim();
            if !text.is_empty() {
                let values = text
                    .split(',')
                    .map(|value| parse_number(value.trim(), "ProbeBox.FD_Samples"))
                    .collect::<Result<Vec<_>, _>>()?;
                probe.set_fd_samples(values);
            }
        }
        if let Some(functions) = node.child("ModeFunction") {
            for (direction, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
                if let Some(function) = functions.attribute(axis) {
                    probe.set_mode_function(direction, function);
                }
            }
        } else {
            for (direction, function) in legacy_mode_functions.iter().enumerate() {
                if let Some(function) = function {
                    probe.set_mode_function(direction, function);
                }
            }
        }
        if let Some(origin) = node.child("ModeOrigin") {
            let mut coordinates = [0.0; 3];
            for (axis, coordinate) in ["X", "Y", "Z"].into_iter().zip(&mut coordinates) {
                if let Some(value) = origin.attribute(axis) {
                    *coordinate = parse_number(value, &format!("ProbeBox.ModeOrigin.{axis}"))?;
                }
            }
            probe.set_mode_origin(coordinates);
        }
    }
    if property.probe().is_some() && node.child("ModeFunction").is_none() {
        for name in ["ModeFunctionX", "ModeFunctionY", "ModeFunctionZ"] {
            property.remove_attribute(name);
        }
    }
    if let Some(dump_box) = property.dump_box_mut() {
        if let Some(value) = node.attribute("DumpType") {
            dump_box.set_dump_type(parse_integer(value, "DumpBox.DumpType")?);
        }
        if let Some(value) = node.attribute("DumpMode") {
            dump_box.set_dump_mode(parse_integer(value, "DumpBox.DumpMode")?);
        }
        if let Some(value) = node.attribute("FileType") {
            dump_box.set_file_type(parse_integer(value, "DumpBox.FileType")?);
        }
        if let Some(value) = node.attribute("MultiGridLevel") {
            dump_box.set_multi_grid_level(parse_integer(value, "DumpBox.MultiGridLevel")?);
        }
        if let Some(value) = node.attribute("SubSampling") {
            for (direction, component) in value
                .split(',')
                .filter(|component| !component.trim().is_empty())
                .take(3)
                .enumerate()
            {
                let factor = component.trim().parse::<u32>().map_err(|_| {
                    StructureXmlError::InvalidValue {
                        field: "DumpBox.SubSampling".to_owned(),
                        value: value.to_owned(),
                    }
                })?;
                dump_box.set_sub_sampling(direction, factor);
            }
        }
        if let Some(value) = node.attribute("Frequency") {
            let samples = value
                .split(',')
                .filter(|component| !component.trim().is_empty())
                .map(|component| parse_number(component.trim(), "DumpBox.Frequency"))
                .collect::<Result<Vec<_>, _>>()?;
            dump_box.set_fd_samples(samples);
        }
        if let Some(samples) = node.child("FD_Samples") {
            let text = samples.text.trim();
            if !text.is_empty() {
                let values = text
                    .split(',')
                    .map(|value| parse_number(value.trim(), "DumpBox.FD_Samples"))
                    .collect::<Result<Vec<_>, _>>()?;
                dump_box.set_fd_samples(values);
            }
        }
        if let Some(value) = node.attribute("SAR_Method") {
            dump_box.set_sar_method(value.to_owned());
        }
        if let Some(value) = node.attribute("OptResolution") {
            let resolutions = value
                .split(',')
                .filter(|component| !component.trim().is_empty())
                .map(|component| parse_number(component.trim(), "DumpBox.OptResolution"))
                .collect::<Result<Vec<_>, _>>()?;
            if resolutions.len() == 1 {
                for direction in 0..3 {
                    dump_box.set_optimal_resolution(direction, resolutions[0]);
                }
            } else {
                for (direction, resolution) in resolutions.into_iter().take(3).enumerate() {
                    dump_box.set_optimal_resolution(direction, resolution);
                }
            }
        }
    }
    if let Some(excitation) = property.excitation_mut() {
        if let Some(value) = node.attribute("Number") {
            excitation.set_number(parse_integer(value, "Excitation.Number")? as u32);
        }
        if let Some(value) = node.attribute("Enabled") {
            excitation.set_enabled(parse_bool(value, "Excitation.Enabled")?);
        }
        excitation.set_excitation_type(parse_integer(
            required_attribute(node, "Type")?,
            "Excitation.Type",
        )?);
        if let Some(value) = node.attribute("Frequency") {
            excitation.set_frequency_term(value)?;
        }
        if let Some(value) = node.attribute("Delay") {
            excitation.set_delay_term(value)?;
        }
        let excite_terms = required_attribute(node, "Excite")?
            .split(',')
            .collect::<Vec<_>>();
        if excite_terms.len() > 3 {
            return Err(StructureXmlError::InvalidValue {
                field: "Excitation.Excite".to_owned(),
                value: required_attribute(node, "Excite")?.to_owned(),
            });
        }
        for component in 0..3 {
            excitation.set_excitation_term(
                component,
                excite_terms.get(component).copied().unwrap_or("0"),
            )?;
        }
        if let Some(value) = node.attribute("WeightFile") {
            excitation.set_weight_file(value);
        }
        if let Some(origin) = node.child("WeightOrigin") {
            let mut coordinates = [0.0; 3];
            for (axis, coordinate) in ["X", "Y", "Z"].into_iter().zip(&mut coordinates) {
                if let Some(value) = origin.attribute(axis) {
                    *coordinate = parse_number(value, &format!("Excitation.WeightOrigin.{axis}"))?;
                }
            }
            excitation.set_weight_origin(coordinates);
        }
        if excitation.weight_file().is_empty() {
            if let Some(weight) = node.child("Weight") {
                for (component, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
                    if let Some(term) = weight.attribute(axis) {
                        excitation.set_weighting_term(component, term)?;
                    }
                }
            }
        }
        if let Some(value) = node.attribute("PropDir") {
            let terms = value.split(',').collect::<Vec<_>>();
            if terms.len() > 3 {
                return Err(StructureXmlError::InvalidValue {
                    field: "Excitation.PropDir".to_owned(),
                    value: value.to_owned(),
                });
            }
            for component in 0..3 {
                excitation.set_propagation_direction_term(
                    component,
                    terms.get(component).copied().unwrap_or("0"),
                )?;
            }
        }
    }
    if let Some(resolution_box) = property.resolution_box_mut() {
        if let Some(value) = node.attribute("Factor") {
            resolution_box.set_factor(parse_integer(value, "ResBox.Factor")? as u32);
        }
    }
    if property.property_type() == PropertyType::Unknown {
        property.set_unknown_property(node.attribute("Property").unwrap_or("unknown"));
    }
    Ok(property)
}

fn read_unknown_property(
    node: &XmlNode,
    native_compatibility: bool,
) -> Result<Property, StructureXmlError> {
    let mut property = Property::new(node.attribute("Name").unwrap_or(""), PropertyType::Unknown);
    read_property_base(node, &mut property, native_compatibility)?;
    property.set_unknown_property(node.attribute("Property").unwrap_or("unknown"));
    Ok(property)
}

fn read_color(node: &XmlNode) -> Result<PropertyColor, StructureXmlError> {
    let mut components = [0; 4];
    for (index, attribute) in ["R", "G", "B", "a"].into_iter().enumerate() {
        let value = required_attribute(node, attribute)?
            .parse::<u8>()
            .map_err(|_| StructureXmlError::InvalidValue {
                field: format!("{}.{}", node.name, attribute),
                value: node.attribute(attribute).unwrap_or_default().to_owned(),
            })?;
        components[index] = value;
    }
    Ok(PropertyColor::new(
        components[0],
        components[1],
        components[2],
        components[3],
    ))
}

fn read_material_values(
    node: &XmlNode,
    material: &mut MaterialProperties,
    is_weight: bool,
    explicit_isotropy: bool,
) -> Result<(), StructureXmlError> {
    for (field, name) in material_fields() {
        if let Some(value) = node.attribute(name) {
            let components = value.split(',').collect::<Vec<_>>();
            if components.len() > 3 {
                return Err(StructureXmlError::InvalidValue {
                    field: format!("{name} vector"),
                    value: value.to_owned(),
                });
            }
            // Native CSXCAD marks a material anisotropic when a property is
            // given per direction. Infer that when no explicit `Isotropy`
            // attribute is present and the components differ.
            if !is_weight
                && !explicit_isotropy
                && node.attribute("Isotropy").is_none()
                && components.len() > 1
                && components
                    .windows(2)
                    .any(|pair| pair[0].trim() != pair[1].trim())
            {
                material.set_isotropic(false);
            }
            for (component, term) in components.into_iter().enumerate() {
                let applied = if is_weight {
                    material.set_weight_expression(field, component, term.trim())?
                } else {
                    material.set_value_expression(field, component, term.trim())?
                };
                if !applied {
                    return Err(StructureXmlError::InvalidValue {
                        field: name.to_owned(),
                        value: term.to_owned(),
                    });
                }
            }
        }
    }
    if let Some(density) = node.attribute("Density") {
        if is_weight {
            material.set_density_weight_expression(density)?;
        } else {
            material.set_density_expression(density)?;
        }
    }
    Ok(())
}

fn read_debye_vector(
    node: &XmlNode,
    name: &str,
    legacy_name: Option<&str>,
    field: &str,
    default_term: &str,
    mut set_term: impl FnMut(usize, &str) -> Result<bool, crate::parameter::ParameterError>,
) -> Result<(), StructureXmlError> {
    let Some(value) = node
        .attribute(name)
        .or_else(|| legacy_name.and_then(|name| node.attribute(name)))
    else {
        return Ok(());
    };
    let components = value.split(',').collect::<Vec<_>>();
    if components.len() > 3 {
        return Err(StructureXmlError::InvalidValue {
            field: format!("{field} vector"),
            value: value.to_owned(),
        });
    }
    // Native `ReadVectorTerm` fills all three components with the default and
    // then overwrites the components that were given. A single value is
    // therefore an isotropic component, not a component-0-only value.
    for component in 0..3 {
        if !set_term(component, default_term)? {
            return Err(StructureXmlError::InvalidValue {
                field: field.to_owned(),
                value: default_term.to_owned(),
            });
        }
    }
    for (component, term) in components.into_iter().enumerate() {
        if !set_term(component, term.trim())? {
            return Err(StructureXmlError::InvalidValue {
                field: field.to_owned(),
                value: term.to_owned(),
            });
        }
    }
    Ok(())
}

fn read_primitive(
    node: &XmlNode,
    parameters: &ParameterSet,
    native_compatibility: bool,
    warnings: &mut Vec<StructureXmlWarning>,
) -> Result<Primitive, StructureXmlError> {
    let mut primitive = match node.name.as_str() {
        "Point" => Primitive::point_from_expressions([
            required_attribute(node, "X")?,
            required_attribute(node, "Y")?,
            required_attribute(node, "Z")?,
        ])?,
        "Box" => {
            let (start, stop) = read_endpoints(node, "P1", "P2")?;
            Primitive::box_from_expressions(start, stop)?
        }
        "Sphere" => {
            let center = required_child(node, "Center")?;
            Primitive::sphere_from_expressions(
                read_coordinate_terms(center)?,
                required_attribute(node, "Radius")?,
            )?
        }
        "SphericalShell" => {
            let center = required_child(node, "Center")?;
            Primitive::spherical_shell_from_expressions(
                read_coordinate_terms(center)?,
                required_attribute(node, "Radius")?,
                required_attribute(node, "ShellWidth")?,
            )?
        }
        "Cylinder" => {
            let (start, stop) = read_endpoints(node, "P1", "P2")?;
            Primitive::cylinder_from_expressions(start, stop, required_attribute(node, "Radius")?)?
        }
        "CylindricalShell" => {
            let (start, stop) = read_endpoints(node, "P1", "P2")?;
            Primitive::cylindrical_shell_from_expressions(
                start,
                stop,
                required_attribute(node, "Radius")?,
                required_attribute(node, "ShellWidth")?,
            )?
        }
        "Curve" => Primitive::curve(),
        "Wire" => Primitive::curve_from_expressions(Some(required_attribute(node, "WireRadius")?))?,
        "Polygon" | "LinPoly" | "RotPoly" => {
            let elevation = if node.name == "RotPoly" {
                "0"
            } else {
                node.attribute("Elevation").unwrap_or("0")
            };
            let mut primitive = if node.name == "LinPoly" {
                Primitive::lin_poly_from_expressions(
                    elevation,
                    required_attribute(node, "Length")?,
                )?
            } else if node.name == "RotPoly" {
                let angles = required_child(node, "Angles")?;
                let axis_direction = required_attribute(node, "RotAxisDir")?
                    .parse::<usize>()
                    .map_err(|_| StructureXmlError::InvalidValue {
                        field: "RotPoly.RotAxisDir".to_owned(),
                        value: node.attribute("RotAxisDir").unwrap_or_default().to_owned(),
                    })?;
                if axis_direction >= 3 {
                    return Err(StructureXmlError::InvalidValue {
                        field: "RotPoly.RotAxisDir".to_owned(),
                        value: axis_direction.to_string(),
                    });
                }
                Primitive::rot_poly_from_expressions(
                    elevation,
                    axis_direction,
                    required_attribute(angles, "Start")?,
                    required_attribute(angles, "Stop")?,
                )?
            } else {
                Primitive::polygon_from_expressions(elevation)?
            };
            let direction = required_attribute(node, "NormDir")?
                .parse::<usize>()
                .map_err(|_| StructureXmlError::InvalidValue {
                    field: "Polygon.NormDir".to_owned(),
                    value: node.attribute("NormDir").unwrap_or_default().to_owned(),
                })?;
            let polygon = primitive.polygon_shape_mut().expect("constructed polygon");
            if !polygon.set_normal_direction(direction) {
                return Err(StructureXmlError::InvalidValue {
                    field: "Polygon.NormDir".to_owned(),
                    value: direction.to_string(),
                });
            }
            for vertex in node.children_named("Vertex") {
                polygon.add_vertex_from_expressions([
                    required_attribute(vertex, "X1")?,
                    required_attribute(vertex, "X2")?,
                ])?;
            }
            primitive
        }
        "Polyhedron" => {
            let mut primitive = Primitive::polyhedron();
            let polyhedron = primitive
                .polyhedron_shape_mut()
                .expect("constructed polyhedron");
            for vertex in node.children_named("Vertex") {
                let coordinates = vertex
                    .text
                    .split(',')
                    .map(|coordinate| parse_number(coordinate.trim(), "Polyhedron.Vertex"))
                    .collect::<Result<Vec<_>, _>>()?;
                let coordinates: [f64; 3] =
                    coordinates.try_into().map_err(|coordinates: Vec<f64>| {
                        StructureXmlError::InvalidValue {
                            field: "Polyhedron.Vertex".to_owned(),
                            value: format!(
                                "expected 3 coordinates, received {}",
                                coordinates.len()
                            ),
                        }
                    })?;
                polyhedron.add_vertex(coordinates);
            }
            for face in node.children_named("Face") {
                let vertices =
                    face.text
                        .split(',')
                        .map(|index| {
                            index.trim().parse::<i32>().map_err(|_| {
                                StructureXmlError::InvalidValue {
                                    field: "Polyhedron.Face".to_owned(),
                                    value: face.text.clone(),
                                }
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                polyhedron.add_face(vertices);
            }
            primitive
        }
        "PolyhedronReader" => {
            let mut primitive = Primitive::polyhedron_reader();
            let reader = primitive
                .polyhedron_reader_shape_mut()
                .expect("constructed polyhedron reader");
            reader.set_filename(required_attribute(node, "FileName")?);
            reader.set_file_type(PolyhedronFileType::from_xml_value(required_attribute(
                node, "FileType",
            )?));
            reader
                .read_file()
                .map_err(|error| StructureXmlError::Model(ModelError::PolyhedronReader(error)))?;
            primitive
        }
        "MultiBox" => {
            let starts = node.children_named("StartP").collect::<Vec<_>>();
            let stops = node.children_named("EndP").collect::<Vec<_>>();
            if starts.len() != stops.len() {
                return Err(StructureXmlError::InvalidValue {
                    field: "MultiBox endpoint count".to_owned(),
                    value: format!("{} starts and {} ends", starts.len(), stops.len()),
                });
            }
            let mut primitive = Primitive::multibox();
            let multi_box = primitive
                .multibox_shape_mut()
                .expect("constructed multibox");
            for (start, stop) in starts.into_iter().zip(stops) {
                multi_box.add_box(BoxPrimitive::from_expressions(
                    read_coordinate_terms(start)?,
                    read_coordinate_terms(stop)?,
                )?);
            }
            primitive
        }
        "User-Defined" | "UserDefined" => {
            let coordinate_system = required_attribute(node, "CoordSystem")?
                .parse::<i32>()
                .ok()
                .and_then(UserDefinedCoordinateSystem::from_xml_value)
                .ok_or_else(|| StructureXmlError::InvalidValue {
                    field: "User-Defined.CoordSystem".to_owned(),
                    value: node.attribute("CoordSystem").unwrap_or_default().to_owned(),
                })?;
            let shift = required_child(node, "CoordShift")?;
            let mut primitive = Primitive::user_defined();
            let shape = primitive
                .user_defined_shape_mut()
                .expect("constructed user-defined primitive");
            shape.set_coordinate_system(coordinate_system);
            for (axis, term) in read_coordinate_terms(shift)?.into_iter().enumerate() {
                shape.set_coordinate_shift_expression(axis, term)?;
            }
            shape.set_function(required_child(node, "Function")?.text.clone());
            primitive
        }
        kind => {
            return Err(StructureXmlError::UnsupportedPrimitiveType(kind.to_owned()));
        }
    };
    let priority = required_attribute(node, "Priority")?
        .parse::<i32>()
        .map_err(|_| StructureXmlError::InvalidValue {
            field: "Primitive.Priority".to_owned(),
            value: node.attribute("Priority").unwrap_or_default().to_owned(),
        })?;
    if let Some(shape) = primitive.curve_or_wire_shape_mut() {
        for vertex in node.children_named("Vertex") {
            shape.add_point_from_expressions(read_coordinate_terms(vertex)?)?;
        }
    }
    primitive.set_priority(priority);
    if let Some(coordinate_system) = node.attribute("CoordSystem") {
        primitive.set_coordinate_system(parse_coordinate_system(
            coordinate_system,
            "Primitive.CoordSystem",
        )?);
    }
    if let Some(transformation) = node.child("Transformation") {
        let transform = primitive.transform_mut();
        apply_transform_operations(
            transformation,
            transform,
            parameters,
            native_compatibility,
            warnings,
        )?;
    }
    Ok(primitive)
}

fn apply_transform_operations(
    transformation: &XmlNode,
    transform: &mut crate::transform::Transform,
    parameters: &ParameterSet,
    native_compatibility: bool,
    warnings: &mut Vec<StructureXmlWarning>,
) -> Result<(), StructureXmlError> {
    // Native CSTransform::ReadFromXML warns and skips an operation it cannot
    // apply (unknown name, wrong argument count, failed expression) and keeps
    // the operations that did apply. Strict mode reports the first failure.
    for operation in &transformation.children {
        let arguments = operation.attribute("Argument").unwrap_or("");
        match transform.transform_by_string(&operation.name, arguments, Some(parameters), true) {
            Ok(()) => {}
            Err(error) if native_compatibility => {
                warnings.push(StructureXmlWarning::InvalidTransformSkipped {
                    kind: operation.name.clone(),
                    reason: error.to_string(),
                });
            }
            Err(error) => return Err(StructureXmlError::Transform(error)),
        }
    }
    Ok(())
}

fn read_endpoints<'a>(
    node: &'a XmlNode,
    start_name: &str,
    stop_name: &str,
) -> Result<([&'a str; 3], [&'a str; 3]), StructureXmlError> {
    Ok((
        read_coordinate_terms(required_child(node, start_name)?)?,
        read_coordinate_terms(required_child(node, stop_name)?)?,
    ))
}

fn read_coordinate_terms(node: &XmlNode) -> Result<[&str; 3], StructureXmlError> {
    Ok([
        required_attribute(node, "X")?,
        required_attribute(node, "Y")?,
        required_attribute(node, "Z")?,
    ])
}

fn required_child<'a>(node: &'a XmlNode, name: &str) -> Result<&'a XmlNode, StructureXmlError> {
    node.child(name)
        .ok_or_else(|| StructureXmlError::MissingChild {
            node: node.name.clone(),
            child: name.to_owned(),
        })
}

fn required_attribute<'a>(node: &'a XmlNode, name: &str) -> Result<&'a str, StructureXmlError> {
    node.attribute(name)
        .ok_or_else(|| StructureXmlError::MissingAttribute {
            node: node.name.clone(),
            attribute: name.to_owned(),
        })
}

fn parse_coordinate_system(
    value: &str,
    field: &str,
) -> Result<CoordinateSystem, StructureXmlError> {
    match value.parse::<i32>() {
        Ok(0) => Ok(CoordinateSystem::Cartesian),
        Ok(1) => Ok(CoordinateSystem::Cylindrical),
        Ok(2) => Ok(CoordinateSystem::Undefined),
        _ => Err(StructureXmlError::InvalidValue {
            field: field.to_owned(),
            value: value.to_owned(),
        }),
    }
}

fn parse_number(value: &str, field: &str) -> Result<f64, StructureXmlError> {
    value
        .parse::<f64>()
        .map_err(|_| StructureXmlError::InvalidValue {
            field: field.to_owned(),
            value: value.to_owned(),
        })
}

fn parse_integer(value: &str, field: &str) -> Result<i32, StructureXmlError> {
    value
        .parse::<i32>()
        .map_err(|_| StructureXmlError::InvalidValue {
            field: field.to_owned(),
            value: value.to_owned(),
        })
}

fn parse_bool(value: &str, field: &str) -> Result<bool, StructureXmlError> {
    match value.parse::<i32>() {
        Ok(0) => Ok(false),
        Ok(_) => Ok(true),
        Err(_) => Err(StructureXmlError::InvalidValue {
            field: field.to_owned(),
            value: value.to_owned(),
        }),
    }
}

fn property_xml_name(kind: PropertyType) -> Result<&'static str, StructureXmlError> {
    match kind {
        PropertyType::Unknown => Ok("Unknown"),
        PropertyType::Material => Ok("Material"),
        PropertyType::Metal => Ok("Metal"),
        PropertyType::ConductingSheet => Ok("ConductingSheet"),
        PropertyType::AbsorbingBoundary => Ok("AbsorbingBC"),
        PropertyType::LumpedElement => Ok("LumpedElement"),
        PropertyType::ProbeBox => Ok("ProbeBox"),
        PropertyType::DumpBox => Ok("DumpBox"),
        PropertyType::Excitation => Ok("Excitation"),
        PropertyType::ResBox => Ok("ResBox"),
        PropertyType::DispersiveMaterial => Ok("DispersiveMaterial"),
        PropertyType::DebyeMaterial => Ok("DebyeMaterial"),
        PropertyType::LorentzMaterial => Ok("LorentzMaterial"),
        PropertyType::DiscreteMaterial => Ok("DiscMaterial"),
        _ => Err(StructureXmlError::UnsupportedPropertyType(format!(
            "{kind:?}"
        ))),
    }
}

fn property_type(name: &str) -> Result<PropertyType, StructureXmlError> {
    match name {
        "Unknown" => Ok(PropertyType::Unknown),
        "Material" => Ok(PropertyType::Material),
        "Metal" => Ok(PropertyType::Metal),
        "ConductingSheet" => Ok(PropertyType::ConductingSheet),
        "AbsorbingBC" => Ok(PropertyType::AbsorbingBoundary),
        "LumpedElement" => Ok(PropertyType::LumpedElement),
        "ProbeBox" | "ChargeBox" => Ok(PropertyType::ProbeBox),
        "DumpBox" => Ok(PropertyType::DumpBox),
        "Excitation" => Ok(PropertyType::Excitation),
        "ResBox" => Ok(PropertyType::ResBox),
        "DispersiveMaterial" => Ok(PropertyType::DispersiveMaterial),
        "DebyeMaterial" => Ok(PropertyType::DebyeMaterial),
        "LorentzMaterial" => Ok(PropertyType::LorentzMaterial),
        "DiscMaterial" | "Discrete-Material" => Ok(PropertyType::DiscreteMaterial),
        _ => Err(StructureXmlError::UnsupportedPropertyType(name.to_owned())),
    }
}

fn primitive_xml_name(primitive: &Primitive) -> Result<&'static str, StructureXmlError> {
    match primitive.primitive_type() {
        crate::model::PrimitiveType::Point => Ok("Point"),
        crate::model::PrimitiveType::Box => Ok("Box"),
        crate::model::PrimitiveType::Sphere => Ok("Sphere"),
        crate::model::PrimitiveType::SphericalShell => Ok("SphericalShell"),
        crate::model::PrimitiveType::Cylinder => Ok("Cylinder"),
        crate::model::PrimitiveType::CylindricalShell => Ok("CylindricalShell"),
        crate::model::PrimitiveType::Curve => Ok("Curve"),
        crate::model::PrimitiveType::Wire => Ok("Wire"),
        crate::model::PrimitiveType::Polygon => Ok("Polygon"),
        crate::model::PrimitiveType::LinPoly => Ok("LinPoly"),
        crate::model::PrimitiveType::RotPoly => Ok("RotPoly"),
        crate::model::PrimitiveType::Polyhedron => Ok("Polyhedron"),
        crate::model::PrimitiveType::PolyhedronReader => Ok("PolyhedronReader"),
        crate::model::PrimitiveType::MultiBox => Ok("MultiBox"),
        crate::model::PrimitiveType::UserDefined => Ok("User-Defined"),
    }
}

fn transform_xml_name(kind: TransformType) -> &'static str {
    // Native CSTransform::GetNameByType (CSTransform.cpp) and therefore the
    // element names produced by CSTransform::Write2XML. Native
    // CSTransform::GetTypeByName rejects any other spelling.
    match kind {
        TransformType::Scale => "Scale",
        TransformType::Scale3 => "Scale3",
        TransformType::Translate => "Translate",
        TransformType::RotateOrigin => "Rotate_Origin",
        TransformType::RotateX => "Rotate_X",
        TransformType::RotateY => "Rotate_Y",
        TransformType::RotateZ => "Rotate_Z",
        TransformType::Matrix => "Matrix",
    }
}

fn material_fields() -> [(MaterialField, &'static str); 4] {
    [
        (MaterialField::Epsilon, "Epsilon"),
        (MaterialField::Mue, "Mue"),
        (MaterialField::Kappa, "Kappa"),
        (MaterialField::Sigma, "Sigma"),
    ]
}

#[cfg(test)]
mod tests {
    use super::{Structure, StructureXmlError, StructureXmlWarning};
    use crate::grid::CoordinateSystem;
    use crate::material::MaterialField;
    use crate::model::{
        PolyhedronFileType, Primitive, PrimitiveType, Property, PropertyType,
        UserDefinedCoordinateSystem,
    };
    use crate::parameter::ParameterError;

    #[test]
    fn round_trips_parameters_grid_properties_materials_and_primitive_families() {
        let mut structure = Structure::new();
        structure.set_coordinate_system(CoordinateSystem::Cylindrical);
        structure
            .background_material_mut()
            .set_value(MaterialField::Epsilon, 2.5);
        structure
            .background_material_mut()
            .set_value(MaterialField::Sigma, 0.25);
        structure.parameters_mut().add("height", 2.0);
        structure
            .parameters_mut()
            .add_linear("radius", 1.0, 0.5, 2.0, 0.5);
        structure.grid_mut().set_delta_unit(0.001);
        for axis in 0..3 {
            structure.grid_mut().add_lines(axis, &[0.0, 1.0]);
        }

        let mut material = Property::new("dielectric & core", PropertyType::Material);
        material.set_visible(false);
        material.set_attribute("source", "xml & rust");
        let material_data = material.material_mut().unwrap();
        material_data.set_isotropic(false);
        material_data
            .set_value_expression(MaterialField::Epsilon, 1, "2+radius")
            .unwrap();
        material_data.set_density(4.0);
        let property_id = structure.add_property(material);

        let primitives = [
            Primitive::point_from_expressions(["0", "0", "height"]).unwrap(),
            Primitive::box_from_expressions(["0", "0", "0"], ["radius", "1", "height"]).unwrap(),
            Primitive::sphere_from_expressions(["0", "0", "0"], "radius").unwrap(),
            Primitive::cylinder_from_expressions(["0", "0", "0"], ["0", "0", "height"], "radius")
                .unwrap(),
        ];
        for primitive in primitives {
            structure.add_primitive(property_id, primitive).unwrap();
        }
        let mut multi_box = Primitive::multibox();
        multi_box
            .multibox_shape_mut()
            .unwrap()
            .add_box(crate::model::BoxPrimitive::new([0.0; 3], [1.0; 3]));
        structure.add_primitive(property_id, multi_box).unwrap();

        let xml = structure.to_xml_string().unwrap();
        let mut restored = Structure::from_xml_str(&xml).unwrap();
        assert_eq!(restored.coordinate_system(), CoordinateSystem::Cylindrical);
        assert_eq!(restored.grid().delta_unit(), 0.001);
        assert_eq!(
            restored.background_material().value(MaterialField::Epsilon),
            2.5
        );
        assert_eq!(
            restored.background_material().value(MaterialField::Sigma),
            0.25
        );
        assert_eq!(restored.parameters().len(), 2);
        assert_eq!(restored.primitive_count(), 5);
        let restored_property = restored.property_by_index(0).unwrap();
        assert_eq!(restored_property.name(), "dielectric & core");
        assert!(!restored_property.is_visible());
        assert_eq!(restored_property.attribute("source"), "xml & rust");
        assert_eq!(
            restored_property
                .material()
                .unwrap()
                .value(MaterialField::Epsilon, 1, Some(restored.parameters()))
                .unwrap(),
            Some(3.0)
        );
        assert_eq!(
            restored_property
                .primitive(4)
                .unwrap()
                .multibox_shape()
                .unwrap()
                .box_count(),
            1
        );
        assert_eq!(restored.grid_mut().lines(0).unwrap(), &[0.0, 1.0]);
    }

    #[test]
    fn round_trips_user_defined_primitives_and_evaluates_native_coordinate_variables() {
        let mut structure = Structure::new();
        structure.parameters_mut().add("offset", 1.0);
        let property_id = structure.add_property(Property::new("custom", PropertyType::Material));

        let mut shifted = Primitive::user_defined();
        shifted.transform_mut().translate([1.0, 0.0, 0.0], true);
        let shifted_shape = shifted.user_defined_shape_mut().unwrap();
        shifted_shape
            .set_coordinate_shift_expression(0, "offset")
            .unwrap();
        shifted_shape.set_function("offset+x>0");
        structure.add_primitive(property_id, shifted).unwrap();

        let mut spherical = Primitive::user_defined();
        let spherical_shape = spherical.user_defined_shape_mut().unwrap();
        spherical_shape.set_coordinate_system(UserDefinedCoordinateSystem::Spherical);
        spherical_shape.set_function("t>0.7");
        structure.add_primitive(property_id, spherical).unwrap();

        let mut cylindrical = Primitive::user_defined();
        let cylindrical_shape = cylindrical.user_defined_shape_mut().unwrap();
        cylindrical_shape.set_coordinate_system(UserDefinedCoordinateSystem::Cylindrical);
        cylindrical_shape.set_function("r>1&a>1");
        structure.add_primitive(property_id, cylindrical).unwrap();

        let xml = structure.to_xml_string().unwrap();
        assert!(xml.contains("<User-Defined"));
        assert!(xml.contains("<CoordShift"));
        assert!(xml.contains("<Function>offset+x&gt;0</Function>"));
        let restored = Structure::from_xml_str(&xml).unwrap();
        let property = restored.property_by_index(0).unwrap();
        let shifted = property.primitive(0).unwrap();
        assert_eq!(
            shifted.primitive_type(),
            crate::model::PrimitiveType::UserDefined
        );
        assert!(shifted
            .is_inside([3.0, 0.0, 0.0], Some(restored.parameters()))
            .unwrap());
        assert!(!shifted
            .is_inside([1.0, 0.0, 0.0], Some(restored.parameters()))
            .unwrap());
        assert_eq!(shifted.dimension(Some(restored.parameters())).unwrap(), 3);
        assert_eq!(
            shifted
                .bounding_box(Some(restored.parameters()), false)
                .unwrap(),
            [
                -f64::MAX,
                f64::MAX,
                -f64::MAX,
                f64::MAX,
                -f64::MAX,
                f64::MAX
            ]
        );
        assert!(property
            .primitive(1)
            .unwrap()
            .is_inside([1.0, 0.0, 1.0], Some(restored.parameters()))
            .unwrap());
        assert!(property
            .primitive(2)
            .unwrap()
            .is_inside([-2.0, 2.0, 0.0], Some(restored.parameters()))
            .unwrap());
        let native_reader_alias = xml
            .replace("<User-Defined", "<UserDefined")
            .replace("</User-Defined>", "</UserDefined>");
        let alias_structure = Structure::from_xml_str(&native_reader_alias).unwrap();
        assert_eq!(alias_structure, restored);
        let canonical_alias_xml = alias_structure.to_xml_string().unwrap();
        assert!(canonical_alias_xml.contains("<User-Defined"));
        assert!(!canonical_alias_xml.contains("<UserDefined"));

        let mut invalid = Primitive::user_defined();
        invalid.user_defined_shape_mut().unwrap().set_function("x>");
        assert!(matches!(
            invalid.is_inside([0.0; 3], None),
            Err(crate::model::ModelError::Parameter(ParameterError::Parse(
                _
            )))
        ));
    }

    #[test]
    fn loads_polyhedron_reader_files_and_round_trips_native_xml_metadata() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "csxcad-polyhedron-reader-{}-{timestamp}.ply",
            std::process::id()
        ));
        std::fs::write(
            &path,
            b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n",
        )
        .unwrap();

        let mut structure = Structure::new();
        let property_id = structure.add_property(Property::new("imported", PropertyType::Material));
        let mut primitive = Primitive::polyhedron_reader();
        let reader = primitive.polyhedron_reader_shape_mut().unwrap();
        reader.set_filename(path.to_string_lossy());
        reader.set_file_type(PolyhedronFileType::Ply);
        reader.read_file().unwrap();
        structure.add_primitive(property_id, primitive).unwrap();

        let xml = structure.to_xml_string().unwrap();
        assert!(xml.contains("<PolyhedronReader"));
        assert!(xml.contains("FileType=\"PLY\""));
        let restored = Structure::from_xml_str(&xml).unwrap();
        let imported = restored
            .property_by_index(0)
            .unwrap()
            .primitive(0)
            .unwrap()
            .polyhedron_reader_shape()
            .unwrap();
        assert_eq!(imported.file_type(), PolyhedronFileType::Ply);
        assert_eq!(imported.polyhedron().vertex_count(), 3);
        assert_eq!(imported.polyhedron().face_count(), 1);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_unknown_property_and_missing_required_geometry_fields() {
        let unknown =
            "<ContinuousStructure><Properties><Probe/></Properties></ContinuousStructure>";
        assert!(matches!(
            Structure::from_xml_str(unknown),
            Err(StructureXmlError::UnsupportedPropertyType(_))
        ));

        let missing = "<ContinuousStructure><Properties><Metal><Primitives><Box Priority=\"0\"/></Primitives></Metal></Properties></ContinuousStructure>";
        assert!(matches!(
            Structure::from_xml_str(missing),
            Err(StructureXmlError::MissingChild { .. })
        ));
        let invalid_background = "<ContinuousStructure><BackgroundMaterial Epsilon=\"0\"/><Properties/></ContinuousStructure>";
        assert!(matches!(
            Structure::from_xml_str(invalid_background),
            Err(StructureXmlError::InvalidValue { .. })
        ));
        let invalid_color = "<ContinuousStructure><Properties><Metal><FillColor R=\"256\" G=\"0\" B=\"0\" a=\"255\"/><Primitives/></Metal></Properties></ContinuousStructure>";
        assert!(matches!(
            Structure::from_xml_str(invalid_color),
            Err(StructureXmlError::InvalidValue { .. })
        ));
        let invalid_polygon = "<ContinuousStructure><Properties><Metal><Primitives><Polygon Priority=\"0\" NormDir=\"3\"/></Primitives></Metal></Properties></ContinuousStructure>";
        assert!(matches!(
            Structure::from_xml_str(invalid_polygon),
            Err(StructureXmlError::InvalidValue { .. })
        ));
    }

    #[test]
    fn compatibility_reader_skips_unsupported_nodes_with_warnings() {
        let xml = "<ContinuousStructure><Properties><Metal Name=\"known\"><Primitives><FuturePrimitive/><Box Priority=\"8\"/><Point Priority=\"2\" X=\"1\" Y=\"2\" Z=\"3\"/></Primitives></Metal><FutureProperty/></Properties></ContinuousStructure>";
        let (structure, warnings) = Structure::from_xml_str_with_warnings(xml).unwrap();

        assert_eq!(structure.property_count(), 1);
        assert_eq!(structure.property_by_index(0).unwrap().primitive_count(), 1);
        assert_eq!(
            structure
                .property_by_index(0)
                .unwrap()
                .primitive(0)
                .unwrap()
                .priority(),
            2
        );
        assert_eq!(
            warnings,
            [
                StructureXmlWarning::UnsupportedPrimitiveSkipped("FuturePrimitive".to_owned()),
                StructureXmlWarning::InvalidPrimitiveSkipped {
                    kind: "Box".to_owned(),
                    reason: "<Box> is missing child <P1>".to_owned(),
                },
                StructureXmlWarning::UnsupportedPropertySkipped("FutureProperty".to_owned()),
            ]
        );
        assert!(matches!(
            Structure::from_xml_str(xml),
            Err(StructureXmlError::UnsupportedPrimitiveType(_))
        ));
    }

    #[test]
    fn compatibility_reader_warns_when_a_property_has_no_primitive_list() {
        let xml = "<ContinuousStructure><Properties><Metal Name=\"empty\"/></Properties></ContinuousStructure>";
        let (structure, warnings) = Structure::from_xml_str_with_warnings(xml).unwrap();

        assert_eq!(structure.property_count(), 1);
        assert_eq!(
            warnings,
            [StructureXmlWarning::MissingPrimitives("empty".to_owned())]
        );
    }

    #[test]
    fn compatibility_reader_warns_when_the_properties_node_is_missing() {
        let xml = "<ContinuousStructure/>";
        let (structure, warnings) = Structure::from_xml_str_with_warnings(xml).unwrap();

        assert_eq!(structure.property_count(), 0);
        assert_eq!(warnings, [StructureXmlWarning::PropertiesMissing]);
        assert!(matches!(
            Structure::from_xml_str(xml),
            Err(StructureXmlError::MissingChild { .. })
        ));
    }

    #[test]
    fn compatibility_reader_matches_native_integer_visibility_attributes() {
        let xml = "<ContinuousStructure><Properties><Metal Name=\"text\" Visible=\"false\"><Primitives/></Metal><Metal Name=\"numeric\" Visible=\"0\"><Primitives/></Metal></Properties></ContinuousStructure>";
        let (structure, warnings) = Structure::from_xml_str_with_warnings(xml).unwrap();

        assert!(warnings.is_empty());
        assert!(structure.property_by_index(0).unwrap().is_visible());
        assert!(!structure.property_by_index(1).unwrap().is_visible());
    }

    #[test]
    fn compatibility_reader_skips_invalid_parameters_and_keeps_valid_entries() {
        let xml = "<ContinuousStructure><ParameterSet><Parameter Type=\"Const\" name=\"kept\" value=\"2\"/><Parameter Type=\"Future\" name=\"unsupported\" value=\"5\"/><Parameter Type=\"Linear\" name=\"malformed\" value=\"1\" min=\"0\"/><Parameter name=\"missing-type\" value=\"3\"/><Parameter Type=\"Linear\" name=\"sweep-default\" value=\"2\" min=\"0\" max=\"4\" step=\"1\" Sweep=\"false\"/><Parameter Type=\"Const\" name=\"after\" value=\"7\"/></ParameterSet><Properties/></ContinuousStructure>";
        let (structure, warnings) = Structure::from_xml_str_with_warnings(xml).unwrap();

        let names: Vec<_> = structure
            .parameters()
            .iter()
            .map(|parameter| parameter.name())
            .collect();
        assert_eq!(names, ["kept", "sweep-default", "after"]);
        assert!(structure
            .parameters()
            .iter()
            .nth(1)
            .unwrap()
            .sweep_enabled());
        assert_eq!(
            warnings,
            [
                StructureXmlWarning::InvalidParameterSkipped {
                    kind: Some("Future".to_owned()),
                    name: Some("unsupported".to_owned()),
                    reason: "invalid Parameter.Type value \"Future\"".to_owned(),
                },
                StructureXmlWarning::InvalidParameterSkipped {
                    kind: Some("Linear".to_owned()),
                    name: Some("malformed".to_owned()),
                    reason: "<Parameter> is missing attribute \"max\"".to_owned(),
                },
                StructureXmlWarning::InvalidParameterSkipped {
                    kind: None,
                    name: Some("missing-type".to_owned()),
                    reason: "<Parameter> is missing attribute \"Type\"".to_owned(),
                },
            ]
        );
        assert!(matches!(
            Structure::from_xml_str(xml),
            Err(StructureXmlError::InvalidValue { .. })
        ));
    }

    #[test]
    fn compatibility_reader_retains_invalid_properties_as_unknown() {
        let xml = "<ContinuousStructure><Properties><Excitation Name=\"fallback\" Visible=\"0\" Property=\"wave\"><Attributes source=\"legacy\"/><Primitives><Point Priority=\"1\" X=\"1\" Y=\"2\" Z=\"3\"/></Primitives></Excitation></Properties></ContinuousStructure>";
        let (structure, warnings) = Structure::from_xml_str_with_warnings(xml).unwrap();

        assert_eq!(structure.property_count(), 1);
        let property = structure.property_by_index(0).unwrap();
        assert_eq!(property.property_type(), PropertyType::Unknown);
        assert_eq!(property.name(), "fallback");
        assert_eq!(property.unknown_property(), Some("wave"));
        assert_eq!(property.attribute("source"), "legacy");
        assert!(!property.is_visible());
        assert_eq!(property.primitive_count(), 1);
        assert_eq!(
            warnings,
            [StructureXmlWarning::InvalidPropertyRetainedAsUnknown {
                kind: "Excitation".to_owned(),
                reason: "<Excitation> is missing attribute \"Type\"".to_owned(),
            }]
        );
        assert!(matches!(
            Structure::from_xml_str(xml),
            Err(StructureXmlError::MissingAttribute { .. })
        ));
    }

    #[test]
    fn unknown_property_defaults_match_native_xml_state() {
        let mut structure = Structure::new();
        structure.add_property(Property::new("opaque", PropertyType::Unknown));

        let xml = structure.to_xml_string().unwrap();
        assert!(xml.contains("Visible=\"0\""));
        assert!(xml.contains("Property=\"\""));

        let restored = Structure::from_xml_str(&xml).unwrap();
        let property = restored.property_by_index(0).unwrap();
        assert!(!property.is_visible());
        assert_eq!(property.unknown_property(), Some(""));
    }

    #[test]
    fn reads_legacy_property_aliases_and_writes_canonical_names() {
        let xml = "<ContinuousStructure><Properties><ChargeBox><Primitives/></ChargeBox><Discrete-Material><Primitives/></Discrete-Material></Properties></ContinuousStructure>";
        let structure = Structure::from_xml_str(xml).unwrap();
        assert_eq!(structure.property_count(), 2);
        assert_eq!(
            structure.property_by_index(0).unwrap().property_type(),
            PropertyType::ProbeBox
        );
        assert_eq!(
            structure.property_by_index(1).unwrap().property_type(),
            PropertyType::DiscreteMaterial
        );

        let canonical = structure.to_xml_string().unwrap();
        assert!(canonical.contains("<ProbeBox"));
        assert!(canonical.contains("<DiscMaterial"));
        assert!(!canonical.contains("<ChargeBox"));
        assert!(!canonical.contains("<Discrete-Material"));
    }

    #[test]
    fn reads_native_generated_continuous_structure_xml() {
        let structure =
            Structure::from_xml_str(include_str!("../tests/fixtures/native_structure.xml"))
                .unwrap();

        assert_eq!(structure.grid().delta_unit(), 1.0);
        assert_eq!(structure.property_count(), 1);
        assert_eq!(structure.primitive_count(), 1);

        let material = structure.property_by_index(0).unwrap();
        assert_eq!(material.name(), "native-material");
        assert_eq!(
            material.fill_color().unwrap().components(),
            [103, 198, 105, 123]
        );
        assert_eq!(
            material
                .material()
                .unwrap()
                .value(MaterialField::Epsilon, 0, None)
                .unwrap(),
            Some(2.5)
        );

        let primitive = material.primitive(0).unwrap();
        assert_eq!(primitive.primitive_type(), PrimitiveType::Box);
        assert_eq!(
            primitive.bounding_box(None, false).unwrap(),
            [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]
        );

        let canonical_xml = structure.to_xml_string().unwrap();
        let round_tripped = Structure::from_xml_str(&canonical_xml).unwrap();
        assert_eq!(round_tripped, structure);
    }

    #[test]
    fn reads_native_generated_parameterized_grid_and_geometry_xml() {
        let mut structure = Structure::from_xml_str(include_str!(
            "../tests/fixtures/native_parameterized_structure.xml"
        ))
        .unwrap();

        assert_eq!(structure.grid().delta_unit(), 0.001);
        assert_eq!(structure.grid_mut().lines(0).unwrap(), &[0.0, 1.0]);
        assert_eq!(structure.grid_mut().lines(1).unwrap(), &[0.0, 2.0]);
        assert_eq!(structure.grid_mut().lines(2).unwrap(), &[0.0, 3.0]);

        let parameters = structure.parameters();
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters.get(0).unwrap().name(), "factor");
        assert_eq!(parameters.get(0).unwrap().value(), 2.0);
        assert_eq!(parameters.get(1).unwrap().name(), "radius");
        assert_eq!(
            parameters.get(1).unwrap().linear_range(),
            Some((0.5, 2.0, 0.5))
        );

        let property = structure.property_by_index(0).unwrap();
        assert_eq!(property.name(), "parameterized-material");
        assert_eq!(
            property
                .material()
                .unwrap()
                .value(MaterialField::Epsilon, 0, None)
                .unwrap(),
            Some(2.5)
        );
        assert_eq!(
            property
                .primitive(0)
                .unwrap()
                .bounding_box(Some(structure.parameters()), false)
                .unwrap(),
            [0.0, 1.0, 0.0, 1.0, 0.0, 2.0]
        );
    }

    #[test]
    fn reads_native_shaped_parameterized_material_xml() {
        let xml = r#"
            <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <openEMS>
              <ContinuousStructure CoordSystem="0">
                <RectilinearGrid DeltaUnit="0.001" CoordSystem="0">
                  <XLines Qty="2">0,1</XLines>
                  <YLines Qty="2">0,1</YLines>
                  <ZLines Qty="2">0,1</ZLines>
                </RectilinearGrid>
                <BackgroundMaterial Epsilon="2.5" Mue="1.2" Kappa="0.1" Sigma="0.2"/>
                <ParameterSet>
                  <Parameter Type="Const" name="factor" Sweep="0" value="2"/>
                </ParameterSet>
                <Properties>
                  <Material ID="42" Name="substrate" Isotropy="1">
                    <FillColor R="12" G="34" B="56" a="78"/>
                    <EdgeColor R="87" G="65" B="43" a="21"/>
                    <Primitives>
                      <Box Priority="0">
                        <P1 X="0" Y="0" Z="0"/>
                        <P2 X="factor" Y="1" Z="1"/>
                        <Transformation>
                          <Translate Argument="factor,0,0"/>
                        </Transformation>
                      </Box>
                      <SphericalShell Priority="0" Radius="1" ShellWidth="0.2">
                        <Center X="0" Y="0" Z="0"/>
                      </SphericalShell>
                      <CylindricalShell Priority="0" Radius="1" ShellWidth="0.25">
                        <P1 X="0" Y="0" Z="0"/>
                        <P2 X="0" Y="0" Z="2"/>
                      </CylindricalShell>
                      <Curve Priority="0">
                        <Vertex X="0" Y="0" Z="0"/>
                        <Vertex X="1" Y="2" Z="3"/>
                      </Curve>
                      <Wire Priority="0" WireRadius="0.1">
                        <Vertex X="0" Y="0" Z="0"/>
                        <Vertex X="0" Y="0" Z="1"/>
                      </Wire>
                      <Polygon Priority="0" Elevation="factor" NormDir="2" QtyVertices="3">
                        <Vertex X1="0" X2="0"/>
                        <Vertex X1="factor" X2="0"/>
                        <Vertex X1="0" X2="factor"/>
                      </Polygon>
                      <LinPoly Priority="0" Elevation="0" NormDir="2" QtyVertices="3" Length="factor">
                        <Vertex X1="0" X2="0"/>
                        <Vertex X1="1" X2="0"/>
                        <Vertex X1="0" X2="1"/>
                      </LinPoly>
                      <Polyhedron Priority="0">
                        <Vertex>0,0,0</Vertex>
                        <Vertex>1,0,0</Vertex>
                        <Vertex>0,1,0</Vertex>
                        <Face>0,1,2</Face>
                      </Polyhedron>
                      <RotPoly Priority="0" Elevation="3" NormDir="1" RotAxisDir="2" QtyVertices="3">
                        <Angles Start="0" Stop="1.57"/>
                        <Vertex X1="0" X2="1"/>
                        <Vertex X1="1" X2="1"/>
                        <Vertex X1="0" X2="2"/>
                      </RotPoly>
                    </Primitives>
                    <Property Epsilon="3*factor,3*factor,3*factor" Mue="1,1,1"
                              Kappa="0,0,0" Sigma="0,0,0" Density="0"/>
                    <Weight Epsilon="1,1,1" Mue="1,1,1" Kappa="1,1,1"
                            Sigma="1,1,1" Density="1"/>
                  </Material>
                  <ConductingSheet ID="7" Name="sheet" Conductivity="2*factor"
                                   Thickness="0.01">
                    <Primitives/>
                  </ConductingSheet>
                  <AbsorbingBC ID="11" Name="boundary" NormalSignPositive="0"
                               PhaseVelocity="factor*100" AbsorbingBoundaryType="17">
                    <Primitives/>
                  </AbsorbingBC>
                  <LumpedElement ID="12" Name="lumped" Direction="1" Caps="0"
                                 R="factor*10" C="1e-12" L="2e-9" LEtype="1">
                    <Primitives/>
                  </LumpedElement>
                  <LumpedElement ID="13" Name="default-lumped">
                    <Primitives/>
                  </LumpedElement>
                  <ProbeBox ID="14" Name="probe" Number="3" Type="2" Weight="0.5"
                            NormDir="2" StartTime="0.123456789" StopTime="2e-9"
                            OverSampling="4" ModeFile="mode.h5">
                    <Primitives/>
                    <FD_Samples>1e9,2e9</FD_Samples>
                    <ModeFunction X="x+y" Z="z"/>
                    <ModeOrigin X="1" Y="2" Z="3"/>
                  </ProbeBox>
                  <ProbeBox ID="15" Name="legacy-probe">
                    <Attributes ModeFunctionY="x*z"/>
                    <Primitives/>
                  </ProbeBox>
                  <ProbeBox ID="16" Name="default-probe">
                    <Primitives/>
                  </ProbeBox>
                  <DumpBox ID="17" Name="dump" Number="7" Type="3" Weight="0.75"
                           NormDir="1" StartTime="1e-9" StopTime="2e-9" OverSampling="5"
                           DumpType="2" DumpMode="1" FileType="3" MultiGridLevel="4"
                           SubSampling="2,3,4" OptResolution="0.25">
                    <Primitives/>
                    <FD_Samples>5e8,1e9</FD_Samples>
                  </DumpBox>
                  <Excitation ID="18" Name="source" Number="8" Enabled="0"
                              Frequency="factor*10" Delay="factor/10" Type="4"
                              Excite="0,factor,1" PropDir="0,0,1">
                    <Primitives/>
                    <Weight X="factor" Y="2" Z="1"/>
                    <WeightOrigin X="1" Y="2" Z="3"/>
                  </Excitation>
                  <ResBox ID="19" Name="refinement" Factor="4">
                    <Primitives/>
                  </ResBox>
                  <ResBox ID="20" Name="default-refinement">
                    <Primitives/>
                  </ResBox>
                  <Unknown ID="21" Name="unmapped" Property="probeunknown">
                    <Primitives/>
                  </Unknown>
                  <DebyeMaterial ID="22" Name="debye" Isotropy="0">
                    <Property Epsilon="1,1,1" EpsilonDelta_1="factor,2,3"
                              EpsilonRelaxTime_1="factor/10,0.2,0.3"
                              EpsilonDelta_2="3,4,5"
                              EpsilonRelaxTime_2="0.4,0.5,0.6"/>
                    <Weight EpsilonDelta_1="x,1,1" EpsilonRelaxTime_1="1,1,1"
                            EpsilonDelta_2="1,1,1" EpsilonRelaxTime_2="1,1,1"/>
                    <Primitives/>
                  </DebyeMaterial>
                  <DiscMaterial ID="23" Name="discrete" Isotropy="1">
                    <Property Epsilon="1,1,1" Mue="1,1,1" Kappa="0,0,0"
                              Sigma="0,0,0" Density="0"/>
                    <Weight Epsilon="1,1,1" Mue="1,1,1" Kappa="1,1,1"
                            Sigma="1,1,1" Density="1"/>
                    <Transformation>
                      <Translate Argument="factor,0,0"/>
                    </Transformation>
                    <DiscFile Type="0" File="mesh.h5" UseDBBackground="0" Scale="0.001"/>
                    <Primitives/>
                  </DiscMaterial>
                  <DiscMaterial ID="24" Name="legacy-discrete" File="legacy.h5"
                                Type="0" UseDBBackground="1" Scale="2">
                    <Primitives/>
                  </DiscMaterial>
                  <LorentzMaterial ID="25" Name="lorentz" Isotropy="1">
                    <Property Epsilon="1,1,1" EpsilonPlasmaFrequency_1="factor,2,3"
                              MueLorPoleFrequency_1="0.1,0.2,0.3"
                              EpsilonPlasmaFrequency_2="4,5,6"/>
                    <Weight EpsilonPlasmaFrequency_1="x+1,1,1"/>
                    <Primitives/>
                  </LorentzMaterial>
                  <DispersiveMaterial ID="26" Name="dispersive-base">
                    <Property Epsilon="2,2,2" Mue="1,1,1" Kappa="0,0,0"
                              Sigma="0,0,0" Density="0"/>
                    <Weight Epsilon="1,1,1" Mue="1,1,1" Kappa="1,1,1"
                            Sigma="1,1,1" Density="1"/>
                    <Primitives/>
                  </DispersiveMaterial>
                </Properties>
              </ContinuousStructure>
            </openEMS>
        "#;

        let restored = Structure::from_xml_str(xml).unwrap();
        assert_eq!(restored.grid().delta_unit(), 0.001);
        assert_eq!(
            restored.background_material().value(MaterialField::Epsilon),
            2.5
        );
        assert_eq!(
            restored.background_material().value(MaterialField::Kappa),
            0.1
        );
        assert_eq!(restored.parameters().get(0).unwrap().value(), 2.0);
        let property = restored.property_by_index(0).unwrap();
        assert_eq!(property.name(), "substrate");
        assert_eq!(
            property.fill_color().unwrap().components(),
            [12, 34, 56, 78]
        );
        assert_eq!(
            property.edge_color().unwrap().components(),
            [87, 65, 43, 21]
        );
        assert_eq!(
            property
                .material()
                .unwrap()
                .value(MaterialField::Epsilon, 0, Some(restored.parameters()))
                .unwrap(),
            Some(6.0)
        );
        let primitive = property.primitive(0).unwrap();
        assert_eq!(primitive.transform().unwrap().operations().len(), 1);
        assert_eq!(
            primitive
                .box_shape()
                .unwrap()
                .evaluated_coordinates(Some(restored.parameters()))
                .unwrap(),
            [[0.0, 0.0, 0.0], [2.0, 1.0, 1.0]]
        );
        let round_trip_xml = restored.to_xml_string().unwrap();
        let round_trip = Structure::from_xml_str(&round_trip_xml).unwrap();
        assert!(round_trip_xml.contains("<Material ID=\"0\""));
        assert!(round_trip_xml.contains("<ConductingSheet ID=\"1\""));
        assert!(round_trip_xml.contains("<AbsorbingBC ID=\"2\""));
        assert!(round_trip_xml.contains("<LumpedElement ID=\"3\""));
        assert!(round_trip_xml.contains("<ProbeBox ID=\"5\""));
        assert!(round_trip_xml.contains("<DumpBox ID=\"8\""));
        assert!(round_trip_xml.contains("<Excitation ID=\"9\""));
        assert!(round_trip_xml.contains("<ResBox ID=\"10\""));
        assert!(round_trip_xml.contains("<ResBox ID=\"11\""));
        assert!(round_trip_xml.contains("<Unknown ID=\"12\""));
        assert!(round_trip_xml.contains("<DebyeMaterial ID=\"13\""));
        assert!(round_trip_xml.contains("<DiscMaterial ID=\"14\""));
        assert!(round_trip_xml.contains("<DiscMaterial ID=\"15\""));
        assert!(round_trip_xml.contains("<LorentzMaterial ID=\"16\""));
        assert!(round_trip_xml.contains("<DispersiveMaterial ID=\"17\""));
        let round_trip_property = round_trip.property_by_index(0).unwrap();
        assert_eq!(round_trip_property.fill_color(), property.fill_color());
        assert_eq!(round_trip_property.edge_color(), property.edge_color());
        let shell = round_trip_property.primitive(1).unwrap();
        assert_eq!(
            shell
                .spherical_shell_shape()
                .unwrap()
                .shell_width(Some(round_trip.parameters()))
                .unwrap(),
            0.2
        );
        let cylindrical_shell = round_trip_property.primitive(2).unwrap();
        assert_eq!(
            cylindrical_shell
                .cylindrical_shell_shape()
                .unwrap()
                .shell_width(Some(round_trip.parameters()))
                .unwrap(),
            Some(0.25)
        );
        assert!(round_trip_xml.contains("<CylindricalShell"));
        assert_eq!(
            round_trip_property
                .primitive(3)
                .unwrap()
                .curve_shape()
                .unwrap()
                .point_count(),
            2
        );
        let wire = round_trip_property.primitive(4).unwrap();
        assert_eq!(wire.primitive_type(), crate::model::PrimitiveType::Wire);
        assert_eq!(
            wire.wire_shape()
                .unwrap()
                .wire_radius(Some(round_trip.parameters()))
                .unwrap(),
            Some(0.1)
        );
        assert!(round_trip_xml.contains("<Curve"));
        assert!(round_trip_xml.contains("<Wire"));
        let polygon = round_trip_property.primitive(5).unwrap();
        assert_eq!(
            polygon.primitive_type(),
            crate::model::PrimitiveType::Polygon
        );
        assert_eq!(polygon.polygon_shape().unwrap().normal_direction(), 2);
        assert_eq!(polygon.polygon_shape().unwrap().vertex_count(), 3);
        assert!(round_trip_xml.contains("<Polygon"));
        let lin_poly = round_trip_property.primitive(6).unwrap();
        assert_eq!(
            lin_poly.primitive_type(),
            crate::model::PrimitiveType::LinPoly
        );
        assert_eq!(
            lin_poly
                .polygon_shape()
                .unwrap()
                .extrusion_length(Some(round_trip.parameters()))
                .unwrap(),
            Some(2.0)
        );
        assert!(round_trip_xml.contains("<LinPoly"));
        let polyhedron = round_trip_property.primitive(7).unwrap();
        let shape = polyhedron.polyhedron_shape().unwrap();
        assert_eq!(shape.vertex_count(), 3);
        assert_eq!(shape.face(0), Some(&[0, 1, 2][..]));
        assert!(round_trip_xml.contains("<Polyhedron"));
        let rot_poly = round_trip_property.primitive(8).unwrap();
        let shape = rot_poly.polygon_shape().unwrap();
        assert_eq!(
            rot_poly.primitive_type(),
            crate::model::PrimitiveType::RotPoly
        );
        assert_eq!(shape.rotation_axis_direction(), Some(2));
        assert_eq!(shape.elevation(Some(round_trip.parameters())).unwrap(), 0.0);
        assert!(round_trip_xml.contains("<RotPoly"));
        let sheet = round_trip.property_by_index(1).unwrap();
        let sheet_data = sheet.conducting_sheet().unwrap();
        assert_eq!(
            sheet_data
                .conductivity(Some(round_trip.parameters()))
                .unwrap(),
            4.0
        );
        assert_eq!(
            sheet_data.thickness(Some(round_trip.parameters())).unwrap(),
            0.01
        );
        let boundary = round_trip.property_by_index(2).unwrap();
        let boundary_data = boundary.absorbing_boundary().unwrap();
        assert!(!boundary_data.normal_sign_positive());
        assert_eq!(
            boundary_data
                .phase_velocity(Some(round_trip.parameters()))
                .unwrap(),
            200.0
        );
        assert_eq!(
            boundary_data.boundary_type(),
            crate::material::AbsorbingBoundaryType::Other(17)
        );
        assert!(round_trip_xml.contains("PhaseVelocity=\"factor*100\""));
        let lumped = round_trip.property_by_index(3).unwrap();
        let lumped_data = lumped.lumped_element().unwrap();
        assert_eq!(lumped_data.direction(), 1);
        assert!(!lumped_data.caps());
        assert_eq!(
            lumped_data
                .resistance(Some(round_trip.parameters()))
                .unwrap(),
            20.0
        );
        assert_eq!(lumped_data.capacitance(None).unwrap(), 1e-12);
        assert_eq!(lumped_data.inductance(None).unwrap(), 2e-9);
        assert_eq!(
            lumped_data.element_type(),
            crate::material::LumpedElementType::Series
        );
        let default_lumped = round_trip.property_by_index(4).unwrap();
        let default_lumped_data = default_lumped.lumped_element().unwrap();
        assert_eq!(default_lumped_data.direction(), -1);
        assert!(default_lumped_data.caps());
        assert!(default_lumped_data.resistance(None).unwrap().is_nan());
        assert!(default_lumped_data.capacitance(None).unwrap().is_nan());
        assert!(default_lumped_data.inductance(None).unwrap().is_nan());
        assert_eq!(
            default_lumped_data.element_type(),
            crate::material::LumpedElementType::Parallel
        );
        assert!(round_trip_xml.contains(
            "<LumpedElement ID=\"4\" Name=\"default-lumped\" Direction=\"-1\" Caps=\"1\" LEtype=\"0\">"
        ));
        let probe_property = round_trip.property_by_index(5).unwrap();
        let probe = probe_property.probe().unwrap();
        assert!(!probe_property.is_visible());
        assert_eq!(probe.number(), 3);
        assert_eq!(probe.probe_type(), 2);
        assert_eq!(probe.weight(), 0.5);
        assert_eq!(probe.normal_direction(), 2);
        assert_eq!(probe.start_time(), 0.123456789);
        assert_eq!(probe.stop_time(), 2e-9);
        assert_eq!(probe.oversampling(), 4);
        assert_eq!(probe.fd_samples(), &[1e9, 2e9]);
        assert_eq!(probe.mode_file(), "mode.h5");
        assert_eq!(probe.mode_function(0), Some("x+y"));
        assert_eq!(probe.mode_function(1), Some(""));
        assert_eq!(probe.mode_function(2), Some("z"));
        assert_eq!(probe.mode_origin(), [1.0, 2.0, 3.0]);
        let legacy_probe = round_trip.property_by_index(6).unwrap();
        assert_eq!(legacy_probe.probe().unwrap().mode_function(1), Some("x*z"));
        assert!(!legacy_probe.has_attribute("ModeFunctionY"));
        let default_probe = round_trip.property_by_index(7).unwrap();
        assert!(!default_probe.is_visible());
        assert_eq!(default_probe.probe().unwrap().number(), 0);
        assert_eq!(default_probe.probe().unwrap().normal_direction(), -1);
        assert_eq!(default_probe.probe().unwrap().weight(), 1.0);
        assert_eq!(default_probe.probe().unwrap().oversampling(), -1);
        let dump_property = round_trip.property_by_index(8).unwrap();
        let dump_probe = dump_property.probe().unwrap();
        assert_eq!(dump_probe.number(), 7);
        assert_eq!(dump_probe.probe_type(), 3);
        assert_eq!(dump_probe.fd_samples(), &[5e8, 1e9]);
        let dump = dump_property.dump_box().unwrap();
        assert_eq!(dump.dump_type(), 2);
        assert_eq!(dump.dump_mode(), 1);
        assert_eq!(dump.file_type(), 3);
        assert_eq!(dump.multi_grid_level(), 4);
        assert!(dump.sub_sampling_enabled());
        assert_eq!(dump.sub_sampling(), [2, 3, 4]);
        assert!(dump.optimal_resolution_enabled());
        assert_eq!(dump.optimal_resolution(), [0.25; 3]);
        let excitation_property = round_trip.property_by_index(9).unwrap();
        let excitation = excitation_property.excitation().unwrap();
        assert_eq!(
            excitation_property.coordinate_input_type(),
            crate::grid::CoordinateSystem::Cartesian
        );
        assert_eq!(excitation.number(), 8);
        assert!(!excitation.is_enabled());
        assert_eq!(excitation.excitation_type(), 4);
        assert_eq!(
            excitation.frequency(Some(round_trip.parameters())).unwrap(),
            20.0
        );
        assert_eq!(
            excitation.delay(Some(round_trip.parameters())).unwrap(),
            0.2
        );
        assert_eq!(
            excitation
                .excitation(1, Some(round_trip.parameters()))
                .unwrap(),
            Some(2.0)
        );
        assert_eq!(
            excitation
                .weighting(0, Some(round_trip.parameters()))
                .unwrap(),
            Some(2.0)
        );
        assert_eq!(
            excitation
                .propagation_direction(2, Some(round_trip.parameters()))
                .unwrap(),
            Some(1.0)
        );
        assert_eq!(excitation.weight_origin(), [1.0, 2.0, 3.0]);
        let resolution_property = round_trip.property_by_index(10).unwrap();
        assert!(!resolution_property.is_visible());
        assert_eq!(resolution_property.resolution_box().unwrap().factor(), 4);
        let default_resolution_property = round_trip.property_by_index(11).unwrap();
        assert!(!default_resolution_property.is_visible());
        assert_eq!(
            default_resolution_property
                .resolution_box()
                .unwrap()
                .factor(),
            1
        );
        assert_eq!(
            round_trip.property_by_index(12).unwrap().unknown_property(),
            Some("probeunknown")
        );
        let debye = round_trip.property_by_index(13).unwrap();
        let debye_data = debye.debye_material().unwrap();
        assert_eq!(debye_data.order(), 2);
        assert_eq!(
            debye_data
                .epsilon_delta(0, 0, Some(round_trip.parameters()))
                .unwrap(),
            Some(2.0)
        );
        assert_eq!(
            debye_data
                .epsilon_relax_time(0, 0, Some(round_trip.parameters()))
                .unwrap(),
            Some(0.2)
        );
        assert_eq!(
            debye_data.epsilon_delta_weight_term(0, 0).as_deref(),
            Some("x")
        );
        assert_eq!(
            debye_data
                .epsilon_delta(1, 1, Some(round_trip.parameters()))
                .unwrap(),
            Some(4.0)
        );
        let discrete = round_trip
            .property_by_index(14)
            .unwrap()
            .discrete_material()
            .unwrap();
        assert_eq!(discrete.file_name(), "mesh.h5");
        assert_eq!(discrete.file_type(), 0);
        assert!(!discrete.use_database_for_background());
        assert_eq!(discrete.scale(), 0.001);
        assert_eq!(discrete.transform().unwrap().operations().len(), 1);
        assert_eq!(
            discrete.transform().unwrap().transform([0.0; 3]),
            [2.0, 0.0, 0.0]
        );
        let legacy_discrete = round_trip
            .property_by_index(15)
            .unwrap()
            .discrete_material()
            .unwrap();
        assert_eq!(legacy_discrete.file_name(), "legacy.h5");
        assert_eq!(legacy_discrete.scale(), 2.0);
        let lorentz = round_trip
            .property_by_index(16)
            .unwrap()
            .lorentz_material()
            .unwrap();
        assert_eq!(lorentz.order(), 2);
        assert_eq!(
            lorentz
                .value(
                    crate::material::LorentzParameter::EpsilonPlasmaFrequency,
                    0,
                    0,
                    Some(round_trip.parameters()),
                )
                .unwrap(),
            Some(2.0)
        );
        assert_eq!(
            lorentz
                .weighted_value(
                    crate::material::LorentzParameter::EpsilonPlasmaFrequency,
                    0,
                    0,
                    Some(round_trip.parameters()),
                    [2.0, 0.0, 0.0],
                    crate::grid::CoordinateSystem::Cartesian,
                )
                .unwrap(),
            Some(6.0)
        );
        assert_eq!(
            round_trip
                .property_by_index(17)
                .unwrap()
                .material()
                .unwrap()
                .value(MaterialField::Epsilon, 0, Some(round_trip.parameters()))
                .unwrap(),
            Some(2.0)
        );
    }
}

#[cfg(test)]
mod anisotropic_parse_tests {
    use super::*;
    use crate::material::MaterialField;

    #[test]
    fn vector_material_attributes_are_anisotropic() {
        let xml = "<ContinuousStructure><Properties><Material><Property Epsilon=\"2,4,6\" Mue=\"1\" Kappa=\"0\" Sigma=\"0\"/><Primitives/></Material></Properties></ContinuousStructure>";
        let structure = Structure::from_xml_str(xml).unwrap();
        let material = structure.property_by_index(0).unwrap().material().unwrap();
        assert!(!material.is_isotropic());
        for (component, expected) in [2.0, 4.0, 6.0].iter().enumerate() {
            assert_eq!(
                material
                    .value(MaterialField::Epsilon, component, None)
                    .unwrap(),
                Some(*expected)
            );
        }
    }

    #[test]
    fn isotropy_on_the_material_element_wins_over_the_vector_components() {
        // native CSXCAD writes Isotropy on <Material> and the values on the
        // <Property> child, where an isotropic material repeats its value
        let xml = "<ContinuousStructure><Properties><Material Name=\"m\" Isotropy=\"1\"><Primitives/><Property Epsilon=\"2.5,1,1\" Mue=\"1,1,1\" Kappa=\"0,0,0\" Sigma=\"0,0,0\"/></Material></Properties></ContinuousStructure>";
        let structure = Structure::from_xml_str(xml).unwrap();
        let material = structure.property_by_index(0).unwrap().material().unwrap();
        assert!(material.is_isotropic());
        assert_eq!(
            material.value(MaterialField::Epsilon, 0, None).unwrap(),
            Some(2.5)
        );
        // an isotropic material resolves every component to its single value
        assert_eq!(
            material.value(MaterialField::Epsilon, 1, None).unwrap(),
            Some(2.5)
        );
    }

    #[test]
    fn equal_vector_components_stay_isotropic() {
        let xml = "<ContinuousStructure><Properties><Material><Property Epsilon=\"3,3,3\" Mue=\"1\" Kappa=\"0\" Sigma=\"0\"/><Primitives/></Material></Properties></ContinuousStructure>";
        let structure = Structure::from_xml_str(xml).unwrap();
        let material = structure.property_by_index(0).unwrap().material().unwrap();
        assert!(material.is_isotropic());
    }

    #[test]
    fn explicit_isotropy_attribute_wins_over_vector_length() {
        let xml = "<ContinuousStructure><Properties><Material><Property Isotropy=\"1\" Epsilon=\"2,4,6\" Mue=\"1\" Kappa=\"0\" Sigma=\"0\"/><Primitives/></Material></Properties></ContinuousStructure>";
        let structure = Structure::from_xml_str(xml).unwrap();
        let material = structure.property_by_index(0).unwrap().material().unwrap();
        assert!(material.is_isotropic());
    }
}
