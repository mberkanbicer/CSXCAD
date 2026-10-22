use fparser::{EvalError, ParseError, Parser};
use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum CoordinateSystem {
    #[default]
    Cartesian = 0,
    Cylindrical = 1,
    Undefined = 2,
}

pub fn transform_coordinates(
    coordinates: [f64; 3],
    input: CoordinateSystem,
    output: CoordinateSystem,
) -> [f64; 3] {
    match (input, output) {
        (CoordinateSystem::Cartesian, CoordinateSystem::Cylindrical) => {
            let [x, y, z] = coordinates;
            [(x.powi(2) + y.powi(2)).sqrt(), y.atan2(x), z]
        }
        (CoordinateSystem::Cylindrical, CoordinateSystem::Cartesian) => {
            let [rho, alpha, z] = coordinates;
            [rho * alpha.cos(), rho * alpha.sin(), z]
        }
        _ => coordinates,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GridError {
    InvalidDirection,
    Parse(ParseError),
    Evaluation(EvalError),
}

impl fmt::Display for GridError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDirection => formatter.write_str("grid direction must be 0, 1, or 2"),
            Self::Parse(error) => write!(formatter, "grid distribution parse failed: {error}"),
            Self::Evaluation(error) => {
                write!(formatter, "grid distribution evaluation failed: {error}")
            }
        }
    }
}

impl Error for GridError {}

#[derive(Clone, Debug, Default, PartialEq)]
struct LineVector {
    lines: Vec<f64>,
    dirty: bool,
}

impl LineVector {
    fn ensure_sorted(&mut self) {
        if !self.dirty {
            return;
        }
        self.lines.sort_by(f64::total_cmp);
        self.lines.dedup_by(|left, right| *left == *right);
        self.dirty = false;
    }

    fn add(&mut self, value: f64) {
        if !self.dirty {
            if self.lines.is_empty() || value > *self.lines.last().expect("non-empty checked") {
                self.lines.push(value);
                return;
            }
            if value == *self.lines.last().expect("non-empty checked") {
                return;
            }
            self.dirty = true;
        }
        self.lines.push(value);
    }

    fn set(&mut self, index: usize, value: f64) -> bool {
        self.ensure_sorted();
        let Some(line) = self.lines.get_mut(index) else {
            return false;
        };
        *line = value;
        self.dirty = true;
        true
    }

    fn remove_at(&mut self, index: usize) -> bool {
        self.ensure_sorted();
        if index >= self.lines.len() {
            return false;
        }
        self.lines.remove(index);
        true
    }

    fn remove_value(&mut self, value: f64) -> bool {
        self.ensure_sorted();
        let Some(index) = self.lines.iter().position(|line| *line == value) else {
            return false;
        };
        self.lines.remove(index);
        true
    }

    fn get(&mut self, index: usize) -> Option<f64> {
        self.ensure_sorted();
        self.lines.get(index).copied()
    }

    fn lines(&mut self) -> &[f64] {
        self.ensure_sorted();
        &self.lines
    }

    fn clear(&mut self) {
        self.lines.clear();
        self.dirty = false;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RectGrid {
    lines: [LineVector; 3],
    delta_unit: f64,
    coordinate_system: CoordinateSystem,
}

impl Default for RectGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl RectGrid {
    pub fn new() -> Self {
        Self {
            lines: std::array::from_fn(|_| LineVector::default()),
            delta_unit: 1.0,
            coordinate_system: CoordinateSystem::Cartesian,
        }
    }

    pub fn add_line(&mut self, direction: usize, value: f64) {
        if let Some(lines) = self.lines.get_mut(direction) {
            lines.add(value);
        }
    }

    pub fn add_lines(&mut self, direction: usize, values: &[f64]) {
        for value in values {
            self.add_line(direction, *value);
        }
    }

    pub fn add_lines_with_function(
        &mut self,
        direction: usize,
        values: &[f64],
        expression: &str,
    ) -> Result<(), GridError> {
        let variable = match direction {
            0 => "x",
            1 => "y",
            2 => "z",
            _ => return Err(GridError::InvalidDirection),
        };
        let mut parser = Parser::new();
        parser
            .parse(expression, variable, false)
            .map_err(GridError::Parse)?;
        let transformed = values
            .iter()
            .map(|value| parser.eval(&[*value]).map_err(GridError::Evaluation))
            .collect::<Result<Vec<_>, _>>()?;
        self.add_lines(direction, &transformed);
        Ok(())
    }

    pub fn set_line(&mut self, direction: usize, index: usize, value: f64) -> bool {
        self.lines
            .get_mut(direction)
            .is_some_and(|lines| lines.set(index, value))
    }

    pub fn remove_line(&mut self, direction: usize, index: usize) -> bool {
        self.lines
            .get_mut(direction)
            .is_some_and(|lines| lines.remove_at(index))
    }

    pub fn remove_value(&mut self, direction: usize, value: f64) -> bool {
        self.lines
            .get_mut(direction)
            .is_some_and(|lines| lines.remove_value(value))
    }

    pub fn clear(&mut self) {
        for lines in &mut self.lines {
            lines.clear();
        }
        self.delta_unit = 1.0;
    }

    pub fn clear_lines(&mut self, direction: usize) {
        if let Some(lines) = self.lines.get_mut(direction) {
            lines.clear();
        }
    }

    pub fn line(&mut self, direction: usize, index: usize) -> f64 {
        self.lines
            .get_mut(direction)
            .and_then(|lines| lines.get(index))
            .unwrap_or(0.0)
    }

    pub fn lines(&mut self, direction: usize) -> Option<&[f64]> {
        self.lines.get_mut(direction).map(LineVector::lines)
    }

    pub fn line_count(&mut self, direction: usize) -> usize {
        self.lines
            .get_mut(direction)
            .map_or(0, |lines| lines.lines().len())
    }

    pub fn snap_to_line(&mut self, direction: usize, value: f64) -> (Option<usize>, bool) {
        let Some(lines) = self.lines.get_mut(direction).map(LineVector::lines) else {
            return (None, false);
        };
        let Some(first) = lines.first() else {
            return (None, false);
        };
        let last_index = lines.len() - 1;
        if value < *first {
            return (Some(0), false);
        }
        if value > lines[last_index] {
            return (Some(last_index), false);
        }
        for index in 0..last_index {
            if value < 0.5 * (lines[index] + lines[index + 1]) {
                return (Some(index), true);
            }
        }
        (Some(last_index), true)
    }

    pub fn dimension(&mut self) -> i32 {
        for lines in &mut self.lines {
            lines.ensure_sorted();
        }
        if self.lines.iter().any(|lines| lines.lines.is_empty()) {
            return -1;
        }
        self.lines
            .iter()
            .filter(|lines| lines.lines.len() > 1)
            .count() as i32
    }

    pub fn increase_resolution(&mut self, direction: usize, factor: usize) {
        let dimension = self.dimension();
        if dimension < 0 || direction >= dimension as usize || !(2..=9).contains(&factor) {
            return;
        }
        let Some(lines) = self.lines.get_mut(direction).map(LineVector::lines) else {
            return;
        };
        let original = lines.to_vec();
        for pair in original.windows(2) {
            let delta = (pair[1] - pair[0]) / factor as f64;
            for subdivision in 1..factor {
                self.lines[direction].add(pair[0] + subdivision as f64 * delta);
            }
        }
    }

    pub fn simulation_area(&mut self) -> [f64; 6] {
        let mut area = [0.0; 6];
        for direction in 0..3 {
            if let Some(lines) = self.lines[direction].lines().first().copied() {
                let direction_lines = self.lines[direction].lines();
                area[2 * direction] = lines;
                area[2 * direction + 1] = *direction_lines.last().expect("first exists");
            }
        }
        area
    }

    pub fn is_valid(&mut self) -> bool {
        (0..3).all(|direction| self.line_count(direction) >= 2)
    }

    pub fn sort(&mut self, direction: usize) {
        if let Some(lines) = self.lines.get_mut(direction) {
            lines.ensure_sorted();
        }
    }

    pub fn delta_unit(&self) -> f64 {
        self.delta_unit
    }

    pub fn set_delta_unit(&mut self, value: f64) {
        self.delta_unit = value;
    }

    pub fn coordinate_system(&self) -> CoordinateSystem {
        self.coordinate_system
    }

    pub fn set_coordinate_system(&mut self, coordinate_system: CoordinateSystem) {
        self.coordinate_system = coordinate_system;
    }
}

#[cfg(test)]
mod tests {
    use super::{CoordinateSystem, RectGrid};

    #[test]
    fn sorts_and_deduplicates_lines_on_read() {
        let mut grid = RectGrid::new();
        grid.add_lines(0, &[5.0, 0.0, 5.0, 2.0]);
        assert_eq!(grid.line_count(0), 3);
        assert_eq!(grid.lines(0), Some(&[0.0, 2.0, 5.0][..]));
        assert_eq!(grid.line(0, 3), 0.0);
    }

    #[test]
    fn edits_remove_and_clone_keep_line_storage_independent() {
        let mut grid = RectGrid::new();
        grid.add_lines(0, &[0.0, 2.0, 4.0]);
        assert!(grid.set_line(0, 1, 5.0));
        assert_eq!(grid.lines(0), Some(&[0.0, 4.0, 5.0][..]));
        assert!(grid.remove_value(0, 4.0));
        assert!(grid.remove_line(0, 0));
        assert!(!grid.remove_line(0, 5));
        assert_eq!(grid.lines(0), Some(&[5.0][..]));

        let mut clone = grid.clone();
        clone.add_line(0, 6.0);
        assert_eq!(grid.lines(0), Some(&[5.0][..]));
        assert_eq!(clone.lines(0), Some(&[5.0, 6.0][..]));
    }

    #[test]
    fn reports_dimension_area_and_validity() {
        let mut grid = RectGrid::new();
        grid.add_lines(0, &[0.0, 1.0]);
        grid.add_line(1, 4.0);
        grid.add_lines(2, &[-2.0, 3.0]);
        assert_eq!(grid.dimension(), 2);
        assert_eq!(grid.simulation_area(), [0.0, 1.0, 4.0, 4.0, -2.0, 3.0]);
        assert!(!grid.is_valid());
        grid.add_line(1, 5.0);
        assert!(grid.is_valid());
    }

    #[test]
    fn coordinate_transforms_match_native_cartesian_cylindrical_and_undefined_rules() {
        assert_eq!(
            super::transform_coordinates(
                [0.0, 2.0, 3.0],
                CoordinateSystem::Cartesian,
                CoordinateSystem::Cylindrical
            ),
            [2.0, std::f64::consts::FRAC_PI_2, 3.0]
        );
        let cartesian = super::transform_coordinates(
            [2.0, std::f64::consts::FRAC_PI_2, 3.0],
            CoordinateSystem::Cylindrical,
            CoordinateSystem::Cartesian,
        );
        assert!(cartesian[0].abs() < 1e-15);
        assert_eq!(cartesian[1], 2.0);
        assert_eq!(cartesian[2], 3.0);
        assert_eq!(
            super::transform_coordinates(
                [1.0, 2.0, 3.0],
                CoordinateSystem::Undefined,
                CoordinateSystem::Cartesian
            ),
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn snapping_clamps_outside_and_uses_the_upper_line_at_midpoint() {
        let mut grid = RectGrid::new();
        grid.add_lines(0, &[0.0, 2.0, 4.0]);
        assert_eq!(grid.snap_to_line(0, -1.0), (Some(0), false));
        assert_eq!(grid.snap_to_line(0, 1.0), (Some(1), true));
        assert_eq!(grid.snap_to_line(0, 3.0), (Some(2), true));
        assert_eq!(grid.snap_to_line(0, 5.0), (Some(2), false));
        assert_eq!(grid.snap_to_line(3, 1.0), (None, false));
    }

    #[test]
    fn refinement_adds_intermediate_lines_and_respects_grid_dimension() {
        let mut grid = RectGrid::new();
        grid.add_lines(0, &[0.0, 4.0]);
        grid.add_line(1, 0.0);
        grid.add_line(2, 0.0);
        grid.increase_resolution(0, 4);
        assert_eq!(grid.lines(0), Some(&[0.0, 1.0, 2.0, 3.0, 4.0][..]));

        grid.clear_lines(1);
        grid.add_line(1, 0.0);
        grid.increase_resolution(1, 4);
        assert_eq!(grid.line_count(1), 1);
    }

    #[test]
    fn distribution_function_transforms_values_and_rejects_bad_inputs() {
        let mut grid = RectGrid::new();
        grid.add_lines_with_function(0, &[1.0, 2.0], "x^2").unwrap();
        assert_eq!(grid.lines(0), Some(&[1.0, 4.0][..]));
        assert!(grid.add_lines_with_function(3, &[1.0], "x").is_err());
        assert!(grid.add_lines_with_function(0, &[1.0], "x+").is_err());
    }

    #[test]
    fn grid_defaults_and_clear_match_csrectgrid() {
        let mut grid = RectGrid::new();
        assert_eq!(grid.delta_unit(), 1.0);
        assert_eq!(grid.coordinate_system(), CoordinateSystem::Cartesian);
        grid.set_coordinate_system(CoordinateSystem::Cylindrical);
        grid.set_delta_unit(0.001);
        grid.add_line(0, 2.0);
        grid.clear();
        assert_eq!(grid.delta_unit(), 1.0);
        assert_eq!(grid.coordinate_system(), CoordinateSystem::Cylindrical);
        assert_eq!(grid.line_count(0), 0);
    }
}
