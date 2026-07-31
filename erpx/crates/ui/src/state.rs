use std::fmt;

/// Explicit UI state kinds used to keep non-successful read and write states visible to users.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiStateKind {
    /// Data is being requested.
    Loading,
    /// The request succeeded but has no rows/content.
    Empty,
    /// The current principal cannot view the content.
    Denied,
    /// Data was loaded from a projection older than the freshness budget.
    Stale,
    /// The current context permits reads but not mutations.
    ReadOnly,
    /// A write lost an optimistic-concurrency race.
    Conflict,
    /// The client cannot reach its backend.
    Offline,
    /// An unexpected request failure occurred.
    Error,
}

/// Target-neutral accessibility behavior for a state panel.
///
/// The consuming Dioxus target maps these values to web/native accessibility properties. The
/// state kind remains supplied by the authoritative application read boundary; this contract
/// only makes the rendering semantics consistent across web, desktop, Android, and iOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiAccessibilityContract {
    /// Semantic accessibility role for the state announcement.
    pub role: &'static str,
    /// Live-region priority used when the state changes.
    pub aria_live: &'static str,
    /// Whether the complete state panel should be announced atomically.
    pub aria_atomic: bool,
    /// Whether the panel represents an in-flight authoritative read.
    pub aria_busy: bool,
    /// Whether a safe cached/read-only payload may remain visible beside this state.
    pub cached_payload_allowed: bool,
}

impl UiStateKind {
    /// Return the stable DOM/QA key for this rendered state.
    ///
    /// Applications use this only as an observable presentation marker. The server remains
    /// authoritative for the underlying read and authorization decision.
    pub const fn dom_key(self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Empty => "empty",
            Self::Denied => "denied",
            Self::Stale => "stale",
            Self::ReadOnly => "read-only",
            Self::Conflict => "conflict",
            Self::Offline => "offline",
            Self::Error => "error",
        }
    }

    /// Return the observable authorization posture associated with the state panel.
    ///
    /// `authorized` means the read boundary supplied a displayable result or cache policy;
    /// `pending` and `unknown` deliberately avoid implying an authorization decision.
    pub const fn authorization_key(self) -> &'static str {
        match self {
            Self::Loading => "pending",
            Self::Empty | Self::Stale | Self::Conflict => "authorized",
            Self::ReadOnly => "read-only",
            Self::Denied => "denied",
            Self::Offline | Self::Error => "unknown",
        }
    }

    /// Return the shared accessibility and safe-payload policy for this state.
    pub const fn accessibility(self) -> UiAccessibilityContract {
        match self {
            Self::Loading => UiAccessibilityContract {
                role: "status",
                aria_live: "polite",
                aria_atomic: true,
                aria_busy: true,
                cached_payload_allowed: false,
            },
            Self::Empty => UiAccessibilityContract {
                role: "status",
                aria_live: "polite",
                aria_atomic: true,
                aria_busy: false,
                cached_payload_allowed: false,
            },
            Self::Denied | Self::Error => UiAccessibilityContract {
                role: "alert",
                aria_live: "assertive",
                aria_atomic: true,
                aria_busy: false,
                cached_payload_allowed: false,
            },
            Self::Stale | Self::ReadOnly | Self::Offline => UiAccessibilityContract {
                role: "status",
                aria_live: "polite",
                aria_atomic: true,
                aria_busy: false,
                cached_payload_allowed: true,
            },
            Self::Conflict => UiAccessibilityContract {
                role: "alert",
                aria_live: "assertive",
                aria_atomic: true,
                aria_busy: false,
                cached_payload_allowed: true,
            },
        }
    }
}

/// Display-safe text for state titles, reasons, and error messages.
///
/// Error details are untrusted even when the read itself was authorized. This boundary keeps
/// credential-bearing diagnostics, control characters, and oversized payloads out of every
/// shared state component without pretending that the UI authorizes the underlying read.
#[derive(Clone, PartialEq, Eq)]
pub struct SafeUiText(String);

impl SafeUiText {
    /// Maximum UTF-8 bytes retained for one display string.
    pub const MAX_BYTES: usize = 512;

    /// Redact secret-bearing or malformed text before it reaches a renderer.
    pub fn new(value: impl AsRef<str>) -> Self {
        let value = value.as_ref().trim();
        if value.is_empty() || contains_secret_marker(value) || contains_control_character(value) {
            return Self("[redacted]".to_string());
        }

        let mut end = value.len().min(Self::MAX_BYTES);
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        let mut safe = value[..end].to_string();
        if end < value.len() {
            safe.push('…');
        }
        Self(safe)
    }

    /// Borrow the safe text for rendering.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SafeUiText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("SafeUiText").field(&self.0).finish()
    }
}

fn contains_secret_marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "secret",
        "password",
        "credential",
        "access_token=",
        "token=",
        "refresh_token=",
        "client_secret=",
        "client-secret=",
        "database_url=",
        "password=",
        "password:",
        "authorization: bearer ",
        "authorization:bearer ",
        "bearer ",
        "-----begin ",
        "_secret",
        "secret=",
        "secret:",
        "api_key=",
        "api-key=",
        "apikey=",
        "private_key=",
        "private-key=",
        "signed_url=",
        "presigned_url=",
        "postgres://",
        "postgresql://",
        "mysql://",
        "redis://",
        "amqp://",
        "ldap://",
        "ldaps://",
        "ldapi://",
        "ssh://",
        "grpc://",
        "http://",
        "https://",
        "sk_live_",
        "rk_live_",
        "mfa_code=",
        "otp=",
        "one_time_code",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

fn contains_control_character(value: &str) -> bool {
    value.chars().any(|character| {
        character.is_control() && character != '\n' && character != '\r' && character != '\t'
    })
}

/// Generic state machine for read models and forms. The UI crate deliberately does not attach
/// authorization decisions to it; the server remains authoritative for access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiState<T> {
    /// Request in progress.
    Loading,
    /// Successful empty result.
    Empty,
    /// Successful result.
    Ready(T),
    /// Read denied by the server or policy boundary.
    Denied { reason: String },
    /// Result is available but older than the freshness budget.
    Stale { age_seconds: u64, reason: String },
    /// Mutations are disabled for the current context.
    ReadOnly { reason: String },
    /// A write conflict needs user review.
    Conflict { reason: String },
    /// No network/backend connection is currently available.
    Offline { reason: String },
    /// Request failed for an unspecified reason.
    Error { reason: String },
}

impl<T> UiState<T> {
    /// Reset a state after the authoritative session boundary reports revocation.
    ///
    /// This method consumes the previous state so a ready value or cached/stale result cannot
    /// accidentally remain renderable after revocation. The resulting denied reason is also
    /// passed through the shared display-safety boundary because transport diagnostics are
    /// untrusted even when the revocation decision itself is authoritative.
    pub fn reset_after_revoke(self, reason: impl AsRef<str>) -> Self {
        drop(self);
        Self::Denied {
            reason: SafeUiText::new(reason).as_str().to_owned(),
        }
    }

    /// Return the visible state kind.
    pub const fn kind(&self) -> Option<UiStateKind> {
        match self {
            Self::Loading => Some(UiStateKind::Loading),
            Self::Empty => Some(UiStateKind::Empty),
            Self::Ready(_) => None,
            Self::Denied { .. } => Some(UiStateKind::Denied),
            Self::Stale { .. } => Some(UiStateKind::Stale),
            Self::ReadOnly { .. } => Some(UiStateKind::ReadOnly),
            Self::Conflict { .. } => Some(UiStateKind::Conflict),
            Self::Offline { .. } => Some(UiStateKind::Offline),
            Self::Error { .. } => Some(UiStateKind::Error),
        }
    }

    /// Map a successful result into the explicit empty/ready states.
    pub fn from_option(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Ready(value),
            None => Self::Empty,
        }
    }
}

/// A field-level validation error safe to display beside an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Stable form field name.
    pub field: String,
    /// Localized or localization-key-resolved message.
    pub message: String,
}

impl ValidationError {
    /// Create an error for a field.
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Small form state container for dirty/submitting/error rendering.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormState {
    /// Whether a user changed a value locally.
    pub dirty: bool,
    /// Whether a write is currently in flight.
    pub submitting: bool,
    /// Field-level validation errors.
    pub errors: Vec<ValidationError>,
}

impl FormState {
    /// Clear all errors after a successful write or reset.
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }

    /// Return the first error for a field.
    pub fn error_for(&self, field: &str) -> Option<&str> {
        self.errors
            .iter()
            .find(|error| error.field == field)
            .map(|error| error.message.as_str())
    }
}

/// Validate a required field after trimming whitespace.
pub fn validate_required(field: &str, value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::new(field, "This field is required."))
    } else {
        Ok(())
    }
}

/// Validate a conservative email shape for client-side feedback. The server must revalidate.
pub fn validate_email(field: &str, value: &str) -> Result<(), ValidationError> {
    let value = value.trim();
    let valid = value.len() >= 3
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });
    if valid {
        Ok(())
    } else {
        Err(ValidationError::new(field, "Enter a valid email address."))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FormState, SafeUiText, UiState, UiStateKind, ValidationError, validate_email,
        validate_required,
    };

    #[test]
    fn explicit_states_are_not_collapsed_into_generic_error() {
        assert_eq!(UiState::<()>::Loading.kind(), Some(UiStateKind::Loading));
        assert_eq!(UiState::<()>::Empty.kind(), Some(UiStateKind::Empty));
        assert_eq!(UiState::<()>::Ready(()).kind(), None);
        assert_eq!(
            UiState::<()>::Denied {
                reason: "policy".into()
            }
            .kind(),
            Some(UiStateKind::Denied)
        );
        assert_eq!(
            UiState::<()>::Stale {
                age_seconds: 60,
                reason: "lag".into()
            }
            .kind(),
            Some(UiStateKind::Stale)
        );
        assert_eq!(
            UiState::<()>::ReadOnly {
                reason: "billing".into()
            }
            .kind(),
            Some(UiStateKind::ReadOnly)
        );
        assert_eq!(
            UiState::<()>::Conflict {
                reason: "version".into()
            }
            .kind(),
            Some(UiStateKind::Conflict)
        );
        assert_eq!(
            UiState::<()>::Offline {
                reason: "network".into()
            }
            .kind(),
            Some(UiStateKind::Offline)
        );
        assert_eq!(
            UiState::<()>::Error {
                reason: "failure".into()
            }
            .kind(),
            Some(UiStateKind::Error)
        );
    }

    #[test]
    fn validation_and_form_errors_are_field_scoped() {
        assert!(validate_required("name", "  ").is_err());
        assert!(validate_required("name", "Folio").is_ok());
        assert!(validate_email("email", "ops@example.com").is_ok());
        assert!(validate_email("email", "not-an-email").is_err());

        let mut form = FormState {
            errors: vec![ValidationError::new("email", "Invalid")],
            ..FormState::default()
        };
        assert_eq!(form.error_for("email"), Some("Invalid"));
        assert_eq!(form.error_for("name"), None);
        form.clear_errors();
        assert!(form.errors.is_empty());
    }

    #[test]
    fn accessibility_contract_keeps_authoritative_state_distinguishable() {
        let alert_states = [
            UiStateKind::Denied,
            UiStateKind::Conflict,
            UiStateKind::Error,
        ];
        for state in alert_states {
            let contract = state.accessibility();
            assert_eq!(contract.role, "alert");
            assert_eq!(contract.aria_live, "assertive");
            assert!(contract.aria_atomic);
            assert!(!contract.aria_busy);
        }

        assert!(UiStateKind::Loading.accessibility().aria_busy);
        assert!(!UiStateKind::Loading.accessibility().cached_payload_allowed);
        assert!(UiStateKind::Stale.accessibility().cached_payload_allowed);
        assert!(UiStateKind::Offline.accessibility().cached_payload_allowed);
        assert!(UiStateKind::Conflict.accessibility().cached_payload_allowed);
    }

    #[test]
    fn state_copy_is_redacted_and_bounded_at_the_shared_display_boundary() {
        for secret in [
            "access_token=secret",
            "Authorization: Bearer secret",
            "postgres://user:password=secret@example.test/db",
            "postgresql://user:secret@example.test/db",
            "ldaps://directory.example.test/ou=users",
            "api-key=secret",
            "token=secret",
            "https://example.test/file?presigned_url=secret",
            "-----BEGIN PRIVATE KEY-----",
        ] {
            let safe = SafeUiText::new(secret);
            assert_eq!(safe.as_str(), "[redacted]", "secret leaked: {secret}");
            assert!(!format!("{safe:?}").contains("secret"));
        }

        let long = "a".repeat(SafeUiText::MAX_BYTES + 10);
        let safe = SafeUiText::new(long);
        assert!(safe.as_str().len() <= SafeUiText::MAX_BYTES + '…'.len_utf8());
        assert!(safe.as_str().ends_with('…'));
    }

    #[test]
    fn revocation_reset_discards_every_prior_state_and_denies_rendering() {
        let states = [
            UiState::Loading,
            UiState::Empty,
            UiState::Ready("authoritative payload"),
            UiState::Denied {
                reason: "old denial".to_string(),
            },
            UiState::Stale {
                age_seconds: 300,
                reason: "cached projection".to_string(),
            },
            UiState::ReadOnly {
                reason: "old policy".to_string(),
            },
            UiState::Conflict {
                reason: "old version".to_string(),
            },
            UiState::Offline {
                reason: "old network error".to_string(),
            },
            UiState::Error {
                reason: "old request error".to_string(),
            },
        ];

        for state in states {
            let reset = state.reset_after_revoke("Session access was revoked.");
            assert_eq!(reset.kind(), Some(UiStateKind::Denied));
            assert!(matches!(
                reset,
                UiState::Denied { reason } if reason == "Session access was revoked."
            ));
        }
    }

    #[test]
    fn revocation_reset_redacts_and_bounds_untrusted_reason() {
        let reset = UiState::<()>::Ready(()).reset_after_revoke(format!(
            "access_token=secret {}",
            "x".repeat(SafeUiText::MAX_BYTES + 32)
        ));

        assert!(matches!(
            reset,
            UiState::Denied { reason } if reason == "[redacted]"
        ));

        let reset =
            UiState::<()>::Ready(()).reset_after_revoke("r".repeat(SafeUiText::MAX_BYTES + 1));
        assert!(matches!(
            reset,
            UiState::Denied { reason }
                if reason.len() <= SafeUiText::MAX_BYTES + '…'.len_utf8()
                    && reason.ends_with('…')
        ));
    }
}
