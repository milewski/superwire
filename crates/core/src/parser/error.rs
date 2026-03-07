use std::num::ParseFloatError;
use std::ops::Range;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("grammar error: {message}")]
    Grammar { message: String },
    #[error("missing field `{field}` at span {span:?}")]
    MissingField { field: String, span: Range<usize> },
    #[error("unexpected parser rule `{rule}` at span {span:?}")]
    UnexpectedRule { rule: String, span: Range<usize> },
    #[error("invalid property `{property}` type: expected {expected}, got {actual}")]
    InvalidPropertyType {
        property: String,
        expected: String,
        actual: String,
    },
    #[error("invalid model reference `{value}`, expected provider/model")]
    InvalidModelReference { value: String },
    #[error("invalid reference `{reference}`, expected {expected}")]
    InvalidReference { reference: String, expected: String },
    #[error("failed to parse number `{value}`")]
    NumberParse {
        value: String,
        #[source]
        source: ParseFloatError,
    },
}
