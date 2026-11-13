//! Native AI-gateway policy, statement, verified cost and reconciliation
//! decisions. Ports are faked; exact retained failure codes and replay
//! equality are asserted without a database.
use erpx_server_agent::public::gateway::*;
use erpx_server_shared::public::BusinessError;

#[derive(Default)]
struct Fake {
    policy: Option<AiPolicyFacts>,
    set_outcome: Option<AiSetEnabledOutcome>,
    update_result: bool,
    insert_statement: bool,
    statement: Option<AiStatementEvidence>,
    insert_cost: bool,
    existing_cost: Option<AiExistingCost>,
    reconcile_result: bool,
    reconciliation: Option<AiReconciliationFact>,
    rows: Vec<AiGatewayRow>,
    calls: Vec<&'static str>,
    set_input: Option<AiSetEnabledInput>,
    update_input: Option<AiPolicyChange>,
    statement_input: Option<AiProviderStatement>,
    cost_input: Option<AiVerifiedCost>,
    reconciled_id: Option<String>,
}

impl AiGatewayPorts for Fake {
    type Error = BusinessError;

    async fn policy(&mut self, _: i64, _: i64) -> Result<Option<AiPolicyFacts>, Self::Error> {
        self.calls.push("policy");
        Ok(self.policy.clone())
    }
    async fn set_enabled(
        &mut self,
        input: &AiSetEnabledInput,
        actor_id: &str,
        audit_id: &str,
        _: i64,
    ) -> Result<AiSetEnabledOutcome, Self::Error> {
        self.calls.push("set_enabled");
        assert!(!actor_id.is_empty());
        assert!(!audit_id.is_empty());
        self.set_input = Some(input.clone());
        Ok(self.set_outcome.unwrap_or(AiSetEnabledOutcome::Updated))
    }
    async fn update_policy(
        &mut self,
        input: &AiPolicyChange,
        _: &str,
        _: &str,
        _: i64,
    ) -> Result<bool, Self::Error> {
        self.calls.push("update_policy");
        self.update_input = Some(input.clone());
        Ok(self.update_result)
    }
    async fn insert_statement(
        &mut self,
        input: &AiProviderStatement,
        _: i64,
    ) -> Result<bool, Self::Error> {
        self.calls.push("insert_statement");
        self.statement_input = Some(input.clone());
        Ok(self.insert_statement)
    }
    async fn statement_evidence(
        &mut self,
        _: &str,
    ) -> Result<Option<AiStatementEvidence>, Self::Error> {
        self.calls.push("statement_evidence");
        Ok(self.statement.clone())
    }
    async fn insert_cost_evidence(
        &mut self,
        input: &AiVerifiedCost,
        _: i64,
    ) -> Result<bool, Self::Error> {
        self.calls.push("insert_cost_evidence");
        self.cost_input = Some(input.clone());
        Ok(self.insert_cost)
    }
    async fn existing_cost_evidence(
        &mut self,
        _: &str,
    ) -> Result<Option<AiExistingCost>, Self::Error> {
        self.calls.push("existing_cost_evidence");
        Ok(self.existing_cost.clone())
    }
    async fn reconcile(&mut self, statement_id: &str, _: i64) -> Result<bool, Self::Error> {
        self.calls.push("reconcile");
        self.reconciled_id = Some(statement_id.to_owned());
        Ok(self.reconcile_result)
    }
    async fn existing_reconciliation(
        &mut self,
        _: &str,
    ) -> Result<Option<AiReconciliationFact>, Self::Error> {
        self.calls.push("existing_reconciliation");
        Ok(self.reconciliation.clone())
    }
    async fn safe_rows(&mut self, _: i64, _: i64) -> Result<Vec<AiGatewayRow>, Self::Error> {
        self.calls.push("safe_rows");
        Ok(self.rows.clone())
    }
}

fn facts() -> AiPolicyFacts {
    AiPolicyFacts {
        platform_enabled: true,
        monthly_request_limit: 1000,
        monthly_spend_cap_satang: 10_000_000,
        reservation_satang: 1000,
        updated_by: "platform-admin".into(),
        updated_at: 42,
        monthly_requests: 2,
        monthly_reserved_or_verified_satang: 1000,
        pending_platform_cost_requests: 1,
    }
}

fn statement() -> AiProviderStatement {
    AiProviderStatement {
        id: "statement-1".into(),
        holding_id: "gateway-holding".into(),
        provider: "api.provider.example".into(),
        month: "2026-11".into(),
        currency: "USD".into(),
        amount_minor: 20,
        statement_digest: "a".repeat(64),
        verified_by: "platform-admin".into(),
    }
}

fn cost() -> AiVerifiedCost {
    AiVerifiedCost {
        request_id: "request-1".into(),
        provider_statement_id: "statement-1".into(),
        currency: "USD".into(),
        amount_minor: 10,
        thb_satang: 350,
        fx_evidence_ref: Some("bank-rate-1".into()),
        verified_by: "platform-admin".into(),
    }
}

fn code(error: BusinessError) -> &'static str {
    error.code
}

#[tokio::test]
async fn policy_read_requires_the_singleton_row_and_projects_safe_facts() {
    let mut missing = Fake::default();
    assert_eq!(
        code(read_ai_gateway_policy(&mut missing, 0).await.unwrap_err()),
        "AI_GATEWAY_POLICY_MISSING"
    );
    let mut fake = Fake {
        policy: Some(facts()),
        ..Fake::default()
    };
    let view = read_ai_gateway_policy(&mut fake, 0).await.unwrap();
    assert_eq!(view.monthly_request_limit, 1000);
    assert_eq!(view.monthly_reserved_or_verified_satang, 1000);
    assert_eq!(view.updated_at, 42);
    assert_eq!(
        serde_json::to_value(view).unwrap()["monthlyRequestLimit"],
        1000
    );
}

#[tokio::test]
async fn kill_switch_requires_reason_and_preserves_compare_and_set_codes() {
    let mut fake = Fake::default();
    let input = AiSetEnabledInput {
        enabled: false,
        expected_updated_at: 42,
        reason: " ".into(),
    };
    assert_eq!(
        code(
            set_ai_platform_key_enabled(&mut fake, 0, "audit-1", &input, "platform-admin")
                .await
                .unwrap_err()
        ),
        "AI_GATEWAY_REASON"
    );
    assert!(fake.calls.is_empty());

    let input = AiSetEnabledInput {
        enabled: false,
        expected_updated_at: 42,
        reason: "Provider incident".into(),
    };
    for (outcome, expected, view) in [
        (
            AiSetEnabledOutcome::Missing,
            "AI_GATEWAY_POLICY_MISSING",
            None,
        ),
        (
            AiSetEnabledOutcome::Changed,
            "AI_GATEWAY_POLICY_CHANGED",
            None,
        ),
        (
            AiSetEnabledOutcome::Unchanged,
            "AI_GATEWAY_POLICY_UNCHANGED",
            None,
        ),
        (AiSetEnabledOutcome::Updated, "", Some(facts())),
    ] {
        let mut fake = Fake {
            policy: view,
            set_outcome: Some(outcome),
            ..Fake::default()
        };
        let result =
            set_ai_platform_key_enabled(&mut fake, 0, "audit-1", &input, "platform-admin").await;
        if expected.is_empty() {
            assert!(result.unwrap().platform_enabled);
        } else {
            assert_eq!(code(result.unwrap_err()), expected);
        }
        assert_eq!(fake.set_input.unwrap().reason, "Provider incident");
    }
}

#[tokio::test]
async fn limit_update_rejects_nonpositive_values_and_records_the_audit_actor() {
    let base = AiPolicyChange {
        enabled: true,
        monthly_request_limit: 1000,
        monthly_spend_cap_satang: 5_000,
        reservation_satang: 1_000,
        reason: "Restore allowance".into(),
    };
    let mut fake = Fake::default();
    let negative = AiPolicyChange {
        monthly_request_limit: -1,
        ..base.clone()
    };
    assert_eq!(
        code(
            update_ai_gateway_policy(&mut fake, 0, "audit-1", &negative, "platform-admin")
                .await
                .unwrap_err()
        ),
        "AI_COST_INVALID"
    );
    let zero = AiPolicyChange {
        reservation_satang: 0,
        ..base.clone()
    };
    assert_eq!(
        code(
            update_ai_gateway_policy(&mut fake, 0, "audit-1", &zero, "platform-admin")
                .await
                .unwrap_err()
        ),
        "AI_GATEWAY_POLICY"
    );
    let no_reason = AiPolicyChange {
        reason: String::new(),
        ..base.clone()
    };
    assert_eq!(
        code(
            update_ai_gateway_policy(&mut fake, 0, "audit-1", &no_reason, "platform-admin")
                .await
                .unwrap_err()
        ),
        "AI_GATEWAY_REASON"
    );
    assert!(fake.calls.is_empty());

    let mut fake = Fake {
        update_result: true,
        policy: Some(facts()),
        ..Fake::default()
    };
    assert_eq!(
        update_ai_gateway_policy(&mut fake, 0, "audit-1", &base, "platform-admin")
            .await
            .unwrap()
            .reservation_satang,
        1000
    );
    assert_eq!(fake.update_input.unwrap().monthly_spend_cap_satang, 5000);
    assert_eq!(fake.calls, vec!["update_policy", "policy"]);
}

#[tokio::test]
async fn statement_record_is_immutable_and_replay_safe() {
    let mut fake = Fake::default();
    let invalid = AiProviderStatement {
        month: "2026-13".into(),
        ..statement()
    };
    assert_eq!(
        code(
            record_ai_provider_statement(&mut fake, 0, &invalid)
                .await
                .unwrap_err()
        ),
        "AI_STATEMENT_INVALID"
    );
    assert!(fake.calls.is_empty());
    let invalid = AiProviderStatement {
        statement_digest: "xyz".into(),
        ..statement()
    };
    assert_eq!(
        code(
            record_ai_provider_statement(&mut fake, 0, &invalid)
                .await
                .unwrap_err()
        ),
        "AI_STATEMENT_INVALID"
    );

    let mut fake = Fake {
        insert_statement: true,
        ..Fake::default()
    };
    assert_eq!(
        record_ai_provider_statement(&mut fake, 0, &statement())
            .await
            .unwrap(),
        serde_json::json!({"ok": true})
    );
    assert_eq!(fake.statement_input.unwrap().holding_id, "gateway-holding");

    let evidence = AiStatementEvidence {
        holding_id: "gateway-holding".into(),
        provider: "api.provider.example".into(),
        month: "2026-11".into(),
        currency: "USD".into(),
        amount_minor: 20,
        statement_digest: "a".repeat(64),
    };
    let mut replay = Fake {
        statement: Some(evidence.clone()),
        ..Fake::default()
    };
    assert!(
        record_ai_provider_statement(&mut replay, 0, &statement())
            .await
            .is_ok()
    );
    let mut conflict = Fake {
        statement: Some(AiStatementEvidence {
            amount_minor: 21,
            ..evidence
        }),
        ..Fake::default()
    };
    assert_eq!(
        code(
            record_ai_provider_statement(&mut conflict, 0, &statement())
                .await
                .unwrap_err()
        ),
        "AI_STATEMENT_CONFLICT"
    );
}

#[tokio::test]
async fn verified_cost_requires_fx_before_any_repository_write_and_replays_exactly() {
    let mut fake = Fake::default();
    let no_fx = AiVerifiedCost {
        fx_evidence_ref: Some("  ".into()),
        ..cost()
    };
    assert_eq!(
        code(
            record_verified_ai_cost(&mut fake, 0, &no_fx)
                .await
                .unwrap_err()
        ),
        "AI_COST_EVIDENCE_INVALID"
    );
    assert!(fake.calls.is_empty());

    let mut inserted = Fake {
        insert_cost: true,
        ..Fake::default()
    };
    assert_eq!(
        record_verified_ai_cost(&mut inserted, 0, &cost())
            .await
            .unwrap(),
        serde_json::json!({"ok": true})
    );
    assert_eq!(inserted.cost_input.unwrap().thb_satang, 350);

    let existing = AiExistingCost {
        provider_statement_id: "statement-1".into(),
        currency: "USD".into(),
        amount_minor: 10,
        thb_satang: 350,
        fx_evidence_ref: Some("bank-rate-1".into()),
    };
    let mut replay = Fake {
        existing_cost: Some(existing.clone()),
        ..Fake::default()
    };
    assert!(
        record_verified_ai_cost(&mut replay, 0, &cost())
            .await
            .is_ok()
    );
    let mut conflict = Fake {
        existing_cost: Some(AiExistingCost {
            thb_satang: 351,
            ..existing
        }),
        ..Fake::default()
    };
    assert_eq!(
        code(
            record_verified_ai_cost(&mut conflict, 0, &cost())
                .await
                .unwrap_err()
        ),
        "AI_COST_EVIDENCE_CONFLICT"
    );
    let thb = AiVerifiedCost {
        currency: "THB".into(),
        fx_evidence_ref: None,
        ..cost()
    };
    let mut thb_replay = Fake {
        insert_cost: true,
        ..Fake::default()
    };
    assert!(
        record_verified_ai_cost(&mut thb_replay, 0, &thb)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn reconciliation_is_only_accepted_when_balanced_or_already_reconciled() {
    let mut updated = Fake {
        reconcile_result: true,
        ..Fake::default()
    };
    assert_eq!(
        reconcile_ai_provider_statement(&mut updated, 0, "statement-1")
            .await
            .unwrap(),
        serde_json::json!({"ok": true})
    );
    assert_eq!(updated.reconciled_id.as_deref(), Some("statement-1"));

    let mut unbalanced = Fake::default();
    assert_eq!(
        code(
            reconcile_ai_provider_statement(&mut unbalanced, 0, "statement-1")
                .await
                .unwrap_err()
        ),
        "AI_STATEMENT_UNBALANCED"
    );
    let mut replay = Fake {
        reconciliation: Some(AiReconciliationFact {
            reconciled_at: Some(42),
        }),
        ..Fake::default()
    };
    assert!(
        reconcile_ai_provider_statement(&mut replay, 0, "statement-1")
            .await
            .is_ok()
    );
}

#[test]
fn month_bounds_keep_the_bangkok_window() {
    assert_eq!(billing_month(0), "1970-01");
    assert_eq!(
        month_bounds("2026-11").unwrap(),
        (1_793_466_000_000, 1_796_058_000_000)
    );
    assert_eq!(
        month_bounds("2026-13").unwrap_err().code,
        "AI_GATEWAY_MONTH_INVALID"
    );
}
