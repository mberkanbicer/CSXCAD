use crate::grid::{transform_coordinates, CoordinateSystem};
use crate::parameter::{ParameterError, ParameterScalar, ParameterSet};
use crate::transform::{Transform, TransformError};
use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialField {
    Epsilon,
    Mue,
    Kappa,
    Sigma,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscreteMaterialField {
    Epsilon,
    Mue,
    Kappa,
    Sigma,
    Density,
}

impl DiscreteMaterialField {
    fn index(self) -> usize {
        match self {
            Self::Epsilon => 0,
            Self::Mue => 1,
            Self::Kappa => 2,
            Self::Sigma => 3,
            Self::Density => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundMaterial {
    epsilon: f64,
    mue: f64,
    kappa: f64,
    sigma: f64,
}

impl Default for BackgroundMaterial {
    fn default() -> Self {
        Self {
            epsilon: 1.0,
            mue: 1.0,
            kappa: 0.0,
            sigma: 0.0,
        }
    }
}

impl BackgroundMaterial {
    pub fn value(&self, field: MaterialField) -> f64 {
        match field {
            MaterialField::Epsilon => self.epsilon,
            MaterialField::Mue => self.mue,
            MaterialField::Kappa => self.kappa,
            MaterialField::Sigma => self.sigma,
        }
    }

    pub fn set_value(&mut self, field: MaterialField, value: f64) -> bool {
        if matches!(field, MaterialField::Epsilon | MaterialField::Mue) && value < 1.0
            || matches!(field, MaterialField::Kappa | MaterialField::Sigma) && value < 0.0
        {
            return false;
        }
        match field {
            MaterialField::Epsilon => self.epsilon = value,
            MaterialField::Mue => self.mue = value,
            MaterialField::Kappa => self.kappa = value,
            MaterialField::Sigma => self.sigma = value,
        }
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConductingSheetProperties {
    conductivity: ParameterScalar,
    thickness: ParameterScalar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbsorbingBoundaryType {
    Undefined,
    MurFirstOrder,
    MurFirstOrderSuperAbsorption,
    Other(i32),
}

impl AbsorbingBoundaryType {
    pub fn from_xml_value(value: i32) -> Self {
        match value {
            0 => Self::Undefined,
            1 => Self::MurFirstOrder,
            2 => Self::MurFirstOrderSuperAbsorption,
            other => Self::Other(other),
        }
    }

    pub fn xml_value(self) -> i32 {
        match self {
            Self::Undefined => 0,
            Self::MurFirstOrder => 1,
            Self::MurFirstOrderSuperAbsorption => 2,
            Self::Other(value) => value,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AbsorbingBoundaryProperties {
    normal_sign_positive: bool,
    phase_velocity: ParameterScalar,
    boundary_type: AbsorbingBoundaryType,
}

impl Default for AbsorbingBoundaryProperties {
    fn default() -> Self {
        Self {
            normal_sign_positive: true,
            phase_velocity: ParameterScalar::new(299_792_458.0),
            boundary_type: AbsorbingBoundaryType::Undefined,
        }
    }
}

impl AbsorbingBoundaryProperties {
    pub fn normal_sign_positive(&self) -> bool {
        self.normal_sign_positive
    }

    pub fn set_normal_sign_positive(&mut self, val: bool) {
        self.normal_sign_positive = val;
    }

    pub fn boundary_type(&self) -> AbsorbingBoundaryType {
        self.boundary_type
    }

    pub fn set_boundary_type(&mut self, val: AbsorbingBoundaryType) {
        self.boundary_type = val;
    }

    pub fn phase_velocity(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.phase_velocity.evaluate(parameters)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LumpedElementType {
    Parallel,
    Series,
}

impl LumpedElementType {
    pub fn from_xml_value(value: i32) -> Self {
        if value == 1 {
            Self::Series
        } else {
            Self::Parallel
        }
    }

    pub fn xml_value(self) -> i32 {
        match self {
            Self::Parallel => 0,
            Self::Series => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LumpedElementProperties {
    direction: i32,
    caps: bool,
    resistance: ParameterScalar,
    capacitance: ParameterScalar,
    inductance: ParameterScalar,
    element_type: LumpedElementType,
}

impl Default for LumpedElementProperties {
    fn default() -> Self {
        Self {
            direction: -1,
            caps: true,
            resistance: ParameterScalar::new(f64::NAN),
            capacitance: ParameterScalar::new(f64::NAN),
            inductance: ParameterScalar::new(f64::NAN),
            element_type: LumpedElementType::Parallel,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProbeProperties {
    number: u32,
    normal_direction: i32,
    weight: f64,
    probe_type: i32,
    start_time: f64,
    stop_time: f64,
    oversampling: i32,
    fd_samples: Vec<f64>,
    mode_file: String,
    mode_functions: [String; 3],
    mode_origin: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct DumpBoxProperties {
    dump_type: i32,
    dump_mode: i32,
    file_type: i32,
    multi_grid_level: i32,
    sub_sampling_enabled: bool,
    sub_sampling: [u32; 3],
    optimal_resolution_enabled: bool,
    optimal_resolution: [f64; 3],
    /// Frequency samples of a frequency-domain dump (`Frequency` attribute).
    fd_samples: Vec<f64>,
    /// Averaging method of a SAR dump (`SAR_Method` attribute).
    sar_method: String,
}

impl Default for DumpBoxProperties {
    fn default() -> Self {
        Self {
            dump_type: 0,
            dump_mode: 0,
            file_type: 0,
            multi_grid_level: 0,
            sub_sampling_enabled: false,
            sub_sampling: [1; 3],
            optimal_resolution_enabled: false,
            optimal_resolution: [1.0; 3],
            fd_samples: Vec::new(),
            sar_method: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExcitationProperties {
    number: u32,
    excitation_type: i32,
    enabled: bool,
    active_directions: [bool; 3],
    frequency: ParameterScalar,
    excitation: [ParameterScalar; 3],
    weighting: [ParameterScalar; 3],
    propagation_direction: [ParameterScalar; 3],
    delay: ParameterScalar,
    weight_file: String,
    weight_origin: [f64; 3],
    /// Loaded mode profile of `weight_file`, mirroring the native
    /// `m_WeightFileData` cache so that a copied property keeps its data.
    #[cfg(feature = "hdf5")]
    weight_data: std::cell::RefCell<Option<crate::ModeData>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolutionBoxProperties {
    factor: u32,
}

impl Default for ResolutionBoxProperties {
    fn default() -> Self {
        Self { factor: 1 }
    }
}

impl ResolutionBoxProperties {
    pub fn factor(&self) -> u32 {
        self.factor
    }

    pub fn set_factor(&mut self, factor: u32) {
        self.factor = factor;
    }
}

impl Default for ExcitationProperties {
    fn default() -> Self {
        Self {
            number: 0,
            excitation_type: 1,
            enabled: true,
            active_directions: [true; 3],
            frequency: ParameterScalar::new(0.0),
            excitation: std::array::from_fn(|_| ParameterScalar::new(0.0)),
            weighting: std::array::from_fn(|_| ParameterScalar::new(1.0)),
            propagation_direction: std::array::from_fn(|_| ParameterScalar::new(0.0)),
            delay: ParameterScalar::new(0.0),
            weight_file: String::new(),
            weight_origin: [0.0; 3],
            #[cfg(feature = "hdf5")]
            weight_data: std::cell::RefCell::new(None),
        }
    }
}

impl ExcitationProperties {
    pub fn number(&self) -> u32 {
        self.number
    }

    pub fn set_number(&mut self, number: u32) {
        self.number = number;
    }

    pub fn excitation_type(&self) -> i32 {
        self.excitation_type
    }

    pub fn set_excitation_type(&mut self, excitation_type: i32) {
        self.excitation_type = excitation_type;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn active_direction(&self, component: usize) -> Option<bool> {
        self.active_directions.get(component).copied()
    }

    pub fn set_active_direction(&mut self, component: usize, active: bool) -> bool {
        let Some(direction) = self.active_directions.get_mut(component) else {
            return false;
        };
        *direction = active;
        true
    }

    pub fn frequency_term(&self) -> String {
        self.frequency.value_string()
    }

    pub fn set_frequency_term(&mut self, term: &str) -> Result<(), ParameterError> {
        self.frequency.set_value_string(term)
    }

    pub fn frequency(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.frequency.evaluate(parameters)
    }

    pub fn excitation_term(&self, component: usize) -> Option<String> {
        self.excitation
            .get(component)
            .map(ParameterScalar::value_string)
    }

    pub fn set_excitation_term(
        &mut self,
        component: usize,
        term: &str,
    ) -> Result<bool, ParameterError> {
        let Some(value) = self.excitation.get_mut(component) else {
            return Ok(false);
        };
        value.set_value_string(term)?;
        Ok(true)
    }

    pub fn excitation(
        &self,
        component: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        self.excitation
            .get(component)
            .map(|value| value.evaluate(parameters))
            .transpose()
    }

    pub fn weighting_term(&self, component: usize) -> Option<String> {
        self.weighting
            .get(component)
            .map(ParameterScalar::value_string)
    }

    /// Weight function expression as stored (native `GetString()`: empty for a
    /// plain number, the expression text otherwise).
    pub fn weighting_expression(&self, component: usize) -> Option<String> {
        self.weighting
            .get(component)
            .and_then(ParameterScalar::expression)
            .map(str::to_owned)
    }

    pub fn set_weighting_term(
        &mut self,
        component: usize,
        term: &str,
    ) -> Result<bool, ParameterError> {
        if self.weighting.get(component).is_none() {
            return Ok(false);
        }
        // native SetWeightFunction: a weight function replaces any weight file
        self.weight_file.clear();
        #[cfg(feature = "hdf5")]
        {
            *self.weight_data.borrow_mut() = None;
        }
        let value = self.weighting.get_mut(component).expect("checked above");
        if term.is_empty() {
            // native: an empty function means a zero weight
            value.set_value(0.0);
            return Ok(true);
        }
        value.set_value_string(term)?;
        Ok(true)
    }

    /// Evaluate the weighted excitation for one component at `coordinates`
    /// (native `CSPropExcitation::GetWeightedExcitation`).
    /// Evaluate only the spatial weight of an excitation (native
    /// `GetWeightedExcitation` without the excitation amplitude).
    ///
    /// `coordinates` are in the unit the weight function and the weight origin
    /// are authored in (drawing units for the operator, metres for the file
    /// interface).
    pub fn weighted_excitation_weight(
        &self,
        component: usize,
        parameters: Option<&ParameterSet>,
        coordinates: [f64; 3],
        coordinate_system: CoordinateSystem,
    ) -> Result<f64, ParameterError> {
        if component >= 3 {
            return Ok(0.0);
        }
        // native: convert the input to Cartesian first, then apply the weight origin
        let mut local = coordinates;
        if coordinate_system == CoordinateSystem::Cylindrical {
            let [rho, alpha, z] = coordinates;
            local = [rho * alpha.cos(), rho * alpha.sin(), z];
        }
        for (axis, origin) in self.weight_origin.iter().enumerate() {
            local[axis] -= origin;
        }

        let weight = if !self.weight_file.is_empty() {
            #[cfg(feature = "hdf5")]
            {
                if self.weight_data.borrow().is_none() {
                    let loaded = crate::ModeData::read_hdf5(&self.weight_file).ok();
                    *self.weight_data.borrow_mut() = loaded;
                }
                let cached = self.weight_data.borrow();
                let Some(data) = cached.as_ref() else {
                    return Ok(0.0);
                };
                // exactly one non-zero propagation direction is required
                let mut direction = None;
                let mut count = 0;
                for axis in 0..3 {
                    if self.propagation_direction[axis].evaluate(parameters)? != 0.0 {
                        direction = Some(axis);
                        count += 1;
                    }
                }
                let Some(direction) = direction else {
                    return Ok(0.0);
                };
                if count != 1 {
                    return Ok(0.0);
                }
                if direction == component {
                    0.0
                } else {
                    let first = (direction + 1) % 3;
                    let second = (direction + 2) % 3;
                    let fields = data.interpolate(local[first], local[second]);
                    fields[usize::from(component == second)]
                }
            }
            #[cfg(not(feature = "hdf5"))]
            {
                return Ok(0.0);
            }
        } else {
            let coordinate_parameters =
                coordinate_parameters(local, CoordinateSystem::Cartesian);
            self.weighting[component].evaluate(Some(&coordinate_parameters))?
        };

        Ok(weight)
    }

    /// Full weighted excitation amplitude (native `GetWeightedExcitation`).
    pub fn weighted_excitation(
        &mut self,
        component: usize,
        parameters: Option<&ParameterSet>,
        coordinates: [f64; 3],
        coordinate_system: CoordinateSystem,
    ) -> Result<f64, ParameterError> {
        let amplitude = self.excitation[component].evaluate(parameters)?;
        let weight = self.weighted_excitation_weight(
            component,
            parameters,
            coordinates,
            coordinate_system,
        )?;
        Ok(weight * amplitude)
    }

    pub fn weighting(
        &self,
        component: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        self.weighting
            .get(component)
            .map(|value| value.evaluate(parameters))
            .transpose()
    }

    pub fn propagation_direction_term(&self, component: usize) -> Option<String> {
        self.propagation_direction
            .get(component)
            .map(ParameterScalar::value_string)
    }

    pub fn set_propagation_direction_term(
        &mut self,
        component: usize,
        term: &str,
    ) -> Result<bool, ParameterError> {
        let Some(value) = self.propagation_direction.get_mut(component) else {
            return Ok(false);
        };
        value.set_value_string(term)?;
        Ok(true)
    }

    pub fn propagation_direction(
        &self,
        component: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        self.propagation_direction
            .get(component)
            .map(|value| value.evaluate(parameters))
            .transpose()
    }

    pub fn delay_term(&self) -> String {
        self.delay.value_string()
    }

    pub fn set_delay_term(&mut self, term: &str) -> Result<(), ParameterError> {
        self.delay.set_value_string(term)
    }

    pub fn delay(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.delay.evaluate(parameters)
    }

    pub fn weight_file(&self) -> &str {
        &self.weight_file
    }

    pub fn set_weight_file(&mut self, path: impl Into<String>) {
        self.weight_file = path.into();
        #[cfg(feature = "hdf5")]
        {
            *self.weight_data.borrow_mut() = None;
        }
    }

    pub fn weight_origin(&self) -> [f64; 3] {
        self.weight_origin
    }

    pub fn set_weight_origin(&mut self, origin: [f64; 3]) {
        self.weight_origin = origin;
    }
}

impl DumpBoxProperties {
    pub fn dump_type(&self) -> i32 {
        self.dump_type
    }

    pub fn set_dump_type(&mut self, dump_type: i32) {
        self.dump_type = dump_type;
    }

    pub fn dump_mode(&self) -> i32 {
        self.dump_mode
    }

    pub fn set_dump_mode(&mut self, dump_mode: i32) {
        self.dump_mode = dump_mode;
    }

    pub fn file_type(&self) -> i32 {
        self.file_type
    }

    pub fn set_file_type(&mut self, file_type: i32) {
        self.file_type = file_type;
    }

    pub fn multi_grid_level(&self) -> i32 {
        self.multi_grid_level
    }

    pub fn set_multi_grid_level(&mut self, level: i32) {
        self.multi_grid_level = level;
    }

    pub fn sub_sampling_enabled(&self) -> bool {
        self.sub_sampling_enabled
    }

    pub fn set_sub_sampling_enabled(&mut self, enabled: bool) {
        self.sub_sampling_enabled = enabled;
    }

    pub fn sub_sampling(&self) -> [u32; 3] {
        self.sub_sampling
    }

    pub fn set_sub_sampling(&mut self, direction: usize, value: u32) -> bool {
        if value == 0 {
            return false;
        }
        let Some(sub_sampling) = self.sub_sampling.get_mut(direction) else {
            return false;
        };
        *sub_sampling = value;
        self.sub_sampling_enabled = true;
        true
    }

    pub fn optimal_resolution_enabled(&self) -> bool {
        self.optimal_resolution_enabled
    }

    pub fn set_optimal_resolution_enabled(&mut self, enabled: bool) {
        self.optimal_resolution_enabled = enabled;
    }

    pub fn optimal_resolution(&self) -> [f64; 3] {
        self.optimal_resolution
    }

    /// Frequency samples of a frequency-domain dump.
    pub fn fd_samples(&self) -> &[f64] {
        &self.fd_samples
    }

    pub fn set_fd_samples(&mut self, samples: Vec<f64>) {
        self.fd_samples = samples;
    }

    /// Averaging method of a SAR dump (empty for the default).
    pub fn sar_method(&self) -> &str {
        &self.sar_method
    }

    pub fn set_sar_method(&mut self, method: impl Into<String>) {
        self.sar_method = method.into();
    }

    pub fn set_optimal_resolution(&mut self, direction: usize, value: f64) -> bool {
        if value < 0.0 {
            return false;
        }
        let Some(resolution) = self.optimal_resolution.get_mut(direction) else {
            return false;
        };
        *resolution = value;
        self.optimal_resolution_enabled = true;
        true
    }
}

impl Default for ProbeProperties {
    fn default() -> Self {
        Self {
            number: 0,
            normal_direction: -1,
            weight: 1.0,
            probe_type: 0,
            start_time: 0.0,
            stop_time: 0.0,
            oversampling: -1,
            fd_samples: Vec::new(),
            mode_file: String::new(),
            mode_functions: std::array::from_fn(|_| String::new()),
            mode_origin: [0.0; 3],
        }
    }
}

impl ProbeProperties {
    pub fn number(&self) -> u32 {
        self.number
    }

    pub fn set_number(&mut self, number: u32) {
        self.number = number;
    }

    pub fn normal_direction(&self) -> i32 {
        self.normal_direction
    }

    pub fn set_normal_direction(&mut self, direction: i32) {
        self.normal_direction = direction;
    }

    pub fn weight(&self) -> f64 {
        self.weight
    }

    pub fn set_weight(&mut self, weight: f64) {
        self.weight = weight;
    }

    pub fn probe_type(&self) -> i32 {
        self.probe_type
    }

    pub fn set_probe_type(&mut self, probe_type: i32) {
        self.probe_type = probe_type;
    }

    pub fn start_time(&self) -> f64 {
        self.start_time
    }

    pub fn set_start_time(&mut self, time: f32) {
        self.start_time = f64::from(time);
    }

    pub(crate) fn set_start_time_from_xml(&mut self, time: f64) {
        self.start_time = time;
    }

    pub fn stop_time(&self) -> f64 {
        self.stop_time
    }

    pub fn set_stop_time(&mut self, time: f32) {
        self.stop_time = f64::from(time);
    }

    pub(crate) fn set_stop_time_from_xml(&mut self, time: f64) {
        self.stop_time = time;
    }

    pub fn oversampling(&self) -> i32 {
        self.oversampling
    }

    pub fn set_oversampling(&mut self, oversampling: i32) {
        self.oversampling = oversampling;
    }

    pub fn fd_samples(&self) -> &[f64] {
        &self.fd_samples
    }

    pub fn set_fd_samples(&mut self, samples: Vec<f64>) {
        self.fd_samples = samples;
    }

    pub fn mode_file(&self) -> &str {
        &self.mode_file
    }

    pub fn set_mode_file(&mut self, path: impl Into<String>) {
        self.mode_file = path.into();
    }

    pub fn mode_function(&self, direction: usize) -> Option<&str> {
        self.mode_functions.get(direction).map(String::as_str)
    }

    pub fn set_mode_function(&mut self, direction: usize, function: impl Into<String>) -> bool {
        let Some(value) = self.mode_functions.get_mut(direction) else {
            return false;
        };
        *value = function.into();
        true
    }

    pub fn mode_origin(&self) -> [f64; 3] {
        self.mode_origin
    }

    pub fn set_mode_origin(&mut self, origin: [f64; 3]) {
        self.mode_origin = origin;
    }
}

impl LumpedElementProperties {
    pub fn direction(&self) -> i32 {
        self.direction
    }

    pub fn set_direction(&mut self, direction: i32) {
        if (0..=2).contains(&direction) {
            self.direction = direction;
        }
    }

    pub(crate) fn set_direction_from_xml(&mut self, direction: i32) {
        self.direction = direction;
    }

    pub fn caps(&self) -> bool {
        self.caps
    }

    pub fn set_caps(&mut self, caps: bool) {
        self.caps = caps;
    }

    pub fn resistance_term(&self) -> String {
        self.resistance.value_string()
    }

    pub fn capacitance_term(&self) -> String {
        self.capacitance.value_string()
    }

    pub fn inductance_term(&self) -> String {
        self.inductance.value_string()
    }

    pub fn set_resistance_term(&mut self, term: &str) -> Result<(), ParameterError> {
        self.resistance.set_value_string(term)
    }

    pub fn set_capacitance_term(&mut self, term: &str) -> Result<(), ParameterError> {
        self.capacitance.set_value_string(term)
    }

    pub fn set_inductance_term(&mut self, term: &str) -> Result<(), ParameterError> {
        self.inductance.set_value_string(term)
    }

    pub fn resistance(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.resistance.evaluate(parameters)
    }

    pub fn capacitance(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.capacitance.evaluate(parameters)
    }

    pub fn inductance(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.inductance.evaluate(parameters)
    }

    pub fn element_type(&self) -> LumpedElementType {
        self.element_type
    }

    pub fn set_element_type(&mut self, element_type: LumpedElementType) {
        self.element_type = element_type;
    }
}

impl AbsorbingBoundaryProperties {
    pub fn phase_velocity_term(&self) -> String {
        self.phase_velocity.value_string()
    }

    pub fn set_phase_velocity(&mut self, velocity: f64) {
        self.phase_velocity
            .set_value(if velocity < 0.0 { 0.0 } else { velocity });
    }

    pub fn set_phase_velocity_term(&mut self, term: &str) -> Result<(), ParameterError> {
        self.phase_velocity.set_value_string(term)
    }
}

impl ConductingSheetProperties {
    pub fn conductivity_term(&self) -> String {
        self.conductivity.value_string()
    }

    pub fn thickness_term(&self) -> String {
        self.thickness.value_string()
    }

    pub fn set_conductivity_term(&mut self, value: &str) -> Result<(), ParameterError> {
        self.conductivity.set_value_string(value)
    }

    pub fn set_thickness_term(&mut self, value: &str) -> Result<(), ParameterError> {
        self.thickness.set_value_string(value)
    }

    pub fn conductivity(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.conductivity.evaluate(parameters)
    }

    pub fn thickness(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.thickness.evaluate(parameters)
    }
}

impl MaterialField {
    fn index(self) -> usize {
        match self {
            Self::Epsilon => 0,
            Self::Mue => 1,
            Self::Kappa => 2,
            Self::Sigma => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialProperties {
    isotropic: bool,
    values: [[ParameterScalar; 3]; 4],
    weights: [[ParameterScalar; 3]; 4],
    density: ParameterScalar,
    density_weight: ParameterScalar,
}

#[derive(Clone, Debug, PartialEq)]
struct DebyePole {
    epsilon_delta: [ParameterScalar; 3],
    epsilon_delta_weight: [ParameterScalar; 3],
    epsilon_relax_time: [ParameterScalar; 3],
    epsilon_relax_time_weight: [ParameterScalar; 3],
}

impl Default for DebyePole {
    fn default() -> Self {
        Self {
            epsilon_delta: std::array::from_fn(|_| ParameterScalar::new(0.0)),
            epsilon_delta_weight: std::array::from_fn(|_| ParameterScalar::new(1.0)),
            epsilon_relax_time: std::array::from_fn(|_| ParameterScalar::new(0.0)),
            epsilon_relax_time_weight: std::array::from_fn(|_| ParameterScalar::new(1.0)),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DebyeMaterialProperties {
    poles: Vec<DebyePole>,
}

impl DebyeMaterialProperties {
    pub fn order(&self) -> usize {
        self.poles.len()
    }

    pub fn set_order(&mut self, order: usize) {
        self.poles = vec![DebyePole::default(); order];
    }

    pub(crate) fn expression_errors(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Vec<ParameterError> {
        let mut errors = Vec::new();
        let coordinates = coordinate_parameters([0.0; 3], coordinate_system);
        for pole in &self.poles {
            for component in 0..3 {
                for (scalar, values) in [
                    (&pole.epsilon_delta[component], parameters),
                    (&pole.epsilon_delta_weight[component], Some(&coordinates)),
                    (&pole.epsilon_relax_time[component], parameters),
                    (
                        &pole.epsilon_relax_time_weight[component],
                        Some(&coordinates),
                    ),
                ] {
                    if let Err(error) = scalar.evaluate(values) {
                        errors.push(error);
                    }
                }
            }
        }
        errors
    }

    pub fn epsilon_delta_term(&self, order: usize, component: usize) -> Option<String> {
        self.poles
            .get(order)?
            .epsilon_delta
            .get(component)
            .map(|value| value.value_string())
    }

    pub fn epsilon_delta(
        &self,
        order: usize,
        component: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        let Some(value) = self
            .poles
            .get(order)
            .and_then(|pole| pole.epsilon_delta.get(component))
        else {
            return Ok(None);
        };
        value.evaluate(parameters).map(Some)
    }

    pub fn set_epsilon_delta(&mut self, order: usize, component: usize, value: f64) -> bool {
        let Some(value_term) = self
            .poles
            .get_mut(order)
            .and_then(|pole| pole.epsilon_delta.get_mut(component))
        else {
            return false;
        };
        value_term.set_value(value);
        true
    }

    pub fn set_epsilon_delta_expression(
        &mut self,
        order: usize,
        component: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(value_term) = self
            .poles
            .get_mut(order)
            .and_then(|pole| pole.epsilon_delta.get_mut(component))
        else {
            return Ok(false);
        };
        value_term.set_value_string(expression)?;
        Ok(true)
    }

    pub fn epsilon_relax_time_term(&self, order: usize, component: usize) -> Option<String> {
        self.poles
            .get(order)?
            .epsilon_relax_time
            .get(component)
            .map(|value| value.value_string())
    }

    pub fn epsilon_relax_time(
        &self,
        order: usize,
        component: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        let Some(value) = self
            .poles
            .get(order)
            .and_then(|pole| pole.epsilon_relax_time.get(component))
        else {
            return Ok(None);
        };
        value.evaluate(parameters).map(Some)
    }

    pub fn set_epsilon_relax_time(&mut self, order: usize, component: usize, value: f64) -> bool {
        let Some(value_term) = self
            .poles
            .get_mut(order)
            .and_then(|pole| pole.epsilon_relax_time.get_mut(component))
        else {
            return false;
        };
        value_term.set_value(value);
        true
    }

    pub fn set_epsilon_relax_time_expression(
        &mut self,
        order: usize,
        component: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(value_term) = self
            .poles
            .get_mut(order)
            .and_then(|pole| pole.epsilon_relax_time.get_mut(component))
        else {
            return Ok(false);
        };
        value_term.set_value_string(expression)?;
        Ok(true)
    }

    pub fn epsilon_delta_weight_term(&self, order: usize, component: usize) -> Option<String> {
        self.poles
            .get(order)?
            .epsilon_delta_weight
            .get(component)
            .map(|value| value.value_string())
    }

    pub fn set_epsilon_delta_weight_expression(
        &mut self,
        order: usize,
        component: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(value_term) = self
            .poles
            .get_mut(order)
            .and_then(|pole| pole.epsilon_delta_weight.get_mut(component))
        else {
            return Ok(false);
        };
        value_term.set_value_string(expression)?;
        Ok(true)
    }

    pub fn weighted_epsilon_delta(
        &self,
        order: usize,
        component: usize,
        parameters: Option<&ParameterSet>,
        coordinates: [f64; 3],
        coordinate_system: CoordinateSystem,
    ) -> Result<Option<f64>, ParameterError> {
        let Some(pole) = self.poles.get(order) else {
            return Ok(None);
        };
        let (Some(value), Some(weight)) = (
            pole.epsilon_delta.get(component),
            pole.epsilon_delta_weight.get(component),
        ) else {
            return Ok(None);
        };
        let coordinate_parameters = coordinate_parameters(coordinates, coordinate_system);
        Ok(Some(
            value.evaluate(parameters)? * weight.evaluate(Some(&coordinate_parameters))?,
        ))
    }

    pub fn epsilon_relax_time_weight_term(&self, order: usize, component: usize) -> Option<String> {
        self.poles
            .get(order)?
            .epsilon_relax_time_weight
            .get(component)
            .map(|value| value.value_string())
    }

    pub fn set_epsilon_relax_time_weight_expression(
        &mut self,
        order: usize,
        component: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(value_term) = self
            .poles
            .get_mut(order)
            .and_then(|pole| pole.epsilon_relax_time_weight.get_mut(component))
        else {
            return Ok(false);
        };
        value_term.set_value_string(expression)?;
        Ok(true)
    }

    pub fn weighted_epsilon_relax_time(
        &self,
        order: usize,
        component: usize,
        parameters: Option<&ParameterSet>,
        coordinates: [f64; 3],
        coordinate_system: CoordinateSystem,
    ) -> Result<Option<f64>, ParameterError> {
        let Some(pole) = self.poles.get(order) else {
            return Ok(None);
        };
        let (Some(value), Some(weight)) = (
            pole.epsilon_relax_time.get(component),
            pole.epsilon_relax_time_weight.get(component),
        ) else {
            return Ok(None);
        };
        let coordinate_parameters = coordinate_parameters(coordinates, coordinate_system);
        Ok(Some(
            value.evaluate(parameters)? * weight.evaluate(Some(&coordinate_parameters))?,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LorentzParameter {
    EpsilonPlasmaFrequency,
    MuePlasmaFrequency,
    EpsilonLorentzPoleFrequency,
    MueLorentzPoleFrequency,
    EpsilonRelaxTime,
    MueRelaxTime,
}

impl LorentzParameter {
    pub const ALL: [Self; 6] = [
        Self::EpsilonPlasmaFrequency,
        Self::MuePlasmaFrequency,
        Self::EpsilonLorentzPoleFrequency,
        Self::MueLorentzPoleFrequency,
        Self::EpsilonRelaxTime,
        Self::MueRelaxTime,
    ];

    pub fn xml_name(self) -> &'static str {
        match self {
            Self::EpsilonPlasmaFrequency => "EpsilonPlasmaFrequency",
            Self::MuePlasmaFrequency => "MuePlasmaFrequency",
            Self::EpsilonLorentzPoleFrequency => "EpsilonLorPoleFrequency",
            Self::MueLorentzPoleFrequency => "MueLorPoleFrequency",
            Self::EpsilonRelaxTime => "EpsilonRelaxTime",
            Self::MueRelaxTime => "MueRelaxTime",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::EpsilonPlasmaFrequency => 0,
            Self::MuePlasmaFrequency => 1,
            Self::EpsilonLorentzPoleFrequency => 2,
            Self::MueLorentzPoleFrequency => 3,
            Self::EpsilonRelaxTime => 4,
            Self::MueRelaxTime => 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct LorentzPole {
    values: [[ParameterScalar; 3]; 6],
    weights: [[ParameterScalar; 3]; 6],
}

impl Default for LorentzPole {
    fn default() -> Self {
        Self {
            values: std::array::from_fn(|_| std::array::from_fn(|_| ParameterScalar::new(0.0))),
            weights: std::array::from_fn(|_| std::array::from_fn(|_| ParameterScalar::new(1.0))),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LorentzMaterialProperties {
    poles: Vec<LorentzPole>,
}

impl LorentzMaterialProperties {
    pub fn order(&self) -> usize {
        self.poles.len()
    }

    pub fn set_order(&mut self, order: usize) {
        self.poles = vec![LorentzPole::default(); order];
    }

    pub(crate) fn expression_errors(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Vec<ParameterError> {
        let mut errors = Vec::new();
        let coordinates = coordinate_parameters([0.0; 3], coordinate_system);
        for pole in &self.poles {
            for component in 0..3 {
                for parameter_group in [
                    (
                        [
                            LorentzParameter::EpsilonPlasmaFrequency,
                            LorentzParameter::MuePlasmaFrequency,
                        ],
                        [
                            LorentzParameter::EpsilonPlasmaFrequency,
                            LorentzParameter::MuePlasmaFrequency,
                        ],
                    ),
                    (
                        [
                            LorentzParameter::EpsilonLorentzPoleFrequency,
                            LorentzParameter::MueLorentzPoleFrequency,
                        ],
                        [
                            LorentzParameter::EpsilonLorentzPoleFrequency,
                            LorentzParameter::MueLorentzPoleFrequency,
                        ],
                    ),
                    (
                        [
                            LorentzParameter::EpsilonRelaxTime,
                            LorentzParameter::MueRelaxTime,
                        ],
                        [
                            LorentzParameter::EpsilonRelaxTime,
                            LorentzParameter::MueRelaxTime,
                        ],
                    ),
                ] {
                    for parameter in parameter_group.0 {
                        if let Err(error) =
                            pole.values[parameter.index()][component].evaluate(parameters)
                        {
                            errors.push(error);
                        }
                    }
                    for parameter in parameter_group.1 {
                        if let Err(error) =
                            pole.weights[parameter.index()][component].evaluate(Some(&coordinates))
                        {
                            errors.push(error);
                        }
                    }
                }
            }
        }
        errors
    }

    pub fn term(
        &self,
        parameter: LorentzParameter,
        order: usize,
        component: usize,
    ) -> Option<String> {
        self.poles.get(order)?.values[parameter.index()]
            .get(component)
            .map(|value| value.value_string())
    }

    pub fn weight_term(
        &self,
        parameter: LorentzParameter,
        order: usize,
        component: usize,
    ) -> Option<String> {
        self.poles.get(order)?.weights[parameter.index()]
            .get(component)
            .map(|value| value.value_string())
    }

    pub fn value(
        &self,
        parameter: LorentzParameter,
        order: usize,
        component: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        let Some(value) = self
            .poles
            .get(order)
            .and_then(|pole| pole.values[parameter.index()].get(component))
        else {
            return Ok(None);
        };
        value.evaluate(parameters).map(Some)
    }

    pub fn set_value(
        &mut self,
        parameter: LorentzParameter,
        order: usize,
        component: usize,
        value: f64,
    ) -> bool {
        let Some(value_term) = self
            .poles
            .get_mut(order)
            .and_then(|pole| pole.values[parameter.index()].get_mut(component))
        else {
            return false;
        };
        value_term.set_value(value);
        true
    }

    pub fn set_value_expression(
        &mut self,
        parameter: LorentzParameter,
        order: usize,
        component: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(value_term) = self
            .poles
            .get_mut(order)
            .and_then(|pole| pole.values[parameter.index()].get_mut(component))
        else {
            return Ok(false);
        };
        value_term.set_value_string(expression)?;
        Ok(true)
    }

    pub fn set_weight_expression(
        &mut self,
        parameter: LorentzParameter,
        order: usize,
        component: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        let Some(weight_term) = self
            .poles
            .get_mut(order)
            .and_then(|pole| pole.weights[parameter.index()].get_mut(component))
        else {
            return Ok(false);
        };
        weight_term.set_value_string(expression)?;
        Ok(true)
    }

    pub fn weighted_value(
        &self,
        parameter: LorentzParameter,
        order: usize,
        component: usize,
        parameters: Option<&ParameterSet>,
        coordinates: [f64; 3],
        coordinate_system: CoordinateSystem,
    ) -> Result<Option<f64>, ParameterError> {
        let Some(pole) = self.poles.get(order) else {
            return Ok(None);
        };
        let (Some(value), Some(weight)) = (
            pole.values[parameter.index()].get(component),
            pole.weights[parameter.index()].get(component),
        ) else {
            return Ok(None);
        };
        let coordinate_parameters = coordinate_parameters(coordinates, coordinate_system);
        Ok(Some(
            value.evaluate(parameters)? * weight.evaluate(Some(&coordinate_parameters))?,
        ))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiscreteMaterialProperties {
    file_name: String,
    file_type: i32,
    use_database_for_background: bool,
    scale: f64,
    transform: Option<Transform>,
}

impl Default for DiscreteMaterialProperties {
    fn default() -> Self {
        Self {
            file_name: String::new(),
            file_type: -1,
            use_database_for_background: true,
            scale: 1.0,
            transform: None,
        }
    }
}

impl DiscreteMaterialProperties {
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn set_file_name(&mut self, file_name: impl Into<String>) {
        self.file_name = file_name.into();
    }

    pub fn file_type(&self) -> i32 {
        self.file_type
    }

    pub fn set_file_type(&mut self, file_type: i32) {
        self.file_type = file_type;
    }

    pub fn use_database_for_background(&self) -> bool {
        self.use_database_for_background
    }

    pub fn set_use_database_for_background(&mut self, use_database: bool) {
        self.use_database_for_background = use_database;
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    pub fn transform(&self) -> Option<&Transform> {
        self.transform.as_ref()
    }

    pub fn set_transform(&mut self, transform: Option<Transform>) {
        self.transform = transform;
    }

    #[cfg(feature = "hdf5")]
    pub fn load_grid(&self) -> Result<DiscreteMaterialGrid, crate::DiscreteMaterialLoadError> {
        if self.file_type != 0 {
            return Err(crate::DiscreteMaterialLoadError::UnsupportedFileType(
                self.file_type,
            ));
        }
        if self.file_name.is_empty() {
            return Err(crate::DiscreteMaterialLoadError::MissingFilename);
        }
        DiscreteMaterialGrid::read_hdf5(&self.file_name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscreteMaterialGridError {
    MeshAxisTooShort { axis: usize, actual: usize },
    CellCountOverflow,
    InvalidMaterialIndexCount { expected: usize, actual: usize },
    InvalidDatabaseValueCount { expected: usize, actual: usize },
}

impl fmt::Display for DiscreteMaterialGridError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MeshAxisTooShort { axis, actual } => {
                write!(
                    formatter,
                    "mesh axis {axis} needs at least 2 coordinates, got {actual}"
                )
            }
            Self::CellCountOverflow => formatter.write_str("discrete material cell count overflow"),
            Self::InvalidMaterialIndexCount { expected, actual } => write!(
                formatter,
                "expected {expected} discrete material indices, got {actual}"
            ),
            Self::InvalidDatabaseValueCount { expected, actual } => write!(
                formatter,
                "expected {expected} values for discrete material database, got {actual}"
            ),
        }
    }
}

impl Error for DiscreteMaterialGridError {}

#[derive(Clone, Debug, PartialEq)]
pub struct DiscreteMaterialGrid {
    mesh: [Vec<f32>; 3],
    material_indices: Vec<u8>,
    database_size: usize,
    database_values: [Option<Vec<f32>>; 5],
}

impl DiscreteMaterialGrid {
    pub fn new(
        mesh: [Vec<f32>; 3],
        material_indices: Vec<u8>,
        database_size: usize,
    ) -> Result<Self, DiscreteMaterialGridError> {
        for (axis, coordinates) in mesh.iter().enumerate() {
            if coordinates.len() < 2 {
                return Err(DiscreteMaterialGridError::MeshAxisTooShort {
                    axis,
                    actual: coordinates.len(),
                });
            }
        }

        let cell_count = mesh.iter().try_fold(1usize, |count, coordinates| {
            count.checked_mul(coordinates.len() - 1)
        });
        let Some(cell_count) = cell_count else {
            return Err(DiscreteMaterialGridError::CellCountOverflow);
        };
        if material_indices.len() != cell_count {
            return Err(DiscreteMaterialGridError::InvalidMaterialIndexCount {
                expected: cell_count,
                actual: material_indices.len(),
            });
        }

        Ok(Self {
            mesh,
            material_indices,
            database_size,
            database_values: std::array::from_fn(|_| None),
        })
    }

    pub fn set_database_values(
        &mut self,
        field: DiscreteMaterialField,
        values: Option<Vec<f32>>,
    ) -> Result<(), DiscreteMaterialGridError> {
        if let Some(values) = &values {
            if values.len() != self.database_size {
                return Err(DiscreteMaterialGridError::InvalidDatabaseValueCount {
                    expected: self.database_size,
                    actual: values.len(),
                });
            }
        }
        self.database_values[field.index()] = values;
        Ok(())
    }

    pub fn material_index(
        &self,
        coordinates: [f64; 3],
        coordinate_system: CoordinateSystem,
        properties: &DiscreteMaterialProperties,
    ) -> Result<Option<usize>, TransformError> {
        let mut coordinates =
            transform_coordinates(coordinates, coordinate_system, CoordinateSystem::Cartesian);
        if let Some(transform) = properties.transform() {
            coordinates = transform.inverse_transform(coordinates)?;
        }
        for coordinate in &mut coordinates {
            *coordinate /= properties.scale();
        }

        let mut cell = [0; 3];
        for axis in 0..3 {
            let mesh = &self.mesh[axis];
            if coordinates[axis] < f64::from(mesh[0])
                || coordinates[axis] > f64::from(mesh[mesh.len() - 1])
            {
                return Ok(None);
            }
            if let Some(index) =
                (1..mesh.len()).find(|index| coordinates[axis] < f64::from(mesh[*index]))
            {
                cell[axis] = index - 1;
            }
        }

        let cell_dimensions = [
            self.mesh[0].len() - 1,
            self.mesh[1].len() - 1,
            self.mesh[2].len() - 1,
        ];
        let flat_index = cell[0]
            + cell[1] * cell_dimensions[0]
            + cell[2] * cell_dimensions[0] * cell_dimensions[1];
        let database_index = usize::from(self.material_indices[flat_index]);
        if database_index >= self.database_size
            || (database_index == 0 && !properties.use_database_for_background())
        {
            return Ok(None);
        }
        Ok(Some(database_index))
    }

    pub fn material_value(
        &self,
        field: DiscreteMaterialField,
        coordinates: [f64; 3],
        coordinate_system: CoordinateSystem,
        properties: &DiscreteMaterialProperties,
    ) -> Result<Option<f64>, TransformError> {
        let Some(index) = self.material_index(coordinates, coordinate_system, properties)? else {
            return Ok(None);
        };
        Ok(self.database_values[field.index()]
            .as_ref()
            .and_then(|values| values.get(index))
            .map(|value| f64::from(*value)))
    }
}

impl Default for MaterialProperties {
    fn default() -> Self {
        Self::new()
    }
}

impl MaterialProperties {
    pub fn new() -> Self {
        let defaults = [1.0, 1.0, 0.0, 0.0];
        Self {
            isotropic: true,
            values: std::array::from_fn(|field| {
                std::array::from_fn(|_| ParameterScalar::new(defaults[field]))
            }),
            weights: std::array::from_fn(|_| std::array::from_fn(|_| ParameterScalar::new(1.0))),
            density: ParameterScalar::new(0.0),
            density_weight: ParameterScalar::new(1.0),
        }
    }

    pub(crate) fn expression_errors(
        &self,
        parameters: Option<&ParameterSet>,
        coordinate_system: CoordinateSystem,
    ) -> Vec<ParameterError> {
        let mut errors = Vec::new();
        let coordinates = coordinate_parameters([0.0; 3], coordinate_system);
        for component in 0..3 {
            for values in &self.values {
                if let Err(error) = values[component].evaluate(parameters) {
                    errors.push(error);
                }
            }
            for weights in &self.weights {
                if let Err(error) = weights[component].evaluate(Some(&coordinates)) {
                    errors.push(error);
                }
            }
        }
        if let Err(error) = self.density.evaluate(parameters) {
            errors.push(error);
        }
        if let Err(error) = self.density_weight.evaluate(Some(&coordinates)) {
            errors.push(error);
        }
        errors
    }

    pub fn is_isotropic(&self) -> bool {
        self.isotropic
    }

    pub fn set_isotropic(&mut self, isotropic: bool) {
        self.isotropic = isotropic;
    }

    pub fn value(
        &self,
        field: MaterialField,
        component: usize,
        parameters: Option<&ParameterSet>,
    ) -> Result<Option<f64>, ParameterError> {
        let Some(component) = self.component(component) else {
            return Ok(None);
        };
        self.values[field.index()][component]
            .evaluate(parameters)
            .map(Some)
    }

    pub fn value_term(&self, field: MaterialField, component: usize) -> Option<String> {
        let component = self.component(component)?;
        Some(
            self.values[field.index()][component]
                .expression()
                .unwrap_or("")
                .to_owned(),
        )
    }

    pub(crate) fn value_xml_term(&self, field: MaterialField, component: usize) -> Option<String> {
        self.values
            .get(field.index())?
            .get(component)
            .map(ParameterScalar::value_string)
    }

    pub fn set_value(&mut self, field: MaterialField, component: usize, value: f64) -> bool {
        if component >= 3 {
            return false;
        }
        self.values[field.index()][component].set_value(value);
        true
    }

    pub fn set_value_expression(
        &mut self,
        field: MaterialField,
        component: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        if component >= 3 {
            return Ok(false);
        }
        self.values[field.index()][component].set_value_string(expression)?;
        Ok(true)
    }

    pub fn weight_term(&self, field: MaterialField, component: usize) -> Option<String> {
        let component = self.component(component)?;
        Some(
            self.weights[field.index()][component]
                .expression()
                .unwrap_or("")
                .to_owned(),
        )
    }

    pub(crate) fn weight_xml_term(&self, field: MaterialField, component: usize) -> Option<String> {
        self.weights
            .get(field.index())?
            .get(component)
            .map(ParameterScalar::value_string)
    }

    pub fn set_weight_expression(
        &mut self,
        field: MaterialField,
        component: usize,
        expression: &str,
    ) -> Result<bool, ParameterError> {
        if component >= 3 {
            return Ok(false);
        }
        self.weights[field.index()][component].set_value_string(expression)?;
        Ok(true)
    }

    /// Material density evaluated at `coordinates`
    /// (native `CSPropMaterial::GetDensityWeighted`).
    pub fn weighted_density(
        &self,
        values: Option<&ParameterSet>,
        coordinates: [f64; 3],
        coordinate_system: CoordinateSystem,
    ) -> Result<f64, ParameterError> {
        let density = self.density.evaluate(values)?;
        let coordinate_parameters = coordinate_parameters(coordinates, coordinate_system);
        let weight = self.density_weight.evaluate(Some(&coordinate_parameters))?;
        Ok(density * weight)
    }

    pub fn weighted_value(
        &self,
        field: MaterialField,
        component: usize,
        values: Option<&ParameterSet>,
        coordinates: [f64; 3],
        coordinate_system: CoordinateSystem,
    ) -> Result<Option<f64>, ParameterError> {
        let Some(component) = self.component(component) else {
            return Ok(None);
        };
        let value = self.values[field.index()][component].evaluate(values)?;
        let coordinate_parameters = coordinate_parameters(coordinates, coordinate_system);
        let weight =
            self.weights[field.index()][component].evaluate(Some(&coordinate_parameters))?;
        Ok(Some(value * weight))
    }

    pub fn density(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        self.density.evaluate(parameters)
    }

    pub fn density_term(&self) -> String {
        self.density.expression().unwrap_or("").to_owned()
    }

    pub(crate) fn density_xml_term(&self) -> String {
        self.density.value_string()
    }

    pub fn set_density(&mut self, value: f64) {
        self.density.set_value(value);
    }

    pub fn set_density_expression(&mut self, expression: &str) -> Result<(), ParameterError> {
        self.density.set_value_string(expression)
    }

    pub fn density_weight_term(&self) -> String {
        self.density_weight.expression().unwrap_or("").to_owned()
    }

    pub(crate) fn density_weight_xml_term(&self) -> String {
        self.density_weight.value_string()
    }

    pub fn set_density_weight_expression(
        &mut self,
        expression: &str,
    ) -> Result<(), ParameterError> {
        self.density_weight.set_value_string(expression)
    }

    fn component(&self, component: usize) -> Option<usize> {
        let component = if self.isotropic { 0 } else { component };
        (component < 3).then_some(component)
    }
}

fn coordinate_parameters(
    coordinates: [f64; 3],
    coordinate_system: CoordinateSystem,
) -> ParameterSet {
    let [x, y, z, rho, radius, alpha, theta] = if coordinate_system == CoordinateSystem::Cylindrical
    {
        let [rho, alpha, z] = coordinates;
        [
            rho * alpha.cos(),
            rho * alpha.sin(),
            z,
            rho,
            (rho.powi(2) + z.powi(2)).sqrt(),
            alpha,
            1.0_f64.asin() - (z / rho).atan(),
        ]
    } else {
        let [x, y, z] = coordinates;
        let rho = (x.powi(2) + y.powi(2)).sqrt();
        [
            x,
            y,
            z,
            rho,
            (x.powi(2) + y.powi(2) + z.powi(2)).sqrt(),
            y.atan2(x),
            1.0_f64.asin() - (z / rho).atan(),
        ]
    };
    let names = ["x", "y", "z", "rho", "r", "a", "t"];
    let values = [x, y, z, rho, radius, alpha, theta];
    let mut parameters = ParameterSet::new();
    for (name, value) in names.into_iter().zip(values) {
        parameters.add(name, value);
    }
    // Native: bModified stays true from construction; no native code calls SetModified()
    parameters
}

#[cfg(test)]
mod excitation_weight_tests {
    use super::{CoordinateSystem, ExcitationProperties};

    #[test]
    fn weight_function_is_evaluated_at_the_shifted_coordinates() {
        let mut excitation = ExcitationProperties::default();
        excitation.set_excitation_term(0, "2").unwrap();
        excitation.set_weighting_term(0, "x+y").unwrap();
        let value = excitation
            .weighted_excitation(0, None, [3.0, 4.0, 0.0], CoordinateSystem::Cartesian)
            .unwrap();
        assert_eq!(value, 14.0);
    }

    #[test]
    fn weight_origin_shifts_the_evaluation_point() {
        let mut excitation = ExcitationProperties::default();
        excitation.set_excitation_term(0, "1").unwrap();
        excitation.set_weighting_term(0, "x").unwrap();
        excitation.set_weight_origin([1.0, 0.0, 0.0]);
        let value = excitation
            .weighted_excitation(0, None, [3.0, 4.0, 0.0], CoordinateSystem::Cartesian)
            .unwrap();
        assert_eq!(value, 2.0);
    }

    #[test]
    fn cylindrical_coordinates_are_converted_before_the_weight_is_evaluated() {
        let mut excitation = ExcitationProperties::default();
        excitation.set_excitation_term(0, "1").unwrap();
        excitation.set_weighting_term(0, "rho").unwrap();
        let value = excitation
            .weighted_excitation(0, None, [2.0, 0.0, 5.0], CoordinateSystem::Cylindrical)
            .unwrap();
        assert_eq!(value, 2.0);
    }

    #[test]
    fn an_empty_weight_function_means_a_zero_weight() {
        let mut excitation = ExcitationProperties::default();
        excitation.set_excitation_term(0, "1").unwrap();
        excitation.set_weighting_term(0, "").unwrap();
        let value = excitation
            .weighted_excitation(0, None, [1.0, 1.0, 1.0], CoordinateSystem::Cartesian)
            .unwrap();
        assert_eq!(value, 0.0);
    }

    #[test]
    fn setting_a_weight_function_drops_a_previous_weight_file() {
        let mut excitation = ExcitationProperties::default();
        excitation.set_weight_file("mode.h5");
        assert_eq!(excitation.weight_file(), "mode.h5");
        excitation.set_weighting_term(0, "1").unwrap();
        assert_eq!(excitation.weight_file(), "");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AbsorbingBoundaryProperties, AbsorbingBoundaryType, BackgroundMaterial,
        DebyeMaterialProperties, DiscreteMaterialField, DiscreteMaterialGrid,
        DiscreteMaterialGridError, DiscreteMaterialProperties, DumpBoxProperties,
        ExcitationProperties, LumpedElementProperties, LumpedElementType, MaterialField,
        MaterialProperties, ResolutionBoxProperties,
    };
    use crate::grid::CoordinateSystem;
    use crate::parameter::ParameterSet;
    use crate::transform::{Transform, TransformError};

    #[test]
    fn background_material_defaults_and_bounds_match_native_contract() {
        let mut material = BackgroundMaterial::default();
        assert_eq!(material.value(MaterialField::Epsilon), 1.0);
        assert_eq!(material.value(MaterialField::Mue), 1.0);
        assert_eq!(material.value(MaterialField::Kappa), 0.0);
        assert_eq!(material.value(MaterialField::Sigma), 0.0);

        assert!(!material.set_value(MaterialField::Epsilon, 0.5));
        assert!(!material.set_value(MaterialField::Mue, 0.5));
        assert!(!material.set_value(MaterialField::Kappa, -1.0));
        assert!(material.set_value(MaterialField::Sigma, f64::NAN));
        assert_eq!(material.value(MaterialField::Epsilon), 1.0);
        assert_eq!(material.value(MaterialField::Kappa), 0.0);
        assert!(material.value(MaterialField::Sigma).is_nan());
        assert!(material.set_value(MaterialField::Epsilon, 2.0));
        assert_eq!(material.value(MaterialField::Epsilon), 2.0);
    }

    #[test]
    fn background_material_non_finite_setters_match_native_comparisons() {
        for field in [
            MaterialField::Epsilon,
            MaterialField::Mue,
            MaterialField::Kappa,
            MaterialField::Sigma,
        ] {
            let mut material = BackgroundMaterial::default();
            assert!(material.set_value(field, f64::NAN));
            assert!(material.value(field).is_nan());
            assert!(material.set_value(field, f64::INFINITY));
            assert_eq!(material.value(field), f64::INFINITY);
            assert!(!material.set_value(field, f64::NEG_INFINITY));
            assert_eq!(material.value(field), f64::INFINITY);
        }
    }

    #[test]
    fn absorbing_boundary_defaults_and_setters_match_native_contract() {
        let mut boundary = AbsorbingBoundaryProperties::default();
        assert!(boundary.normal_sign_positive());
        assert_eq!(boundary.phase_velocity(None).unwrap(), 299_792_458.0);
        assert_eq!(boundary.boundary_type(), AbsorbingBoundaryType::Undefined);

        boundary.set_phase_velocity(-1.0);
        assert_eq!(boundary.phase_velocity(None).unwrap(), 0.0);
        boundary.set_phase_velocity(f64::NAN);
        assert!(boundary.phase_velocity(None).unwrap().is_nan());
        boundary.set_normal_sign_positive(false);
        boundary.set_boundary_type(AbsorbingBoundaryType::Other(17));
        assert!(!boundary.normal_sign_positive());
        assert_eq!(boundary.boundary_type().xml_value(), 17);
        assert_eq!(
            AbsorbingBoundaryType::from_xml_value(17),
            AbsorbingBoundaryType::Other(17)
        );
    }

    #[test]
    fn lumped_element_defaults_setters_and_expressions_match_native_contract() {
        let mut element = LumpedElementProperties::default();
        assert_eq!(element.direction(), -1);
        assert!(element.caps());
        assert!(element.resistance(None).unwrap().is_nan());
        assert!(element.capacitance(None).unwrap().is_nan());
        assert!(element.inductance(None).unwrap().is_nan());
        assert_eq!(element.element_type(), LumpedElementType::Parallel);

        element.set_direction(3);
        assert_eq!(element.direction(), -1);
        element.set_direction(2);
        element.set_caps(false);
        element.set_element_type(LumpedElementType::Series);
        element.set_resistance_term("2*base").unwrap();
        element.set_capacitance_term("1e-12").unwrap();
        element.set_inductance_term("3e-9").unwrap();
        let mut parameters = ParameterSet::new();
        parameters.add("base", 5.0);
        assert_eq!(element.direction(), 2);
        assert!(!element.caps());
        assert_eq!(element.resistance(Some(&parameters)).unwrap(), 10.0);
        assert_eq!(element.capacitance(None).unwrap(), 1e-12);
        assert_eq!(element.inductance(None).unwrap(), 3e-9);
    }

    #[test]
    fn dump_box_sampling_and_resolution_settings_keep_native_defaults_and_bounds() {
        let mut dump = DumpBoxProperties::default();
        assert_eq!(dump.dump_type(), 0);
        assert_eq!(dump.dump_mode(), 0);
        assert_eq!(dump.file_type(), 0);
        assert_eq!(dump.multi_grid_level(), 0);
        assert!(!dump.sub_sampling_enabled());
        assert_eq!(dump.sub_sampling(), [1, 1, 1]);
        assert!(!dump.optimal_resolution_enabled());
        assert_eq!(dump.optimal_resolution(), [1.0; 3]);

        assert!(dump.set_sub_sampling(1, 4));
        assert!(!dump.set_sub_sampling(0, 0));
        assert!(!dump.set_sub_sampling(3, 2));
        assert_eq!(dump.sub_sampling(), [1, 4, 1]);
        assert!(dump.sub_sampling_enabled());
        assert!(dump.set_optimal_resolution(0, 0.25));
        assert!(!dump.set_optimal_resolution(1, -1.0));
        assert!(!dump.set_optimal_resolution(3, 2.0));
        assert_eq!(dump.optimal_resolution(), [0.25, 1.0, 1.0]);
        assert!(dump.optimal_resolution_enabled());
    }

    #[test]
    fn excitation_defaults_and_parameterized_terms_match_native_contract() {
        let mut excitation = ExcitationProperties::default();
        assert_eq!(excitation.number(), 0);
        assert_eq!(excitation.excitation_type(), 1);
        assert!(excitation.is_enabled());
        assert_eq!(excitation.active_direction(0), Some(true));
        assert_eq!(excitation.active_direction(3), None);
        assert_eq!(excitation.frequency(None).unwrap(), 0.0);
        assert_eq!(excitation.delay(None).unwrap(), 0.0);

        excitation.set_excitation_type(4);
        excitation.set_enabled(false);
        assert!(excitation.set_active_direction(1, false));
        assert!(!excitation.set_active_direction(3, false));
        excitation.set_frequency_term("base*10").unwrap();
        excitation.set_delay_term("base/10").unwrap();
        excitation.set_excitation_term(0, "base+1").unwrap();
        excitation.set_weighting_term(2, "base*2").unwrap();
        excitation.set_propagation_direction_term(2, "1").unwrap();
        let mut parameters = ParameterSet::new();
        parameters.add("base", 2.0);
        assert_eq!(excitation.excitation_type(), 4);
        assert!(!excitation.is_enabled());
        assert_eq!(excitation.active_direction(1), Some(false));
        assert_eq!(excitation.frequency(Some(&parameters)).unwrap(), 20.0);
        assert_eq!(excitation.delay(Some(&parameters)).unwrap(), 0.2);
        assert_eq!(
            excitation.excitation(0, Some(&parameters)).unwrap(),
            Some(3.0)
        );
        assert_eq!(
            excitation.weighting(2, Some(&parameters)).unwrap(),
            Some(4.0)
        );
        assert_eq!(
            excitation
                .propagation_direction(2, Some(&parameters))
                .unwrap(),
            Some(1.0)
        );
    }

    #[test]
    fn resolution_box_defaults_and_factor_match_native_contract() {
        let mut resolution = ResolutionBoxProperties::default();
        assert_eq!(resolution.factor(), 1);
        resolution.set_factor(4);
        assert_eq!(resolution.factor(), 4);
    }

    #[test]
    fn debye_material_orders_expressions_and_coordinate_weights_match_native_contract() {
        let mut parameters = ParameterSet::new();
        parameters.add("base", 2.0);
        let mut material = DebyeMaterialProperties::default();
        material.set_order(2);
        material
            .set_epsilon_delta_expression(0, 0, "base+1")
            .unwrap();
        material
            .set_epsilon_delta_weight_expression(0, 0, "x+1")
            .unwrap();
        material
            .set_epsilon_relax_time_expression(0, 1, "base/2")
            .unwrap();
        material
            .set_epsilon_relax_time_weight_expression(0, 1, "z+1")
            .unwrap();
        assert_eq!(material.order(), 2);
        assert_eq!(
            material
                .weighted_epsilon_delta(
                    0,
                    0,
                    Some(&parameters),
                    [2.0, 0.0, 0.0],
                    CoordinateSystem::Cartesian,
                )
                .unwrap(),
            Some(9.0)
        );
        assert_eq!(
            material
                .weighted_epsilon_relax_time(
                    0,
                    1,
                    Some(&parameters),
                    [0.0, 0.0, 3.0],
                    CoordinateSystem::Cartesian,
                )
                .unwrap(),
            Some(4.0)
        );
        material.set_order(1);
        assert_eq!(material.epsilon_delta_term(0, 0).as_deref(), Some("0"));
        assert!(!material.set_epsilon_delta(1, 0, 1.0));
    }

    #[test]
    fn discrete_material_defaults_match_native_contract() {
        let material = DiscreteMaterialProperties::default();
        assert_eq!(material.file_name(), "");
        assert_eq!(material.file_type(), -1);
        assert!(material.use_database_for_background());
        assert_eq!(material.scale(), 1.0);
        assert!(material.transform().is_none());
    }

    fn test_discrete_grid(indices: Vec<u8>) -> DiscreteMaterialGrid {
        DiscreteMaterialGrid::new(
            [
                vec![0.0, 1.0, 2.0],
                vec![0.0, 1.0, 2.0],
                vec![0.0, 1.0, 2.0],
            ],
            indices,
            8,
        )
        .unwrap()
    }

    #[test]
    fn discrete_material_grid_uses_native_x_fastest_cell_order() {
        let grid = test_discrete_grid((0..8).collect());
        let properties = DiscreteMaterialProperties::default();
        assert_eq!(
            grid.material_index([0.5, 0.5, 0.5], CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            Some(0)
        );
        assert_eq!(
            grid.material_index([1.5, 0.5, 0.5], CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            Some(1)
        );
        assert_eq!(
            grid.material_index([0.5, 1.5, 0.5], CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            Some(2)
        );
        assert_eq!(
            grid.material_index([0.5, 0.5, 1.5], CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            Some(4)
        );
    }

    #[test]
    fn discrete_material_grid_matches_native_boundary_and_fallback_rules() {
        let grid = test_discrete_grid(vec![0, 1, 2, 3, 4, 5, 6, 9]);
        let mut properties = DiscreteMaterialProperties::default();
        assert_eq!(
            grid.material_index([2.0, 0.5, 0.5], CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            Some(0)
        );
        assert_eq!(
            grid.material_index([-0.1, 0.5, 0.5], CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            None
        );
        properties.set_use_database_for_background(false);
        assert_eq!(
            grid.material_index([0.5, 0.5, 0.5], CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            None
        );
        properties.set_use_database_for_background(true);
        assert_eq!(
            grid.material_index([1.5, 1.5, 1.5], CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            None
        );
    }

    #[test]
    fn discrete_material_grid_reads_optional_database_values_by_index() {
        let mut grid = test_discrete_grid((0..8).collect());
        grid.set_database_values(
            DiscreteMaterialField::Epsilon,
            Some((0..8).map(|index| index as f32 + 1.25).collect()),
        )
        .unwrap();
        grid.set_database_values(
            DiscreteMaterialField::Density,
            Some((0..8).map(|index| index as f32 * 10.0).collect()),
        )
        .unwrap();
        let properties = DiscreteMaterialProperties::default();
        assert_eq!(
            grid.material_value(
                DiscreteMaterialField::Epsilon,
                [1.5, 0.5, 0.5],
                CoordinateSystem::Cartesian,
                &properties
            )
            .unwrap(),
            Some(2.25)
        );
        assert_eq!(
            grid.material_value(
                DiscreteMaterialField::Density,
                [1.5, 1.5, 1.5],
                CoordinateSystem::Cartesian,
                &properties
            )
            .unwrap(),
            Some(70.0)
        );
        assert_eq!(
            grid.material_value(
                DiscreteMaterialField::Mue,
                [0.5, 0.5, 0.5],
                CoordinateSystem::Cartesian,
                &properties
            )
            .unwrap(),
            None
        );
        assert_eq!(
            grid.set_database_values(DiscreteMaterialField::Sigma, Some(vec![1.0])),
            Err(DiscreteMaterialGridError::InvalidDatabaseValueCount {
                expected: 8,
                actual: 1
            })
        );
        grid.set_database_values(DiscreteMaterialField::Epsilon, None)
            .unwrap();
        assert_eq!(
            grid.material_value(
                DiscreteMaterialField::Epsilon,
                [1.5, 0.5, 0.5],
                CoordinateSystem::Cartesian,
                &properties
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn discrete_material_grid_applies_coordinate_conversion_transform_then_scale() {
        let grid = test_discrete_grid((0..8).collect());
        let mut properties = DiscreteMaterialProperties::default();
        let mut transform = Transform::new();
        transform.translate([1.0, 0.0, 0.0], true);
        properties.set_transform(Some(transform));
        properties.set_scale(2.0);
        assert_eq!(
            grid.material_index([3.5, 0.5, 0.5], CoordinateSystem::Cartesian, &properties)
                .unwrap(),
            Some(1)
        );
        properties.set_transform(None);
        properties.set_scale(1.0);
        assert_eq!(
            grid.material_index(
                [1.5, std::f64::consts::FRAC_PI_2, 0.5],
                CoordinateSystem::Cylindrical,
                &properties
            )
            .unwrap(),
            Some(2)
        );
    }

    #[test]
    fn discrete_material_grid_rejects_bad_shapes_and_reports_singular_transform() {
        assert_eq!(
            DiscreteMaterialGrid::new([vec![0.0], vec![0.0, 1.0], vec![0.0, 1.0]], vec![0], 1)
                .unwrap_err(),
            DiscreteMaterialGridError::MeshAxisTooShort { axis: 0, actual: 1 }
        );
        assert_eq!(
            DiscreteMaterialGrid::new([vec![0.0, 1.0], vec![0.0, 1.0], vec![0.0, 1.0]], vec![], 1)
                .unwrap_err(),
            DiscreteMaterialGridError::InvalidMaterialIndexCount {
                expected: 1,
                actual: 0
            }
        );

        let grid = test_discrete_grid(vec![0; 8]);
        let mut properties = DiscreteMaterialProperties::default();
        let mut transform = Transform::new();
        transform.scale3([0.0, 1.0, 1.0], true);
        properties.set_transform(Some(transform));
        assert_eq!(
            grid.material_index([0.5, 0.5, 0.5], CoordinateSystem::Cartesian, &properties),
            Err(TransformError::SingularMatrix)
        );
    }

    #[test]
    fn defaults_and_isotropic_component_selection_match_material_contract() {
        let mut material = MaterialProperties::new();
        assert!(material.is_isotropic());
        assert_eq!(
            material.value(MaterialField::Epsilon, 2, None).unwrap(),
            Some(1.0)
        );
        assert_eq!(
            material.value(MaterialField::Mue, 1, None).unwrap(),
            Some(1.0)
        );
        assert_eq!(
            material.value(MaterialField::Kappa, 0, None).unwrap(),
            Some(0.0)
        );
        assert_eq!(
            material.value_term(MaterialField::Epsilon, 0).as_deref(),
            Some("")
        );
        assert_eq!(material.density(None).unwrap(), 0.0);

        material.set_value(MaterialField::Epsilon, 1, 2.5);
        assert_eq!(
            material.value(MaterialField::Epsilon, 2, None).unwrap(),
            Some(1.0)
        );
        material.set_isotropic(false);
        assert_eq!(
            material.value(MaterialField::Epsilon, 1, None).unwrap(),
            Some(2.5)
        );
        assert_eq!(
            material.value(MaterialField::Epsilon, 3, None).unwrap(),
            None
        );
    }

    #[test]
    fn parameter_values_are_evaluated_against_structure_parameters() {
        let mut parameters = ParameterSet::new();
        parameters.add("base", 2.0);
        let mut material = MaterialProperties::new();
        material
            .set_value_expression(MaterialField::Epsilon, 0, "base+1")
            .unwrap();
        assert_eq!(
            material
                .value(MaterialField::Epsilon, 0, Some(&parameters))
                .unwrap(),
            Some(3.0)
        );
    }

    #[test]
    fn weight_functions_use_cartesian_and_cylindrical_coordinate_variables() {
        let mut material = MaterialProperties::new();
        material.set_value(MaterialField::Epsilon, 0, 3.0);
        material
            .set_weight_expression(MaterialField::Epsilon, 0, "x+y+z+rho+r")
            .unwrap();
        assert_eq!(
            material
                .weighted_value(
                    MaterialField::Epsilon,
                    0,
                    None,
                    [3.0, 4.0, 0.0],
                    CoordinateSystem::Cartesian,
                )
                .unwrap(),
            Some(51.0)
        );

        material
            .set_weight_expression(MaterialField::Epsilon, 0, "x")
            .unwrap();
        let result = material
            .weighted_value(
                MaterialField::Epsilon,
                0,
                None,
                [2.0, 0.0, 0.0],
                CoordinateSystem::Cylindrical,
            )
            .unwrap();
        assert_eq!(result, Some(6.0));
    }

    #[test]
    fn density_weighting_and_parse_errors_are_explicit() {
        let mut material = MaterialProperties::new();
        material.set_density(2.0);
        material.set_density_weight_expression("z*z").unwrap();
        assert_eq!(
            material
                .weighted_density(None, [0.0, 0.0, 3.0], CoordinateSystem::Cartesian)
                .unwrap(),
            18.0
        );

        material.set_density_weight_expression("unknown").unwrap();
        assert!(material
            .weighted_density(None, [0.0; 3], CoordinateSystem::Cartesian)
            .is_err());
    }
}
