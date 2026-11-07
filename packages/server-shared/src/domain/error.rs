use std::fmt;

/// A semantic failure; HTTP status selection belongs to an inbound adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    InvalidInput,
    Unauthenticated,
    Forbidden,
    NotFound,
    Conflict,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusinessError {
    pub kind: FailureKind,
    pub code: &'static str,
    pub message: &'static str,
}

impl BusinessError {
    pub const fn new(kind: FailureKind, code: &'static str, message: &'static str) -> Self {
        Self {
            kind,
            code,
            message,
        }
    }

    pub const fn invalid(code: &'static str, message: &'static str) -> Self {
        Self::new(FailureKind::InvalidInput, code, message)
    }
}

impl fmt::Display for BusinessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BusinessError {}
