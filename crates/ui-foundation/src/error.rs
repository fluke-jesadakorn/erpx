use erpx_cqrs::{CqrsError, EnvelopeError};
use erpx_kernel::{CorrelationId, ErrorCategory, KernelError};
use serde::{Deserialize, Serialize};

pub const MAX_UI_ERROR_CODE_LENGTH: usize = 96;
pub const MAX_UI_ERROR_MESSAGE_LENGTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiErrorKind {
    Invalid,
    Denied,
    NotFound,
    Conflict,
    NotReady,
    Offline,
    Failed,
}

/// A bounded error safe for presenters and views.
///
/// Backend diagnostic context is intentionally absent. Callers should use the
/// correlation ID to find complete diagnostics in server-side telemetry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiError {
    kind: UiErrorKind,
    code: String,
    message: String,
    retryable: bool,
    correlation_id: Option<CorrelationId>,
}

impl UiError {
    pub fn bounded(
        kind: UiErrorKind,
        code: impl AsRef<str>,
        message: impl AsRef<str>,
        retryable: bool,
        correlation_id: Option<CorrelationId>,
    ) -> Self {
        Self {
            kind,
            code: bounded_code(code.as_ref()),
            message: bounded_message(message.as_ref()),
            retryable,
            correlation_id,
        }
    }

    pub fn offline() -> Self {
        Self::bounded(
            UiErrorKind::Offline,
            "transport_unavailable",
            "The service is temporarily unavailable.",
            true,
            None,
        )
    }

    pub fn not_ready(message: impl AsRef<str>) -> Self {
        Self::bounded(
            UiErrorKind::NotReady,
            "projection_not_ready",
            message,
            true,
            None,
        )
    }

    pub fn invalid(message: impl AsRef<str>) -> Self {
        Self::bounded(
            UiErrorKind::Invalid,
            "invalid_request",
            message,
            false,
            None,
        )
    }

    pub const fn kind(&self) -> UiErrorKind {
        self.kind
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn retryable(&self) -> bool {
        self.retryable
    }

    pub const fn correlation_id(&self) -> Option<CorrelationId> {
        self.correlation_id
    }

    pub fn from_kernel(error: KernelError) -> Self {
        let kind = kind_for_category(error.category);
        let (code, message) = if error.category == ErrorCategory::Internal {
            (
                "internal_error",
                "Something went wrong. Use the correlation ID when contacting support.",
            )
        } else {
            (error.code.as_str(), error.safe_message.as_str())
        };
        Self::bounded(
            kind,
            code,
            message,
            error.retryable,
            Some(error.correlation_id),
        )
    }

    pub fn from_cqrs(error: CqrsError) -> Self {
        match error {
            CqrsError::InvalidEnvelope(cause) => Self::from_envelope(cause),
            CqrsError::NotReady(_) => Self::not_ready(
                "Your change was saved, but this view has not caught up yet. Please retry.",
            ),
            CqrsError::Operation(cause) => Self::from_kernel(cause),
        }
    }

    fn from_envelope(_: EnvelopeError) -> Self {
        Self::invalid("The request context is invalid.")
    }
}

fn kind_for_category(category: ErrorCategory) -> UiErrorKind {
    match category {
        ErrorCategory::Invalid => UiErrorKind::Invalid,
        ErrorCategory::Denied => UiErrorKind::Denied,
        ErrorCategory::NotFound => UiErrorKind::NotFound,
        ErrorCategory::Conflict => UiErrorKind::Conflict,
        ErrorCategory::NotReady => UiErrorKind::NotReady,
        ErrorCategory::Unavailable => UiErrorKind::Offline,
        ErrorCategory::Internal => UiErrorKind::Failed,
    }
}

fn bounded_code(value: &str) -> String {
    let valid = !value.is_empty()
        && value.len() <= MAX_UI_ERROR_CODE_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        value.to_owned()
    } else {
        "unclassified_error".into()
    }
}

fn bounded_message(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len().min(MAX_UI_ERROR_MESSAGE_LENGTH));
    for character in value.chars().filter(|character| !character.is_control()) {
        if sanitized.len() + character.len_utf8() > MAX_UI_ERROR_MESSAGE_LENGTH {
            break;
        }
        sanitized.push(character);
    }
    if sanitized.trim().is_empty() {
        "The request could not be completed.".into()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_kernel::ErrorContext;

    fn correlation_id(value: u128) -> CorrelationId {
        CorrelationId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .expect("test UUID must be non-nil")
    }

    #[test]
    fn internal_errors_never_expose_server_diagnostics() {
        let mut context = ErrorContext::empty();
        context
            .try_insert("database", "postgres://admin:secret@example.test")
            .unwrap();
        let error = UiError::from_kernel(KernelError {
            code: "sql_failure".into(),
            category: ErrorCategory::Internal,
            safe_message: "query included customer secret".into(),
            retryable: false,
            correlation_id: correlation_id(1),
            context,
        });
        assert_eq!(error.kind(), UiErrorKind::Failed);
        assert_eq!(error.code(), "internal_error");
        assert!(!error.message().contains("secret"));
        assert_eq!(error.correlation_id(), Some(correlation_id(1)));
    }

    #[test]
    fn public_errors_are_bounded_and_control_characters_are_removed() {
        let error = UiError::bounded(
            UiErrorKind::Denied,
            "INVALID CODE",
            format!("denied\n{}", "x".repeat(MAX_UI_ERROR_MESSAGE_LENGTH + 20)),
            false,
            None,
        );
        assert_eq!(error.code(), "unclassified_error");
        assert!(!error.message().contains('\n'));
        assert!(error.message().len() <= MAX_UI_ERROR_MESSAGE_LENGTH);
    }
}
