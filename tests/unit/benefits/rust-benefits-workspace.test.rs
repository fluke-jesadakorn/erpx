//! Native Benefits enrollment workspace policy, row, bound and ordering tests.
//! These are owner unit facts, not HTTP, PostgreSQL, command or browser proof.
use erpx_server_benefits::public::workspace::{
    BenefitDependentRow, BenefitElectionRow, BenefitPlanRow, BenefitWindowRow, BenefitsActor,
    BenefitsEntitlement, BenefitsEntitlementPort, BenefitsFuture, BenefitsPeoplePort,
    BenefitsWorker, BenefitsWorkspaceRepository, DEPENDENT_LIMIT, ELECTION_LIMIT, PLAN_LIMIT,
    WINDOW_LIMIT, benefits_workspace,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> BenefitsActor {
    BenefitsActor {
        user_id: "owner".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "benefits.review".into(),
            "benefits.documents.manage".into(),
        ],
        modules: vec!["benefits".into()],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 60_000,
    }
}

fn worker() -> BenefitsWorker {
    BenefitsWorker {
        person_id: "person-owner".into(),
        engagement_id: "engagement-owner".into(),
    }
}

fn plan(id: &str, updated_at: i64) -> BenefitPlanRow {
    BenefitPlanRow {
        id: id.into(),
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
        created_by: "owner".into(),
        reviewed_by: Some("reviewer".into()),
        created_at: 10,
        updated_at,
        required_eligibility_code: Some("PROBATION_COMPLETE".into()),
    }
}

fn window(id: &str, updated_at: i64) -> BenefitWindowRow {
    BenefitWindowRow {
        id: id.into(),
        company_id: "company".into(),
        name: "Annual enrollment".into(),
        opens_on: "2026-11-01".into(),
        closes_on: "2026-11-30".into(),
        effective_on: "2027-01-01".into(),
        status: "open".into(),
        version: 1,
        created_by: "owner".into(),
        reviewed_by: Some("reviewer".into()),
        created_at: 10,
        updated_at,
    }
}

fn dependent(id: &str, person_id: &str, updated_at: i64) -> BenefitDependentRow {
    BenefitDependentRow {
        id: id.into(),
        company_id: "company".into(),
        person_id: person_id.into(),
        name: "Synthetic dependent".into(),
        relationship: "child".into(),
        birth_date: "2018-01-01".into(),
        status: "verified".into(),
        version: 1,
        created_by: "owner".into(),
        reviewed_by: Some("reviewer".into()),
        created_at: 10,
        updated_at,
    }
}

fn election(id: &str, person_id: &str, updated_at: i64) -> BenefitElectionRow {
    BenefitElectionRow {
        id: id.into(),
        company_id: "company".into(),
        person_id: person_id.into(),
        engagement_id: "engagement-owner".into(),
        plan_id: "plan-family".into(),
        plan_name: "Family medical plan".into(),
        employee_name: String::new(),
        plan_version: 1,
        window_id: "window-annual".into(),
        window_version: 1,
        coverage_type: "family".into(),
        currency: "THB".into(),
        employee_deduction_minor: 12_550,
        employer_cost_minor: 30_000,
        effective_from: "2027-01-01".into(),
        effective_to: Some("2027-12-31".into()),
        status: "elected".into(),
        version: 1,
        created_at: 10,
        updated_at,
    }
}

struct Repo {
    plans: Vec<BenefitPlanRow>,
    windows: Vec<BenefitWindowRow>,
    dependents: Vec<BenefitDependentRow>,
    elections: Vec<BenefitElectionRow>,
    error: Option<BusinessError>,
    plan_calls: usize,
    window_calls: usize,
    dependent_calls: usize,
    election_calls: usize,
    last_company: String,
    last_person: Option<String>,
    last_reviewer: bool,
    last_plan_limit: usize,
    last_window_limit: usize,
    last_dependent_limit: usize,
    last_election_limit: usize,
}

impl Repo {
    fn new(
        plans: Vec<BenefitPlanRow>,
        windows: Vec<BenefitWindowRow>,
        dependents: Vec<BenefitDependentRow>,
        elections: Vec<BenefitElectionRow>,
    ) -> Self {
        Self {
            plans,
            windows,
            dependents,
            elections,
            error: None,
            plan_calls: 0,
            window_calls: 0,
            dependent_calls: 0,
            election_calls: 0,
            last_company: String::new(),
            last_person: None,
            last_reviewer: false,
            last_plan_limit: 0,
            last_window_limit: 0,
            last_dependent_limit: 0,
            last_election_limit: 0,
        }
    }

    fn valid() -> Self {
        Self::new(
            vec![plan("plan-family", 20)],
            vec![window("window-annual", 30)],
            vec![dependent("dependent-one", "person-owner", 40)],
            vec![election("election-owner", "person-owner", 50)],
        )
    }
}

impl BenefitsWorkspaceRepository for Repo {
    type Error = BusinessError;

    fn plans<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitPlanRow>, Self::Error> {
        self.plan_calls += 1;
        self.last_company = company.as_str().into();
        self.last_plan_limit = limit;
        Box::pin(async move {
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            Ok(self.plans.clone())
        })
    }

    fn windows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitWindowRow>, Self::Error> {
        self.window_calls += 1;
        self.last_company = company.as_str().into();
        self.last_window_limit = limit;
        Box::pin(async move {
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            Ok(self.windows.clone())
        })
    }

    fn dependents<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitDependentRow>, Self::Error> {
        self.dependent_calls += 1;
        self.last_company = company.as_str().into();
        self.last_person = Some(person_id.into());
        self.last_dependent_limit = limit;
        Box::pin(async move {
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            Ok(self.dependents.clone())
        })
    }

    fn elections<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: Option<&'a str>,
        reviewer: bool,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitElectionRow>, Self::Error> {
        self.election_calls += 1;
        self.last_company = company.as_str().into();
        self.last_person = person_id.map(str::to_owned);
        self.last_reviewer = reviewer;
        self.last_election_limit = limit;
        Box::pin(async move {
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            Ok(self.elections.clone())
        })
    }
}

struct People {
    worker: Option<BenefitsWorker>,
    names: Vec<(String, String)>,
    error: Option<BusinessError>,
    worker_calls: usize,
    name_calls: usize,
    last_ids: Vec<String>,
}

impl People {
    fn new(worker: Option<BenefitsWorker>, names: Vec<(String, String)>) -> Self {
        Self {
            worker,
            names,
            error: None,
            worker_calls: 0,
            name_calls: 0,
            last_ids: Vec::new(),
        }
    }
}

impl BenefitsPeoplePort for People {
    type Error = BusinessError;

    fn worker<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> BenefitsFuture<'a, Option<BenefitsWorker>, Self::Error> {
        self.worker_calls += 1;
        Box::pin(async move {
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            Ok(self.worker.clone())
        })
    }

    fn employee_names<'a>(
        &'a mut self,
        _: &'a CompanyId,
        person_ids: &'a [String],
    ) -> BenefitsFuture<'a, Vec<(String, String)>, Self::Error> {
        self.name_calls += 1;
        self.last_ids = person_ids.to_vec();
        Box::pin(async move {
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            Ok(self.names.clone())
        })
    }
}

struct Packages {
    entitlement: BenefitsEntitlement,
    calls: usize,
}

impl BenefitsEntitlementPort for Packages {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _: &'a str,
    ) -> BenefitsFuture<'a, BenefitsEntitlement, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.entitlement.clone()) })
    }
}

fn allowed(enforced: bool) -> Packages {
    Packages {
        entitlement: BenefitsEntitlement {
            enforced,
            history_allowed: true,
            new_work: true,
            settle_existing: false,
            revision: "7".into(),
        },
        calls: 0,
    }
}

async fn run(
    actor: &BenefitsActor,
    personal: bool,
    repo: &mut Repo,
    people: &mut People,
    packages: &mut Packages,
) -> Result<erpx_server_benefits::public::workspace::BenefitsWorkspace, BusinessError> {
    benefits_workspace(actor, personal, repo, people, packages, NOW).await
}

fn standard_people() -> People {
    People::new(
        Some(worker()),
        vec![("person-owner".into(), "Synthetic Owner".into())],
    )
}

#[tokio::test]
async fn current_permission_module_context_and_package_precede_worker_and_source() {
    for (current, code) in [
        (
            {
                let mut value = actor();
                value.permissions.clear();
                value
            },
            "BENEFITS_PERMISSION_REQUIRED",
        ),
        (
            {
                let mut value = actor();
                value.modules.clear();
                value
            },
            "BENEFITS_MODULE_FORBIDDEN",
        ),
        (
            {
                let mut value = actor();
                value.expires_at = NOW.get();
                value
            },
            "WORK_CONTEXT_CHANGED",
        ),
        (
            {
                let mut value = actor();
                value.work_context = "not-a-context".into();
                value
            },
            "BENEFITS_SOURCE_INVALID",
        ),
    ] {
        let mut repo = Repo::valid();
        let mut people = standard_people();
        let mut packages = allowed(true);
        let error = run(&current, false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(repo.plan_calls, 0);
        assert_eq!(repo.election_calls, 0);
        assert_eq!(people.worker_calls, 0);
        assert_eq!(packages.calls, 0);
    }

    let mut repo = Repo::valid();
    let mut people = standard_people();
    let mut packages = Packages {
        entitlement: BenefitsEntitlement {
            enforced: true,
            history_allowed: false,
            new_work: false,
            settle_existing: false,
            revision: "9".into(),
        },
        calls: 0,
    };
    let error = run(&actor(), false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PACKAGE_REQUIRED");
    assert_eq!(repo.plan_calls, 0);
    assert_eq!(people.worker_calls, 0);
}

#[tokio::test]
async fn reviewer_employee_and_personal_scopes_choose_the_exact_repository_scope() {
    let reviewer = Repo::new(
        vec![],
        vec![],
        vec![dependent("dependent-review", "person-owner", 40)],
        vec![
            election("election-other", "person-other", 60),
            election("election-owner", "person-owner", 50),
        ],
    );
    let mut repo = reviewer;
    let mut people = People::new(
        Some(worker()),
        vec![
            ("person-owner".into(), "Synthetic Owner".into()),
            ("person-other".into(), "Synthetic Other".into()),
        ],
    );
    let mut packages = allowed(false);
    let workspace = run(&actor(), false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap();
    assert!(repo.last_reviewer);
    assert_eq!(repo.last_person, None);
    assert_eq!(workspace.elections.len(), 2);

    let mut repo = Repo::valid();
    let mut people = standard_people();
    let mut packages = allowed(false);
    let employee = {
        let mut value = actor();
        value.permissions = vec!["records.read".into(), "records.write".into()];
        value
    };
    let workspace = run(&employee, false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap();
    assert!(!repo.last_reviewer);
    assert_eq!(repo.last_person.as_deref(), Some("person-owner"));
    assert!(!workspace.can_review);

    let mut repo = Repo::valid();
    let mut people = standard_people();
    let mut packages = allowed(false);
    run(&actor(), true, &mut repo, &mut people, &mut packages)
        .await
        .unwrap();
    assert!(!repo.last_reviewer);
    assert_eq!(repo.last_person.as_deref(), Some("person-owner"));
}

#[tokio::test]
async fn missing_worker_skips_dependents_and_scopes_elections_to_no_person() {
    let mut repo = Repo::new(
        vec![plan("plan-family", 20)],
        vec![window("window-annual", 30)],
        vec![],
        vec![],
    );
    let mut people = People::new(None, vec![]);
    let mut packages = allowed(false);
    let mut employee = actor();
    employee.permissions = vec!["records.read".into(), "records.write".into()];
    let workspace = run(&employee, false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap();
    assert_eq!(repo.dependent_calls, 0);
    assert_eq!(repo.election_calls, 1);
    assert_eq!(repo.last_person, None);
    assert!(!repo.last_reviewer);
    assert_eq!(people.name_calls, 0);
    assert!(workspace.dependents.is_empty());
    assert!(workspace.elections.is_empty());
}

#[tokio::test]
async fn valid_workspace_projects_exact_flags_context_rows_and_purchase() {
    let mut repo = Repo::valid();
    let mut people = standard_people();
    let mut packages = allowed(true);
    let workspace = run(&actor(), false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap();
    assert_eq!(workspace.actor_id, "owner");
    assert!(workspace.can_write);
    assert!(workspace.can_review);
    assert!(workspace.can_manage_documents);
    assert_eq!(workspace.work_context, "a".repeat(64));
    assert_eq!(workspace.plans.len(), 1);
    assert_eq!(workspace.windows.len(), 1);
    assert_eq!(workspace.dependents.len(), 1);
    assert_eq!(workspace.elections.len(), 1);
    assert_eq!(workspace.elections[0].employee_name, "Synthetic Owner");
    assert_eq!(
        workspace.plans[0].required_eligibility_code.as_deref(),
        Some("PROBATION_COMPLETE")
    );
    assert_eq!(
        workspace.purchase,
        Some(
            erpx_server_benefits::public::workspace::BenefitsPurchaseControls {
                new_work: true,
                settle_existing: false,
                history: true,
                revision: "7".into(),
            }
        )
    );
    let wire = serde_json::to_value(&workspace).unwrap();
    assert_eq!(wire["actorId"], "owner");
    assert_eq!(wire["canManageDocuments"], true);
    assert_eq!(wire["workContext"], "a".repeat(64));
    assert_eq!(wire["purchase"]["newWork"], true);
    assert_eq!(wire["purchase"]["settleExisting"], false);
    assert_eq!(wire["purchase"]["history"], true);
    assert_eq!(wire["plans"][0]["company_id"], "company");
    assert_eq!(wire["plans"][0]["employee_deduction_minor"], 12_550);
    assert!(wire.get("dependents").is_some());
}

#[tokio::test]
async fn plan_rows_reject_foreign_scope_and_every_invalid_shape() {
    let cases: Vec<(&str, BenefitPlanRow)> = vec![
        ("foreign company", {
            let mut row = plan("plan-family", 20);
            row.company_id = "foreign".into();
            row
        }),
        ("empty id", {
            let mut row = plan("plan-family", 20);
            row.id = String::new();
            row
        }),
        ("short name", {
            let mut row = plan("plan-family", 20);
            row.name = " A ".into();
            row
        }),
        ("short provider", {
            let mut row = plan("plan-family", 20);
            row.provider = "P".into();
            row
        }),
        ("coverage", {
            let mut row = plan("plan-family", 20);
            row.coverage_type = "spouse".into();
            row
        }),
        ("currency", {
            let mut row = plan("plan-family", 20);
            row.currency = "thb".into();
            row
        }),
        ("employee money", {
            let mut row = plan("plan-family", 20);
            row.employee_deduction_minor = -1;
            row
        }),
        ("employer money", {
            let mut row = plan("plan-family", 20);
            row.employer_cost_minor = -1;
            row
        }),
        ("invalid date", {
            let mut row = plan("plan-family", 20);
            row.effective_from = "2026-02-30".into();
            row
        }),
        ("reversed effective range", {
            let mut row = plan("plan-family", 20);
            row.effective_to = Some("2025-12-31".into());
            row
        }),
        ("status", {
            let mut row = plan("plan-family", 20);
            row.status = "archived".into();
            row
        }),
        ("version", {
            let mut row = plan("plan-family", 20);
            row.version = 0;
            row
        }),
        ("created by", {
            let mut row = plan("plan-family", 20);
            row.created_by = String::new();
            row
        }),
        ("reviewed by", {
            let mut row = plan("plan-family", 20);
            row.reviewed_by = Some(String::new());
            row
        }),
        ("created at", {
            let mut row = plan("plan-family", 20);
            row.created_at = -1;
            row
        }),
        ("eligibility", {
            let mut row = plan("plan-family", 20);
            row.required_eligibility_code = Some(" ".into());
            row
        }),
    ];
    for (label, row) in cases {
        let mut repo = Repo::new(vec![row], vec![], vec![], vec![]);
        let mut people = standard_people();
        let mut packages = allowed(false);
        let error = run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err();
        assert_eq!(error.code, "BENEFITS_SOURCE_INVALID", "case: {label}");
    }
}

#[tokio::test]
async fn plan_duplicates_order_and_bound_fail_closed() {
    for (label, plans) in [
        (
            "duplicate",
            vec![plan("plan-family", 20), plan("plan-family", 10)],
        ),
        (
            "ascending",
            vec![plan("plan-old", 10), plan("plan-new", 20)],
        ),
        (
            "same timestamp descending id",
            vec![plan("plan-b", 20), plan("plan-a", 20)],
        ),
    ] {
        let mut repo = Repo::new(plans, vec![], vec![], vec![]);
        let mut people = standard_people();
        let mut packages = allowed(false);
        let error = run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err();
        assert_eq!(error.code, "BENEFITS_SOURCE_INVALID", "case: {label}");
    }

    let plans = (0..=PLAN_LIMIT)
        .map(|index| plan(&format!("plan-{index:04}"), index as i64))
        .collect::<Vec<_>>();
    let mut repo = Repo::new(plans, vec![], vec![], vec![]);
    let mut people = standard_people();
    let mut packages = allowed(false);
    let error = run(&actor(), false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap_err();
    assert_eq!(error.code, "BENEFITS_WORKSPACE_TOO_LARGE");
    assert_eq!(repo.last_plan_limit, PLAN_LIMIT + 1);
}

#[tokio::test]
async fn window_rows_reject_invalid_shapes_order_and_bound() {
    for (label, row) in [
        ("foreign company", {
            let mut row = window("window-annual", 30);
            row.company_id = "foreign".into();
            row
        }),
        ("short name", {
            let mut row = window("window-annual", 30);
            row.name = " A ".into();
            row
        }),
        ("invalid opens", {
            let mut row = window("window-annual", 30);
            row.opens_on = "2026-13-01".into();
            row
        }),
        ("reversed window", {
            let mut row = window("window-annual", 30);
            row.closes_on = "2026-10-31".into();
            row
        }),
        ("invalid effective", {
            let mut row = window("window-annual", 30);
            row.effective_on = "2026-02-30".into();
            row
        }),
        ("status", {
            let mut row = window("window-annual", 30);
            row.status = "published".into();
            row
        }),
        ("version", {
            let mut row = window("window-annual", 30);
            row.version = 0;
            row
        }),
        ("reviewed by", {
            let mut row = window("window-annual", 30);
            row.reviewed_by = Some(String::new());
            row
        }),
    ] {
        let mut repo = Repo::new(vec![], vec![row], vec![], vec![]);
        let mut people = standard_people();
        let mut packages = allowed(false);
        let error = run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err();
        assert_eq!(error.code, "BENEFITS_SOURCE_INVALID", "case: {label}");
    }

    let mut repo = Repo::new(
        vec![],
        vec![window("window-old", 20), window("window-new", 30)],
        vec![],
        vec![],
    );
    let mut people = standard_people();
    let mut packages = allowed(false);
    assert_eq!(
        run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err()
            .code,
        "BENEFITS_SOURCE_INVALID"
    );

    let windows = (0..=WINDOW_LIMIT)
        .map(|index| window(&format!("window-{index:04}"), index as i64))
        .collect::<Vec<_>>();
    let mut repo = Repo::new(vec![], windows, vec![], vec![]);
    let mut people = standard_people();
    let mut packages = allowed(false);
    assert_eq!(
        run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err()
            .code,
        "BENEFITS_WORKSPACE_TOO_LARGE"
    );
    assert_eq!(repo.last_window_limit, WINDOW_LIMIT + 1);
}

#[tokio::test]
async fn dependent_rows_reject_foreign_scope_and_invalid_shapes() {
    for (label, row) in [
        ("foreign company", {
            let mut row = dependent("dependent-one", "person-owner", 40);
            row.company_id = "foreign".into();
            row
        }),
        ("another worker", {
            let mut row = dependent("dependent-one", "person-other", 40);
            row.person_id = "person-other".into();
            row
        }),
        ("short name", {
            let mut row = dependent("dependent-one", "person-owner", 40);
            row.name = " A ".into();
            row
        }),
        ("short relationship", {
            let mut row = dependent("dependent-one", "person-owner", 40);
            row.relationship = "C".into();
            row
        }),
        ("birth date", {
            let mut row = dependent("dependent-one", "person-owner", 40);
            row.birth_date = "2021-02-29".into();
            row
        }),
        ("status", {
            let mut row = dependent("dependent-one", "person-owner", 40);
            row.status = "active".into();
            row
        }),
        ("version", {
            let mut row = dependent("dependent-one", "person-owner", 40);
            row.version = 0;
            row
        }),
        ("created by", {
            let mut row = dependent("dependent-one", "person-owner", 40);
            row.created_by = String::new();
            row
        }),
    ] {
        let mut repo = Repo::new(vec![], vec![], vec![row], vec![]);
        let mut people = standard_people();
        let mut packages = allowed(false);
        let error = run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err();
        assert_eq!(error.code, "BENEFITS_SOURCE_INVALID", "case: {label}");
    }

    let dependents = (0..=DEPENDENT_LIMIT)
        .map(|index| {
            dependent(
                &format!("dependent-{index:04}"),
                "person-owner",
                index as i64,
            )
        })
        .collect::<Vec<_>>();
    let mut repo = Repo::new(vec![], vec![], dependents, vec![]);
    let mut people = standard_people();
    let mut packages = allowed(false);
    assert_eq!(
        run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err()
            .code,
        "BENEFITS_WORKSPACE_TOO_LARGE"
    );
    assert_eq!(repo.last_dependent_limit, DEPENDENT_LIMIT + 1);
}

#[tokio::test]
async fn election_rows_reject_foreign_scope_invalid_shapes_order_and_bound() {
    for (label, row, personal) in [
        (
            "foreign company",
            {
                let mut row = election("election-owner", "person-owner", 50);
                row.company_id = "foreign".into();
                row
            },
            true,
        ),
        (
            "another worker",
            {
                let mut row = election("election-owner", "person-other", 50);
                row.person_id = "person-other".into();
                row
            },
            true,
        ),
        (
            "plan name",
            {
                let mut row = election("election-owner", "person-owner", 50);
                row.plan_name = " ".into();
                row
            },
            true,
        ),
        (
            "coverage",
            {
                let mut row = election("election-owner", "person-owner", 50);
                row.coverage_type = "spouse".into();
                row
            },
            true,
        ),
        (
            "currency",
            {
                let mut row = election("election-owner", "person-owner", 50);
                row.currency = "THB1".into();
                row
            },
            true,
        ),
        (
            "money",
            {
                let mut row = election("election-owner", "person-owner", 50);
                row.employer_cost_minor = -1;
                row
            },
            true,
        ),
        (
            "effective range",
            {
                let mut row = election("election-owner", "person-owner", 50);
                row.effective_to = Some("2026-12-31".into());
                row
            },
            true,
        ),
        (
            "status",
            {
                let mut row = election("election-owner", "person-owner", 50);
                row.status = "draft".into();
                row
            },
            true,
        ),
        (
            "version",
            {
                let mut row = election("election-owner", "person-owner", 50);
                row.version = 0;
                row
            },
            true,
        ),
        (
            "plan version",
            {
                let mut row = election("election-owner", "person-owner", 50);
                row.plan_version = 0;
                row
            },
            true,
        ),
        (
            "reviewer foreign worker",
            {
                let mut row = election("election-other", "person-other", 50);
                row.person_id = "person-other".into();
                row
            },
            false,
        ),
    ] {
        let mut repo = Repo::new(vec![], vec![], vec![], vec![row]);
        let mut people = People::new(
            Some(worker()),
            vec![("person-owner".into(), "Synthetic Owner".into())],
        );
        let mut packages = allowed(false);
        let error = run(&actor(), personal, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err();
        assert_eq!(error.code, "BENEFITS_SOURCE_INVALID", "case: {label}");
        assert_eq!(repo.last_reviewer, !personal, "case: {label}");
    }

    let mut repo = Repo::new(
        vec![],
        vec![],
        vec![],
        vec![
            election("election-old", "person-owner", 40),
            election("election-new", "person-owner", 50),
        ],
    );
    let mut people = standard_people();
    let mut packages = allowed(false);
    assert_eq!(
        run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err()
            .code,
        "BENEFITS_SOURCE_INVALID"
    );

    let elections = (0..=ELECTION_LIMIT)
        .map(|index| {
            election(
                &format!("election-{index:04}"),
                "person-owner",
                index as i64,
            )
        })
        .collect::<Vec<_>>();
    let mut repo = Repo::new(vec![], vec![], vec![], elections);
    let mut people = standard_people();
    let mut packages = allowed(false);
    assert_eq!(
        run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err()
            .code,
        "BENEFITS_WORKSPACE_TOO_LARGE"
    );
    assert_eq!(repo.last_election_limit, ELECTION_LIMIT + 1);
}

#[tokio::test]
async fn employee_names_are_requested_once_for_unique_people_and_must_be_complete() {
    let mut repo = Repo::new(
        vec![],
        vec![],
        vec![],
        vec![
            election("election-a", "person-a", 60),
            election("election-b", "person-b", 50),
            election("election-c", "person-a", 40),
        ],
    );
    let mut people = People::new(
        Some(worker()),
        vec![
            ("person-a".into(), "Synthetic A".into()),
            ("person-b".into(), "Synthetic B".into()),
        ],
    );
    let mut packages = allowed(false);
    let workspace = run(&actor(), false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap();
    assert_eq!(people.name_calls, 1);
    assert_eq!(
        people.last_ids,
        vec!["person-a".to_string(), "person-b".to_string()]
    );
    assert_eq!(workspace.elections[0].employee_name, "Synthetic A");
    assert_eq!(workspace.elections[1].employee_name, "Synthetic B");
    assert_eq!(workspace.elections[2].employee_name, "Synthetic A");

    for (label, names) in [
        ("missing", vec![("person-a".into(), "Synthetic A".into())]),
        (
            "extra",
            vec![
                ("person-a".into(), "Synthetic A".into()),
                ("person-b".into(), "Synthetic B".into()),
                ("person-c".into(), "Synthetic C".into()),
            ],
        ),
        (
            "duplicate",
            vec![
                ("person-a".into(), "Synthetic A".into()),
                ("person-a".into(), "Synthetic A".into()),
                ("person-b".into(), "Synthetic B".into()),
            ],
        ),
        (
            "invalid name",
            vec![
                ("person-a".into(), " ".into()),
                ("person-b".into(), "Synthetic B".into()),
            ],
        ),
    ] {
        let mut repo = Repo::new(
            vec![],
            vec![],
            vec![],
            vec![
                election("election-a", "person-a", 60),
                election("election-b", "person-b", 50),
            ],
        );
        let mut people = People::new(Some(worker()), names);
        let mut packages = allowed(false);
        let error = run(&actor(), false, &mut repo, &mut people, &mut packages)
            .await
            .unwrap_err();
        assert_eq!(error.code, "BENEFITS_SOURCE_INVALID", "case: {label}");
    }
}

#[tokio::test]
async fn repository_and_people_failures_stop_the_read_at_their_boundary() {
    let mut repo = Repo::valid();
    repo.error = Some(BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_SOURCE_INVALID",
        "The current Benefits source is invalid.",
    ));
    let mut people = standard_people();
    let mut packages = allowed(false);
    let error = run(&actor(), false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap_err();
    assert_eq!(error.code, "BENEFITS_SOURCE_INVALID");
    assert_eq!(people.worker_calls, 1);
    assert_eq!(people.name_calls, 0);

    let mut repo = Repo::valid();
    let mut people = standard_people();
    people.error = Some(BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_PEOPLE_SOURCE_INVALID",
        "The current People source is invalid.",
    ));
    let mut packages = allowed(false);
    let error = run(&actor(), false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap_err();
    assert_eq!(error.code, "BENEFITS_PEOPLE_SOURCE_INVALID");
    assert_eq!(repo.plan_calls, 0);
}

#[tokio::test]
async fn disabled_enforcement_omits_purchase_controls() {
    let mut repo = Repo::valid();
    let mut people = standard_people();
    let mut packages = allowed(false);
    let workspace = run(&actor(), false, &mut repo, &mut people, &mut packages)
        .await
        .unwrap();
    assert!(workspace.purchase.is_none());
    let wire = serde_json::to_value(&workspace).unwrap();
    assert!(wire["purchase"].is_null());
}
#[test]
fn exact_row_defaults_are_well_formed() {
    let actor = actor();
    assert!(plan("plan-family", 20).validate(&actor).is_ok());
    assert!(window("window-annual", 30).validate(&actor).is_ok());
    assert!(
        dependent("dependent-one", "person-owner", 40)
            .validate(&actor, Some(&worker()))
            .is_ok()
    );
    let mut row = election("election-owner", "person-owner", 50);
    row.employee_name = "Synthetic Owner".into();
    assert!(row.validate(&actor, Some(&worker()), false).is_ok());
}
