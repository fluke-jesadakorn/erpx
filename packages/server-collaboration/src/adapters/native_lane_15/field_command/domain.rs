//! Pure domain types for the native Collaboration field-command routing read.
//!
//! The axum owner owns both `collaboration_*` SELECTs and the boolean routing
//! calculation. This module carries no SQL: it validates the bounded request,
//! pins the response shape and owns the `field_service_order` predicate.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const FIELD_SOURCE: &str = "field_service_order";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldCommandKind {
    Relation,
    Card,
}

/// Bounded transport request. `companyId` is always supplied by the caller's
/// WorkContext; `id` selects the SQL branch, otherwise the routed source types
/// are projected directly (matching the original no-id branch).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldCommandRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    pub company_id: String,
}

impl FieldCommandRequest {
    pub fn kind(&self) -> FieldCommandKind {
        if self.entity.as_deref() == Some("relation") {
            FieldCommandKind::Relation
        } else {
            FieldCommandKind::Card
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCommandFacts {
    pub field_owned: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct RelationSource {
    pub source_type: String,
    pub target_type: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CardSource {
    pub source_type: String,
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "COLLABORATION_LANE15_FIELD_COMMAND_INVALID",
        "Use one bounded field-command routing read.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COLLABORATION_LANE15_FIELD_COMMAND_UNAVAILABLE",
        "The native field-command routing read is unavailable.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "COLLABORATION_LANE15_FIELD_COMMAND_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_host(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}

pub fn validate(request: &FieldCommandRequest) -> Result<(), BusinessError> {
    let bounded_source = |value: Option<&str>| {
        value.is_none_or(|value| !value.is_empty() && value.len() <= 64 && value.is_ascii())
    };
    let valid = valid_id(&request.company_id)
        && request.id.as_deref().is_none_or(valid_id)
        && bounded_source(request.source_type.as_deref())
        && bounded_source(request.target_type.as_deref());
    if valid {
        Ok(())
    } else {
        Err(invalid_request())
    }
}

/// The original routing predicate, now owned by Rust.
pub fn field_owned(
    kind: FieldCommandKind,
    source_type: Option<&str>,
    target_type: Option<&str>,
) -> bool {
    source_type == Some(FIELD_SOURCE)
        || (kind == FieldCommandKind::Relation && target_type == Some(FIELD_SOURCE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bounded_ids_and_source_types() {
        assert!(validate(&FieldCommandRequest {
            company_id: "c1".into(),
            id: Some("relation-1".into()),
            ..FieldCommandRequest::default()
        })
        .is_ok());
        assert_eq!(
            validate(&FieldCommandRequest {
                company_id: "bad company!".into(),
                ..FieldCommandRequest::default()
            }),
            Err(invalid_request())
        );
        assert_eq!(
            validate(&FieldCommandRequest {
                company_id: "c1".into(),
                source_type: Some("x".repeat(65)),
                ..FieldCommandRequest::default()
            }),
            Err(invalid_request())
        );
        assert!(valid_slug("acme-holding_1"));
        assert!(!valid_slug("Acme"));
        assert!(valid_host("acme.jesadakorn.com"));
        assert!(!valid_host("bad\nhost"));
    }

    #[test]
    fn owns_the_field_routing_predicate() {
        assert!(field_owned(
            FieldCommandKind::Relation,
            Some("support_ticket"),
            Some(FIELD_SOURCE)
        ));
        assert!(!field_owned(
            FieldCommandKind::Card,
            Some("support_ticket"),
            Some(FIELD_SOURCE)
        ));
        assert!(field_owned(
            FieldCommandKind::Card,
            Some(FIELD_SOURCE),
            None
        ));
        assert!(!field_owned(FieldCommandKind::Relation, None, None));
    }

    #[test]
    fn tagged_wire_shape_is_stable() {
        let facts = FieldCommandFacts { field_owned: true };
        assert_eq!(
            serde_json::to_string(&facts).expect("serialize"),
            r#"{"fieldOwned":true}"#
        );
    }
}
