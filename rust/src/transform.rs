use crate::parameter::{ParameterError, ParameterScalar, ParameterSet};
use std::error::Error;
use std::fmt;

pub type Matrix4 = [f64; 16];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformType {
    Scale,
    Scale3,
    Translate,
    RotateOrigin,
    RotateX,
    RotateY,
    RotateZ,
    Matrix,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransformOperation {
    pub kind: TransformType,
    pub arguments: Vec<ParameterScalar>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TransformError {
    InvalidDirection,
    InvalidArgumentCount { expected: String, actual: usize },
    UnknownOperation(String),
    ZeroRotationAxis,
    SingularMatrix,
    Parameter(ParameterError),
}

impl fmt::Display for TransformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDirection => formatter.write_str("rotation direction must be 0, 1, or 2"),
            Self::InvalidArgumentCount { expected, actual } => {
                write!(formatter, "expected {expected} arguments, got {actual}")
            }
            Self::UnknownOperation(operation) => {
                write!(formatter, "unknown transformation: {operation}")
            }
            Self::ZeroRotationAxis => formatter.write_str("rotation axis must not be zero"),
            Self::SingularMatrix => formatter.write_str("transform matrix is singular"),
            Self::Parameter(error) => write!(formatter, "transform argument failed: {error}"),
        }
    }
}

impl Error for TransformError {}

impl From<ParameterError> for TransformError {
    fn from(error: ParameterError) -> Self {
        Self::Parameter(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transform {
    matrix: Matrix4,
    inverse_matrix: Option<Matrix4>,
    post_multiply: bool,
    angle_radian: bool,
    operations: Vec<TransformOperation>,
}

impl Default for Transform {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform {
    pub fn new() -> Self {
        Self {
            matrix: identity_matrix(),
            inverse_matrix: Some(identity_matrix()),
            post_multiply: true,
            angle_radian: true,
            operations: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.matrix = identity_matrix();
        self.inverse_matrix = Some(identity_matrix());
        self.post_multiply = true;
        self.angle_radian = true;
        self.operations.clear();
    }

    pub fn matrix(&self) -> &Matrix4 {
        &self.matrix
    }

    pub fn operations(&self) -> &[TransformOperation] {
        &self.operations
    }

    pub fn has_transform(&self) -> bool {
        !self.operations.is_empty()
    }

    pub fn set_pre_multiply(&mut self) {
        self.post_multiply = false;
    }

    pub fn set_post_multiply(&mut self) {
        self.post_multiply = true;
    }

    pub fn set_angle_degree(&mut self) {
        self.angle_radian = false;
    }

    pub fn set_angle_radian(&mut self) {
        self.angle_radian = true;
    }

    pub fn transform(&self, coordinates: [f64; 3]) -> [f64; 3] {
        transform_point(&self.matrix, coordinates)
    }

    pub fn inverse_transform(&self, coordinates: [f64; 3]) -> Result<[f64; 3], TransformError> {
        self.inverse_matrix
            .as_ref()
            .map(|matrix| transform_point(matrix, coordinates))
            .ok_or(TransformError::SingularMatrix)
    }

    pub fn invert(&mut self) -> Result<(), TransformError> {
        let inverse = self
            .inverse_matrix
            .take()
            .ok_or(TransformError::SingularMatrix)?;
        self.inverse_matrix = Some(self.matrix);
        self.matrix = inverse;
        Ok(())
    }

    pub fn set_matrix(&mut self, matrix: Matrix4, concatenate: bool) {
        self.apply_matrix(&matrix, concatenate);
        self.record(TransformType::Matrix, &matrix);
    }

    pub fn translate(&mut self, translation: [f64; 3], concatenate: bool) {
        let mut matrix = identity_matrix();
        for (row, value) in translation.into_iter().enumerate() {
            matrix[4 * row + 3] = value;
        }
        self.apply_matrix(&matrix, concatenate);
        self.record(TransformType::Translate, &translation);
    }

    pub fn scale(&mut self, scale: f64, concatenate: bool) {
        self.scale3_with_type([scale, scale, scale], concatenate, TransformType::Scale);
    }

    pub fn scale3(&mut self, scale: [f64; 3], concatenate: bool) {
        self.scale3_with_type(scale, concatenate, TransformType::Scale3);
    }

    fn scale3_with_type(&mut self, scale: [f64; 3], concatenate: bool, kind: TransformType) {
        let mut matrix = identity_matrix();
        for (axis, value) in scale.into_iter().enumerate() {
            matrix[4 * axis + axis] = value;
        }
        self.apply_matrix(&matrix, concatenate);
        self.record(kind, &scale);
    }

    pub fn rotate_axis(
        &mut self,
        direction: usize,
        angle: f64,
        concatenate: bool,
    ) -> Result<(), TransformError> {
        if direction > 2 {
            return Err(TransformError::InvalidDirection);
        }
        let mut axis = [0.0; 3];
        axis[direction] = 1.0;
        let matrix = self.rotation_matrix(axis, angle)?;
        self.apply_matrix(&matrix, concatenate);
        let kind = match direction {
            0 => TransformType::RotateX,
            1 => TransformType::RotateY,
            _ => TransformType::RotateZ,
        };
        self.record(kind, &[angle]);
        Ok(())
    }

    pub fn rotate_origin(
        &mut self,
        axis: [f64; 3],
        angle: f64,
        concatenate: bool,
    ) -> Result<(), TransformError> {
        let matrix = self.rotation_matrix(axis, angle)?;
        self.apply_matrix(&matrix, concatenate);
        self.record(
            TransformType::RotateOrigin,
            &[axis[0], axis[1], axis[2], angle],
        );
        Ok(())
    }

    pub fn translate_expressions(
        &mut self,
        arguments: &str,
        parameters: Option<&ParameterSet>,
        concatenate: bool,
    ) -> Result<(), TransformError> {
        let values = evaluate_arguments(arguments, 3, parameters)?;
        let scalar_arguments = expression_scalars(arguments)?;
        let mut matrix = identity_matrix();
        for row in 0..3 {
            matrix[4 * row + 3] = values[row];
        }
        self.apply_matrix(&matrix, concatenate);
        self.record_scalars(TransformType::Translate, scalar_arguments);
        Ok(())
    }

    pub fn scale_expressions(
        &mut self,
        arguments: &str,
        parameters: Option<&ParameterSet>,
        concatenate: bool,
    ) -> Result<(), TransformError> {
        let raw_arguments = split_arguments(arguments);
        match raw_arguments.len() {
            1 => {
                let value = evaluate_argument(raw_arguments[0], parameters)?;
                let scalar = ParameterScalar::from_value_string(raw_arguments[0])?;
                self.scale(value, concatenate);
                if let Some(operation) = self.operations.last_mut() {
                    operation.arguments = vec![scalar];
                }
            }
            3 => {
                let values = raw_arguments
                    .iter()
                    .map(|argument| evaluate_argument(argument, parameters))
                    .collect::<Result<Vec<_>, _>>()?;
                let scalar_arguments = expression_scalars(arguments)?;
                let scales = [values[0], values[1], values[2]];
                let mut matrix = identity_matrix();
                for axis in 0..3 {
                    matrix[4 * axis + axis] = scales[axis];
                }
                self.apply_matrix(&matrix, concatenate);
                self.record_scalars(TransformType::Scale3, scalar_arguments);
            }
            actual => {
                return Err(TransformError::InvalidArgumentCount {
                    expected: "1 or 3".to_owned(),
                    actual,
                });
            }
        }
        Ok(())
    }

    pub fn rotate_axis_expression(
        &mut self,
        direction: usize,
        angle: &str,
        parameters: Option<&ParameterSet>,
        concatenate: bool,
    ) -> Result<(), TransformError> {
        if direction > 2 {
            return Err(TransformError::InvalidDirection);
        }
        let angle_value = evaluate_argument(angle, parameters)?;
        let mut axis = [0.0; 3];
        axis[direction] = 1.0;
        let matrix = self.rotation_matrix(axis, angle_value)?;
        self.apply_matrix(&matrix, concatenate);
        let kind = match direction {
            0 => TransformType::RotateX,
            1 => TransformType::RotateY,
            _ => TransformType::RotateZ,
        };
        self.record_scalars(kind, vec![ParameterScalar::from_value_string(angle)?]);
        Ok(())
    }

    pub fn transform_by_string(
        &mut self,
        operation: &str,
        arguments: &str,
        parameters: Option<&ParameterSet>,
        concatenate: bool,
    ) -> Result<(), TransformError> {
        match operation {
            "Translate" => self.translate_expressions(arguments, parameters, concatenate),
            "Scale" => self.scale_expressions(arguments, parameters, concatenate),
            "RotateX" | "RotateY" | "RotateZ" => {
                let direction = match operation {
                    "RotateX" => 0,
                    "RotateY" => 1,
                    _ => 2,
                };
                self.rotate_axis_expression(direction, arguments, parameters, concatenate)
            }
            "RotateOrigin" => {
                let values = evaluate_arguments(arguments, 4, parameters)?;
                let scalar_arguments = expression_scalars(arguments)?;
                self.rotate_origin([values[0], values[1], values[2]], values[3], concatenate)?;
                if let Some(operation) = self.operations.last_mut() {
                    operation.arguments = scalar_arguments;
                }
                Ok(())
            }
            "Matrix" => {
                let values = evaluate_arguments(arguments, 16, parameters)?;
                let scalar_arguments = expression_scalars(arguments)?;
                let matrix: Matrix4 = values.try_into().expect("argument count checked");
                self.apply_matrix(&matrix, concatenate);
                self.record_scalars(TransformType::Matrix, scalar_arguments);
                Ok(())
            }
            _ => Err(TransformError::UnknownOperation(operation.to_owned())),
        }
    }

    fn rotation_matrix(&self, axis: [f64; 3], angle: f64) -> Result<Matrix4, TransformError> {
        let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        if length == 0.0 {
            return Err(TransformError::ZeroRotationAxis);
        }
        let angle = if self.angle_radian {
            angle
        } else {
            angle * std::f64::consts::PI / 180.0
        };
        let unit = [axis[0] / length, axis[1] / length, axis[2] / length];
        let (sin, cos) = angle.sin_cos();
        let mut matrix = [0.0; 16];
        matrix[15] = 1.0;
        for row in 0..3 {
            let next = (row + 1) % 3;
            let previous = (row + 2) % 3;
            matrix[4 * row + row] = unit[row] * unit[row] + (1.0 - unit[row] * unit[row]) * cos;
            matrix[4 * row + next] += unit[row] * unit[next] * (1.0 - cos) - unit[previous] * sin;
            matrix[4 * row + previous] +=
                unit[row] * unit[previous] * (1.0 - cos) + unit[next] * sin;
        }
        Ok(matrix)
    }

    fn apply_matrix(&mut self, matrix: &Matrix4, concatenate: bool) {
        if concatenate {
            self.matrix = if self.post_multiply {
                multiply_matrices(matrix, &self.matrix)
            } else {
                multiply_matrices(&self.matrix, matrix)
            };
        } else {
            self.matrix = *matrix;
            self.operations.clear();
        }
        self.inverse_matrix = invert_matrix(&self.matrix);
    }

    fn record(&mut self, kind: TransformType, values: &[f64]) {
        self.operations.push(TransformOperation {
            kind,
            arguments: values.iter().copied().map(ParameterScalar::new).collect(),
        });
    }

    fn record_scalars(&mut self, kind: TransformType, arguments: Vec<ParameterScalar>) {
        self.operations.push(TransformOperation { kind, arguments });
    }
}

fn identity_matrix() -> Matrix4 {
    [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

fn multiply_matrices(left: &Matrix4, right: &Matrix4) -> Matrix4 {
    let mut result = [0.0; 16];
    for row in 0..4 {
        for column in 0..4 {
            for inner in 0..4 {
                result[4 * row + column] += left[4 * row + inner] * right[4 * inner + column];
            }
        }
    }
    result
}

fn invert_matrix(matrix: &Matrix4) -> Option<Matrix4> {
    let mut augmented = [[0.0; 8]; 4];
    for row in 0..4 {
        augmented[row][..4].copy_from_slice(&matrix[4 * row..4 * row + 4]);
        augmented[row][4 + row] = 1.0;
    }
    for pivot in 0..4 {
        let pivot_row = (pivot..4).max_by(|left, right| {
            augmented[*left][pivot]
                .abs()
                .total_cmp(&augmented[*right][pivot].abs())
        })?;
        if augmented[pivot_row][pivot] == 0.0 || !augmented[pivot_row][pivot].is_finite() {
            return None;
        }
        augmented.swap(pivot, pivot_row);
        let divisor = augmented[pivot][pivot];
        for value in &mut augmented[pivot] {
            *value /= divisor;
        }
        let pivot_values = augmented[pivot];
        for (row_index, row) in augmented.iter_mut().enumerate() {
            if row_index == pivot {
                continue;
            }
            let factor = row[pivot];
            for (value, pivot_value) in row.iter_mut().zip(pivot_values) {
                *value -= factor * pivot_value;
            }
        }
    }
    let mut inverse = [0.0; 16];
    for row in 0..4 {
        inverse[4 * row..4 * row + 4].copy_from_slice(&augmented[row][4..8]);
    }
    Some(inverse)
}

fn transform_point(matrix: &Matrix4, coordinates: [f64; 3]) -> [f64; 3] {
    let homogeneous = [coordinates[0], coordinates[1], coordinates[2], 1.0];
    std::array::from_fn(|row| {
        (0..4)
            .map(|column| matrix[4 * row + column] * homogeneous[column])
            .sum()
    })
}

fn split_arguments(arguments: &str) -> Vec<&str> {
    arguments.split(',').map(str::trim).collect()
}

fn expression_scalars(arguments: &str) -> Result<Vec<ParameterScalar>, ParameterError> {
    split_arguments(arguments)
        .into_iter()
        .map(ParameterScalar::from_value_string)
        .collect()
}

fn evaluate_arguments(
    arguments: &str,
    expected: usize,
    parameters: Option<&ParameterSet>,
) -> Result<Vec<f64>, TransformError> {
    let arguments = split_arguments(arguments);
    if arguments.len() != expected {
        return Err(TransformError::InvalidArgumentCount {
            expected: expected.to_string(),
            actual: arguments.len(),
        });
    }
    arguments
        .into_iter()
        .map(|argument| evaluate_argument(argument, parameters))
        .collect()
}

fn evaluate_argument(
    argument: &str,
    parameters: Option<&ParameterSet>,
) -> Result<f64, TransformError> {
    let scalar = ParameterScalar::from_value_string(argument)?;
    scalar
        .evaluate(parameters)
        .map_err(TransformError::Parameter)
}

#[cfg(test)]
mod tests {
    use super::{Transform, TransformError, TransformType};
    use crate::parameter::ParameterSet;
    use std::f64::consts::FRAC_PI_2;

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-12, "{left} != {right}");
    }

    fn close_point(left: [f64; 3], right: [f64; 3]) {
        for axis in 0..3 {
            close(left[axis], right[axis]);
        }
    }

    #[test]
    fn identity_translation_and_inverse_match_native_row_major_convention() {
        let mut transform = Transform::new();
        assert!(!transform.has_transform());
        transform.translate([1.0, 2.0, 1.0], true);
        close_point(transform.transform([1.0, 1.0, 1.0]), [2.0, 3.0, 2.0]);
        close_point(
            transform.inverse_transform([2.0, 3.0, 2.0]).unwrap(),
            [1.0, 1.0, 1.0],
        );
        assert_eq!(transform.matrix()[3], 1.0);
        assert_eq!(transform.matrix()[7], 2.0);
    }

    #[test]
    fn pre_and_post_multiply_follow_cst_transform_order() {
        let mut pre = Transform::new();
        pre.translate([1.0, 0.0, 0.0], true);
        pre.set_pre_multiply();
        pre.rotate_axis(2, FRAC_PI_2, true).unwrap();
        close_point(pre.transform([1.0, 0.0, 0.0]), [1.0, 1.0, 0.0]);

        let mut post = Transform::new();
        post.translate([1.0, 0.0, 0.0], true);
        post.set_post_multiply();
        post.rotate_axis(2, FRAC_PI_2, true).unwrap();
        close_point(post.transform([1.0, 0.0, 0.0]), [0.0, 2.0, 0.0]);
    }

    #[test]
    fn axis_rotation_supports_radians_and_degrees() {
        let mut radians = Transform::new();
        radians.rotate_axis(0, FRAC_PI_2, true).unwrap();
        close_point(radians.transform([0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);

        let mut degrees = Transform::new();
        degrees.set_angle_degree();
        degrees.rotate_axis(2, 90.0, true).unwrap();
        close_point(degrees.transform([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn arbitrary_axis_rotation_matches_cst_transform_convention() {
        let mut transform = Transform::new();
        transform
            .rotate_origin([0.0, 1.0, 1.0], FRAC_PI_2, true)
            .unwrap();
        let reciprocal_sqrt_two = 1.0 / 2.0_f64.sqrt();
        close_point(
            transform.transform([1.0, 0.0, 0.0]),
            [0.0, reciprocal_sqrt_two, -reciprocal_sqrt_two],
        );
    }

    #[test]
    fn parameterized_arguments_are_evaluated_and_kept_in_operation_log() {
        let mut parameters = ParameterSet::new();
        parameters.add("dx", 3.0);
        let mut transform = Transform::new();
        transform
            .translate_expressions("dx, 2, 0", Some(&parameters), true)
            .unwrap();
        close_point(transform.transform([0.0, 0.0, 0.0]), [3.0, 2.0, 0.0]);
        assert_eq!(
            transform.operations()[0].arguments[0].expression(),
            Some("dx")
        );
    }

    #[test]
    fn nonconcatenating_operations_replace_matrix_and_history() {
        let mut transform = Transform::new();
        transform.translate([1.0, 0.0, 0.0], true);
        transform.scale(2.0, false);
        assert_eq!(transform.operations().len(), 1);
        assert_eq!(transform.operations()[0].kind, TransformType::Scale);
        close_point(transform.transform([1.0, 1.0, 1.0]), [2.0, 2.0, 2.0]);
    }

    #[test]
    fn reports_invalid_operations_and_singular_inverse() {
        let mut transform = Transform::new();
        assert_eq!(
            transform.rotate_axis(3, 1.0, true),
            Err(TransformError::InvalidDirection)
        );
        transform.scale3([0.0, 1.0, 1.0], true);
        assert_eq!(
            transform.inverse_transform([0.0; 3]),
            Err(TransformError::SingularMatrix)
        );
        assert_eq!(
            transform.transform_by_string("Unknown", "", None, true),
            Err(TransformError::UnknownOperation("Unknown".to_owned()))
        );
    }

    #[test]
    fn reset_restores_default_modes_and_clears_history() {
        let mut transform = Transform::new();
        transform.set_pre_multiply();
        transform.set_angle_degree();
        transform.rotate_axis(2, 90.0, true).unwrap();
        transform.reset();
        transform.translate([1.0, 0.0, 0.0], true);
        transform.rotate_axis(2, FRAC_PI_2, true).unwrap();
        close_point(transform.transform([1.0, 0.0, 0.0]), [0.0, 2.0, 0.0]);
        assert_eq!(transform.operations().len(), 2);
    }
}
