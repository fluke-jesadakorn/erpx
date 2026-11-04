//! Native Billing workspace authority, isolation and source validation.
use erpx_server_billing::public::workspace::{
    BillingAccountView, BillingActor, BillingAuditView, BillingCostView, BillingEnrollmentView,
    BillingFuture, BillingInvoiceView, BillingJobView, BillingLegacyView, BillingPeriodView,
    BillingPlanView, BillingWorkspaceRepository, WorkspaceRows, billing_workspace,
};
use erpx_server_shared::public::{AssignmentId, BusinessError, HoldingId, TimestampMillis, UserId};
use sqlx::test_block_on;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn target() -> HoldingId {
    HoldingId::new("h001").unwrap()
}

fn actor(
    platform: bool,
    role: &str,
    holding: Option<&str>,
    expires_at: Option<i64>,
) -> BillingActor {
    BillingActor {
        user_id: UserId::new(if platform { "admin" } else { "owner" }).unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
        holding_id: holding.map(|value| HoldingId::new(value).unwrap()),
        platform,
        role: role.into(),
        expires_at,
    }
}

fn owner() -> BillingActor {
    actor(false, "holding_owner", Some("h001"), None)
}

fn platform(role: &str) -> BillingActor {
    actor(true, role, None, None)
}

fn account() -> BillingAccountView {
    BillingAccountView {
        holding_id: "h001".into(),
        version: 4,
        customer_id: Some("customer-1".into()),
        payment_mode: "manual".into(),
        consent_at: None,
        setup_state: Some("ready".into()),
        card_json: None,
    }
}

fn enrollment() -> BillingEnrollmentView {
    BillingEnrollmentView {
        id: "enrollment-1".into(),
        plan_id: "standard".into(),
        started_at: 1_760_000_000_000,
        ended_at: None,
        pending_plan: Some("priority".into()),
        pending_end: false,
        effective_at: Some(1_780_000_000_000),
    }
}

fn plans() -> Vec<BillingPlanView> {
    vec![
        BillingPlanView {
            id: "standard".into(),
            revision: 2,
            name: "Standard Support".into(),
            support_satang: 60_000,
            effective_month: "2026-10".into(),
        },
        BillingPlanView {
            id: "standard".into(),
            revision: 1,
            name: "Standard Support".into(),
            support_satang: 50_000,
            effective_month: "2026-01".into(),
        },
    ]
}

fn periods() -> Vec<BillingPeriodView> {
    vec![
        BillingPeriodView {
            id: "period-11".into(),
            month: "2026-11".into(),
            start_at: 1_780_000_000_000,
            end_at: 1_782_000_000_000,
            plan_id: "standard".into(),
            plan_revision: 1,
            support_satang: 50_000,
            costs_complete: false,
            closed_at: None,
        },
        BillingPeriodView {
            id: "period-10".into(),
            month: "2026-10".into(),
            start_at: 1_778_000_000_000,
            end_at: 1_780_000_000_000,
            plan_id: "standard".into(),
            plan_revision: 1,
            support_satang: 50_000,
            costs_complete: true,
            closed_at: Some(1_781_000_000_000),
        },
    ]
}

fn costs() -> Vec<BillingCostView> {
    vec![BillingCostView {
        id: "cost-1".into(),
        period_id: "period-10".into(),
        category: "hosting".into(),
        amount_satang: 12_500,
        service_start: 1_778_000_000_000,
        service_end: 1_780_000_000_000,
        note: "Provider hosting".into(),
        simulated: false,
        evidence_id: Some("evidence-1".into()),
        invoice_id: Some("invoice-1".into()),
        original_invoice_id: None,
    }]
}

fn invoices() -> Vec<BillingInvoiceView> {
    vec![BillingInvoiceView {
        id: "invoice-1".into(),
        period_id: "period-10".into(),
        kind: "main".into(),
        status: "open".into(),
        total_satang: 62_500,
        lines_json: "[{\"description\":\"Hosting\",\"amount\":12500,\"start\":1778000000000,\"end\":1780000000000}]".into(),
        stripe_id: Some("in_test".into()),
        hosted_url: None,
        pdf_url: None,
        due_at: None,
        payment_state: Some("open".into()),
        created_at: 1_781_000_000_000,
        original_invoice_id: None,
    }]
}

fn valid_rows() -> WorkspaceRows {
    WorkspaceRows {
        account: Some(account()),
        enrollment: Some(enrollment()),
        plans: plans(),
        periods: periods(),
        costs: costs(),
        invoices: invoices(),
        audit: vec![BillingAuditView {
            id: "audit-1".into(),
            actor: "admin".into(),
            action: "cost".into(),
            reason: "Reviewed actual cost".into(),
            created_at: 1_781_000_000_000,
        }],
        jobs: vec![BillingJobView {
            id: "job-1".into(),
            state: "pending".into(),
            last_error: None,
        }],
        legacy: Some(BillingLegacyView {
            status: "active".into(),
            subscription_id: Some("subscription-1".into()),
        }),
    }
}

struct Probe {
    rows: WorkspaceRows,
    calls: usize,
}

impl BillingWorkspaceRepository for Probe {
    type Error = BusinessError;

    fn load<'a>(&'a mut self, _: &'a HoldingId) -> BillingFuture<'a, WorkspaceRows, Self::Error> {
        self.calls += 1;
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }
}

fn result(
    actor: &BillingActor,
    rows: WorkspaceRows,
) -> Result<erpx_server_billing::public::workspace::BillingWorkspace, BusinessError> {
    let mut probe = Probe { rows, calls: 0 };
    sqlx::test_block_on(async { billing_workspace(actor, &target(), &mut probe, NOW).await })
}

#[test]
fn requires_a_platform_read_role_or_the_exact_owner_holding() {
    test_block_on(async {
        let mut probe = Probe {
            rows: valid_rows(),
            calls: 0,
        };
        assert!(
            billing_workspace(&owner(), &target(), &mut probe, NOW)
                .await
                .is_ok()
        );
        assert_eq!(probe.calls, 1);

        for actor in [
            actor(false, "holding_owner", Some("h002"), None),
            actor(false, "holding_manager", Some("h001"), None),
            platform("platform_other"),
            actor(true, "platform_auditor", None, Some(NOW.get())),
        ] {
            let mut probe = Probe {
                rows: valid_rows(),
                calls: 0,
            };
            let error = billing_workspace(&actor, &target(), &mut probe, NOW)
                .await
                .unwrap_err();
            assert!(
                matches!(error.code, "FORBIDDEN" | "WORK_CONTEXT_CHANGED"),
                "{error:?}"
            );
            assert_eq!(probe.calls, 0);
        }
        for role in ["platform_admin", "platform_auditor"] {
            let mut probe = Probe {
                rows: valid_rows(),
                calls: 0,
            };
            assert!(
                billing_workspace(&platform(role), &target(), &mut probe, NOW)
                    .await
                    .is_ok()
            );
        }
        let erpx = HoldingId::new("h003").unwrap();
        let mut probe = Probe {
            rows: valid_rows(),
            calls: 0,
        };
        assert_eq!(
            billing_workspace(&platform("platform_admin"), &erpx, &mut probe, NOW)
                .await
                .unwrap_err()
                .code,
            "ERPX_NOT_CUSTOMER"
        );
        assert_eq!(probe.calls, 0);
    });
}

#[test]
fn rejects_foreign_or_malformed_rows_instead_of_filtering_them() {
    let mut rows = valid_rows();
    rows.account.as_mut().unwrap().holding_id = "h002".into();
    assert_eq!(
        result(&owner(), rows).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );

    let mut rows = valid_rows();
    let duplicate_plan = rows.plans[1].clone();
    rows.plans.push(duplicate_plan);
    assert_eq!(
        result(&owner(), rows).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );

    let mut rows = valid_rows();
    rows.periods[0].month = "2026-13".into();
    assert_eq!(
        result(&owner(), rows).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );

    let mut rows = valid_rows();
    rows.costs[0].period_id = "other-period".into();
    assert_eq!(
        result(&owner(), rows).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );

    let mut rows = valid_rows();
    rows.invoices[0].lines_json = "{}".into();
    assert_eq!(
        result(&owner(), rows).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );

    let mut rows = valid_rows();
    let duplicate_job = rows.jobs[0].clone();
    rows.jobs.push(duplicate_job);
    assert_eq!(
        result(&owner(), rows).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );

    let mut rows = valid_rows();
    rows.account.as_mut().unwrap().version = -1;
    assert_eq!(
        result(&owner(), rows).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );

    let mut rows = valid_rows();
    rows.periods[0].plan_revision = 99;
    assert_eq!(
        result(&owner(), rows).unwrap_err().code,
        "BILLING_SOURCE_INVALID"
    );
}

#[test]
fn returns_the_exact_bounded_current_holding_workspace() {
    let workspace = result(&owner(), valid_rows()).unwrap();
    assert!(workspace.sandbox);
    assert_eq!(workspace.now, NOW.get());
    assert_eq!(workspace.account.as_ref().unwrap().version, 4);
    assert_eq!(
        workspace
            .enrollment
            .as_ref()
            .unwrap()
            .pending_plan
            .as_deref(),
        Some("priority")
    );
    assert_eq!(workspace.plans.len(), 2);
    assert_eq!(workspace.periods[0].month, "2026-11");
    assert_eq!(workspace.costs[0].amount_satang, 12_500);
    assert_eq!(workspace.invoices[0].total_satang, 62_500);
    assert_eq!(workspace.audit[0].id, "audit-1");
    assert_eq!(workspace.jobs[0].state, "pending");
    assert_eq!(workspace.legacy.as_ref().unwrap().status, "active");
}
