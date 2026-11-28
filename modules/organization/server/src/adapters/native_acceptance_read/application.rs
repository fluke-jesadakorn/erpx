//! Owner application handler for the native Company acceptance read.
//!
//! The injected port contains no SQL, no pool and no HTTP details. This is the
//! single place where the Company-scope fence, the five setup checks and the
//! decision flags are derived, so the TypeScript application can be retired
//! without changing a payload field or a decision.

use crate::adapters::native_acceptance_read::domain::{
    AcceptanceEnvelope, AcceptanceFacts, AcceptanceQuery, AcceptanceResult, Check, EventRow,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type ReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The owner reader. Composition supplies the Postgres implementation; the
/// router and tests may inject any bounded implementation of the same port.
pub trait AcceptanceRead: Send + Sync {
    fn read<'a>(&'a self, query: &'a AcceptanceQuery) -> ReadFuture<'a, AcceptanceFacts>;
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Forbidden,
        "COMPANY_AUTHORITY_REQUIRED",
        "An active Company organization-management assignment is required.",
    )
}

/// Field-for-field successor of the retained `readChecks` projection.
pub fn checks(facts: &AcceptanceFacts) -> Vec<Check> {
    let branches = facts.branches;
    let levels = facts.levels;
    let ceo = facts.ceo;
    let incomplete = facts.incomplete;
    vec![
        Check {
            id: "company-active".into(),
            label: "Company identity is active".into(),
            ready: facts.company.as_ref().is_some_and(|row| row.status == "active"),
            owner: "Company administrator".into(),
            detail: facts
                .company
                .as_ref()
                .map(|row| row.display_name.clone())
                .unwrap_or_else(|| "Company record is unavailable.".into()),
        },
        Check {
            id: "branch-ready".into(),
            label: "At least one active branch".into(),
            ready: branches > 0,
            owner: "Company administrator".into(),
            detail: if branches > 0 {
                format!("{branches} active branch(es)")
            } else {
                "Create and approve the first branch.".into()
            },
        },
        Check {
            id: "levels-ready".into(),
            label: "Six numbered staff levels".into(),
            ready: levels == 6,
            owner: "Company HR administrator".into(),
            detail: if levels == 6 {
                "Levels 1–6 are installed.".into()
            } else {
                "Install all six numbered levels.".into()
            },
        },
        Check {
            id: "ceo-ready".into(),
            label: "Active CEO position".into(),
            ready: ceo == 1,
            owner: "People administrator".into(),
            detail: if ceo == 1 {
                "One active CEO position is configured.".into()
            } else {
                "Create exactly one active CEO position.".into()
            },
        },
        Check {
            id: "staff-ready".into(),
            label: "Active staff have an organization placement".into(),
            ready: incomplete == 0,
            owner: "People manager".into(),
            detail: if incomplete == 0 {
                "All active staff have a level and position.".into()
            } else {
                format!("{incomplete} active staff record(s) need a level or position.")
            },
        },
    ]
}

/// Validate once at the application boundary, then execute the owner read and
/// derive the complete response. A failed or malformed native read propagates
/// as-is; there is no TypeScript fallback and no partial overlay.
pub async fn handle<R: AcceptanceRead + ?Sized>(
    reader: &R,
    query: &AcceptanceQuery,
) -> Result<AcceptanceEnvelope, BusinessError> {
    query.validate()?;
    let facts = reader.read(query).await?;
    facts.validate()?;
    if !facts.authority {
        return Err(forbidden());
    }
    let checks = checks(&facts);
    let blockers = checks
        .iter()
        .filter(|check| !check.ready)
        .map(|check| check.detail.clone())
        .collect();
    let independent = facts
        .current
        .as_ref()
        .is_some_and(|row| !query.same_person(&row.requested_by));
    let has_permission = query.permissions.iter().any(|value| value == "organization.manage");
    let can_submit = checks.iter().all(|check| check.ready)
        && facts.current.as_ref().is_none_or(|row| {
            matches!(row.status.as_str(), "draft" | "rejected" | "revoked")
        });
    let can_review = facts.current.as_ref().is_some_and(|row| {
        row.status == "submitted" && independent && has_permission
    });
    let can_revoke = facts.current.as_ref().is_some_and(|row| {
        row.status == "accepted" && query.same_person(&row.requested_by)
    });
    let status = facts
        .current
        .as_ref()
        .map(|row| row.status.clone())
        .unwrap_or_else(|| "draft".into());
    let version = facts.current.as_ref().map(|row| row.version).unwrap_or(0);
    let requested_by = facts.current.as_ref().map(|row| row.requested_by.clone());
    let reviewed_by = facts.current.as_ref().and_then(|row| row.reviewed_by.clone());
    let reason = facts.current.as_ref().map(|row| row.reason.clone());
    let review_reason = facts.current.as_ref().and_then(|row| row.review_reason.clone());
    let updated_at = facts.current.as_ref().map(|row| row.updated_at);
    Ok(AcceptanceEnvelope {
        data: AcceptanceResult {
            company_id: query.company.clone(),
            status,
            version,
            checks,
            blockers,
            can_submit,
            can_review,
            can_revoke,
            requested_by,
            reviewed_by,
            reason,
            review_reason,
            updated_at,
            events: facts.events,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_acceptance_read::domain::{AcceptanceRow, CompanyRow};
    use erpx_platform_shared_server::public::FailureKind;

    struct Stub {
        facts: Result<AcceptanceFacts, BusinessError>,
    }

    impl AcceptanceRead for Stub {
        fn read<'a>(&'a self, _: &'a AcceptanceQuery) -> ReadFuture<'a, AcceptanceFacts> {
            let facts = self.facts.clone();
            Box::pin(async move { facts })
        }
    }

    fn query() -> AcceptanceQuery {
        AcceptanceQuery {
            company: "acme-co".into(),
            user: "user-1".into(),
            assignment: "assignment-1".into(),
            permissions: vec!["organization.manage".into()],
            person_ids: vec![],
            scope: "holding".into(),
            branch: None,
            department: None,
        }
    }

    fn facts() -> AcceptanceFacts {
        AcceptanceFacts {
            company: Some(CompanyRow {
                id: "acme-co".into(),
                display_name: "Acme".into(),
                status: "active".into(),
            }),
            branches: 2,
            levels: 6,
            ceo: 1,
            incomplete: 0,
            current: Some(AcceptanceRow {
                company_id: "acme-co".into(),
                status: "submitted".into(),
                version: 3,
                requested_by: "requester-1".into(),
                reviewed_by: None,
                requested_at: 10,
                reviewed_at: None,
                reason: "Ready for review".into(),
                review_reason: None,
                updated_at: 12,
            }),
            events: vec![EventRow {
                id: "event-1".into(),
                action: "submitted".into(),
                actor_id: "requester-1".into(),
                version: 3,
                reason: "Ready for review".into(),
                created_at: 12,
            }],
            authority: true,
        }
    }

    #[tokio::test]
    async fn handler_returns_the_complete_payload() {
        let envelope = handle(&Stub { facts: Ok(facts()) }, &query())
            .await
            .unwrap();
        let value = serde_json::to_value(&envelope).unwrap();
        assert_eq!(value["data"]["companyId"], "acme-co");
        assert_eq!(value["data"]["status"], "submitted");
        assert_eq!(value["data"]["version"], 3);
        assert_eq!(value["data"]["checks"][0]["id"], "company-active");
        assert_eq!(value["data"]["checks"][1]["detail"], "2 active branch(es)");
        assert_eq!(value["data"]["checks"][2]["detail"], "Levels 1–6 are installed.");
        assert_eq!(value["data"]["blockers"].as_array().unwrap().len(), 0);
        assert_eq!(value["data"]["canSubmit"], false);
        assert_eq!(value["data"]["canReview"], true);
        assert_eq!(value["data"]["canRevoke"], false);
        assert_eq!(value["data"]["requestedBy"], "requester-1");
        assert_eq!(value["data"]["reason"], "Ready for review");
        assert_eq!(value["data"]["updatedAt"], 12);
        assert_eq!(value["data"]["events"][0]["actor_id"], "requester-1");
    }

    #[tokio::test]
    async fn blockers_follow_unready_checks() {
        let mut input = facts();
        input.branches = 0;
        input.incomplete = 2;
        let envelope = handle(&Stub { facts: Ok(input) }, &query()).await.unwrap();
        assert_eq!(
            envelope.data.blockers,
            vec![
                "Create and approve the first branch.".to_owned(),
                "2 active staff record(s) need a level or position.".to_owned(),
            ]
        );
        assert!(!envelope.data.can_submit);
    }

    #[tokio::test]
    async fn requester_cannot_review_own_acceptance() {
        let mut actor = query();
        actor.user = "requester-1".into();
        let envelope = handle(&Stub { facts: Ok(facts()) }, &actor).await.unwrap();
        assert!(!envelope.data.can_review);
    }

    #[tokio::test]
    async fn authority_failure_is_forbidden() {
        let mut input = facts();
        input.authority = false;
        assert_eq!(
            handle(&Stub { facts: Ok(input) }, &query()).await,
            Err(forbidden())
        );
    }

    #[tokio::test]
    async fn reader_failure_propagates() {
        let error = BusinessError::new(
            FailureKind::Unavailable,
            "ACCEPTANCE_READ_DOWN",
            "The native acceptance reader is unavailable.",
        );
        assert_eq!(
            handle(&Stub { facts: Err(error.clone()) }, &query()).await,
            Err(error)
        );
    }

    #[tokio::test]
    async fn malformed_owner_facts_are_rejected() {
        let mut input = facts();
        input.current.as_mut().unwrap().status = "unknown".into();
        assert!(handle(&Stub { facts: Ok(input) }, &query()).await.is_err());
    }

    #[tokio::test]
    async fn unauthorized_scope_is_rejected_before_reading() {
        let mut actor = query();
        actor.scope = "department".into();
        assert!(handle(&Stub { facts: Ok(facts()) }, &actor).await.is_err());
    }
}
