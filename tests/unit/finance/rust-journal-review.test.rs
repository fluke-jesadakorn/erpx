use erpx_server_finance::public::{FinanceActor, JournalError, journal_review::*};
use erpx_server_shared::public::{
    AssignmentId, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis, UserId,
};
use num_bigint::BigInt;
use serde_json::{Value, json};

fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/finance/rust-journal-review/conformance.v1.json"
    ))
    .unwrap()
}
fn sample_line(
    id: i64,
    debit: i64,
    credit: i64,
    base_debit: i64,
    base_credit: i64,
) -> JournalLedgerLine {
    JournalLedgerLine {
        id: format!("journal-{id}"),
        journal_id: "journal".into(),
        account_id: format!("account-{id}"),
        description: "Original ก description".into(),
        debit_minor: debit,
        credit_minor: credit,
        base_debit_minor: base_debit,
        base_credit_minor: base_credit,
        dimension_json: r#"{"branch":"BKK","unchanged":true}"#.into(),
        ordinal: id,
    }
}
fn record(value: &Value) -> Option<ReviewJournal> {
    if value.is_null() {
        return None;
    }
    Some(ReviewJournal {
        id: value["id"].as_str().unwrap().into(),
        company_id: value["company_id"].as_str().unwrap().into(),
        chart_id: value["chart_id"].as_str().unwrap().into(),
        period_id: value["period_id"].as_str().unwrap().into(),
        journal_number: value["journal_number"].as_str().unwrap().into(),
        entry_date: value["entry_date"].as_str().unwrap().into(),
        currency: value["currency"].as_str().unwrap().into(),
        status: value["status"].as_str().unwrap().into(),
        version: value["version"].as_i64().unwrap(),
        created_by: value["created_by"].as_str().unwrap().into(),
        source_type: value["source_type"].as_str().unwrap().into(),
        source_id: value["source_id"].as_str().map(str::to_owned),
    })
}
fn actor() -> FinanceActor {
    FinanceActor {
        user_id: UserId::new("reviewer").unwrap(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
    }
}
fn error_status(kind: FailureKind) -> u16 {
    match kind {
        FailureKind::Forbidden => 403,
        FailureKind::NotFound => 404,
        FailureKind::Conflict => 409,
        FailureKind::InvalidInput => 422,
        _ => panic!("Unexpected review failure kind"),
    }
}
fn result(value: &Value) -> JournalReviewResult {
    if let Some(id) = value["id"].as_str() {
        JournalReviewResult::Posted {
            id: id.into(),
            status: value["status"].as_str().unwrap().into(),
            version: value["version"].as_i64().unwrap(),
        }
    } else {
        JournalReviewResult::Reversed {
            reversal_journal_id: value["reversalJournalId"].as_str().unwrap().into(),
            reversed_journal_id: value["reversedJournalId"].as_str().unwrap().into(),
        }
    }
}
fn command(row: &Value) -> JournalReviewCommand {
    JournalReviewCommand {
        key: CommandKey::new("synthetic-command").unwrap(),
        journal_id: RecordId::new("journal").unwrap(),
        input: if row["action"] == "post" {
            JournalReviewInput::Post {
                expected_version: row["input"]["expectedVersion"].as_i64().unwrap(),
            }
        } else {
            JournalReviewInput::Reverse {
                period_id: row["input"]["periodId"].as_str().unwrap().into(),
                entry_date: row["input"]["entryDate"].as_str().unwrap().into(),
                memo: row["input"]["memo"].as_str().map(str::to_owned),
            }
        },
    }
}

/// Application sequencing doubles only. These do not implement production
/// Identity, purchases, SQL, deferred constraints, or transaction acceptance.
struct Ports {
    row: Value,
    calls: Vec<String>,
    fail_at: Option<&'static str>,
    applied: Option<Value>,
    plan_image: Option<Value>,
    bad_period: bool,
    bad_accounts: bool,
    bad_ledger: bool,
    bad_company: bool,
    bad_opening: bool,
    bad_money_total: bool,
}
impl Ports {
    fn new(row: Value) -> Self {
        Self {
            row,
            calls: vec![],
            fail_at: None,
            applied: None,
            plan_image: None,
            bad_period: false,
            bad_accounts: false,
            bad_ledger: false,
            bad_company: false,
            bad_opening: false,
            bad_money_total: false,
        }
    }
    fn step(&mut self, name: &'static str) -> Result<(), JournalError> {
        self.calls.push(name.into());
        if self.fail_at == Some(name) {
            Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "SYNTHETIC_CURRENT_SOURCE_DENIED",
                message: "Synthetic current source denied.".into(),
            })
        } else {
            Ok(())
        }
    }
    fn allows(&self, permission: &str) -> bool {
        self.row["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == permission)
    }
    fn lines(&self) -> Vec<JournalLedgerLine> {
        let debit = self.row["debit"].as_i64().unwrap();
        vec![
            sample_line(0, debit, 0, 400, 0),
            sample_line(1, 0, debit, 0, if self.bad_ledger { 401 } else { 400 }),
        ]
    }
}
struct Current;
struct MoneyWitness;
impl JournalReviewPorts for Ports {
    type Error = JournalError;
    type Authority = Current;
    type Source = ReviewJournal;
    type MoneyWitness = MoneyWitness;
    async fn lock_sources(
        &mut self,
        _: &FinanceActor,
        _: &JournalReviewCommand,
    ) -> Result<(), Self::Error> {
        self.step("locks")
    }
    async fn authorize_current(
        &mut self,
        _: &FinanceActor,
        _: &JournalReviewCommand,
    ) -> Result<Current, Self::Error> {
        self.step("authorize")?;
        if !self.allows("finance.write") {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "PERMISSION_DENIED",
                message: "Synthetic current authority denied.".into(),
            });
        }
        Ok(Current)
    }
    async fn reviewer(
        &mut self,
        _: &Current,
        source: Option<&ReviewJournal>,
    ) -> Result<JournalReviewerFacts, Self::Error> {
        self.step("reviewer")?;
        Ok(JournalReviewerFacts {
            can_read: self.allows("finance.read"),
            can_approve: self.allows("finance.approve"),
            same_person: source.is_some_and(|s| {
                s.created_by == "reviewer"
                    || self.row["personUserIds"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|id| id == &s.created_by)
            }),
        })
    }
    async fn ready(&mut self, _: &Current) -> Result<(), Self::Error> {
        self.step("ready")
    }
    async fn replay_exact(
        &mut self,
        _: &Current,
        _: &JournalReviewCommand,
    ) -> Result<Option<JournalReviewResult>, Self::Error> {
        self.step("replay")?;
        Ok(self.row.get("replay").filter(|v| !v.is_null()).map(result))
    }
    async fn load_source(
        &mut self,
        _: &Current,
        _: &JournalReviewCommand,
        posted: bool,
    ) -> Result<Option<ReviewJournal>, Self::Error> {
        self.step("source")?;
        Ok(record(&self.row["journal"])
            .filter(|j| !posted || j.status == "posted")
            .map(|mut j| {
                if self.bad_company {
                    j.company_id = "other-company".into();
                }
                j
            }))
    }
    fn journal(source: &ReviewJournal) -> &ReviewJournal {
        source
    }
    async fn posting_facts(
        &mut self,
        _: &Current,
        _: &ReviewJournal,
    ) -> Result<JournalPostingFacts, Self::Error> {
        self.step("posting-facts")?;
        Ok(JournalPostingFacts {
            period_accepts_date: !self.bad_period,
            active_accounts: !self.bad_accounts,
            opening_subledgers_ready: !self.bad_opening,
            lines: self.lines(),
        })
    }
    async fn reversal_facts(
        &mut self,
        _: &Current,
        source: &ReviewJournal,
        _: &JournalReviewCommand,
    ) -> Result<JournalReversalSourceFacts, Self::Error> {
        self.step("reversal-facts")?;
        let decision: JournalReversalFacts =
            serde_json::from_value(self.row["facts"].clone()).unwrap();
        let mut ancestry = vec![JournalMoneyAncestor {
            id: source.id.clone(),
            company_id: source.company_id.clone(),
            depth: 0,
            cycle: false,
            source_type: source.source_type.clone(),
            source_id: source.source_id.clone(),
            currency: source.currency.clone(),
            total: self.row["debit"].as_i64().unwrap().to_string(),
            entry_date: source.entry_date.clone(),
        }];
        if source.source_type == "reversal" {
            ancestry.push(JournalMoneyAncestor {
                id: source.source_id.clone().unwrap(),
                company_id: source.company_id.clone(),
                depth: 1,
                cycle: false,
                source_type: decision.root_source_type.clone().unwrap_or_default(),
                source_id: None,
                currency: source.currency.clone(),
                total: self.row["debit"].as_i64().unwrap().to_string(),
                entry_date: source.entry_date.clone(),
            });
        }
        if self.bad_money_total {
            ancestry[0].total = "1".into();
        }
        Ok(JournalReversalSourceFacts {
            decision,
            period_accepts_date: !self.bad_period,
            lines: self.lines(),
            ancestry,
            money: vec![],
        })
    }
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
    async fn require_source_purchase(
        &mut self,
        _: &Current,
        _: &ReviewJournal,
        _: &JournalReviewCommand,
    ) -> Result<(), Self::Error> {
        self.step("purchase")
    }
    async fn prepare_money(
        &mut self,
        _: &Current,
        _: &ReviewJournal,
        _: &JournalReviewPlan,
    ) -> Result<MoneyWitness, Self::Error> {
        self.step("money")?;
        Ok(MoneyWitness)
    }
    async fn apply_command(
        &mut self,
        _: &Current,
        _: &ReviewJournal,
        _: &JournalReviewCommand,
        plan: &JournalReviewPlan,
        _: &MoneyWitness,
    ) -> Result<(), Self::Error> {
        self.step("apply-command")?;
        self.applied = Some(serde_json::to_value(plan.result()).unwrap());
        self.plan_image = Some(match plan.action() {
            JournalReviewAction::Post {
                debit_total_units, ..
            } => {
                json!({"journalId":plan.journal().id,"debit":debit_total_units.parse::<i64>().unwrap(),"now":plan.now().get(),"opening":plan.opening()})
            }
            JournalReviewAction::Reverse { .. } => {
                json!({"opening":plan.opening(),"now":plan.now().get()})
            }
        });
        Ok(())
    }
    async fn flush_secondary_books(
        &mut self,
        _: &Current,
        _: &ReviewJournal,
    ) -> Result<(), Self::Error> {
        self.step("books")
    }
    async fn retain_source(
        &mut self,
        _: &Current,
        _: &ReviewJournal,
        _: &JournalReviewCommand,
        _: &JournalReviewPlan,
        _: &MoneyWitness,
    ) -> Result<(), Self::Error> {
        self.step("retain")
    }
    async fn require_final_authority(
        &mut self,
        _: &Current,
        _: &JournalReviewCommand,
    ) -> Result<(), Self::Error> {
        self.step("final-authority")
    }
}

#[tokio::test]
async fn original_node_review_decisions_and_results_include_independent_same_person_and_error_order()
 {
    let corpus = corpus();
    assert_eq!(corpus["node"], "v26.11.1");
    assert_eq!(
        corpus["sourceCommit"],
        "a5ed8737a2f05eff1ee9841fda4a094b3926d732"
    );
    let rows = corpus["reviews"].as_array().unwrap();
    assert_eq!(rows.len(), 42);
    for row in rows {
        let mut ports = Ports::new(row.clone());
        let result = review_journal(&mut ports, &actor(), &command(row)).await;
        if let Some(error) = row["expected"].get("error") {
            let result = result.unwrap_err();
            assert_eq!(
                u64::from(error_status(result.kind)),
                error["status"].as_u64().unwrap(),
                "{}",
                row["id"]
            );
            assert_eq!(
                result.code,
                error["code"].as_str().unwrap(),
                "{}",
                row["id"]
            );
            assert_eq!(
                result.message,
                error["message"].as_str().unwrap(),
                "{}",
                row["id"]
            );
            assert!(ports.applied.is_none(), "{}", row["id"]);
        } else {
            let expected = &row["expected"]["value"];
            assert_eq!(
                serde_json::to_value(result.unwrap()).unwrap(),
                expected.get("result").unwrap_or(expected).clone(),
                "{}",
                row["id"]
            );
            if let Some(plan) = expected.get("plan") {
                assert_eq!(ports.plan_image.as_ref().unwrap(), plan, "{}", row["id"]);
            }
            if expected.get("opening").is_some() {
                assert_eq!(
                    ports.plan_image.as_ref().unwrap(),
                    &json!({"opening":expected["opening"],"now":expected["now"]}),
                    "{}",
                    row["id"]
                );
            }
        }
    }
}

#[test]
fn actual_private_node_money_functions_match_exact_selection_sign_and_historical_rate_policy() {
    let corpus = corpus();
    let rows = corpus["money"].as_array().unwrap();
    assert_eq!(rows.len(), 133);
    for row in rows {
        let chain: Vec<JournalMoneyAncestor> =
            serde_json::from_value(row["chain"].clone()).unwrap();
        let money: Vec<RetainedJournalMoney> =
            serde_json::from_value(row["money"].clone()).unwrap();
        let result = plan_journal_reversal_money("company", &chain, &money);
        if let Some(error) = row["expected"].get("error") {
            let actual = result.unwrap_err();
            assert_eq!(
                u64::from(error_status(actual.kind)),
                error["status"].as_u64().unwrap(),
                "{}",
                row["id"]
            );
            assert_eq!(
                actual.code,
                error["code"].as_str().unwrap(),
                "{}",
                row["id"]
            );
            assert_eq!(
                actual.message,
                error["message"].as_str().unwrap(),
                "{}",
                row["id"]
            );
        } else {
            assert_eq!(
                serde_json::to_value(result.unwrap()).unwrap(),
                row["expected"]["value"],
                "{}",
                row["id"]
            );
        }
    }
}

#[tokio::test]
async fn mandatory_current_guards_precede_effects_and_books_then_retention_then_final_authority() {
    let row = corpus()["reviews"][0].clone();
    let mut ports = Ports::new(row.clone());
    review_journal(&mut ports, &actor(), &command(&row))
        .await
        .unwrap();
    assert_eq!(
        ports.calls,
        [
            "locks",
            "authorize",
            "ready",
            "replay",
            "source",
            "reviewer",
            "posting-facts",
            "purchase",
            "money",
            "apply-command",
            "books",
            "retain",
            "final-authority"
        ]
    );
    for phase in [
        "locks",
        "authorize",
        "ready",
        "replay",
        "purchase",
        "money",
        "apply-command",
        "books",
        "retain",
        "final-authority",
    ] {
        let mut ports = Ports::new(row.clone());
        ports.fail_at = Some(phase);
        assert_eq!(
            review_journal(&mut ports, &actor(), &command(&row))
                .await
                .unwrap_err()
                .code,
            "SYNTHETIC_CURRENT_SOURCE_DENIED",
            "{phase}"
        );
        assert_eq!(ports.calls.last().unwrap(), phase);
        // A DB adapter must roll back the caller-owned transaction if any of
        // these post-apply guards fails; this unit double claims no SQL rollback.
    }
}

#[tokio::test]
async fn stale_source_period_accounts_both_currency_sums_and_opening_drafts_cannot_reach_effects() {
    let original = corpus()["reviews"][1].clone();
    for change in 0..6 {
        let mut row = original.clone();
        if change == 5 {
            row["input"]["expectedVersion"] = 2.into();
        }
        let mut ports = Ports::new(row.clone());
        match change {
            0 => ports.bad_company = true,
            1 => ports.bad_period = true,
            2 => ports.bad_accounts = true,
            3 => ports.bad_ledger = true,
            4 => ports.bad_opening = true,
            _ => {}
        }
        assert_eq!(
            review_journal(&mut ports, &actor(), &command(&row))
                .await
                .unwrap_err()
                .code,
            "JOURNAL_REVIEW_SOURCE_CHANGED"
        );
        assert!(ports.applied.is_none());
        assert!(
            !ports
                .calls
                .iter()
                .any(|c| c == "money" || c == "apply-command")
        );
    }
}

#[tokio::test]
async fn exact_replay_never_prepares_new_money_or_owner_effects_and_still_checks_current_authority()
{
    let row = corpus()["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "post-replay-before-independence")
        .unwrap()
        .clone();
    let mut ports = Ports::new(row.clone());
    review_journal(&mut ports, &actor(), &command(&row))
        .await
        .unwrap();
    assert_eq!(
        ports.calls,
        ["locks", "authorize", "ready", "replay", "final-authority"]
    );
    assert!(ports.applied.is_none());
    ports.fail_at = Some("final-authority");
    assert!(
        review_journal(&mut ports, &actor(), &command(&row))
            .await
            .is_err()
    );
}

#[test]
fn reversal_of_recorded_ledger_is_an_involution_without_fx_or_dimension_recalculation() {
    for amount in [1, 2, 99, 100, 101, 10_001, 9_007_199_254_740_991] {
        for base in [0, 1, 7, 99, 10_001, 9_007_199_254_740_991] {
            let lines = vec![
                sample_line(0, amount, 0, base, 0),
                sample_line(1, 0, amount, 0, base),
            ];
            assert!(journal_ledger_balanced(&lines));
            let reverse = inverse_journal_lines(&lines, "reversal");
            assert!(journal_ledger_balanced(&reverse));
            assert_eq!(inverse_journal_lines(&reverse, "journal"), lines);
        }
    }
    let huge = vec![
        sample_line(0, i64::MAX, 0, i64::MAX, 0),
        sample_line(1, i64::MAX, 0, i64::MAX, 0),
        sample_line(2, 0, i64::MAX, 0, i64::MAX),
        sample_line(3, 0, i64::MAX, 0, i64::MAX),
    ];
    assert!(journal_ledger_balanced(&huge));
    assert!(!journal_ledger_balanced(&[]));
    assert!(!journal_ledger_balanced(&[sample_line(0, 1, 0, 1, 0)]));
    assert!(!journal_ledger_balanced(&[
        sample_line(0, 1, 0, 1, 0),
        sample_line(1, 0, 2, 0, 1)
    ]));
    assert!(!journal_ledger_balanced(&[
        sample_line(0, 1, 0, 1, 0),
        sample_line(1, 0, 1, 0, 2)
    ]));
}

#[tokio::test]
async fn reversal_rejects_a_ledger_or_money_total_that_does_not_match_its_retained_source() {
    let row = corpus()["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "reverse-manual-2026-01-01")
        .unwrap()
        .clone();
    for bad_total in [false, true] {
        let mut ports = Ports::new(row.clone());
        ports.bad_ledger = !bad_total;
        ports.bad_money_total = bad_total;
        assert_eq!(
            review_journal(&mut ports, &actor(), &command(&row))
                .await
                .unwrap_err()
                .code,
            "JOURNAL_REVIEW_SOURCE_CHANGED"
        );
        assert!(ports.applied.is_none());
        assert!(!ports.calls.iter().any(|call| call == "money"));
    }
}

#[test]
fn every_available_ancestor_preserves_reversal_sign_including_large_exact_units() {
    let total = BigInt::from(9_007_199_254_740_993_i64);
    for length in 1..=101 {
        let chain: Vec<_> = (0..length)
            .map(|i| JournalMoneyAncestor {
                id: format!("journal-{i}"),
                company_id: "company".into(),
                depth: i,
                cycle: false,
                source_type: if i < length - 1 { "reversal" } else { "manual" }.into(),
                source_id: (i < length - 1).then(|| format!("journal-{}", i + 1)),
                currency: "USD".into(),
                total: total.to_string(),
                entry_date: "2026-01-01".into(),
            })
            .collect();
        let expected: BigInt = &total
            * if (length - 1).is_multiple_of(2) {
                -1
            } else {
                1
            };
        for index in 0..length {
            let amount: BigInt = &total
                * if (length - 1 - index).is_multiple_of(2) {
                    1
                } else {
                    -1
                };
            let row = RetainedJournalMoney {
                company_id: "company".into(),
                source_id: chain[index].id.clone(),
                amount_kind: "debit_total".into(),
                transaction_date: "2026-01-01".into(),
                currency: "USD".into(),
                currency_scale: 2,
                amount_units: amount.to_string(),
                usd_units: amount.to_string(),
                fx_rate_id: None,
                used_latest_fallback: false,
                original_source_type: None,
                original_source_id: None,
                original_amount_kind: None,
            };
            let plan = plan_journal_reversal_money("company", &chain, &[row]).unwrap();
            assert_eq!(plan.amount_units, expected.to_string());
            assert_eq!(plan.usd_units.as_ref().unwrap(), &expected.to_string());
        }
        let fresh = plan_journal_reversal_money("company", &chain, &[]).unwrap();
        assert_eq!(fresh.amount_units, expected.to_string());
        assert_eq!(fresh.mode, JournalReversalMoneyMode::CaptureNew);
        assert!(
            fresh.usd_units.is_none()
                && fresh.fx_rate_id.is_none()
                && fresh.used_latest_fallback.is_none()
        );
    }
}

#[test]
fn impossible_cross_company_duplicate_or_oversized_source_images_are_rejected_at_native_fact_admission()
 {
    let row = corpus()["money"][1].clone();
    let chain: Vec<JournalMoneyAncestor> = serde_json::from_value(row["chain"].clone()).unwrap();
    let money: Vec<RetainedJournalMoney> = serde_json::from_value(row["money"].clone()).unwrap();
    assert!(plan_journal_reversal_money("other-company", &chain, &money).is_err());
    let mut duplicate = money.clone();
    duplicate.extend(money.clone());
    assert!(plan_journal_reversal_money("company", &chain, &duplicate).is_err());
    let mut bad = chain.clone();
    bad[0].total = "9".repeat(129);
    assert!(plan_journal_reversal_money("company", &bad, &[]).is_err());
    let mut bad = chain;
    bad[0].total = "1.2".into();
    assert!(plan_journal_reversal_money("company", &bad, &[]).is_err());
}
