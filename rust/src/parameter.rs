use fparser::{EvalError, ParseError, Parser};
use std::error::Error;
use std::fmt;

// Native PSErrorCode2Msg table from CSXCAD/src/ParameterObjects.cpp:654-720.
// Codes -1, 0, 1..5, 100..110 have messages; every other code → empty string
// (no default: case in the native switch ⇒ unlisted codes append nothing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PsErrorCode {
    InternalError,     // -1
    NoError,           // 0
    DivisionByZero,    // 1
    SqrtError,         // 2
    LogError,          // 3
    TrigError,         // 4
    MaxRecursion,      // 5
    SyntaxError,       // 100
    MismatchedParen,   // 101
    MissingParen,      // 102
    EmptyParens,       // 103
    ExpectedOperator,  // 104
    OutOfMemory,       // 105
    UnexpectedError,   // 106
    SyntaxInParams,    // 107
    IllParamsAmount,   // 108
    PrematureEos,      // 109
    ExpectFuncParen,   // 110
}

impl PsErrorCode {
    /// Human-readable message for a PSErrorCode, matching the native
    /// ParameterObjects.cpp::PSErrorCode2Msg switch (no default: case).
    pub fn message(self) -> &'static str {
        match self {
            PsErrorCode::InternalError => "Internal Error",
            PsErrorCode::NoError => "No Error",
            PsErrorCode::DivisionByZero => "Division by zero",
            PsErrorCode::SqrtError => "sqrt error (sqrt of a negative value)",
            PsErrorCode::LogError => "log error (logarithm of a negative value)",
            PsErrorCode::TrigError => "trigonometric error (asin or acos of illegal value)",
            PsErrorCode::MaxRecursion => "Maximum recursion level reached",
            PsErrorCode::SyntaxError => "Syntax error (check Parameter)",
            PsErrorCode::MismatchedParen => "Mismatched parenthesis",
            PsErrorCode::MissingParen => "Missing ')'",
            PsErrorCode::EmptyParens => "Empty parentheses",
            PsErrorCode::ExpectedOperator => "Syntax error: Operator expected",
            PsErrorCode::OutOfMemory => "Not enough memory",
            PsErrorCode::UnexpectedError => "An unexpected error occurred",
            PsErrorCode::SyntaxInParams => "Syntax error in parameters",
            PsErrorCode::IllParamsAmount => "Illegal number of parameters to function",
            PsErrorCode::PrematureEos => "Syntax error: Premature end of string",
            PsErrorCode::ExpectFuncParen => "Syntax error: Expecting ( after function",
        }
    }
}

/// Look up a PS error code by its numeric value, matching the native
/// ParameterObjects.cpp::PSErrorCode2Msg switch behavior (no default: case).
/// Unlisted codes return an empty string.
pub fn ps_error_code2_msg(code: i32) -> &'static str {
    match code {
        -1 => "Internal Error",
        0 => "No Error",
        1 => "Division by zero",
        2 => "sqrt error (sqrt of a negative value)",
        3 => "log error (logarithm of a negative value)",
        4 => "trigonometric error (asin or acos of illegal value)",
        5 => "Maximum recursion level reached",
        100 => "Syntax error (check Parameter)",
        101 => "Mismatched parenthesis",
        102 => "Missing ')'",
        103 => "Empty parentheses",
        104 => "Syntax error: Operator expected",
        105 => "Not enough memory",
        106 => "An unexpected error occurred",
        107 => "Syntax error in parameters",
        108 => "Illegal number of parameters to function",
        109 => "Syntax error: Premature end of string",
        110 => "Syntax error: Expecting ( after function",
        _ => "", // unlisted codes → empty string (no default: case)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ParameterKind {
    Constant,
    Linear { min: f64, max: f64, step: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    name: String,
    value: f64,
    kind: ParameterKind,
    modified: bool,
    sweep_enabled: bool,
}

impl Parameter {
    pub fn new(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value,
            kind: ParameterKind::Constant,
            modified: true,
            sweep_enabled: true,
        }
    }

    pub fn linear(name: impl Into<String>, value: f64, min: f64, max: f64, step: f64) -> Self {
        let max = max.max(min);
        let mut parameter = Self {
            name: name.into(),
            value: 0.0,
            kind: ParameterKind::Linear {
                min,
                max,
                step: step.max(0.0),
            },
            modified: true,
            sweep_enabled: true,
        };
        parameter.set_value(value);
        parameter
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
        self.modified = true;
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn set_value(&mut self, value: f64) {
        self.value = match self.kind {
            ParameterKind::Constant => value,
            ParameterKind::Linear { min, max, step } => {
                let value = value.clamp(min, max);
                if step == 0.0 {
                    value
                } else {
                    let snapped = min + step * ((value - min) / step + 0.5).floor();
                    if snapped > max {
                        snapped - step
                    } else {
                        snapped
                    }
                }
            }
        };
        self.modified = true;
    }

    pub fn is_linear(&self) -> bool {
        matches!(self.kind, ParameterKind::Linear { .. })
    }

    pub fn get_sweep(&self) -> bool {
        self.is_linear() && self.sweep_enabled
    }

    pub fn sweep_enabled(&self) -> bool {
        self.sweep_enabled
    }

    pub fn set_sweep(&mut self, enabled: bool) {
        self.sweep_enabled = enabled;
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn set_modified(&mut self, modified: bool) {
        self.modified = modified;
    }

    pub fn linear_range(&self) -> Option<(f64, f64, f64)> {
        match self.kind {
            ParameterKind::Constant => None,
            ParameterKind::Linear { min, max, step } => Some((min, max, step)),
        }
    }

    pub fn init_sweep(&mut self) {
        if let ParameterKind::Linear { min, .. } = self.kind {
            self.set_value(min);
        }
    }

    pub fn increase_step(&mut self) -> bool {
        let Some((_, max, step)) = self.linear_range() else {
            return false;
        };
        if step == 0.0 || self.value + step > max {
            return false;
        }
        self.set_value(self.value + step);
        true
    }

    pub fn count_steps(&self) -> usize {
        match self.kind {
            ParameterKind::Constant => 1,
            ParameterKind::Linear {
                min: _,
                max: _,
                step: 0.0,
            } => 1,
            ParameterKind::Linear { min, max, step } => ((max - min) / step) as usize + 1,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParameterSet {
    parameters: Vec<Parameter>,
    modified: bool,
}

impl ParameterSet {
    pub fn new() -> Self {
        Self {
            parameters: Vec::new(),
            modified: true,
        }
    }

    pub fn insert(&mut self, parameter: Parameter) -> usize {
        self.parameters.push(parameter);
        self.modified = true;
        self.parameters.len()
    }

    pub fn add(&mut self, name: impl Into<String>, value: f64) -> usize {
        self.insert(Parameter::new(name, value))
    }

    pub fn add_linear(
        &mut self,
        name: impl Into<String>,
        value: f64,
        min: f64,
        max: f64,
        step: f64,
    ) -> usize {
        self.insert(Parameter::linear(name, value, min, max, step))
    }

    pub fn get(&self, index: usize) -> Option<&Parameter> {
        self.parameters.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Parameter> {
        let parameter = self.parameters.get_mut(index)?;
        self.modified = true;
        Some(parameter)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Parameter> {
        self.parameters.iter()
    }

    pub fn len(&self) -> usize {
        self.parameters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parameters.is_empty()
    }

    pub fn clear(&mut self) {
        self.parameters.clear();
        self.modified = true;
    }

    pub fn is_modified(&self) -> bool {
        self.modified || self.parameters.iter().any(Parameter::is_modified)
    }

    pub fn set_modified(&mut self, modified: bool) {
        self.modified = modified;
        for parameter in &mut self.parameters {
            parameter.set_modified(modified);
        }
    }

    pub fn parameter_string(&self, spacer: &str) -> String {
        self.parameters
            .iter()
            .map(Parameter::name)
            .collect::<Vec<_>>()
            .join(spacer)
    }

    pub fn value_array(&self) -> Vec<f64> {
        self.parameters.iter().map(Parameter::value).collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterError {
    EmptyExpression,
    Parse(ParseError),
    Evaluation(EvalError),
}

impl fmt::Display for ParameterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExpression => formatter.write_str("expression must not be empty"),
            Self::Parse(error) => write!(formatter, "expression parse failed: {error}"),
            Self::Evaluation(error) => write!(formatter, "expression evaluation failed: {error}"),
        }
    }
}

impl Error for ParameterError {}

#[derive(Clone, Debug, PartialEq)]
enum ScalarValue {
    Literal(f64),
    Expression(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterScalar {
    value: ScalarValue,
}

impl Default for ParameterScalar {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl ParameterScalar {
    pub fn new(value: f64) -> Self {
        Self {
            value: ScalarValue::Literal(value),
        }
    }

    pub fn from_expression(expression: impl Into<String>) -> Result<Self, ParameterError> {
        let expression = expression.into();
        if expression.is_empty() {
            return Err(ParameterError::EmptyExpression);
        }
        Ok(Self {
            value: ScalarValue::Expression(expression),
        })
    }

    pub fn from_value_string(value: impl Into<String>) -> Result<Self, ParameterError> {
        let value = value.into();
        let mut scalar = Self::default();
        scalar.set_value_string(&value)?;
        Ok(scalar)
    }

    pub fn set_value(&mut self, value: f64) {
        self.value = ScalarValue::Literal(value);
    }

    pub fn set_expression(&mut self, expression: impl Into<String>) -> Result<(), ParameterError> {
        let expression = expression.into();
        if expression.is_empty() {
            return Err(ParameterError::EmptyExpression);
        }
        self.value = ScalarValue::Expression(expression);
        Ok(())
    }

    pub fn set_value_string(&mut self, value: &str) -> Result<(), ParameterError> {
        if value.is_empty() {
            // Native: set_value_string("") returns -1 (empty expression error)
            // Rust maps this to ParameterError::EmptyExpression
            return Err(ParameterError::EmptyExpression);
        }
        if let Ok(number) = value.parse::<f64>() {
            self.set_value(number);
            Ok(())
        } else {
            // Unbound scalar with expression: parse and evaluate at set time,
            // matching native CSXCAD behavior where every setter uses Eval=true
            // and discards the returned code. Errors are consumed internally;
            // the set always succeeds.
            let mut parser = Parser::new();
            let variables = String::new(); // no parameters passed
            let _ = parser.parse(value, &variables, false);
            let _ = parser.eval(&[]);
            let _ = self.set_expression(value);
            Ok(())
        }
    }

    pub fn is_expression(&self) -> bool {
        matches!(self.value, ScalarValue::Expression(_))
    }

    pub fn expression(&self) -> Option<&str> {
        match &self.value {
            ScalarValue::Literal(_) => None,
            ScalarValue::Expression(expression) => Some(expression),
        }
    }

    pub fn value_string(&self) -> String {
        match &self.value {
            ScalarValue::Literal(value) => value.to_string(),
            ScalarValue::Expression(expression) => expression.clone(),
        }
    }

    pub fn evaluate(&self, parameters: Option<&ParameterSet>) -> Result<f64, ParameterError> {
        let ScalarValue::Expression(expression) = &self.value else {
            let ScalarValue::Literal(value) = &self.value else {
                unreachable!();
            };
            return Ok(*value);
        };
        let mut parser = Parser::new();
        let variables = parameters.map_or_else(String::new, |set| set.parameter_string(","));
        parser
            .parse(expression, &variables, false)
            .map_err(ParameterError::Parse)?;
        let values = parameters.map_or_else(Vec::new, ParameterSet::value_array);
        parser.eval(&values).map_err(ParameterError::Evaluation)
    }
}

#[cfg(test)]
mod tests {
    use super::{Parameter, ParameterError, ParameterScalar, ParameterSet};

    #[test]
    fn evaluates_expressions_against_ordered_parameters() {
        let mut parameters = ParameterSet::new();
        parameters.add("width", 2.0);
        parameters.add("height", 3.0);
        let expression = ParameterScalar::from_expression("2*width+height").unwrap();

        assert_eq!(parameters.parameter_string(","), "width,height");
        assert_eq!(expression.evaluate(Some(&parameters)).unwrap(), 7.0);
    }

    #[test]
    fn expression_evaluation_uses_current_parameter_values() {
        let mut parameters = ParameterSet::new();
        parameters.add("length", 4.0);
        let expression = ParameterScalar::from_expression("length^2").unwrap();
        assert_eq!(expression.evaluate(Some(&parameters)).unwrap(), 16.0);

        parameters.get_mut(0).unwrap().set_value(5.0);
        assert_eq!(expression.evaluate(Some(&parameters)).unwrap(), 25.0);
    }

    #[test]
    fn reports_expression_errors_without_falling_back_to_a_value() {
        let expression = ParameterScalar::from_expression("missing+1").unwrap();
        assert!(matches!(
            expression.evaluate(None),
            Err(ParameterError::Parse(_))
        ));
    }

    #[test]
    fn linear_parameters_clamp_snap_and_sweep() {
        let mut parameter = Parameter::linear("x", 2.6, 0.0, 5.0, 2.0);
        assert_eq!(parameter.value(), 2.0);
        assert_eq!(parameter.count_steps(), 3);
        parameter.init_sweep();
        assert_eq!(parameter.value(), 0.0);
        assert!(parameter.increase_step());
        assert_eq!(parameter.value(), 2.0);
        assert!(parameter.increase_step());
        assert_eq!(parameter.value(), 4.0);
        assert!(!parameter.increase_step());
        parameter.set_value(5.0);
        assert_eq!(parameter.value(), 4.0);
    }

    #[test]
    fn zero_step_linear_parameter_is_one_value() {
        let mut parameter = Parameter::linear("x", 3.0, 2.0, 5.0, 0.0);
        assert_eq!(parameter.value(), 3.0);
        assert_eq!(parameter.count_steps(), 1);
        assert!(!parameter.increase_step());
    }
}
