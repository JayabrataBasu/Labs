//! The kernel's single error type.
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabError {
    /// Invalid configuration or an invalid request from a rule.
    Config(String),
    /// A checked integer operation would have wrapped.
    Overflow(&'static str),
    /// The exact conservation identity failed for a resource.
    Conservation { resource: usize, expected: u64, actual: u64 },
    /// Malformed checkpoint bytes.
    Codec(String),
    Io(String),
}

impl fmt::Display for LabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LabError::Config(m) => write!(f, "configuration error: {m}"),
            LabError::Overflow(what) => write!(f, "integer overflow in {what}"),
            LabError::Conservation { resource, expected, actual } => {
                write!(f, "conservation violated for resource {resource}: expected {expected}, found {actual}")
            }
            LabError::Codec(m) => write!(f, "checkpoint error: {m}"),
            LabError::Io(m) => write!(f, "I/O error: {m}"),
        }
    }
}

impl std::error::Error for LabError {}
