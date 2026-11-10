//! Native Projects time and cost actual command/read policy tests.
//!
//! These tests prove work-day/engagement limits, independent review, cost
//! source and cap rules, exact money conversion, Finance-scope reads and
//! replay against a fake repository. No journal is posted by this owner.
use erpx_server_projects::public::project_actuals::{
    ActualCommandReceipt, ActualReviewAction, CostAllocationFact, CostAllocationRow, CostPlan,
    CostSourceRow, CreateCostAllocationCommand, DecideCostAllocationCommand, DecideWorkLogCommand,
    ExpenseLineFact, ProjectActualsActor, ProjectActualsCommandRepository, ProjectActualsFuture,
    SaveWorkLogCommand, StoredActualReceipt, WorkLogFact, WorkLogPlan, WorkLogRow,
    create_cost_allocation, decide_cost_allocation, decide_work_log, list_cost_sources,
    list_work_logs, save_work_log,
};
use erpx_server_projects::public::project_commands::ProjectCommandProject;
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const KEY: &str = "key-actuals-0001";

fn actor(permissions: &[&str]) -> ProjectActualsActor {
    ProjectActualsActor {
        user_id: "owner".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment".into(),
        permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
        modules: vec!["projects".into()],
        expires_at: NOW.get() + 60_000,
        active_staff: true,
        staff_start_date: "2020-01-01".into(),
        staff_end_date: None,
        time_new_work: true,
        time_settle_existing: true,
        costs_new_work: true,
        costs_settle_existing: true,
        cost_history: true,
    }
}

fn project(lifecycle: &str, owner: &str) -> ProjectCommandProject {
    ProjectCommandProject {
        id: "project".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        owner_user_id: owner.into(),
        lifecycle_status: lifecycle.into(),
        version: 4,
        is_owner: owner != "member",
        is_member: true,
    }
}

fn work_log(state: &str, user: &str) -> WorkLogFact {
    WorkLogFact {
        id: "log".into(),
        project_id: "project".into(),
        user_id: user.into(),
        work_date: "2026-11-10".into(),
        minutes: 120,
        state: state.into(),
        version: 1,
    }
}

#[derive(Default)]
struct Repo {
    project: Option<ProjectCommandProject>,
    stored: Option<StoredActualReceipt>,
    for_date: Option<WorkLogFact>,
    daily: i64,
    log: Option<WorkLogFact>,
    line: Option<ExpenseLineFact>,
    allocation: Option<CostAllocationFact>,
    review: Option<CostAllocationFact>,
    allocated: i64,
    work_rows: Vec<WorkLogRow>,
    allocation_rows: Vec<CostAllocationRow>,
    source_rows: Vec<CostSourceRow>,
    commits: usize,
    last_work: Option<WorkLogPlan>,
    last_cost: Option<CostPlan>,
}

impl ProjectActualsCommandRepository for Repo {
    type Error = BusinessError;

    fn stored_receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, Option<StoredActualReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.stored.clone()) })
    }

    fn project<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: bool,
    ) -> ProjectActualsFuture<'a, Option<ProjectCommandProject>, Self::Error> {
        Box::pin(async move { Ok(self.project.clone()) })
    }

    fn work_log_for_date<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, Option<WorkLogFact>, Self::Error> {
        Box::pin(async move { Ok(self.for_date.clone()) })
    }

    fn daily_minutes<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, i64, Self::Error> {
        Box::pin(async move { Ok(self.daily) })
    }

    fn work_log<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, Option<WorkLogFact>, Self::Error> {
        Box::pin(async move { Ok(self.log.clone()) })
    }

    fn expense_line<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, Option<ExpenseLineFact>, Self::Error> {
        Box::pin(async move { Ok(self.line.clone()) })
    }

    fn allocation_for_line<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, Option<CostAllocationFact>, Self::Error> {
        Box::pin(async move { Ok(self.allocation.clone()) })
    }

    fn allocation_review<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, Option<CostAllocationFact>, Self::Error> {
        Box::pin(async move { Ok(self.review.clone()) })
    }

    fn allocated_minor<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, i64, Self::Error> {
        Box::pin(async move { Ok(self.allocated) })
    }

    fn commit_work_log<'a>(
        &'a mut self,
        plan: WorkLogPlan,
    ) -> ProjectActualsFuture<'a, ActualCommandReceipt, Self::Error> {
        Box::pin(async move {
            self.commits += 1;
            self.last_work = Some(plan.clone());
            Ok(plan.result)
        })
    }

    fn commit_cost<'a>(
        &'a mut self,
        plan: CostPlan,
    ) -> ProjectActualsFuture<'a, ActualCommandReceipt, Self::Error> {
        Box::pin(async move {
            self.commits += 1;
            self.last_cost = Some(plan.clone());
            Ok(plan.result)
        })
    }

    fn work_logs<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, Vec<WorkLogRow>, Self::Error> {
        Box::pin(async move { Ok(self.work_rows.clone()) })
    }

    fn cost_allocations<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> ProjectActualsFuture<'a, Vec<CostAllocationRow>, Self::Error> {
        Box::pin(async move { Ok(self.allocation_rows.clone()) })
    }

    fn cost_sources<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: Option<&'a str>,
    ) -> ProjectActualsFuture<'a, Vec<CostSourceRow>, Self::Error> {
        Box::pin(async move { Ok(self.source_rows.clone()) })
    }
}

fn repo() -> Repo {
    Repo {
        project: Some(project("active", "owner")),
        ..Repo::default()
    }
}

fn save_input() -> SaveWorkLogCommand {
    SaveWorkLogCommand {
        work_date: "2026-11-10".into(),
        minutes: 120,
        expected_version: None,
    }
}

#[tokio::test]
async fn submit_work_log_creates_a_current_time_entry() {
    let mut repository = repo();
    let receipt = save_work_log(
        &actor(&["records.write"]),
        &mut repository,
        "project",
        &save_input(),
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.version, 1);
    assert_eq!(receipt.state, "submitted");
    assert!(receipt.id.starts_with("project-time-"));
    let plan = repository.last_work.unwrap();
    assert!(!plan.existing);
    assert_eq!(plan.project_version, 4);
}

#[tokio::test]
async fn only_a_rejected_entry_can_be_revised() {
    let mut repository = repo();
    repository.for_date = Some(work_log("approved", "owner"));
    let input = SaveWorkLogCommand {
        expected_version: Some(1),
        ..save_input()
    };
    let error = save_work_log(
        &actor(&["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_TIME_NOT_EDITABLE");
}

#[tokio::test]
async fn rejected_entry_revision_uses_the_expected_version() {
    let mut repository = repo();
    repository.for_date = Some(work_log("rejected", "owner"));
    let input = SaveWorkLogCommand {
        expected_version: Some(1),
        ..save_input()
    };
    let receipt = save_work_log(
        &actor(&["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.id, "log");
    assert_eq!(receipt.version, 2);
    assert!(repository.last_work.unwrap().existing);
}

#[tokio::test]
async fn future_work_date_is_rejected() {
    let mut repository = repo();
    let mut input = save_input();
    input.work_date = "2099-01-01".into();
    let error = save_work_log(
        &actor(&["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_WORK_DATE_INVALID");
}

#[tokio::test]
async fn daily_time_cannot_exceed_twenty_four_hours() {
    let mut repository = repo();
    repository.daily = 1_400;
    let error = save_work_log(
        &actor(&["records.write"]),
        &mut repository,
        "project",
        &save_input(),
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_TIME_DAILY_LIMIT");
}

#[tokio::test]
async fn author_cannot_review_their_own_time() {
    let mut repository = repo();
    repository.log = Some(work_log("submitted", "owner"));
    let input = DecideWorkLogCommand {
        action: ActualReviewAction::Approve,
        expected_version: 1,
        reason: "Reviewed current project time.".into(),
    };
    let error = decide_work_log(
        &actor(&["records.write"]),
        &mut repository,
        "project",
        "log",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SELF_REVIEW_FORBIDDEN");
}

#[tokio::test]
async fn only_the_project_owner_reviews_time() {
    let mut repository = repo();
    let mut other = project("active", "other-owner");
    other.is_owner = false;
    repository.project = Some(other);
    repository.log = Some(work_log("submitted", "member"));
    let input = DecideWorkLogCommand {
        action: ActualReviewAction::Approve,
        expected_version: 1,
        reason: "Reviewed current project time.".into(),
    };
    let error = decide_work_log(
        &actor(&["records.write"]),
        &mut repository,
        "project",
        "log",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    // The actor is the owner here, so the fake must name a real other owner.
    assert_eq!(error.code, "PROJECT_OWNER_REQUIRED");
}

#[tokio::test]
async fn stale_time_review_is_rejected() {
    let mut repository = repo();
    repository.log = Some(work_log("submitted", "member"));
    let input = DecideWorkLogCommand {
        action: ActualReviewAction::Approve,
        expected_version: 2,
        reason: "Reviewed current project time.".into(),
    };
    let error = decide_work_log(
        &actor(&["records.write"]),
        &mut repository,
        "project",
        "log",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_TIME_REVIEW_STALE");
}

#[tokio::test]
async fn owner_approves_another_members_time() {
    let mut repository = repo();
    repository.project = Some(project("active", "reviewer"));
    repository.log = Some(work_log("submitted", "member"));
    let input = DecideWorkLogCommand {
        action: ActualReviewAction::Approve,
        expected_version: 1,
        reason: "Reviewed current project time.".into(),
    };
    let mut actor = actor(&["records.write"]);
    actor.user_id = "reviewer".into();
    let receipt = decide_work_log(
        &actor,
        &mut repository,
        "project",
        "log",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.state, "approved");
    assert_eq!(receipt.version, 2);
}

fn cost_input() -> CreateCostAllocationCommand {
    CreateCostAllocationCommand {
        project_id: "project".into(),
        source_line_id: "line".into(),
        amount: "10.50".into(),
        expected_version: None,
    }
}

fn posted_line() -> ExpenseLineFact {
    ExpenseLineFact {
        id: "line".into(),
        debit_minor: 5_000,
        currency: "USD".into(),
        entry_date: "2026-11-01".into(),
        journal_status: "posted".into(),
        category: "expense".into(),
    }
}

#[tokio::test]
async fn cost_allocation_uses_exact_minor_units() {
    let mut repository = repo();
    repository.line = Some(posted_line());
    let receipt = create_cost_allocation(
        &actor(&["finance.write"]),
        &mut repository,
        &cost_input(),
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.state, "draft");
    let plan = repository.last_cost.unwrap();
    assert_eq!(plan.amount_minor, 1_050);
    assert_eq!(plan.journal_line_id, "line");
}

#[tokio::test]
async fn cost_allocation_requires_a_posted_expense_line() {
    let mut repository = repo();
    let mut line = posted_line();
    line.journal_status = "draft".into();
    repository.line = Some(line);
    let error = create_cost_allocation(
        &actor(&["finance.write"]),
        &mut repository,
        &cost_input(),
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_COST_SOURCE_INVALID");
}

#[tokio::test]
async fn cost_allocation_cannot_exceed_the_remaining_debit() {
    let mut repository = repo();
    repository.line = Some(posted_line());
    repository.allocated = 4_500;
    let error = create_cost_allocation(
        &actor(&["finance.write"]),
        &mut repository,
        &cost_input(),
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_COST_CAP_EXCEEDED");
}

#[tokio::test]
async fn another_finance_user_cannot_revise_a_rejected_allocation() {
    let mut repository = repo();
    repository.line = Some(posted_line());
    repository.allocation = Some(CostAllocationFact {
        id: "allocation".into(),
        project_id: "project".into(),
        journal_line_id: "line".into(),
        amount_minor: 1_000,
        state: "rejected".into(),
        version: 1,
        created_by: "creator".into(),
        branch_id: None,
        department_id: None,
        journal_status: "posted".into(),
    });
    let input = CreateCostAllocationCommand {
        expected_version: Some(1),
        ..cost_input()
    };
    let error = create_cost_allocation(
        &actor(&["finance.write"]),
        &mut repository,
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_COST_NOT_EDITABLE");
}

#[tokio::test]
async fn cost_self_review_is_forbidden() {
    let mut repository = repo();
    repository.review = Some(CostAllocationFact {
        id: "allocation".into(),
        project_id: "project".into(),
        journal_line_id: "line".into(),
        amount_minor: 1_000,
        state: "draft".into(),
        version: 1,
        created_by: "owner".into(),
        branch_id: None,
        department_id: None,
        journal_status: "posted".into(),
    });
    let input = DecideCostAllocationCommand {
        action: ActualReviewAction::Approve,
        expected_version: 1,
        reason: "Reviewed current project cost.".into(),
    };
    let error = decide_cost_allocation(
        &actor(&["finance.approve"]),
        &mut repository,
        "allocation",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SELF_REVIEW_FORBIDDEN");
}

#[tokio::test]
async fn approval_requires_the_source_journal_to_still_be_posted() {
    let mut repository = repo();
    repository.review = Some(CostAllocationFact {
        id: "allocation".into(),
        project_id: "project".into(),
        journal_line_id: "line".into(),
        amount_minor: 1_000,
        state: "draft".into(),
        version: 1,
        created_by: "creator".into(),
        branch_id: None,
        department_id: None,
        journal_status: "reversed".into(),
    });
    let input = DecideCostAllocationCommand {
        action: ActualReviewAction::Approve,
        expected_version: 1,
        reason: "Reviewed current project cost.".into(),
    };
    let error = decide_cost_allocation(
        &actor(&["finance.approve"]),
        &mut repository,
        "allocation",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PROJECT_COST_SOURCE_STALE");
}

#[tokio::test]
async fn cost_source_read_prepares_decimal_finance_facts() {
    let mut repository = repo();
    repository.source_rows = vec![CostSourceRow {
        source_line_id: "line".into(),
        journal_id: "journal".into(),
        journal_number: "J-1".into(),
        entry_date: "2026-11-01".into(),
        currency: "USD".into(),
        account_id: "account".into(),
        account_code: "5000".into(),
        account_name: "Expense".into(),
        description: "Travel".into(),
        debit_minor: "5000".into(),
        remaining_minor: "3950".into(),
        debit: String::new(),
        remaining: String::new(),
    }];
    let rows = list_cost_sources(
        &actor(&["finance.read"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(rows[0].debit, "50");
    assert_eq!(rows[0].remaining, "39.50");
}

#[tokio::test]
async fn cost_source_read_requires_finance_read() {
    let mut repository = repo();
    let error = list_cost_sources(
        &actor(&["records.read"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "FINANCE_PERMISSION_REQUIRED");
}

#[tokio::test]
async fn work_log_read_requires_project_read() {
    let mut repository = repo();
    repository.work_rows = vec![WorkLogRow {
        id: "log".into(),
        project_id: "project".into(),
        user_id: "owner".into(),
        work_date: "2026-11-10".into(),
        minutes: 60,
        state: "submitted".into(),
        version: 1,
        reviewed_by: None,
        rejection_reason: String::new(),
        created_at: 1,
        updated_at: 1,
        is_actor: true,
        can_review: false,
    }];
    let rows = list_work_logs(
        &actor(&["records.read"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    let error = list_work_logs(
        &actor(&["finance.read"]),
        &mut repository,
        "project",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");
}

#[tokio::test]
async fn replay_returns_the_retained_actual_receipt() {
    let mut repository = repo();
    let prior = ActualCommandReceipt {
        id: "project-time-key-actuals-0001".into(),
        version: 1,
        state: "submitted".into(),
    };
    let operation = "project.actual-time.submit";
    let record_id = "project-time-key-actuals-0001";
    let input = save_input();
    let input_json = serde_json::to_value(&input).unwrap();
    let fingerprint = erpx_server_projects::public::project_actuals::actual_fingerprint(
        operation,
        "project",
        record_id,
        &input_json,
    )
    .unwrap();
    repository.stored = Some(StoredActualReceipt {
        operation: operation.into(),
        record_id: record_id.into(),
        request_fingerprint: fingerprint,
        result_json: Some(serde_json::to_string(&prior).unwrap()),
    });
    let receipt = save_work_log(
        &actor(&["records.write"]),
        &mut repository,
        "project",
        &input,
        KEY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt, prior);
    assert_eq!(repository.commits, 0);
}
