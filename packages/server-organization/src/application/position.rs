//! Native Organization position-save command.
//!
//! The repository supplies current sources, current IAM authority and owns the
//! serializable transaction. This layer keeps the retained command order:
//! permission, idempotent replay, graph snapshot, current row, placement,
//! level, CEO/reporting/cycle, in-use/reporting-seniority, reportingIssues,
//! roster-required default and roster-exit handoff. It never opens a
//! transaction and never writes another owner's tables.
pub use crate::application::organization_nodes::ClockSource;
use crate::{
    application::organization_nodes::{OrganizationCommandActor, OrganizationFuture},
    domain::organization_nodes::permission_required,
    domain::position::{
        PositionGraphGuard, PositionInput, PositionRoster, position_not_found,
        validate_archive_in_use, validate_ceo_and_reporting, validate_level_required,
        validate_reporting_chain, validate_reporting_issues, validate_reporting_levels,
    },
};
use erpx_server_shared::public::{BusinessError, CompanyId};
use serde_json::Value;

/// Composition-owned ordered roster-exit sequence. The implementation is the
/// full retained people-payroll/collaboration sequence: lock and stage, release
/// assignments, invalidate pending requests, with collaboration notifications
/// interleaved at the same points. This crate never issues cross-owner SQL.
pub trait RosterExitPort: Send {
    fn execute<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        engagement_ids: &'a [String],
        key: &'a str,
        reason: &'a str,
        now: i64,
    ) -> OrganizationFuture<'a, (), BusinessError>;
}

pub trait OrganizationPositionRepository: Send {
    type Error: From<BusinessError> + Send;

    fn replay<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a PositionInput,
    ) -> OrganizationFuture<'a, Option<Value>, Self::Error>;

    fn roster<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> OrganizationFuture<'a, PositionRoster, Self::Error>;

    fn branch_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: &'a str,
    ) -> OrganizationFuture<'a, bool, Self::Error>;

    fn department_active<'a>(
        &'a mut self,
        company: &'a CompanyId,
        department: &'a str,
        branch: Option<&'a str>,
    ) -> OrganizationFuture<'a, bool, Self::Error>;

    fn position_level<'a>(
        &'a mut self,
        company: &'a CompanyId,
        level: &'a str,
    ) -> OrganizationFuture<'a, Option<i64>, Self::Error>;

    fn graph_guard<'a>(
        &'a mut self,
        company: &'a CompanyId,
        key: &'a str,
    ) -> OrganizationFuture<'a, PositionGraphGuard, Self::Error>;

    /// Retained `exitingEngagements`: active engagements in this position. The
    /// people-roster assignment/shift/week relations are owned by
    /// `RosterExitPort`, which receives these ids and runs the ordered exit.
    fn exiting_engagements<'a>(
        &'a mut self,
        company: &'a CompanyId,
        position: &'a str,
    ) -> OrganizationFuture<'a, Vec<String>, Self::Error>;

    #[allow(clippy::too_many_arguments)]
    fn persist<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        id: Option<&'a str>,
        input: &'a PositionInput,
        key: &'a str,
        guard: &'a PositionGraphGuard,
        roster_required: bool,
        exiting: &'a [String],
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error>;
}

/// Retained `savePosition` command. Read order is observable through the
/// repository, so each guard remains separate and in this order.
#[allow(clippy::too_many_arguments)]
pub async fn save_position<R: OrganizationPositionRepository + RosterExitPort>(
    repository: &mut R,
    actor: OrganizationCommandActor,
    key: &str,
    id: Option<&str>,
    input: PositionInput,
    clock: &dyn ClockSource,
) -> Result<Value, R::Error> {
    let projection = actor.projection();
    if !projection.can_manage() {
        return Err(permission_required().into());
    }
    let record_id = match id {
        Some(id) => id.to_owned(),
        None => format!("position-{key}"),
    };
    let operation = "organization.position.save";
    if let Some(prior) = repository
        .replay(&actor, key, operation, &record_id, &input)
        .await?
    {
        return Ok(prior);
    }
    let company = actor.command.company_id.clone();
    let guard = repository.graph_guard(&company, key).await?;
    let roster = repository.roster(&company).await?;
    let current = roster
        .positions
        .iter()
        .find(|position| position.id == record_id)
        .cloned();
    if id.is_some() && current.is_none() {
        return Err(position_not_found().into());
    }
    if let Some(current) = &current {
        crate::domain::organization_setup::validate_scope(
            projection.branch_id.as_ref().map(|value| value.as_str()),
            projection
                .department_id
                .as_ref()
                .map(|value| value.as_str()),
            current.branch_id.as_deref(),
            current.department_id.as_deref(),
        )?;
    }
    crate::domain::organization_setup::validate_scope(
        projection.branch_id.as_ref().map(|value| value.as_str()),
        projection
            .department_id
            .as_ref()
            .map(|value| value.as_str()),
        input.branch_id.as_deref(),
        input.department_id.as_deref(),
    )?;
    if let Some(branch) = &input.branch_id
        && !branch.is_empty()
        && !repository.branch_active(&company, branch).await?
    {
        return Err(BusinessError::invalid(
            "BRANCH_INVALID",
            "Choose an active branch in this company.",
        )
        .into());
    }
    if let Some(department) = &input.department_id
        && !department.is_empty()
        && !repository
            .department_active(&company, department, input.branch_id.as_deref())
            .await?
    {
        return Err(BusinessError::invalid(
            "DEPARTMENT_INVALID",
            "Choose a department in the selected branch.",
        )
        .into());
    }
    let level_number =
        validate_level_required(repository.position_level(&company, &input.level_id).await?)?;
    validate_ceo_and_reporting(&input, level_number)?;
    validate_reporting_chain(
        &record_id,
        input.reports_to(),
        level_number,
        &roster.positions,
    )?;
    validate_archive_in_use(id.is_some(), &record_id, input.active, &roster)?;
    validate_reporting_levels(id.is_some(), &record_id, level_number, &roster.positions)?;
    if let Some(current) = &current {
        let today = crate::application::access::utc_date(clock.now().get());
        validate_reporting_issues(current, &input, level_number, &roster, &today)?;
    }
    let now = clock.now().get();
    let roster_required = input
        .roster_required
        .unwrap_or_else(|| current.as_ref().is_some_and(|row| row.roster_required == 1));
    let transitioning =
        current.as_ref().is_some_and(|row| row.roster_required == 1) && !roster_required;
    let exiting = if transitioning {
        repository.exiting_engagements(&company, &record_id).await?
    } else {
        Vec::new()
    };
    if transitioning {
        repository
            .execute(&actor, &exiting, key, &input.reason, now)
            .await?;
    }
    repository
        .persist(
            &actor,
            id,
            &input,
            key,
            &guard,
            roster_required,
            &exiting,
            now,
        )
        .await
}
