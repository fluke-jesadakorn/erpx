//! Native Benefits command policy and port tests. These are owner unit facts;
//! PostgreSQL and HTTP are proven by the sibling integration targets.
use erpx_server_benefits::public::command::{
    BenefitCommandResult, BenefitPlanInput, BenefitRecordKind, BenefitRecordState,
    BenefitWindowInput, BenefitsCommandActor, ChangeBenefit, CreateDependent, CreateElection,
    CreatePlan, CreateWindow, StoredBenefitReceipt, create_dependent, create_election, create_plan,
    create_window, execute_benefits_command, parse_money_minor,
};
use erpx_server_benefits::public::command::{BenefitsCommandFuture, BenefitsCommandRepository};
use erpx_server_benefits::public::workspace::{
    BenefitPlanRow, BenefitWindowRow, BenefitsEntitlement, BenefitsEntitlementPort, BenefitsFuture,
    BenefitsPeoplePort, BenefitsWorker,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, TimestampMillis};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const NOW: TimestampMillis = TimestampMillis::new(1_790_812_800_000);

fn actor(review: bool) -> BenefitsCommandActor {
    BenefitsCommandActor {
        user_id: "owner".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment".into(),
        role: "benefits_administrator".into(),
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            if review {
                "benefits.review".into()
            } else {
                "benefits.read".into()
            },
        ],
        modules: vec!["benefits".into()],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 60_000,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn worker() -> BenefitsWorker {
    BenefitsWorker {
        person_id: "person-owner".into(),
        engagement_id: "engagement-owner".into(),
    }
}

fn plan() -> BenefitPlanRow {
    BenefitPlanRow {
        id: "plan-published".into(),
        company_id: "company".into(),
        name: "Family medical plan".into(),
        provider: "Synthetic provider".into(),
        coverage_type: "family".into(),
        currency: "THB".into(),
        employee_deduction_minor: 12_550,
        employer_cost_minor: 30_000,
        effective_from: "2026-01-01".into(),
        effective_to: Some("2026-12-31".into()),
        status: "published".into(),
        version: 1,
        created_by: "author".into(),
        reviewed_by: Some("reviewer".into()),
        created_at: 10,
        updated_at: 10,
        required_eligibility_code: None,
    }
}

fn window() -> BenefitWindowRow {
    BenefitWindowRow {
        id: "window-open".into(),
        company_id: "company".into(),
        name: "Annual enrollment".into(),
        opens_on: "2026-01-01".into(),
        closes_on: "2026-12-31".into(),
        effective_on: "2027-01-01".into(),
        status: "open".into(),
        version: 1,
        created_by: "author".into(),
        reviewed_by: Some("reviewer".into()),
        created_at: 10,
        updated_at: 10,
    }
}

struct People {
    worker: Option<BenefitsWorker>,
}

impl BenefitsPeoplePort for People {
    type Error = BusinessError;
    fn worker<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> BenefitsFuture<'a, Option<BenefitsWorker>, Self::Error> {
        Box::pin(async move { Ok(self.worker.clone()) })
    }
    fn employee_names<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a [String],
    ) -> BenefitsFuture<'a, Vec<(String, String)>, Self::Error> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

struct Packages {
    entitlement: BenefitsEntitlement,
}

impl BenefitsEntitlementPort for Packages {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _: &'a str,
    ) -> BenefitsFuture<'a, BenefitsEntitlement, Self::Error> {
        Box::pin(async move { Ok(self.entitlement.clone()) })
    }
}

fn active_packages() -> Packages {
    Packages {
        entitlement: BenefitsEntitlement {
            enforced: true,
            history_allowed: true,
            new_work: true,
            settle_existing: true,
            revision: "1".into(),
        },
    }
}

#[derive(Default)]
struct Repo {
    receipts: BTreeMap<String, StoredBenefitReceipt>,
    records: BTreeMap<(u8, String), BenefitRecordState>,
    plan: Option<BenefitPlanRow>,
    window: Option<BenefitWindowRow>,
    verified: Vec<String>,
    last_plan: Option<BenefitPlanInput>,
    last_window: Option<BenefitWindowInput>,
    last_dependent: Option<(String, String)>,
    last_election: Option<(String, String, i64, i64, Vec<String>)>,
    last_change: Option<(BenefitRecordKind, String, String, String, bool)>,
    lock_calls: usize,
}

impl Repo {
    fn with_sources() -> Self {
        Self {
            plan: Some(plan()),
            window: Some(window()),
            verified: vec!["dependent-verified".into()],
            ..Self::default()
        }
    }

    fn hash(identity: &str) -> String {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in identity.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{hash:064x}")
    }

    fn finish(
        &mut self,
        key: &str,
        operation: &str,
        request_hash: &str,
        result: BenefitCommandResult,
    ) -> BenefitCommandResult {
        self.receipts.insert(
            key.into(),
            StoredBenefitReceipt {
                request_hash: request_hash.into(),
                result: Some(result.clone()),
            },
        );
        let _ = operation;
        result
    }
}

impl BenefitsCommandRepository for Repo {
    type Error = BusinessError;

    fn lock_command<'a>(&'a mut self, _: &'a str) -> BenefitsCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.lock_calls += 1;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> BenefitsCommandFuture<'a, String, Self::Error> {
        Box::pin(async move { Ok(Self::hash(identity)) })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> BenefitsCommandFuture<'a, Option<StoredBenefitReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.receipts.get(key).cloned()) })
    }

    fn record_state<'a>(
        &'a mut self,
        kind: BenefitRecordKind,
        id: &'a str,
    ) -> BenefitsCommandFuture<'a, Option<BenefitRecordState>, Self::Error> {
        Box::pin(async move { Ok(self.records.get(&(kind as u8, id.to_owned())).cloned()) })
    }

    fn plan_source<'a>(
        &'a mut self,
        id: &'a str,
        version: i64,
    ) -> BenefitsCommandFuture<'a, Option<BenefitPlanRow>, Self::Error> {
        Box::pin(async move {
            Ok(self
                .plan
                .clone()
                .filter(|plan| plan.id == id && plan.version == version))
        })
    }

    fn window_source<'a>(
        &'a mut self,
        id: &'a str,
        version: i64,
    ) -> BenefitsCommandFuture<'a, Option<BenefitWindowRow>, Self::Error> {
        Box::pin(async move {
            Ok(self
                .window
                .clone()
                .filter(|window| window.id == id && window.version == version))
        })
    }

    fn verified_dependents<'a>(
        &'a mut self,
        _: &'a str,
    ) -> BenefitsCommandFuture<'a, Vec<String>, Self::Error> {
        Box::pin(async move { Ok(self.verified.clone()) })
    }

    fn create_plan<'a>(
        &'a mut self,
        command: &'a CreatePlan<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.last_plan = Some(command.input.clone());
            let result = BenefitCommandResult::new(command.id, "draft", 1);
            Ok(self.finish(command.key, command.operation, command.request_hash, result))
        })
    }

    fn create_window<'a>(
        &'a mut self,
        command: &'a CreateWindow<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.last_window = Some(command.input.clone());
            let result = BenefitCommandResult::new(command.id, "draft", 1);
            Ok(self.finish(command.key, command.operation, command.request_hash, result))
        })
    }

    fn create_dependent<'a>(
        &'a mut self,
        command: &'a CreateDependent<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.last_dependent = Some((command.person_id.into(), command.input.name.clone()));
            let result = BenefitCommandResult::new(command.id, "pending", 1);
            Ok(self.finish(command.key, command.operation, command.request_hash, result))
        })
    }

    fn create_election<'a>(
        &'a mut self,
        command: &'a CreateElection<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.last_election = Some((
                command.worker.person_id.clone(),
                command.plan.id.clone(),
                command.plan.employee_deduction_minor,
                command.plan.employer_cost_minor,
                command.input.dependent_ids.clone(),
            ));
            let result = BenefitCommandResult::new(command.id, "elected", 1);
            Ok(self.finish(command.key, command.operation, command.request_hash, result))
        })
    }

    fn change<'a>(
        &'a mut self,
        command: &'a ChangeBenefit<'a>,
    ) -> BenefitsCommandFuture<'a, BenefitCommandResult, Self::Error> {
        Box::pin(async move {
            self.last_change = Some((
                command.kind,
                command.id.into(),
                command.from.into(),
                command.to.into(),
                command.release_payroll,
            ));
            let result = BenefitCommandResult::new(
                command.id,
                command.to,
                command.input.expected_version + 1,
            );
            Ok(self.finish(command.key, command.operation, command.request_hash, result))
        })
    }
}

fn plan_input() -> Value {
    json!({
        "entity": "plan",
        "name": "Health Plus",
        "provider": "Synthetic Provider",
        "coverageType": "family",
        "currency": "thb",
        "employeeDeduction": "125.50",
        "employerCost": "300.00",
        "effectiveFrom": "2026-09-01",
        "effectiveTo": null,
    })
}

#[tokio::test]
async fn plan_create_converts_exact_money_and_replays_the_same_key() {
    let mut repo = Repo::with_sources();
    let mut packages = active_packages();
    let command = key("plan-command-1");
    let first = create_plan(
        &actor(true),
        &plan_input(),
        &command,
        &mut repo,
        &mut packages,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        first,
        BenefitCommandResult::new("benefit-plan-plan-command-1", "draft", 1)
    );
    assert_eq!(
        repo.last_plan.as_ref().unwrap().employee_deduction_minor,
        12_550
    );
    assert_eq!(repo.last_plan.as_ref().unwrap().employer_cost_minor, 30_000);
    assert_eq!(repo.last_plan.as_ref().unwrap().currency, "THB");
    let replay = create_plan(
        &actor(true),
        &plan_input(),
        &command,
        &mut repo,
        &mut packages,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(replay, first);
    assert_eq!(repo.lock_calls, 2);
    let mut changed = plan_input();
    changed["name"] = json!("Changed plan");
    let error = create_plan(
        &actor(true),
        &changed,
        &command,
        &mut repo,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
async fn plan_command_rechecks_current_write_and_review_permission_before_any_effect() {
    let mut repo = Repo::with_sources();
    let mut packages = active_packages();
    let error = create_plan(
        &actor(false),
        &plan_input(),
        &key("plan-command-2"),
        &mut repo,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "BENEFITS_REVIEW_REQUIRED");
    assert!(repo.last_plan.is_none());
    assert_eq!(repo.lock_calls, 0);
}

#[tokio::test]
async fn window_create_requires_a_reviewer_and_a_valid_date_range() {
    let mut repo = Repo::with_sources();
    let mut packages = active_packages();
    let mut bad = json!({
        "entity": "window",
        "name": "Closed before open",
        "opensOn": "2026-10-02",
        "closesOn": "2026-10-01",
        "effectiveOn": "2026-11-01",
    });
    let error = create_window(
        &actor(true),
        &bad,
        &key("window-command-1"),
        &mut repo,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "INVALID_WINDOW");
    bad["closesOn"] = json!("2026-10-03");
    let result = create_window(
        &actor(true),
        &bad,
        &key("window-command-1"),
        &mut repo,
        &mut packages,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "draft");
    assert_eq!(
        repo.last_window.as_ref().unwrap().name,
        "Closed before open"
    );
}

#[tokio::test]
async fn dependent_create_requires_a_current_worker_and_persists_its_person() {
    let mut repo = Repo::with_sources();
    let mut packages = active_packages();
    let mut people = People { worker: None };
    let input = json!({
        "entity": "dependent",
        "name": "Family Member",
        "relationship": "child",
        "birthDate": "2018-02-03",
    });
    let error = create_dependent(
        &actor(false),
        &input,
        &key("dependent-command-1"),
        &mut repo,
        &mut people,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "BENEFIT_WORKER_REQUIRED");
    people.worker = Some(worker());
    let result = create_dependent(
        &actor(false),
        &input,
        &key("dependent-command-1"),
        &mut repo,
        &mut people,
        &mut packages,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "pending");
    assert_eq!(
        repo.last_dependent.as_ref().unwrap(),
        &("person-owner".into(), "Family Member".into())
    );
}

#[tokio::test]
async fn election_requires_current_sources_verified_dependents_and_learning_boundary() {
    let mut repo = Repo::with_sources();
    let mut packages = active_packages();
    let mut people = People {
        worker: Some(worker()),
    };
    let mut input = json!({
        "entity": "election",
        "planId": "plan-published",
        "planVersion": 1,
        "windowId": "window-open",
        "windowVersion": 1,
        "dependentIds": ["dependent-verified"],
    });
    let result = create_election(
        &actor(false),
        &input,
        &key("election-command-1"),
        &mut repo,
        &mut people,
        &mut packages,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "elected");
    let observed = repo.last_election.as_ref().unwrap();
    assert_eq!(observed.0, "person-owner");
    assert_eq!(observed.1, "plan-published");
    assert_eq!(observed.2, 12_550);
    assert_eq!(observed.3, 30_000);
    assert_eq!(observed.4, vec!["dependent-verified"]);

    input["dependentIds"] = json!(["dependent-unverified"]);
    let error = create_election(
        &actor(false),
        &input,
        &key("election-command-2"),
        &mut repo,
        &mut people,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "VERIFIED_DEPENDENT_REQUIRED");

    let mut needs_learning = plan();
    needs_learning.coverage_type = "employee".into();
    needs_learning.required_eligibility_code = Some("PROBATION_COMPLETE".into());
    repo.plan = Some(needs_learning);
    input["dependentIds"] = json!([]);
    let error = create_election(
        &actor(false),
        &input,
        &key("election-command-3"),
        &mut repo,
        &mut people,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED");
}

#[tokio::test]
async fn transition_rechecks_status_version_independence_and_employee_ownership() {
    let mut repo = Repo::with_sources();
    let mut packages = active_packages();
    let mut people = People {
        worker: Some(worker()),
    };
    let plan_state = BenefitRecordState {
        id: "plan-draft".into(),
        status: "draft".into(),
        version: 1,
        created_by: "owner".into(),
        acknowledged_by: None,
        person_id: None,
    };
    repo.records.insert(
        (BenefitRecordKind::Plan as u8, plan_state.id.clone()),
        plan_state,
    );
    let publish = json!({
        "entity": "plan",
        "id": "plan-draft",
        "action": "publish",
        "expectedVersion": 1,
        "reason": "Independent terms review",
    });
    let error = change_for_test(
        &actor(true),
        BenefitRecordKind::Plan,
        "plan-draft",
        &publish,
        &key("plan-change-self"),
        &mut repo,
        &mut people,
        &mut packages,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CONFLICT");
    let result = change_for_test(
        &reviewer_actor(),
        BenefitRecordKind::Plan,
        "plan-draft",
        &publish,
        &key("plan-change-review"),
        &mut repo,
        &mut people,
        &mut packages,
    )
    .await
    .unwrap();
    assert_eq!(
        result,
        BenefitCommandResult::new("plan-draft", "published", 2)
    );

    let election_state = BenefitRecordState {
        id: "election-owner".into(),
        status: "elected".into(),
        version: 1,
        created_by: "person-owner".into(),
        acknowledged_by: None,
        person_id: Some("person-other".into()),
    };
    repo.records.insert(
        (BenefitRecordKind::Election as u8, election_state.id.clone()),
        election_state,
    );
    let submit = json!({
        "entity": "election",
        "id": "election-owner",
        "action": "submit",
        "expectedVersion": 1,
        "reason": "Employee confirms own coverage",
    });
    let error = change_for_test(
        &actor(false),
        BenefitRecordKind::Election,
        "election-owner",
        &submit,
        &key("election-submit-other"),
        &mut repo,
        &mut people,
        &mut packages,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "BENEFIT_ELECTION_OWNER_REQUIRED");
}

#[allow(clippy::too_many_arguments)]
async fn change_for_test(
    actor: &BenefitsCommandActor,
    kind: BenefitRecordKind,
    id: &str,
    input: &Value,
    key: &CommandKey,
    repo: &mut Repo,
    people: &mut People,
    packages: &mut Packages,
) -> Result<BenefitCommandResult, BusinessError> {
    erpx_server_benefits::public::command::change(
        actor, kind, id, input, key, repo, people, packages, NOW,
    )
    .await
}

fn reviewer_actor() -> BenefitsCommandActor {
    let mut actor = actor(true);
    actor.user_id = "reviewer".into();
    actor
}

#[tokio::test]
async fn release_requires_payroll_handoff_and_independent_provider_acknowledgement() {
    let mut repo = Repo::with_sources();
    let mut packages = active_packages();
    let mut people = People {
        worker: Some(worker()),
    };
    let state = BenefitRecordState {
        id: "election-owner".into(),
        status: "acknowledged".into(),
        version: 3,
        created_by: "person-owner".into(),
        acknowledged_by: Some("acknowledger".into()),
        person_id: Some("person-owner".into()),
    };
    repo.records
        .insert((BenefitRecordKind::Election as u8, state.id.clone()), state);
    let release = json!({
        "entity": "election",
        "id": "election-owner",
        "action": "release",
        "expectedVersion": 3,
        "reason": "Payroll amount and provider receipt reconciled",
    });
    let mut acknowledger = actor(true);
    acknowledger.user_id = "acknowledger".into();
    let error = change_for_test(
        &acknowledger,
        BenefitRecordKind::Election,
        "election-owner",
        &release,
        &key("release-same-acknowledger"),
        &mut repo,
        &mut people,
        &mut packages,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CONFLICT");
    let result = change_for_test(
        &reviewer_actor(),
        BenefitRecordKind::Election,
        "election-owner",
        &release,
        &key("release-independent"),
        &mut repo,
        &mut people,
        &mut packages,
    )
    .await
    .unwrap();
    assert_eq!(
        result,
        BenefitCommandResult::new("election-owner", "released", 4)
    );
    assert!(repo.last_change.as_ref().unwrap().4);
}

#[tokio::test]
async fn draining_and_inactive_packages_block_new_work_and_settlement_separately() {
    let mut repo = Repo::with_sources();
    let mut packages = Packages {
        entitlement: BenefitsEntitlement {
            enforced: true,
            history_allowed: true,
            new_work: false,
            settle_existing: true,
            revision: "1".into(),
        },
    };
    let error = create_plan(
        &actor(true),
        &plan_input(),
        &key("plan-draining-1"),
        &mut repo,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PACKAGE_DRAINING");
    assert!(repo.last_plan.is_none());

    let state = BenefitRecordState {
        id: "plan-published".into(),
        status: "published".into(),
        version: 2,
        created_by: "author".into(),
        acknowledged_by: None,
        person_id: None,
    };
    repo.records
        .insert((BenefitRecordKind::Plan as u8, state.id.clone()), state);
    let retire = json!({
        "entity": "plan",
        "id": "plan-published",
        "action": "retire",
        "expectedVersion": 2,
        "reason": "Existing coverage settles under a draining package",
    });
    let mut people = People {
        worker: Some(worker()),
    };
    let retired = change_for_test(
        &reviewer_actor(),
        BenefitRecordKind::Plan,
        "plan-published",
        &retire,
        &key("plan-retire-draining"),
        &mut repo,
        &mut people,
        &mut packages,
    )
    .await
    .unwrap();
    assert_eq!(
        retired,
        BenefitCommandResult::new("plan-published", "retired", 3)
    );

    packages.entitlement.settle_existing = false;
    let state = BenefitRecordState {
        id: "plan-other".into(),
        status: "published".into(),
        version: 2,
        created_by: "author".into(),
        acknowledged_by: None,
        person_id: None,
    };
    repo.records
        .insert((BenefitRecordKind::Plan as u8, state.id.clone()), state);
    let error = change_for_test(
        &reviewer_actor(),
        BenefitRecordKind::Plan,
        "plan-other",
        &json!({
            "entity": "plan",
            "id": "plan-other",
            "action": "retire",
            "expectedVersion": 2,
            "reason": "Inactive package must retain history only",
        }),
        &key("plan-retire-inactive"),
        &mut repo,
        &mut people,
        &mut packages,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PACKAGE_INACTIVE");
}

#[tokio::test]
async fn envelope_dispatch_rejects_an_unregistered_entity_and_missing_worker_scope() {
    let mut repo = Repo::with_sources();
    let mut packages = active_packages();
    let mut people = People {
        worker: Some(worker()),
    };
    let error = execute_benefits_command(
        &actor(true),
        &json!({"entity": "claim"}),
        &key("unknown-entity"),
        &mut repo,
        &mut people,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "BENEFIT_ENTITY_INVALID");
    assert_eq!(parse_money_minor("125.50").unwrap(), 12_550);
    let error = execute_benefits_command(
        &actor(true),
        &json!({"entity": "election", "planId": "plan-published", "planVersion": 1}),
        &key("invalid-election"),
        &mut repo,
        &mut people,
        &mut packages,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "BENEFITS_VALIDATION_FAILED");
}
