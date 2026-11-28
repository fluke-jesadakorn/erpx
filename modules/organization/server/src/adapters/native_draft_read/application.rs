//! Owner application handler for the native Organization draft read.
//!
//! The port is injected and contains no SQL, no pool and no HTTP details. This
//! is the single place where the request fence and the owner payload invariants
//! are enforced before a reply can be released.

use crate::adapters::native_draft_read::domain::{
    DraftReadEnvelope, DraftReadQuery, DraftReadResult,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type ReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The Organization owner reader. Composition supplies the Pg implementation;
/// the router and tests may inject any bounded implementation of the same port.
pub trait DraftRead: Send + Sync {
    fn read<'a>(&'a self, query: &'a DraftReadQuery) -> ReadFuture<'a, DraftReadResult>;
}

/// Validate once at the application boundary, then execute the owner read and
/// enforce its response shape. A failed or malformed native read propagates
/// as-is; there is no TypeScript fallback and no partial overlay.
pub async fn handle<R: DraftRead + ?Sized>(
    reader: &R,
    query: &DraftReadQuery,
) -> Result<DraftReadEnvelope, BusinessError> {
    query.validate()?;
    let facts = reader.read(query).await?;
    facts.validate()?;
    // Parity with the retired `getOrganizationDraft`: an explicit id that does
    // not resolve is a 404, not an empty draft.
    if query.draft_id.is_some() && facts.draft.is_none() {
        return Err(BusinessError::new(
            erpx_platform_shared_server::public::FailureKind::NotFound,
            "DRAFT_NOT_FOUND",
            "Draft not found.",
        ));
    }
    Ok(DraftReadEnvelope { data: facts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_draft_read::domain::{
        DraftRow, RecentRow, RequestRow, REQUIRED_PERMISSIONS,
    };
    use erpx_platform_shared_server::public::FailureKind;

    #[derive(Clone)]
    struct Stub {
        result: Result<DraftReadResult, BusinessError>,
    }

    impl DraftRead for Stub {
        fn read<'a>(&'a self, _: &'a DraftReadQuery) -> ReadFuture<'a, DraftReadResult> {
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    fn query() -> DraftReadQuery {
        DraftReadQuery {
            company: "acme-co".into(),
            user: "user-1".into(),
            permissions: REQUIRED_PERMISSIONS.iter().map(|value| value.to_string()).collect(),
            scope: "holding".into(),
            branch: None,
            department: None,
            draft_id: None,
        }
    }

    fn facts() -> DraftReadResult {
        DraftReadResult {
            draft: Some(DraftRow {
                id: "draft-1".into(),
                company_id: "acme-co".into(),
                owner_id: "user-1".into(),
                state: "draft".into(),
                version: 1,
                definition: serde_json::json!({"items": [], "reason": "Move", "effectiveAt": 1, "timezone": "Asia/Bangkok", "managers": {}, "leave": {}}),
                created_at: 1,
                updated_at: 2,
                base_snapshot: serde_json::json!({}),
            }),
            requests: vec![RequestRow {
                id: "request-1".into(),
                item_keys: serde_json::json!(["person:eng-1"]),
                state: "scheduled".into(),
                last_error: None,
                change_id: None,
            }],
            recent: vec![RecentRow {
                id: "draft-0".into(),
                updated_at: 1,
                count: 1,
            }],
        }
    }

    #[tokio::test]
    async fn handler_returns_the_complete_validated_payload() {
        let envelope = handle(&Stub { result: Ok(facts()) }, &query())
            .await
            .unwrap();
        assert_eq!(envelope.data.draft.unwrap().owner_id, "user-1");
        assert_eq!(envelope.data.requests[0].item_keys[0], "person:eng-1");
        assert_eq!(envelope.data.recent[0].count, 1);
    }

    #[tokio::test]
    async fn handler_propagates_reader_failure() {
        let error = BusinessError::new(
            FailureKind::Unavailable,
            "DRAFT_READ_DOWN",
            "The native draft reader is unavailable.",
        );
        assert_eq!(
            handle(&Stub { result: Err(error.clone()) }, &query())
                .await,
            Err(error)
        );
    }

    #[tokio::test]
    async fn handler_rejects_malformed_owner_facts() {
        let mut malformed = facts();
        malformed.draft.as_mut().unwrap().definition = serde_json::json!([]);
        assert!(
            handle(&Stub { result: Ok(malformed) }, &query())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn handler_returns_not_found_for_an_unresolvable_id() {
        let mut by_id = query();
        by_id.draft_id = Some("missing".into());
        let result = handle(&Stub { result: Ok(DraftReadResult::default()) }, &by_id).await;
        assert_eq!(
            result,
            Err(BusinessError::new(
                erpx_platform_shared_server::public::FailureKind::NotFound,
                "DRAFT_NOT_FOUND",
                "Draft not found.",
            ))
        );
    }

    #[tokio::test]
    async fn handler_rejects_unauthorized_request_before_reading() {
        let mut unauthorized = query();
        unauthorized.permissions = vec![];
        let result = handle(&Stub { result: Ok(facts()) }, &unauthorized).await;
        assert_eq!(
            result,
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "COMPANY_DRAFT_AUTHORITY_REQUIRED",
                "Open the Company workspace with Company HR authority to read drafts.",
            ))
        );
    }
}
