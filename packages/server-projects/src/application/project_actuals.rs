//! Native Projects time and cost actual command use cases.
//!
//! The repository owns one serializable unit and re-checks the same current
//! source facts after it locks the project. Clock, active engagement, purchase,
//! Finance permission and current scope facts are supplied by composition from
//! their owning ports. No journal is created or posted here; cost reads only
//! prepare the posted Finance expense facts a Project allocation may reference.
use crate::domain::project_actuals::{
    ActualCommandReceipt, ActualReviewAction, CostAllocationFact, CostAllocationRow, CostPlan,
    CostSourceRow, CreateCostAllocationCommand, DecideCostAllocationCommand, DecideWorkLogCommand,
    ExpenseLineFact, ProjectActualsActor, SaveWorkLogCommand, StoredActualReceipt, WorkLogFact,
    WorkLogPlan, WorkLogRow, authority, cost_cap_exceeded, cost_not_editable, cost_not_found,
    cost_review_stale, cost_source_invalid, cost_source_stale, fingerprint, idempotency_conflict,
    incomplete_receipt, invalid_receipt, invalid_work_date, minor_to_decimal,
    missing_project, owner_required, parse_amount_minor, self_review_forbidden, structure_frozen,
    time_daily_limit, time_not_editable, time_not_found, time_review_stale, today_utc,
};
use crate::domain::project_commands::ProjectCommandProject;
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis,
};
use std::future::Future;

pub type ProjectActualsFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait ProjectActualsCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectActualsFuture<'a, Option<StoredActualReceipt>, Self::Error>;
    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
        lock: bool,
    ) -> ProjectActualsFuture<'a, Option<ProjectCommandProject>, Self::Error>;
    fn work_log_for_date<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
        work_date: &'a str,
    ) -> ProjectActualsFuture<'a, Option<WorkLogFact>, Self::Error>;
    fn daily_minutes<'a>(
        &'a mut self,
        company: &'a CompanyId,
        user_id: &'a str,
        work_date: &'a str,
        exclude_id: &'a str,
    ) -> ProjectActualsFuture<'a, i64, Self::Error>;
    fn work_log<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        log_id: &'a str,
    ) -> ProjectActualsFuture<'a, Option<WorkLogFact>, Self::Error>;
    fn expense_line<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        project_branch_id: Option<&'a str>,
        project_department_id: Option<&'a str>,
        source_line_id: &'a str,
    ) -> ProjectActualsFuture<'a, Option<ExpenseLineFact>, Self::Error>;
    fn allocation_for_line<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        journal_line_id: &'a str,
    ) -> ProjectActualsFuture<'a, Option<CostAllocationFact>, Self::Error>;
    fn allocation_review<'a>(
        &'a mut self,
        company: &'a CompanyId,
        allocation_id: &'a str,
    ) -> ProjectActualsFuture<'a, Option<CostAllocationFact>, Self::Error>;
    fn allocated_minor<'a>(
        &'a mut self,
        company: &'a CompanyId,
        journal_line_id: &'a str,
        exclude_id: &'a str,
    ) -> ProjectActualsFuture<'a, i64, Self::Error>;
    fn commit_work_log<'a>(
        &'a mut self,
        plan: WorkLogPlan,
    ) -> ProjectActualsFuture<'a, ActualCommandReceipt, Self::Error>;
    fn commit_cost<'a>(
        &'a mut self,
        plan: CostPlan,
    ) -> ProjectActualsFuture<'a, ActualCommandReceipt, Self::Error>;
    fn work_logs<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectActualsFuture<'a, Vec<WorkLogRow>, Self::Error>;
    fn cost_allocations<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectActualsFuture<'a, Vec<CostAllocationRow>, Self::Error>;
    fn cost_sources<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        project_branch_id: Option<&'a str>,
        project_department_id: Option<&'a str>,
    ) -> ProjectActualsFuture<'a, Vec<CostSourceRow>, Self::Error>;
}

fn replay(
    stored: Option<StoredActualReceipt>,
    operation: &str,
    record_id: &str,
    request_fingerprint: &str,
    expected_state: Option<&str>,
) -> Result<Option<ActualCommandReceipt>, BusinessError> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    if stored.operation != operation
        || stored.record_id != record_id
        || stored.request_fingerprint != request_fingerprint
    {
        return Err(idempotency_conflict());
    }
    let Some(result) = stored.result_json else {
        return Err(incomplete_receipt());
    };
    let receipt: ActualCommandReceipt =
        serde_json::from_str(&result).map_err(|_| invalid_receipt())?;
    if receipt.id != record_id
        || receipt.version < 1
        || expected_state.is_some_and(|state| receipt.state != state)
    {
        return Err(invalid_receipt());
    }
    Ok(Some(receipt))
}

fn validate_project(
    project: &ProjectCommandProject,
    company: &CompanyId,
) -> Result<(), BusinessError> {
    crate::domain::project_commands::validate_project(project, company)?;
    if !project.is_owner && !project.is_member {
        return Err(missing_project());
    }
    Ok(())
}

fn work_date_in_engagement(
    actor: &ProjectActualsActor,
    work_date: &str,
    now: TimestampMillis,
) -> bool {
    work_date <= today_utc(now).as_str()
        && work_date >= actor.staff_start_date.as_str()
        && actor
            .staff_end_date
            .as_deref()
            .is_none_or(|end| work_date <= end)
}

pub async fn save_work_log<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    project_id: &str,
    input: &SaveWorkLogCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<ActualCommandReceipt, R::Error> {
    actor.require_write(now)?;
    input.validate()?;
    if !actor.time_new_work {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "PACKAGE_REQUIRED",
            "This Project work requires its purchased capability.",
        )
        .into());
    }
    let key = CommandKey::new(key)?;
    let project_id = RecordId::new(project_id)?;
    let input_json = serde_json::to_value(input).map_err(|_| invalid_receipt())?;
    if let Some(stored) = repository.stored_receipt(key.as_str()).await? {
        if !matches!(
            stored.operation.as_str(),
            "project.actual-time.submit" | "project.actual-time.revise"
        ) {
            return Err(idempotency_conflict().into());
        }
        let operation = stored.operation.clone();
        let record_id = stored.record_id.clone();
        let fingerprint = fingerprint(&operation, project_id.as_str(), &record_id, &input_json)?;
        if let Some(receipt) = replay(
            Some(stored),
            &operation,
            &record_id,
            &fingerprint,
            Some("submitted"),
        )? {
            return Ok(receipt);
        }
    }
    let project = repository
        .project(
            &actor.company_id,
            project_id.as_str(),
            &actor.user_id,
            true,
        )
        .await?
        .ok_or_else(missing_project)?;
    validate_project(&project, &actor.company_id)?;
    if project.lifecycle_status != "active" {
        return Err(structure_frozen().into());
    }
    if !work_date_in_engagement(actor, &input.work_date, now) {
        return Err(invalid_work_date().into());
    }
    let existing = repository
        .work_log_for_date(
            &actor.company_id,
            project_id.as_str(),
            &actor.user_id,
            &input.work_date,
        )
        .await?;
    let (log_id, operation, version) = match &existing {
        Some(row) => {
            if row.state != "rejected" || Some(row.version) != input.expected_version {
                return Err(time_not_editable().into());
            }
            (
                row.id.clone(),
                "project.actual-time.revise",
                input.expected_version.unwrap_or(row.version) + 1,
            )
        }
        None => (
            format!("project-time-{}", key.as_str()),
            "project.actual-time.submit",
            1,
        ),
    };
    let prior = repository.stored_receipt(key.as_str()).await?;
    let fingerprint = fingerprint(operation, project_id.as_str(), &log_id, &input_json)?;
    if let Some(receipt) = replay(
        prior,
        operation,
        &log_id,
        &fingerprint,
        Some("submitted"),
    )? {
        return Ok(receipt);
    }
    let other = repository
        .daily_minutes(
            &actor.company_id,
            &actor.user_id,
            &input.work_date,
            &log_id,
        )
        .await?;
    if other + input.minutes > 1_440 {
        return Err(time_daily_limit().into());
    }
    let result = ActualCommandReceipt {
        id: log_id.clone(),
        version,
        state: "submitted".to_owned(),
    };
    let plan = WorkLogPlan {
        key,
        operation: operation.to_owned(),
        company_id: actor.company_id.clone(),
        actor_id: actor.user_id.clone(),
        assignment_id: actor.assignment_id.clone(),
        authority_json: authority(actor)?,
        project_id: project_id.as_str().to_owned(),
        project_version: project.version,
        log_id,
        existing: existing.is_some(),
        minutes: input.minutes,
        work_date: input.work_date.clone(),
        user_id: actor.user_id.clone(),
        event_action: if existing.is_some() {
            "revised".to_owned()
        } else {
            "submitted".to_owned()
        },
        reason: if existing.is_some() {
            "Revised rejected project time entry.".to_owned()
        } else {
            "Submitted project time entry for owner review.".to_owned()
        },
        version,
        result: result.clone(),
        request_fingerprint: fingerprint,
        now: now.get(),
    };
    let receipt = repository.commit_work_log(plan).await?;
    if receipt.id != result.id || receipt.version != result.version {
        return Err(invalid_receipt().into());
    }
    Ok(receipt)
}

pub async fn decide_work_log<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    project_id: &str,
    log_id: &str,
    input: &DecideWorkLogCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<ActualCommandReceipt, R::Error> {
    actor.require_write(now)?;
    input.validate()?;
    if !actor.time_settle_existing {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "PACKAGE_REQUIRED",
            "This Project work requires its purchased settlement capability.",
        )
        .into());
    }
    let key = CommandKey::new(key)?;
    let project_id = RecordId::new(project_id)?;
    let log_id = RecordId::new(log_id)?;
    let operation = format!("project.actual-time.{}", input.action.as_str());
    let state = match input.action {
        ActualReviewAction::Approve => "approved",
        ActualReviewAction::Reject => "rejected",
    };
    let input_json = serde_json::to_value(input).map_err(|_| invalid_receipt())?;
    let fingerprint = fingerprint(&operation, project_id.as_str(), log_id.as_str(), &input_json)?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        &operation,
        log_id.as_str(),
        &fingerprint,
        Some(state),
    )? {
        return Ok(receipt);
    }
    let project = repository
        .project(
            &actor.company_id,
            project_id.as_str(),
            &actor.user_id,
            true,
        )
        .await?
        .ok_or_else(missing_project)?;
    validate_project(&project, &actor.company_id)?;
    if !project.is_owner {
        return Err(owner_required().into());
    }
    let log = repository
        .work_log(&actor.company_id, project_id.as_str(), log_id.as_str())
        .await?
        .ok_or_else(time_not_found)?;
    if log.user_id == actor.user_id {
        return Err(self_review_forbidden().into());
    }
    if log.state != "submitted" || log.version != input.expected_version {
        return Err(time_review_stale().into());
    }
    let version = input.expected_version + 1;
    let result = ActualCommandReceipt {
        id: log_id.as_str().to_owned(),
        version,
        state: state.to_owned(),
    };
    let plan = WorkLogPlan {
        key,
        operation,
        company_id: actor.company_id.clone(),
        actor_id: actor.user_id.clone(),
        assignment_id: actor.assignment_id.clone(),
        authority_json: authority(actor)?,
        project_id: project_id.as_str().to_owned(),
        project_version: project.version,
        log_id: log_id.as_str().to_owned(),
        existing: true,
        minutes: log.minutes,
        work_date: log.work_date,
        user_id: log.user_id,
        event_action: state.to_owned(),
        reason: input.reason.clone(),
        version,
        result: result.clone(),
        request_fingerprint: fingerprint,
        now: now.get(),
    };
    let receipt = repository.commit_work_log(plan).await?;
    if receipt.id != result.id || receipt.version != result.version {
        return Err(invalid_receipt().into());
    }
    Ok(receipt)
}

pub async fn create_cost_allocation<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    input: &CreateCostAllocationCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<ActualCommandReceipt, R::Error> {
    input.validate()?;
    actor.require_finance("finance.write")?;
    if !actor.costs_new_work {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "PACKAGE_REQUIRED",
            "This Project work requires its purchased capability.",
        )
        .into());
    }
    let key = CommandKey::new(key)?;
    let project_id = RecordId::new(input.project_id.as_str())?;
    let amount_minor = parse_amount_minor(&input.amount)?;
    let input_json = serde_json::to_value(input).map_err(|_| invalid_receipt())?;
    if let Some(stored) = repository.stored_receipt(key.as_str()).await? {
        if !matches!(
            stored.operation.as_str(),
            "project.cost-allocation.create" | "project.cost-allocation.revise"
        ) {
            return Err(idempotency_conflict().into());
        }
        let operation = stored.operation.clone();
        let record_id = stored.record_id.clone();
        let fingerprint = fingerprint(&operation, project_id.as_str(), &record_id, &input_json)?;
        if let Some(receipt) = replay(
            Some(stored),
            &operation,
            &record_id,
            &fingerprint,
            Some("draft"),
        )? {
            return Ok(receipt);
        }
    }
    let project = repository
        .project(
            &actor.company_id,
            project_id.as_str(),
            &actor.user_id,
            true,
        )
        .await?
        .ok_or_else(missing_project)?;
    validate_project(&project, &actor.company_id)?;
    if !actor.scope_access(project.branch_id.as_deref(), project.department_id.as_deref()) {
        return Err(missing_project().into());
    }
    if project.lifecycle_status != "active" {
        return Err(structure_frozen().into());
    }
    let line = repository
        .expense_line(
            &actor.company_id,
            actor.branch_id.as_ref().map(RecordId::as_str),
            actor.department_id.as_ref().map(RecordId::as_str),
            project.branch_id.as_deref(),
            project.department_id.as_deref(),
            &input.source_line_id,
        )
        .await?
        .ok_or_else(cost_source_invalid)?;
    if line.journal_status != "posted" || !matches!(line.category.as_str(), "expense" | "other_expense") {
        return Err(cost_source_invalid().into());
    }
    let existing = repository
        .allocation_for_line(
            &actor.company_id,
            project_id.as_str(),
            &line.id,
        )
        .await?;
    let (allocation_id, operation, version) = match &existing {
        Some(row) => {
            if row.state != "rejected"
                || Some(row.version) != input.expected_version
                || row.created_by != actor.user_id
            {
                return Err(cost_not_editable().into());
            }
            (
                row.id.clone(),
                "project.cost-allocation.revise",
                input.expected_version.unwrap_or(row.version) + 1,
            )
        }
        None => (
            format!("project-cost-{}", key.as_str()),
            "project.cost-allocation.create",
            1,
        ),
    };
    let prior = repository.stored_receipt(key.as_str()).await?;
    let fingerprint = fingerprint(operation, project_id.as_str(), &allocation_id, &input_json)?;
    if let Some(receipt) = replay(
        prior,
        operation,
        &allocation_id,
        &fingerprint,
        Some("draft"),
    )? {
        return Ok(receipt);
    }
    let allocated = repository
        .allocated_minor(&actor.company_id, &line.id, &allocation_id)
        .await?;
    if amount_minor > line.debit_minor.saturating_sub(allocated) {
        return Err(cost_cap_exceeded().into());
    }
    let result = ActualCommandReceipt {
        id: allocation_id.clone(),
        version,
        state: "draft".to_owned(),
    };
    let plan = CostPlan {
        key,
        operation: operation.to_owned(),
        company_id: actor.company_id.clone(),
        actor_id: actor.user_id.clone(),
        assignment_id: actor.assignment_id.clone(),
        authority_json: authority(actor)?,
        project_id: project_id.as_str().to_owned(),
        project_version: project.version,
        allocation_id,
        existing: existing.is_some(),
        journal_line_id: line.id,
        amount_minor,
        state: "draft".to_owned(),
        event_action: if existing.is_some() {
            "revised".to_owned()
        } else {
            "created".to_owned()
        },
        reason: if existing.is_some() {
            "Revised rejected project cost allocation.".to_owned()
        } else {
            "Submitted posted expense line for project cost review.".to_owned()
        },
        version,
        result: result.clone(),
        request_fingerprint: fingerprint,
        now: now.get(),
    };
    let receipt = repository.commit_cost(plan).await?;
    if receipt.id != result.id || receipt.version != result.version {
        return Err(invalid_receipt().into());
    }
    Ok(receipt)
}

pub async fn decide_cost_allocation<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    allocation_id: &str,
    input: &DecideCostAllocationCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<ActualCommandReceipt, R::Error> {
    input.validate()?;
    actor.require_finance("finance.approve")?;
    if !actor.costs_settle_existing {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "PACKAGE_REQUIRED",
            "This Project work requires its purchased settlement capability.",
        )
        .into());
    }
    let key = CommandKey::new(key)?;
    let allocation_id = RecordId::new(allocation_id)?;
    let operation = format!("project.cost-allocation.{}", input.action.as_str());
    let state = match input.action {
        ActualReviewAction::Approve => "approved",
        ActualReviewAction::Reject => "rejected",
    };
    let input_json = serde_json::to_value(input).map_err(|_| invalid_receipt())?;
    let row = repository
        .allocation_review(&actor.company_id, allocation_id.as_str())
        .await?
        .ok_or_else(cost_not_found)?;
    let project = repository
        .project(
            &actor.company_id,
            row.project_id.as_str(),
            &actor.user_id,
            true,
        )
        .await?
        .ok_or_else(cost_not_found)?;
    validate_project(&project, &actor.company_id)?;
    let project_id = RecordId::new(row.project_id.as_str())?;
    let fingerprint = fingerprint(
        &operation,
        project_id.as_str(),
        allocation_id.as_str(),
        &input_json,
    )?;
    if let Some(receipt) = replay(
        repository.stored_receipt(key.as_str()).await?,
        &operation,
        allocation_id.as_str(),
        &fingerprint,
        Some(state),
    )? {
        return Ok(receipt);
    }
    if (actor.branch_id.is_some() && row.branch_id.as_deref() != actor.branch_id.as_ref().map(RecordId::as_str))
        || (actor.department_id.is_some()
            && row.department_id.as_deref() != actor.department_id.as_ref().map(RecordId::as_str))
    {
        return Err(cost_not_found().into());
    }
    if row.created_by == actor.user_id {
        return Err(self_review_forbidden().into());
    }
    if row.state != "draft" || row.version != input.expected_version {
        return Err(cost_review_stale().into());
    }
    if input.action == ActualReviewAction::Approve && row.journal_status != "posted" {
        return Err(cost_source_stale().into());
    }
    let version = input.expected_version + 1;
    let result = ActualCommandReceipt {
        id: allocation_id.as_str().to_owned(),
        version,
        state: state.to_owned(),
    };
    let plan = CostPlan {
        key,
        operation,
        company_id: actor.company_id.clone(),
        actor_id: actor.user_id.clone(),
        assignment_id: actor.assignment_id.clone(),
        authority_json: authority(actor)?,
        project_id: project_id.as_str().to_owned(),
        project_version: project.version,
        allocation_id: allocation_id.as_str().to_owned(),
        existing: true,
        journal_line_id: row.journal_line_id,
        amount_minor: row.amount_minor,
        state: state.to_owned(),
        event_action: state.to_owned(),
        reason: input.reason.clone(),
        version,
        result: result.clone(),
        request_fingerprint: fingerprint,
        now: now.get(),
    };
    let receipt = repository.commit_cost(plan).await?;
    if receipt.id != result.id || receipt.version != result.version {
        return Err(invalid_receipt().into());
    }
    Ok(receipt)
}

async fn visible_project<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    project_id: &str,
    now: TimestampMillis,
) -> Result<ProjectCommandProject, R::Error> {
    actor.require_read(now)?;
    visible_project_unchecked(actor, repository, project_id).await
}

async fn visible_finance_project<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    project_id: &str,
    now: TimestampMillis,
) -> Result<ProjectCommandProject, R::Error> {
    actor.require_finance_context(now)?;
    visible_project_unchecked(actor, repository, project_id).await
}

async fn visible_project_unchecked<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    project_id: &str,
) -> Result<ProjectCommandProject, R::Error> {
    let project_id = RecordId::new(project_id)?;
    let project = repository
        .project(
            &actor.company_id,
            project_id.as_str(),
            &actor.user_id,
            false,
        )
        .await?
        .ok_or_else(missing_project)?;
    validate_project(&project, &actor.company_id)?;
    Ok(project)
}

pub async fn list_work_logs<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    project_id: &str,
    now: TimestampMillis,
) -> Result<Vec<WorkLogRow>, R::Error> {
    let project = visible_project(actor, repository, project_id, now).await?;
    repository
        .work_logs(
            &actor.company_id,
            &project.id,
            &actor.user_id,
        )
        .await
}

pub async fn list_cost_allocations<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    project_id: &str,
    now: TimestampMillis,
) -> Result<Vec<CostAllocationRow>, R::Error> {
    actor.require_finance("finance.read")?;
    let project = visible_finance_project(actor, repository, project_id, now).await?;
    if !actor.cost_access(project.branch_id.as_deref(), project.department_id.as_deref()) {
        return Err(missing_project().into());
    }
    let mut rows = repository
        .cost_allocations(
            &actor.company_id,
            &project.id,
            &actor.user_id,
        )
        .await?;
    for row in &mut rows {
        if let Ok(minor) = row.amount_minor.parse::<i64>() {
            row.amount = minor_to_decimal(minor);
        }
    }
    Ok(rows)
}

/// Posted Finance expense facts a Project cost allocation may reference. This
/// reads only; Finance remains the posting owner.
pub async fn list_cost_sources<R: ProjectActualsCommandRepository>(
    actor: &ProjectActualsActor,
    repository: &mut R,
    project_id: &str,
    now: TimestampMillis,
) -> Result<Vec<CostSourceRow>, R::Error> {
    actor.require_finance("finance.read")?;
    let project = visible_finance_project(actor, repository, project_id, now).await?;
    if !actor.cost_access(project.branch_id.as_deref(), project.department_id.as_deref()) {
        return Err(missing_project().into());
    }
    let mut rows = repository
        .cost_sources(
            &actor.company_id,
            actor.branch_id.as_ref().map(RecordId::as_str),
            actor.department_id.as_ref().map(RecordId::as_str),
            project.branch_id.as_deref(),
            project.department_id.as_deref(),
        )
        .await?;
    for row in &mut rows {
        if let (Ok(debit), Ok(remaining)) = (
            row.debit_minor.parse::<i64>(),
            row.remaining_minor.parse::<i64>(),
        ) {
            row.debit = minor_to_decimal(debit);
            row.remaining = minor_to_decimal(remaining);
        }
    }
    Ok(rows)
}
