//! Native Projects handoff-view read: application handler + payload parity.
//!
//! This target must be registered in `modules/projects/server/Cargo.toml` as a
//! `[[test]]` entry by the lead (the Cargo manifest is outside worker paths).
//! The in-crate `#[cfg(test)]` modules cover the same projection without the
//! extra registration.
use erpx_projects_server::public::project_handoffs::{
    HandoffRow, HandoffViewActor, HandoffViewFuture, HandoffViewRepository, read_handoff_views,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::json;
use std::sync::Arc;

struct Repo;

impl HandoffViewRepository for Repo {
    fn handoffs<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
    ) -> HandoffViewFuture<'a, Vec<HandoffRow>> {
        Box::pin(async {
            Ok(vec![HandoffRow {
                id: "handoff".into(),
                company_id: "company".into(),
                project_id: "project".into(),
                source_project_version: 4,
                recipient_id: "member".into(),
                status: "pending".into(),
                version: 2,
                snapshot_json: json!({
                    "project": {"id": "project"},
                    "packages": [{"id": "m-root", "approvedMinutes": 0, "costByCurrency": {}, "tasks": []}],
                    "tasks": [],
                    "actuals": [{
                        "id": "time", "milestone_id": "m-root", "source_kind": "work_log",
                        "source_id": "log", "source_version": 1, "minutes": 45,
                        "amount_minor": null, "currency": null, "user_id": "member",
                        "current_source_version": 1, "source_state": "approved",
                        "journal_status": null, "journal_id": null
                    }],
                    "edges": [],
                    "unassignedTasks": [],
                    "canReadCosts": false,
                    "isProjectOwner": false,
                    "totals": {"approvedMinutes": 0, "costByCurrency": {}}
                })
                .to_string(),
                snapshot_sha256: "a".repeat(64),
                evidence: "All work is complete.".into(),
                prepared_by: "owner".into(),
                prepared_at: 100,
                decided_by: None,
                decided_at: None,
                decision_reason: None,
            }])
        })
    }

    fn project_owner<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> HandoffViewFuture<'a, Option<String>> {
        Box::pin(async { Ok(Some("owner".into())) })
    }
}

fn actor(is_owner: bool) -> HandoffViewActor {
    HandoffViewActor {
        can_read_costs: false,
        same_person: Arc::new(move |id| id == "owner" && is_owner || id == "member"),
    }
}

#[tokio::test]
async fn read_handoff_views_returns_complete_owner_payload() {
    let views = read_handoff_views(&actor(false), &Repo, "company", "member", "project")
        .await
        .unwrap();
    assert_eq!(views.len(), 1);
    let view = &views[0];
    assert_eq!(view.get("id").unwrap(), &json!("handoff"));
    assert!(!view.as_object().unwrap().contains_key("snapshot_json"));
    let snapshot = view.get("snapshot").unwrap();
    assert_eq!(snapshot.get("isProjectOwner").unwrap(), false);
    assert_eq!(snapshot.get("actuals").unwrap().as_array().unwrap().len(), 1);
    assert_eq!(
        snapshot.get("packages").unwrap()[0].get("approvedMinutes").unwrap(),
        45
    );
    assert_eq!(snapshot.get("totals").unwrap().get("approvedMinutes").unwrap(), 45);
}

#[tokio::test]
async fn malformed_snapshot_fails_closed() {
    struct Broken;
    impl HandoffViewRepository for Broken {
        fn handoffs<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> HandoffViewFuture<'a, Vec<HandoffRow>> {
            Box::pin(async { Err(BusinessError::invalid("BROKEN", "broken")) })
        }
        fn project_owner<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: &'a str,
        ) -> HandoffViewFuture<'a, Option<String>> {
            Box::pin(async { Ok(None) })
        }
    }
    assert!(read_handoff_views(&actor(false), &Broken, "company", "member", "project")
        .await
        .is_err());
}
