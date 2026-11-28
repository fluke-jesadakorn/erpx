//! Pure domain types for the native Collaboration beta-facts read family.
//!
//! The axum owner owns the SQL and the projection; this module carries no SQL.
//! It pins the four bounded wire shapes, validates the request identities and
//! enforces the row caps that the owner SQL promises.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

/// The closed discriminator accepted by the native read port. No other value
/// reaches the owner query adapter.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BetaFactKind {
    #[default]
    PostDrafts,
    Reports,
    PostTask,
    RecoverPostTask,
}

impl BetaFactKind {
    /// True when the read is scoped by a Company identity. The two task-origin
    /// lookups are post-id lookups whose visibility is authorized by the
    /// caller's `readablePost` port before this read runs.
    pub fn company_scoped(self) -> bool {
        matches!(self, Self::PostDrafts | Self::Reports)
    }
}

/// Bounded transport request. Every field is optional at the wire boundary;
/// `load` requires exactly the identities the selected kind needs.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BetaFactRequest {
    pub kind: BetaFactKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_id: Option<String>,
}

/// One collaboration-owned social draft. Field order is the wire contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PostDraft {
    pub id: String,
    pub body: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub audience_kind: Option<String>,
    pub audience_id: Option<String>,
    pub participants_json: String,
    pub sources_json: String,
    pub purpose: String,
    pub version: i64,
    pub mentions_json: String,
    pub content_json: Option<String>,
    pub rich_content_json: Option<String>,
}

/// One collaboration-owned social report joined to its post state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PostReport {
    pub id: String,
    pub reason: String,
    pub post_id: String,
    pub state: String,
}

/// The single task origin projected by both task read kinds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskOrigin {
    pub task_id: String,
}

/// Tagged response. `task` is explicit `null` when no origin exists; the TS
/// client maps that back to the original `null` repository result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BetaFacts {
    PostDrafts { drafts: Vec<PostDraft> },
    Reports { reports: Vec<PostReport> },
    PostTask { task: Option<TaskOrigin> },
    RecoverPostTask { task: Option<TaskOrigin> },
}

pub const MAX_DRAFTS: usize = 20;
pub const MAX_REPORTS: usize = 100;

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "COLLABORATION_LANE_15_REQUEST_INVALID",
        "Use one bounded beta-facts read request.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COLLABORATION_LANE_15_UNAVAILABLE",
        "The native Collaboration beta-facts read is unavailable.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "COLLABORATION_LANE_15_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

/// Opaque but bounded identifier used only as a SQL bind.
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

/// Require the identities a kind needs, in original bind order. Company and
/// user ids must be valid ids; post ids must be valid ids for task lookups.
pub fn validate(request: &BetaFactRequest) -> Result<(), BusinessError> {
    let company = request.company_id.as_deref();
    let user = request.user_id.as_deref();
    let post = request.post_id.as_deref();
    let valid = match request.kind {
        BetaFactKind::PostDrafts => company.is_some_and(valid_id) && user.is_some_and(valid_id),
        BetaFactKind::Reports => company.is_some_and(valid_id),
        BetaFactKind::PostTask | BetaFactKind::RecoverPostTask => post.is_some_and(valid_id),
    };
    if valid {
        Ok(())
    } else {
        Err(invalid_request())
    }
}

/// Fail closed on any projection that exceeds the owner's promised row caps.
pub fn capped(facts: BetaFacts) -> Result<BetaFacts, BusinessError> {
    let overflow = match &facts {
        BetaFacts::PostDrafts { drafts } => drafts.len() > MAX_DRAFTS,
        BetaFacts::Reports { reports } => reports.len() > MAX_REPORTS,
        BetaFacts::PostTask { .. } | BetaFacts::RecoverPostTask { .. } => false,
    };
    if overflow {
        Err(unavailable())
    } else {
        Ok(facts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_rejects_bounded_ids() {
        assert!(valid_id("post-01"));
        assert!(valid_id("acme:co"));
        assert!(!valid_id(""));
        assert!(!valid_id("bad/id?"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_slug("acme-holding_1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_host("bad\nhost"));
        assert!(valid_host("acme.jesadakorn.com"));
    }

    #[test]
    fn validation_requires_the_identities_each_kind_needs() {
        assert!(validate(&BetaFactRequest {
            kind: BetaFactKind::PostDrafts,
            company_id: Some("c1".into()),
            user_id: Some("u1".into()),
            ..BetaFactRequest::default()
        })
        .is_ok());
        assert_eq!(
            validate(&BetaFactRequest {
                kind: BetaFactKind::PostDrafts,
                company_id: Some("c1".into()),
                ..BetaFactRequest::default()
            }),
            Err(invalid_request())
        );
        assert_eq!(
            validate(&BetaFactRequest {
                kind: BetaFactKind::PostTask,
                post_id: Some("p1".into()),
                ..BetaFactRequest::default()
            }),
            Ok(())
        );
        assert_eq!(
            validate(&BetaFactRequest {
                kind: BetaFactKind::Reports,
                ..BetaFactRequest::default()
            }),
            Err(invalid_request())
        );
    }

    #[test]
    fn caps_the_promised_row_counts() {
        let draft = PostDraft {
            id: "d".into(),
            body: String::new(),
            branch_id: None,
            department_id: None,
            audience_kind: None,
            audience_id: None,
            participants_json: "[]".into(),
            sources_json: "[]".into(),
            purpose: "update".into(),
            version: 1,
            mentions_json: "[]".into(),
            content_json: None,
            rich_content_json: None,
        };
        let within = BetaFacts::PostDrafts {
            drafts: vec![draft.clone(); MAX_DRAFTS],
        };
        assert_eq!(capped(within.clone()), Ok(within));
        let over = BetaFacts::PostDrafts {
            drafts: vec![draft; MAX_DRAFTS + 1],
        };
        assert_eq!(capped(over), Err(unavailable()));
    }

    #[test]
    fn tagged_wire_shape_is_stable() {
        let facts = BetaFacts::PostTask {
            task: Some(TaskOrigin {
                task_id: "t1".into(),
            }),
        };
        assert_eq!(
            serde_json::to_string(&facts).expect("serialize"),
            r#"{"kind":"post_task","task":{"task_id":"t1"}}"#
        );
        let none = BetaFacts::RecoverPostTask { task: None };
        assert_eq!(
            serde_json::to_string(&none).expect("serialize"),
            r#"{"kind":"recover_post_task","task":null}"#
        );
    }
}
