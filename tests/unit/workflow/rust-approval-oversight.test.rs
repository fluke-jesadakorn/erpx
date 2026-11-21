//! Portable Workflow approval oversight domain and application tests.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, HoldingId, RecordId, UserId,
};
use erpx_server_workflow::public::approval_oversight::{
    ApprovalAuditEvent, ApprovalCommandReceipt, ApprovalEvidence, ApprovalEvidenceDecision,
    ApprovalEvidenceInstance, ApprovalEvidenceQuery, ApprovalEvidenceStep, ApprovalKind,
    ApprovalOutboxEvent, ApprovalOversightActor, ApprovalOversightFuture,
    ApprovalOversightRepository, ApprovalQueueRow, approval_evidence, approval_queue,
    approval_queue_value, validate_evidence,
};
use serde_json::Value;

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}
fn actor_with(permissions: &[&str]) -> ApprovalOversightActor {
    ApprovalOversightActor {
        user_id: UserId::new("actor").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("assignment-actor").unwrap(),
        scope_kind: "holding".into(),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
    }
}
fn actor() -> ApprovalOversightActor {
    actor_with(&["organization.manage"])
}
fn query() -> ApprovalEvidenceQuery {
    ApprovalEvidenceQuery::new("expense", "expense-1").unwrap()
}
fn instance() -> ApprovalEvidenceInstance {
    ApprovalEvidenceInstance {
        id: "approval-1".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        record_type: "expense".into(),
        record_id: "expense-1".into(),
        phase: "request".into(),
        cycle: 1,
        requester_user_id: "requester".into(),
        assignee_user_id: None,
        state: "pending".into(),
        current_step: 0,
        blocked_reason: None,
        version: 2,
        created_at: 100,
        updated_at: 200,
    }
}
fn step(ordinal: i64) -> ApprovalEvidenceStep {
    ApprovalEvidenceStep {
        id: format!("step-{ordinal}"),
        instance_id: "approval-1".into(),
        ordinal,
        approver_user_id: "approver".into(),
        authority: "named".into(),
        state: "pending".into(),
        approver_pool_json: None,
        required_role: None,
        stage: None,
        route_snapshot_json: None,
    }
}
fn decision() -> ApprovalEvidenceDecision {
    ApprovalEvidenceDecision {
        id: "decision-1".into(),
        instance_id: "approval-1".into(),
        step_id: None,
        action: "initialize".into(),
        actor_user_id: "system".into(),
        previous_approver_user_id: None,
        approver_user_id: None,
        reason: "Initialize route".into(),
        reviewed_version: 1,
        command_id: "approval:initialize:1".into(),
        created_at: 110,
    }
}
fn receipt() -> ApprovalCommandReceipt {
    ApprovalCommandReceipt {
        command_id: "approval:initialize:1".into(),
        operation: "approval.initialize".into(),
        actor_id: "system".into(),
        record_id: "expense-1".into(),
        request_hash: "a".repeat(64),
        result_json: Some("{\"state\":\"pending\"}".into()),
        created_at: 110,
    }
}
fn audit() -> ApprovalAuditEvent {
    ApprovalAuditEvent {
        id: "audit-1".into(),
        command_id: "approval:initialize:1".into(),
        actor_id: "system".into(),
        assignment_id: "assignment-system".into(),
        action: "approval.initialize".into(),
        record_type: "expense_approval".into(),
        record_id: "expense-1".into(),
        record_version: 1,
        purpose: "approval.initialize".into(),
        created_at: 110,
    }
}
fn outbox() -> ApprovalOutboxEvent {
    ApprovalOutboxEvent {
        id: "outbox-1".into(),
        command_id: "approval:initialize:1".into(),
        event_type: "approval.initialize".into(),
        aggregate_type: "expense_approval".into(),
        aggregate_id: "expense-1".into(),
        aggregate_version: 1,
        created_at: 110,
    }
}
fn evidence() -> ApprovalEvidence {
    ApprovalEvidence {
        instance: instance(),
        steps: vec![step(0)],
        history: vec![decision()],
        receipts: vec![receipt()],
        audit: vec![audit()],
        outbox: vec![outbox()],
    }
}
fn queue_row(id_value: &str, record: &str, created_at: i64) -> ApprovalQueueRow {
    ApprovalQueueRow {
        id: id_value.into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        record_type: "expense".into(),
        record_id: record.into(),
        state: "pending".into(),
        blocked_reason: None,
        version: 1,
        created_at,
    }
}

#[derive(Clone, Default)]
struct Repository {
    queue: Vec<ApprovalQueueRow>,
    evidence: Option<ApprovalEvidence>,
    queue_calls: usize,
    evidence_calls: usize,
    fail: bool,
}
impl ApprovalOversightRepository for Repository {
    type Error = BusinessError;

    fn open_queue<'a>(
        &'a mut self,
        _: &'a ApprovalOversightActor,
        _: i64,
    ) -> ApprovalOversightFuture<'a, Vec<ApprovalQueueRow>, Self::Error> {
        self.queue_calls += 1;
        let result = if self.fail {
            Err(BusinessError::invalid("SYNTHETIC", "Synthetic failure."))
        } else {
            Ok(self.queue.clone())
        };
        Box::pin(async move { result })
    }

    fn evidence<'a>(
        &'a mut self,
        _: &'a ApprovalOversightActor,
        _: &'a ApprovalEvidenceQuery,
        _: i64,
    ) -> ApprovalOversightFuture<'a, Option<ApprovalEvidence>, Self::Error> {
        self.evidence_calls += 1;
        let result = if self.fail {
            Err(BusinessError::invalid("SYNTHETIC", "Synthetic failure."))
        } else {
            Ok(self.evidence.clone())
        };
        Box::pin(async move { result })
    }
}

#[test]
fn permission_and_exact_holding_scope_are_required() {
    let mut repository = Repository::default();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let error = approval_queue(&mut repository, &actor_with(&["workflow.read"]), 500)
            .await
            .unwrap_err();
        assert_eq!(error.code, "APPROVAL_EVIDENCE_PERMISSION_REQUIRED");
        assert_eq!(repository.queue_calls, 0);
        let mut branch_actor = actor();
        branch_actor.branch_id = Some(id("branch-1"));
        let error = approval_queue(&mut repository, &branch_actor, 500)
            .await
            .unwrap_err();
        assert_eq!(error.code, "APPROVAL_EVIDENCE_SCOPE_FORBIDDEN");
        assert_eq!(repository.queue_calls, 0);
    });
}

#[test]
fn queue_is_company_scoped_bounded_and_ordered() {
    let mut repository = Repository {
        queue: vec![
            queue_row("approval-2", "expense-2", 300),
            queue_row("approval-1", "expense-1", 200),
        ],
        ..Repository::default()
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let queue = approval_queue(&mut repository, &actor(), 500)
            .await
            .unwrap();
        assert_eq!(queue.evaluated_at, 500);
        assert_eq!(queue.approvals.len(), 2);
        assert_eq!(queue.approvals[0].record_id, "expense-2");
        let value = approval_queue_value(&mut repository, &actor(), 501)
            .await
            .unwrap();
        assert_eq!(value["evaluatedAt"], 501);
        assert_eq!(value["approvals"].as_array().unwrap().len(), 2);
        assert_eq!(repository.queue_calls, 2);
    });
}

#[test]
fn queue_rejects_foreign_duplicate_and_unordered_rows() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut foreign = Repository {
            queue: vec![queue_row("approval-1", "expense-1", 100)],
            ..Repository::default()
        };
        foreign.queue[0].company_id = "other".into();
        assert_eq!(
            approval_queue(&mut foreign, &actor(), 500)
                .await
                .unwrap_err()
                .code,
            "APPROVAL_EVIDENCE_SOURCE_INVALID"
        );

        let duplicate = Repository {
            queue: vec![
                queue_row("approval-1", "expense-1", 100),
                queue_row("approval-1", "expense-2", 99),
            ],
            ..Repository::default()
        };
        let mut duplicate = duplicate;
        assert!(approval_queue(&mut duplicate, &actor(), 500).await.is_err());

        let mut unordered = Repository {
            queue: vec![
                queue_row("approval-1", "expense-1", 100),
                queue_row("approval-2", "expense-2", 101),
            ],
            ..Repository::default()
        };
        assert!(approval_queue(&mut unordered, &actor(), 500).await.is_err());

        let mut blocked = Repository {
            queue: vec![queue_row("approval-1", "expense-1", 100)],
            ..Repository::default()
        };
        blocked.queue[0].state = "blocked".into();
        assert!(approval_queue(&mut blocked, &actor(), 500).await.is_err());
    });
}

#[test]
fn evidence_requires_the_exact_company_kind_and_record() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut repository = Repository {
            evidence: Some(evidence()),
            ..Repository::default()
        };
        let value = approval_evidence(&mut repository, &actor(), &query(), 500)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(value["approval"]["instance"]["record_id"], "expense-1");
        assert_eq!(
            value["approval"]["history"][0]["command_id"],
            "approval:initialize:1"
        );

        let mut foreign = Repository {
            evidence: Some(evidence()),
            ..Repository::default()
        };
        foreign.evidence.as_mut().unwrap().instance.company_id = "other".into();
        assert_eq!(
            approval_evidence(&mut foreign, &actor(), &query(), 500)
                .await
                .unwrap_err()
                .code,
            "APPROVAL_EVIDENCE_SOURCE_INVALID"
        );

        let mut wrong_kind = Repository {
            evidence: Some(evidence()),
            ..Repository::default()
        };
        wrong_kind.evidence.as_mut().unwrap().instance.record_type = "leave".into();
        assert!(
            approval_evidence(&mut wrong_kind, &actor(), &query(), 500)
                .await
                .is_err()
        );

        let mut missing = Repository {
            evidence: None,
            ..Repository::default()
        };
        assert!(
            approval_evidence(&mut missing, &actor(), &query(), 500)
                .await
                .unwrap()
                .is_none()
        );
    });
}

#[test]
fn evidence_rejects_duplicate_orphan_and_incoherent_rows() {
    let mut duplicate_step = evidence();
    duplicate_step.steps.push(step(0));
    assert!(validate_evidence(duplicate_step, &actor(), &query()).is_err());

    let mut duplicate_decision = evidence();
    duplicate_decision.history.push(decision());
    assert!(validate_evidence(duplicate_decision, &actor(), &query()).is_err());

    let mut duplicate_receipt = evidence();
    duplicate_receipt.receipts.push(receipt());
    assert!(validate_evidence(duplicate_receipt, &actor(), &query()).is_err());

    let mut foreign_receipt = evidence();
    foreign_receipt.receipts[0].record_id = "expense-2".into();
    assert!(validate_evidence(foreign_receipt, &actor(), &query()).is_err());

    let mut orphan_receipt = evidence();
    orphan_receipt.receipts[0].command_id = "unlinked".into();
    assert!(validate_evidence(orphan_receipt, &actor(), &query()).is_err());

    let mut foreign_audit = evidence();
    foreign_audit.audit[0].record_id = "expense-2".into();
    assert!(validate_evidence(foreign_audit, &actor(), &query()).is_err());

    let mut foreign_outbox = evidence();
    foreign_outbox.outbox[0].aggregate_id = "expense-2".into();
    assert!(validate_evidence(foreign_outbox, &actor(), &query()).is_err());

    let mut blocked = evidence();
    blocked.instance.state = "blocked".into();
    blocked.instance.blocked_reason = None;
    assert!(validate_evidence(blocked, &actor(), &query()).is_err());
}

#[test]
fn query_and_claim_source_bounds_fail_closed() {
    assert!(ApprovalEvidenceQuery::new("legal", "record-1").is_err());
    assert!(ApprovalEvidenceQuery::new("expense", "").is_err());
    assert!(ApprovalEvidenceQuery::new("expense", "has space").is_err());
    assert!(ApprovalEvidenceQuery::new("expense", &"x".repeat(201)).is_err());
    assert_eq!(
        ApprovalEvidenceQuery::new("leave_cancellation", "record-1")
            .unwrap()
            .kind,
        ApprovalKind::LeaveCancellation
    );

    let mut oversized = evidence();
    oversized.steps[0].approver_pool_json = Some(format!("\"{}\"", "x".repeat(70_000)));
    assert!(validate_evidence(oversized, &actor(), &query()).is_err());

    let mut oversized_receipt = evidence();
    oversized_receipt.receipts[0].result_json = Some(format!("\"{}\"", "x".repeat(70_000)));
    assert!(validate_evidence(oversized_receipt, &actor(), &query()).is_err());
}

#[test]
fn repository_failures_propagate_without_a_fallback() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut repository = Repository {
            fail: true,
            ..Repository::default()
        };
        let queue = approval_queue(&mut repository, &actor(), 500)
            .await
            .unwrap_err();
        assert_eq!(queue.code, "SYNTHETIC");
        let evidence = approval_evidence(&mut repository, &actor(), &query(), 500)
            .await
            .unwrap_err();
        assert_eq!(evidence.code, "SYNTHETIC");
    });
}

#[test]
fn serialized_queue_uses_the_exact_transport_field_set() {
    let queue = ApprovalQueueRow {
        id: "approval-1".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        record_type: "expense".into(),
        record_id: "expense-1".into(),
        state: "pending".into(),
        blocked_reason: None,
        version: 1,
        created_at: 100,
    };
    let value = serde_json::to_value(queue).unwrap();
    let keys = value
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        keys,
        [
            "id",
            "company_id",
            "branch_id",
            "department_id",
            "record_type",
            "record_id",
            "state",
            "blocked_reason",
            "version",
            "created_at"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    let _: Value = serde_json::json!({"data": value});
}
