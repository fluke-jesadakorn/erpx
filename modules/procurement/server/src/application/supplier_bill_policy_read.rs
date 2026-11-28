//! Native Procurement supplier-invoice policy read use case.
//!
//! The owner enforces `finance.read` and work-context currency before any row
//! read. Company Finance authority and the commercial purchased-package
//! overlay remain composition facts; the owner only intersects the IAM
//! `finance.approve` fact with the supplied Company authority to decide whether
//! the immutable policy events are revealed, exactly as the retained
//! `canManage && policyEvents()` path did.
use crate::domain::supplier_bill_policy_read::{
    InvoicePolicyView, PolicyEventRow, PolicyHeadRow, PolicyPurchase, PolicyReadActor,
    project_policy,
};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, TimestampMillis};
use std::{future::Future, pin::Pin};

pub type PolicyReadFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait PolicyReadRepository: Send {
    type Error: From<BusinessError> + Send;
    /// Current Company matching policy, if any.
    fn head<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> PolicyReadFuture<'a, Option<PolicyHeadRow>, Self::Error>;
    /// Immutable policy events for the same Company, newest version first.
    fn events<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> PolicyReadFuture<'a, Vec<PolicyEventRow>, Self::Error>;
}

pub async fn invoice_policy<R: PolicyReadRepository>(
    actor: &PolicyReadActor,
    repository: &mut R,
    company_authority: bool,
    purchase: Option<PolicyPurchase>,
    now: TimestampMillis,
) -> Result<InvoicePolicyView, R::Error> {
    actor.require_read(now)?;
    let can_manage = actor.can_approve() && company_authority;
    let head = repository.head(&actor.company_id).await?;
    let rows = if can_manage {
        repository.events(&actor.company_id).await?
    } else {
        Vec::new()
    };
    Ok(project_policy(
        head,
        rows,
        can_manage,
        &actor.company_id,
        purchase,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_shared_server::public::FailureKind;
    use serde_json::json;

    const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

    fn company() -> CompanyId {
        CompanyId::new("company").unwrap()
    }

    fn actor(permissions: &[&str]) -> PolicyReadActor {
        PolicyReadActor {
            user_id: "owner".into(),
            company_id: company(),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            expires_at: NOW.get() + 60_000,
        }
    }

    fn head() -> PolicyHeadRow {
        PolicyHeadRow {
            matching_mode: "required".into(),
            version: 2,
            updated_by: "approver".into(),
            updated_at: 50,
            updated_by_name: Some("Approver Name".into()),
        }
    }

    fn event() -> PolicyEventRow {
        PolicyEventRow {
            id: "policy-event-2".into(),
            company_id: "company".into(),
            version: 2,
            previous_mode: "optional".into(),
            matching_mode: "required".into(),
            actor_id: "approver".into(),
            reason: "Require supplier matches".into(),
            created_at: 50,
            actor_name: Some("Approver Name".into()),
        }
    }

    struct Repo {
        head: Option<PolicyHeadRow>,
        events: Vec<PolicyEventRow>,
        head_calls: usize,
        event_calls: usize,
    }

    impl Repo {
        fn valid() -> Self {
            Self {
                head: Some(head()),
                events: vec![event()],
                head_calls: 0,
                event_calls: 0,
            }
        }
    }

    impl PolicyReadRepository for Repo {
        type Error = BusinessError;

        fn head<'a>(
            &'a mut self,
            _: &'a CompanyId,
        ) -> PolicyReadFuture<'a, Option<PolicyHeadRow>, Self::Error> {
            self.head_calls += 1;
            Box::pin(async move { Ok(self.head.clone()) })
        }

        fn events<'a>(
            &'a mut self,
            _: &'a CompanyId,
        ) -> PolicyReadFuture<'a, Vec<PolicyEventRow>, Self::Error> {
            self.event_calls += 1;
            Box::pin(async move { Ok(self.events.clone()) })
        }
    }

    fn purchase() -> PolicyPurchase {
        PolicyPurchase {
            new_work: true,
            settle_existing: true,
            history: true,
            revision: "sha256:7".into(),
        }
    }

    #[tokio::test]
    async fn policy_payload_matches_the_retained_data_keys() {
        let mut repo = Repo::valid();
        let view = invoice_policy(
            &actor(&["finance.read", "finance.approve"]),
            &mut repo,
            true,
            Some(purchase()),
            NOW,
        )
        .await
        .unwrap();
        let value = serde_json::to_value(&view).unwrap();
        let mut keys = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "canManage",
                "companyId",
                "events",
                "matchingMode",
                "purchase",
                "updatedAt",
                "updatedByName",
                "version",
            ]
        );
        assert_eq!(value["matchingMode"], json!("required"));
        assert_eq!(value["version"], json!(2));
        assert_eq!(value["updatedByName"], json!("Approver Name"));
        assert_eq!(value["updatedAt"], json!(50));
        assert_eq!(value["canManage"], json!(true));
        assert_eq!(value["companyId"], json!("company"));
        assert_eq!(value["purchase"]["settleExisting"], json!(true));
        let mut event_keys = value["events"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        event_keys.sort_unstable();
        assert_eq!(
            event_keys,
            vec![
                "actor_id",
                "actor_name",
                "company_id",
                "created_at",
                "id",
                "matching_mode",
                "previous_mode",
                "reason",
                "version",
            ]
        );
        assert_eq!(repo.head_calls, 1);
        assert_eq!(repo.event_calls, 1);
    }

    #[tokio::test]
    async fn missing_policy_uses_the_retained_defaults() {
        let mut repo = Repo::valid();
        repo.head = None;
        let view = invoice_policy(&actor(&["finance.read"]), &mut repo, false, None, NOW)
            .await
            .unwrap();
        assert_eq!(view.matching_mode, "optional");
        assert_eq!(view.version, 0);
        assert!(view.updated_by_name.is_none());
        assert!(view.updated_at.is_none());
        assert!(view.events.is_empty());
        let value = serde_json::to_value(&view).unwrap();
        assert!(value.get("purchase").is_none());
        assert_eq!(repo.event_calls, 0);
    }

    #[tokio::test]
    async fn can_manage_intersects_owner_iam_and_company_authority() {
        let mut repo = Repo::valid();
        let view = invoice_policy(&actor(&["finance.read"]), &mut repo, true, None, NOW)
            .await
            .unwrap();
        assert!(!view.can_manage);
        assert!(view.events.is_empty());
        assert_eq!(repo.event_calls, 0);

        let mut repo = Repo::valid();
        let view = invoice_policy(
            &actor(&["finance.read", "finance.approve"]),
            &mut repo,
            false,
            None,
            NOW,
        )
        .await
        .unwrap();
        assert!(!view.can_manage);
        assert!(view.events.is_empty());
        assert_eq!(repo.event_calls, 0);
    }

    #[tokio::test]
    async fn permission_guard_precedes_every_repository_read() {
        let mut repo = Repo::valid();
        let error = invoice_policy(&actor(&[]), &mut repo, true, None, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::Forbidden);
        assert_eq!(error.code, "PERMISSION_REQUIRED");
        assert_eq!(repo.head_calls, 0);
        assert_eq!(repo.event_calls, 0);

        let mut repo = Repo::valid();
        let expired = PolicyReadActor {
            expires_at: NOW.get(),
            ..actor(&["finance.read"])
        };
        let error = invoice_policy(&expired, &mut repo, false, None, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
        assert_eq!(repo.head_calls, 0);
    }

    #[tokio::test]
    async fn invalid_owner_rows_fail_closed() {
        let mut repo = Repo::valid();
        repo.head.as_mut().unwrap().matching_mode = "unknown".into();
        let error = invoice_policy(&actor(&["finance.read"]), &mut repo, false, None, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "SUPPLIER_BILL_POLICY_INVALID");

        let mut repo = Repo::valid();
        repo.events[0].company_id = "foreign".into();
        let error = invoice_policy(
            &actor(&["finance.read", "finance.approve"]),
            &mut repo,
            true,
            None,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "SUPPLIER_BILL_POLICY_INVALID");
    }
}
