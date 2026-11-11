//! Native Sales confirmed sales-order cancellation unit rules.
use erpx_server_sales::public::sales_order_cancellation::{
    CancellationAuditEntry, CancellationOutboxEntry, CancellationReceiptEntry,
    CancellationReceiptRecord, SalesCancellationActor, SalesCancellationCommand,
    SalesCancellationFuture, SalesCancellationPlan, SalesCancellationReceipt,
    SalesCancellationRepository, SalesCancellationRow, authorize_confirmed_cancellation,
    cancel_confirmed_sales_order, validate_receipt_shape,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use std::collections::VecDeque;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn actor(permissions: &[&str]) -> SalesCancellationActor {
    SalesCancellationActor {
        user_id: "reviewer".into(),
        person_user_ids: vec![],
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        assignment_id: "assignment-reviewer".into(),
        role: Some("sales_manager".into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["sales".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn order() -> SalesCancellationRow {
    SalesCancellationRow {
        id: "sales-confirmed".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        kind: "sales".into(),
        state: "confirmed".into(),
        version: 3,
        created_by: "creator".into(),
        approved_by: Some("reviewer".into()),
        subtotal_minor: 10_000,
        tax_minor: 700,
        updated_at: 100,
    }
}

fn command() -> SalesCancellationCommand {
    SalesCancellationCommand {
        key: "sales-cancel-0001".into(),
        order_id: "sales-confirmed".into(),
        expected_version: 3,
    }
}

#[derive(Default)]
struct Recorded {
    transition: usize,
    release: usize,
    receipt: usize,
    audit: usize,
    outbox: usize,
    readback: usize,
}

#[derive(Default)]
struct Fake {
    order: Option<SalesCancellationRow>,
    fulfillments: i64,
    authority_write: bool,
    authority_approve: bool,
    receipt: Option<CancellationReceiptRecord>,
    hash: String,
    events: Recorded,
    released_orders: Vec<String>,
    identities: VecDeque<String>,
}

impl SalesCancellationRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a SalesCancellationActor,
        _: bool,
        approve: bool,
        _: TimestampMillis,
    ) -> SalesCancellationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            Ok(if approve {
                self.authority_approve
            } else {
                self.authority_write
            })
        })
    }

    fn lock_command<'a>(&'a mut self, _: &'a str) -> SalesCancellationFuture<'a, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }

    fn digest<'a>(
        &'a mut self,
        value: &'a str,
    ) -> SalesCancellationFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.identities.push_back(value.to_owned());
            Ok(self.hash.clone())
        })
    }

    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> SalesCancellationFuture<'a, Option<CancellationReceiptRecord>, Self::Error> {
        Box::pin(async move { Ok(self.receipt.clone()) })
    }

    fn lock_order<'a>(
        &'a mut self,
        _: &'a SalesCancellationActor,
        _: &'a str,
    ) -> SalesCancellationFuture<'a, Option<SalesCancellationRow>, Self::Error> {
        Box::pin(async move { Ok(self.order.clone()) })
    }

    fn count_fulfillments<'a>(
        &'a mut self,
        _: &'a str,
    ) -> SalesCancellationFuture<'a, i64, Self::Error> {
        Box::pin(async move { Ok(self.fulfillments) })
    }

    fn release_reservations<'a>(
        &'a mut self,
        _: &'a CompanyId,
        order_id: &'a str,
    ) -> SalesCancellationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.events.release += 1;
            self.released_orders.push(order_id.to_owned());
            Ok(())
        })
    }

    fn transition<'a>(
        &'a mut self,
        plan: &'a SalesCancellationPlan,
    ) -> SalesCancellationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.events.transition += 1;
            let order = self.order.as_mut().ok_or_else(|| {
                BusinessError::new(
                    erpx_server_shared::public::FailureKind::NotFound,
                    "SALES_ORDER_NOT_FOUND",
                    "Sales order not found.",
                )
            })?;
            order.state = plan.next_state.clone();
            order.version = plan.expected_version + 1;
            Ok(true)
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a CancellationReceiptEntry<'a>,
        _: TimestampMillis,
    ) -> SalesCancellationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.events.receipt += 1;
            self.receipt = Some(CancellationReceiptRecord {
                request_hash: entry.request_hash.into(),
                result_json: entry.result_json.into(),
                record_id: Some(entry.record_id.into()),
            });
            Ok(true)
        })
    }

    fn audit<'a>(
        &'a mut self,
        _: &'a SalesCancellationActor,
        _: &'a CancellationAuditEntry<'a>,
        _: TimestampMillis,
    ) -> SalesCancellationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.events.audit += 1;
            Ok(())
        })
    }

    fn outbox<'a>(
        &'a mut self,
        _: &'a CancellationOutboxEntry<'a>,
        _: TimestampMillis,
    ) -> SalesCancellationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.events.outbox += 1;
            Ok(())
        })
    }

    fn readback_order<'a>(
        &'a mut self,
        _: &'a SalesCancellationActor,
        _: &'a str,
    ) -> SalesCancellationFuture<'a, Option<SalesCancellationRow>, Self::Error> {
        Box::pin(async move {
            self.events.readback += 1;
            Ok(self.order.clone())
        })
    }
}

fn ready() -> Fake {
    Fake {
        order: Some(order()),
        fulfillments: 0,
        authority_write: true,
        authority_approve: true,
        receipt: None,
        hash: HASH.into(),
        ..Fake::default()
    }
}

#[test]
fn actor_requires_sales_module_permission_and_live_context() {
    let reviewer = actor(&["commerce.sales", "commerce.approve"]);
    reviewer.require_write(NOW).unwrap();
    reviewer.require_approve().unwrap();

    let mut no_module = actor(&["commerce.sales", "commerce.approve"]);
    no_module.modules.clear();
    assert_eq!(
        no_module.require_write(NOW).unwrap_err().code,
        "COMMERCE_MODULE_FORBIDDEN"
    );

    let no_sales = actor(&["commerce.approve"]);
    assert_eq!(
        no_sales.require_write(NOW).unwrap_err().code,
        "COMMERCE_PERMISSION_REQUIRED"
    );
    let no_approve = actor(&["commerce.sales"]);
    assert_eq!(
        no_approve.require_approve().unwrap_err().code,
        "COMMERCE_APPROVAL_REQUIRED"
    );

    let mut expired = actor(&["commerce.sales", "commerce.approve"]);
    expired.expires_at = NOW.get();
    assert_eq!(
        expired.require_write(NOW).unwrap_err().code,
        "WORK_CONTEXT_CHANGED"
    );
}

#[test]
fn command_is_one_exact_version_cancel_without_reason() {
    command().validate().unwrap();
    for expected in [0, -1, 9_007_199_254_740_992] {
        assert_eq!(
            SalesCancellationCommand {
                expected_version: expected,
                ..command()
            }
            .validate()
            .unwrap_err()
            .code,
            "SALES_CANCELLATION_REQUEST_INVALID"
        );
    }
    assert_eq!(
        SalesCancellationCommand {
            key: String::new(),
            ..command()
        }
        .validate()
        .unwrap_err()
        .code,
        "SALES_CANCELLATION_REQUEST_INVALID"
    );
    assert!(
        command()
            .identity(&actor(&["commerce.sales"]))
            .contains("\"action\":\"cancel\"")
    );
}

#[test]
fn confirmed_sale_plan_requires_an_independent_approver_and_no_delivery() {
    let reviewer = actor(&["commerce.sales", "commerce.approve"]);
    let plan = authorize_confirmed_cancellation(&reviewer, &command(), &order(), 0, NOW).unwrap();
    assert_eq!(plan.next_state, "cancelled");
    assert_eq!(
        plan.receipt,
        SalesCancellationReceipt {
            id: "sales-confirmed".into(),
            state: "cancelled".into(),
            version: 4,
            sales_id: None,
        }
    );
    assert_eq!(
        plan.receipt.to_wire(),
        serde_json::json!({
            "id": "sales-confirmed",
            "state": "cancelled",
            "version": 4,
            "salesId": null,
        })
    );

    let creator = SalesCancellationActor {
        user_id: "creator".into(),
        ..reviewer.clone()
    };
    assert_eq!(
        authorize_confirmed_cancellation(&creator, &command(), &order(), 0, NOW)
            .unwrap_err()
            .code,
        "INDEPENDENT_CANCELLER_REQUIRED"
    );

    let alias = SalesCancellationActor {
        user_id: "different".into(),
        person_user_ids: vec!["creator".into()],
        ..reviewer.clone()
    };
    assert_eq!(
        authorize_confirmed_cancellation(&alias, &command(), &order(), 0, NOW)
            .unwrap_err()
            .code,
        "INDEPENDENT_CANCELLER_REQUIRED"
    );

    let no_approve = actor(&["commerce.sales"]);
    assert_eq!(
        authorize_confirmed_cancellation(&no_approve, &command(), &order(), 0, NOW)
            .unwrap_err()
            .code,
        "INDEPENDENT_CANCELLER_REQUIRED"
    );

    assert_eq!(
        authorize_confirmed_cancellation(&reviewer, &command(), &order(), 1, NOW)
            .unwrap_err()
            .code,
        "ORDER_REVERSAL_REQUIRED"
    );
}

#[test]
fn scope_state_and_version_are_narrowed_to_a_confirmed_sale() {
    let reviewer = actor(&["commerce.sales", "commerce.approve"]);

    let mut quote = order();
    quote.kind = "quote".into();
    assert_eq!(
        authorize_confirmed_cancellation(&reviewer, &command(), &quote, 0, NOW)
            .unwrap_err()
            .code,
        "SALES_CANCELLATION_ORDER_REQUIRED"
    );

    for state in ["draft", "partially_fulfilled", "fulfilled", "cancelled"] {
        let mut row = order();
        row.state = state.into();
        assert_eq!(
            authorize_confirmed_cancellation(&reviewer, &command(), &row, 0, NOW)
                .unwrap_err()
                .code,
            "SALES_CANCELLATION_CONFIRMED_REQUIRED"
        );
    }

    let mut stale = command();
    stale.expected_version = 2;
    assert_eq!(
        authorize_confirmed_cancellation(&reviewer, &stale, &order(), 0, NOW)
            .unwrap_err()
            .code,
        "CONFLICT"
    );

    let mut foreign = order();
    foreign.company_id = "foreign".into();
    assert_eq!(
        authorize_confirmed_cancellation(&reviewer, &command(), &foreign, 0, NOW)
            .unwrap_err()
            .code,
        "SALES_CANCELLATION_SOURCE_INVALID"
    );

    let mut other_branch = order();
    other_branch.branch_id = Some("other-branch".into());
    assert_eq!(
        authorize_confirmed_cancellation(&reviewer, &command(), &other_branch, 0, NOW)
            .unwrap_err()
            .code,
        "SALES_CANCELLATION_SOURCE_INVALID"
    );
}

#[tokio::test]
async fn command_releases_reservations_and_writes_receipt_audit_outbox_before_readback() {
    let mut repository = ready();
    let receipt = cancel_confirmed_sales_order(
        &actor(&["commerce.sales", "commerce.approve"]),
        &command(),
        &mut repository,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.state, "cancelled");
    assert_eq!(receipt.version, 4);
    assert_eq!(repository.events.release, 1);
    assert_eq!(repository.events.transition, 1);
    assert_eq!(repository.events.receipt, 1);
    assert_eq!(repository.events.audit, 1);
    assert_eq!(repository.events.outbox, 1);
    assert_eq!(repository.events.readback, 1);
    assert_eq!(repository.released_orders, vec!["sales-confirmed"]);
    assert_eq!(
        repository.order.as_ref().map(|row| row.state.as_str()),
        Some("cancelled")
    );
    let identity = repository.identities.front().unwrap();
    assert!(identity.contains("\"operation\":\"commerce.order.cancel\""));
    assert!(identity.contains("\"expectedVersion\":3"));
}

#[tokio::test]
async fn replay_returns_the_prior_receipt_without_releasing_or_transitioning_again() {
    let mut repository = ready();
    let reviewer = actor(&["commerce.sales", "commerce.approve"]);
    let first = cancel_confirmed_sales_order(&reviewer, &command(), &mut repository, NOW)
        .await
        .unwrap();
    let second = cancel_confirmed_sales_order(&reviewer, &command(), &mut repository, NOW)
        .await
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(repository.events.release, 1);
    assert_eq!(repository.events.transition, 1);
    assert_eq!(repository.events.audit, 1);
    assert_eq!(repository.events.outbox, 1);
    assert_eq!(repository.events.readback, 1);
    assert_eq!(repository.identities.len(), 2);
}

#[tokio::test]
async fn replay_mismatch_and_authority_loss_fail_closed() {
    let mut repository = ready();
    repository.receipt = Some(CancellationReceiptRecord {
        request_hash: "b".repeat(64),
        result_json: serde_json::to_string(&SalesCancellationReceipt::cancelled(
            "sales-confirmed",
            3,
        ))
        .unwrap(),
        record_id: Some("sales-confirmed".into()),
    });
    assert_eq!(
        cancel_confirmed_sales_order(
            &actor(&["commerce.sales", "commerce.approve"]),
            &command(),
            &mut repository,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "IDEMPOTENCY_CONFLICT"
    );

    let mut repository = ready();
    repository.authority_write = false;
    assert_eq!(
        cancel_confirmed_sales_order(
            &actor(&["commerce.sales", "commerce.approve"]),
            &command(),
            &mut repository,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "COMMERCE_PERMISSION_REQUIRED"
    );

    let mut repository = ready();
    repository.authority_approve = false;
    assert_eq!(
        cancel_confirmed_sales_order(
            &actor(&["commerce.sales", "commerce.approve"]),
            &command(),
            &mut repository,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "COMMERCE_APPROVAL_REQUIRED"
    );
    assert_eq!(repository.events.release, 0);
    assert_eq!(repository.events.transition, 0);
}

#[test]
fn receipt_shape_accepts_only_one_cancelled_sales_receipt() {
    assert!(validate_receipt_shape("order-1", "cancelled", 2, None));
    assert!(!validate_receipt_shape("", "cancelled", 2, None));
    assert!(!validate_receipt_shape("order-1", "confirmed", 2, None));
    assert!(!validate_receipt_shape("order-1", "cancelled", 1, None));
    assert!(!validate_receipt_shape(
        "order-1",
        "cancelled",
        2,
        Some("order-2")
    ));
}
