//! Native Quality policy and application decisions over semantic ports.
use erpx_server_quality::public::quality_read::{
    CancellationAction, HoldStatus, InspectionResult, MeasurementInput, QualityActor,
    QualityCancellationCommand, QualityCancellationReceipt, QualityCancellationRepository,
    QualityDetailData, QualityFuture, QualityHoldFacts, QualityMetric, QualityReadRepository,
    QualityWorkspaceData, cancel_quality_hold, evaluate_measurements, plan_cancellation,
    quality_decision, quality_detail, quality_workspace,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis, UserId,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn user(value: &str) -> UserId {
    UserId::new(value).unwrap()
}

fn command(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn metric(minimum: Option<&str>, maximum: Option<&str>) -> QualityMetric {
    QualityMetric {
        id: "metric-weight".into(),
        code: "weight".into(),
        label: "Weight".into(),
        unit: "kg".into(),
        minimum: minimum.map(str::to_owned),
        maximum: maximum.map(str::to_owned),
        ordinal: 0,
    }
}

fn hold(
    status: HoldStatus,
    creator: &str,
    inspector: Option<&str>,
    result: Option<InspectionResult>,
) -> QualityHoldFacts {
    QualityHoldFacts {
        id: "hold".into(),
        status,
        version: 3,
        created_by: creator.into(),
        cancel_requested_by: None,
        last_inspector: inspector.map(str::to_owned),
        last_result: result,
        has_inspections: result.is_some(),
    }
}

fn actor(
    can_hold: bool,
    can_inspect: bool,
    can_release: bool,
) -> erpx_server_quality::public::quality_read::QualityActorFacts {
    erpx_server_quality::public::quality_read::QualityActorFacts {
        user_id: "actor".into(),
        can_hold,
        can_inspect,
        can_release,
        company_scope: true,
    }
}

#[test]
fn derives_measurement_results_and_rejects_conflicting_or_incomplete_values() {
    let metrics = vec![
        metric(Some("9.9"), Some("10.1")),
        QualityMetric {
            id: "metric-temperature".into(),
            code: "temperature".into(),
            label: "Temperature".into(),
            unit: "C".into(),
            minimum: None,
            maximum: Some("30".into()),
            ordinal: 1,
        },
    ];
    let passed = evaluate_measurements(
        &metrics,
        &[
            MeasurementInput {
                metric_id: "metric-weight".into(),
                value: "10".into(),
            },
            MeasurementInput {
                metric_id: "metric-temperature".into(),
                value: "25".into(),
            },
        ],
        Some(InspectionResult::Pass),
    )
    .unwrap();
    assert_eq!(passed.result, InspectionResult::Pass);
    assert_eq!(
        passed.provided.get("metric-weight").map(String::as_str),
        Some("10")
    );

    let failed = evaluate_measurements(
        &metrics,
        &[
            MeasurementInput {
                metric_id: "metric-weight".into(),
                value: "10.2".into(),
            },
            MeasurementInput {
                metric_id: "metric-temperature".into(),
                value: "25".into(),
            },
        ],
        None,
    )
    .unwrap();
    assert_eq!(failed.result, InspectionResult::Fail);

    let conflict = evaluate_measurements(
        &metrics,
        &[
            MeasurementInput {
                metric_id: "metric-weight".into(),
                value: "10.2".into(),
            },
            MeasurementInput {
                metric_id: "metric-temperature".into(),
                value: "25".into(),
            },
        ],
        Some(InspectionResult::Pass),
    )
    .unwrap_err();
    assert_eq!(conflict.code, "QUALITY_RESULT_DERIVED");

    let missing = evaluate_measurements(
        &metrics,
        &[MeasurementInput {
            metric_id: "metric-weight".into(),
            value: "10".into(),
        }],
        None,
    )
    .unwrap_err();
    assert_eq!(missing.code, "QUALITY_MEASUREMENTS_REQUIRED");

    let duplicate = evaluate_measurements(
        &metrics,
        &[
            MeasurementInput {
                metric_id: "metric-weight".into(),
                value: "10".into(),
            },
            MeasurementInput {
                metric_id: "metric-weight".into(),
                value: "10".into(),
            },
            MeasurementInput {
                metric_id: "metric-temperature".into(),
                value: "25".into(),
            },
        ],
        None,
    )
    .unwrap_err();
    assert_eq!(duplicate.code, "QUALITY_MEASUREMENTS_REQUIRED");

    assert_eq!(
        evaluate_measurements(
            &[],
            &[MeasurementInput {
                metric_id: "metric-weight".into(),
                value: "10".into()
            }],
            None
        )
        .unwrap_err()
        .code,
        "QUALITY_SPEC_REQUIRED"
    );
    assert_eq!(
        evaluate_measurements(&[], &[], None).unwrap_err().code,
        "QUALITY_RESULT_REQUIRED"
    );
    assert_eq!(
        evaluate_measurements(&[], &[], Some(InspectionResult::Fail))
            .unwrap()
            .result,
        InspectionResult::Fail
    );
}

#[test]
fn invalid_captured_limits_never_turn_into_unbounded_passing_limits() {
    for limits in [(Some("invalid"), None), (None, Some("NaN"))] {
        let result = evaluate_measurements(
            &[metric(limits.0, limits.1)],
            &[MeasurementInput {
                metric_id: "metric-weight".into(),
                value: "10".into(),
            }],
            None,
        )
        .unwrap();
        assert_eq!(result.result, InspectionResult::Fail);
    }
}

#[test]
fn cancellation_reason_normalization_matches_javascript_trim() {
    let normalized = erpx_server_quality::public::quality_read::validate_reason(
        "\u{feff} reason \u{feff}", "QUALITY_CANCELLATION_REASON", "Invalid reason",
    ).unwrap();
    assert_eq!(normalized, "reason");
    let retained = erpx_server_quality::public::quality_read::validate_reason(
        "\u{0085}reason\u{0085}", "QUALITY_CANCELLATION_REASON", "Invalid reason",
    ).unwrap();
    assert_eq!(retained, "\u{0085}reason\u{0085}");
}

#[test]
fn separates_independent_inspection_release_cancellation_and_disposition_decisions() {
    let held = hold(HoldStatus::Held, "actor", None, None);
    let creator = actor(true, true, true);
    let decision = quality_decision(&held, &creator);
    assert!(!decision.can_inspect);
    assert!(decision.can_cancel);
    assert!(!decision.can_release);
    assert!(!decision.can_disposition);

    let reviewer = actor(true, true, true);
    let reviewer = erpx_server_quality::public::quality_read::QualityActorFacts {
        user_id: "reviewer".into(),
        ..reviewer
    };
    let awaiting = hold(
        HoldStatus::AwaitingRelease,
        "actor",
        Some("inspector"),
        Some(InspectionResult::Pass),
    );
    assert!(quality_decision(&awaiting, &reviewer).can_release);
    assert!(!quality_decision(&awaiting, &creator).can_release);
    let inspector = erpx_server_quality::public::quality_read::QualityActorFacts {
        user_id: "inspector".into(),
        ..reviewer.clone()
    };
    assert!(!quality_decision(&awaiting, &inspector).can_release);
    let failed = hold(
        HoldStatus::Held,
        "actor",
        Some("inspector"),
        Some(InspectionResult::Fail),
    );
    assert!(quality_decision(&failed, &reviewer).can_disposition);
    assert!(!quality_decision(&failed, &creator).can_disposition);
    assert!(!quality_decision(&failed, &inspector).can_disposition);
    assert!(!quality_decision(&failed, &reviewer).can_cancel);

    let mut pending = hold(HoldStatus::CancelPending, "actor", None, None);
    pending.cancel_requested_by = Some("requester".into());
    let requester = erpx_server_quality::public::quality_read::QualityActorFacts {
        user_id: "requester".into(),
        ..reviewer.clone()
    };
    assert!(quality_decision(&pending, &reviewer).can_review_cancellation);
    assert!(!quality_decision(&pending, &requester).can_review_cancellation);
    assert!(!quality_decision(&pending, &creator).can_review_cancellation);
}

#[test]
fn cancellation_plan_preserves_independence_and_uninspected_hold_invariants() {
    let held = hold(HoldStatus::Held, "maker", None, None);
    let planned = plan_cancellation(&held, "maker", CancellationAction::Request, 3).unwrap();
    assert!(planned.request);
    assert_eq!(planned.target, HoldStatus::CancelPending);
    assert_eq!(planned.version, 4);

    assert_eq!(
        plan_cancellation(&held, "maker", CancellationAction::Request, 2)
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    let inspected = hold(
        HoldStatus::Held,
        "maker",
        Some("inspector"),
        Some(InspectionResult::Fail),
    );
    assert_eq!(
        plan_cancellation(&inspected, "maker", CancellationAction::Request, 3)
            .unwrap_err()
            .code,
        "UNINSPECTED_HOLD_REQUIRED"
    );

    let mut pending = hold(HoldStatus::CancelPending, "maker", None, None);
    pending.cancel_requested_by = Some("requester".into());
    assert_eq!(
        plan_cancellation(&pending, "maker", CancellationAction::Approve, 3)
            .unwrap_err()
            .code,
        "INDEPENDENT_CANCELLATION_REQUIRED"
    );
    assert_eq!(
        plan_cancellation(&pending, "requester", CancellationAction::Reject, 3)
            .unwrap_err()
            .code,
        "INDEPENDENT_CANCELLATION_REQUIRED"
    );
    assert_eq!(
        plan_cancellation(&pending, "manager", CancellationAction::Approve, 3)
            .unwrap()
            .target,
        HoldStatus::Cancelled
    );
    assert_eq!(
        plan_cancellation(&pending, "manager", CancellationAction::Reject, 3)
            .unwrap()
            .target,
        HoldStatus::Held
    );
}

struct EmptyRead(AtomicUsize);
impl QualityReadRepository for EmptyRead {
    fn workspace<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _branch: Option<&'a RecordId>,
    ) -> QualityFuture<'a, QualityWorkspaceData> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(QualityWorkspaceData {
                holds: vec![],
                specification_metrics: vec![],
                alerts: vec![],
            })
        })
    }
    fn detail<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _branch: Option<&'a RecordId>,
        _id: &'a str,
    ) -> QualityFuture<'a, Option<QualityDetailData>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(None) })
    }
}

struct EmptyCancel(AtomicUsize);
impl QualityCancellationRepository for EmptyCancel {
    fn cancel<'a>(
        &'a mut self,
        _command: &'a QualityCancellationCommand,
        _company: &'a CompanyId,
        _branch: Option<&'a RecordId>,
        _actor_id: &'a UserId,
        _now: i64,
    ) -> QualityFuture<'a, QualityCancellationReceipt> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(QualityCancellationReceipt {
                id: "hold".into(),
                hold_id: "hold".into(),
                cancellation_id: "cancellation".into(),
                status: HoldStatus::CancelPending,
                version: 2,
            })
        })
    }
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1)
    }
}

fn quality_actor(permissions: &[&str], department: Option<RecordId>) -> QualityActor {
    QualityActor {
        user_id: user("maker"),
        company_id: company(),
        branch_id: Some(id("branch")),
        department_id: department,
        permissions: permissions
            .iter()
            .map(|permission| (*permission).into())
            .collect(),
    }
}

#[tokio::test]
async fn department_scope_and_missing_read_deny_before_the_repository() {
    let mut repository = EmptyRead(AtomicUsize::new(0));
    let denied = quality_workspace(
        &quality_actor(&["quality.read"], Some(id("department"))),
        &mut repository,
    )
    .await
    .unwrap_err();
    assert_eq!(denied.code, "QUALITY_SCOPE_REQUIRED");
    assert_eq!(repository.0.load(Ordering::SeqCst), 0);

    let denied = quality_workspace(&quality_actor(&[], None), &mut repository)
        .await
        .unwrap_err();
    assert_eq!(denied.code, "QUALITY_ACCESS_CHANGED");
    assert_eq!(repository.0.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn missing_detail_is_a_scoped_not_found_after_the_owner_read() {
    let mut repository = EmptyRead(AtomicUsize::new(0));
    let denied = quality_detail(
        &quality_actor(&["quality.read"], None),
        "hold",
        &mut repository,
    )
    .await
    .unwrap_err();
    assert_eq!(denied.kind, FailureKind::NotFound);
    assert_eq!(denied.code, "QUALITY_HOLD_NOT_FOUND");
    assert_eq!(repository.0.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancellation_requires_the_exact_action_permission_and_a_bounded_reason() {
    let mut repository = EmptyCancel(AtomicUsize::new(0));
    let actor = quality_actor(&["quality.read", "quality.inspect"], None);
    let denied = cancel_quality_hold(
        &actor,
        QualityCancellationCommand {
            command_key: command("quality-cancel-command"),
            hold_id: id("hold"),
            expected_version: 1,
            action: CancellationAction::Approve,
            reason: "Independent review".into(),
        },
        &mut repository,
        &FixedClock,
    )
    .await
    .unwrap_err();
    assert_eq!(denied.code, "QUALITY_ACCESS_CHANGED");
    assert_eq!(repository.0.load(Ordering::SeqCst), 0);

    let actor = quality_actor(&["quality.read", "quality.release"], None);
    let denied = cancel_quality_hold(
        &actor,
        QualityCancellationCommand {
            command_key: command("quality-cancel-command"),
            hold_id: id("hold"),
            expected_version: 1,
            action: CancellationAction::Approve,
            reason: "x".into(),
        },
        &mut repository,
        &FixedClock,
    )
    .await
    .unwrap_err();
    assert_eq!(denied.code, "QUALITY_CANCELLATION_REASON");
    assert_eq!(repository.0.load(Ordering::SeqCst), 0);
}

#[test]
fn all_terminal_and_pending_hold_statuses_round_trip() {
    for status in [
        HoldStatus::Held,
        HoldStatus::AwaitingRelease,
        HoldStatus::Released,
        HoldStatus::CancelPending,
        HoldStatus::Cancelled,
        HoldStatus::DispositionPending,
        HoldStatus::Disposed,
    ] {
        assert_eq!(HoldStatus::parse(status.as_str()), Some(status));
    }
    assert!(HoldStatus::Released.is_terminal());
    assert!(HoldStatus::Cancelled.is_terminal());
    assert!(HoldStatus::Disposed.is_terminal());
    assert!(!HoldStatus::Held.is_terminal());
    assert!(HoldStatus::Held.is_inspectable());
    assert!(!HoldStatus::Disposed.is_inspectable());
}

#[allow(dead_code)]
fn _assert_error_shape(error: BusinessError) -> (FailureKind, &'static str) {
    (error.kind, error.code)
}
