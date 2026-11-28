//! Native Operations routine command assembly.
//!
//! The owner stores templates/schedules and the command ledger; composition
//! supplies the current Account/IAM selection and the People, Projects,
//! Organization and Identity facts the retained routine validation consumes.
use crate::business_http::BusinessScopeSlugs;
use erpx_commercial_server::public::capability_decision::{PurchasedAccess, purchased_accesses};
use erpx_commercial_server::public::purchase_after::write_purchase_shadow_checked_observation;
use erpx_commercial_server::public::purchase_command_boundary::{
    PurchaseCommandBoundary, PurchaseCommandScope, lock_company_purchase_read_sources,
};
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, CompanyPurchaseReadPort, PackageEnforcementMode,
    PgCompanyPurchaseReader, current_package_catalogue,
};
use erpx_identity_access_server::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest,
        WorkPermissionRequirement,
    },
    routine_owner::company_owner,
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_operations_server::public::native_routines_detail_read::read_template_schedules;
use erpx_operations_server::public::routine_commands::{
    PgRoutineCommands, PgRoutineSources, RoutineAuthority, RoutineCapture,
    RoutineCommandRepository, RoutineFuture, RunRoutineTemplateCommand, change_schedule_state,
    end_schedule, fingerprint_json, lock_routine_sources, run_template, run_template_source,
    save_schedule, save_template,
};
use erpx_operations_server::public::routine_commands_http::{
    RoutineHttpBackend, RoutineHttpCommand, RoutineHttpCommandRequest, RoutineHttpFuture,
    RoutineHttpResponse,
};
use erpx_operations_server::public::routine_occurrences::{
    OccurrenceOutcome, OccurrencePass, OccurrencePolicy, OccurrenceSourceFacts,
    RoutineExecutionAuthorization, RoutineOccurrenceFuture,
    RoutineOccurrenceUnit as OccurrenceUnit, authorization as occurrence_authorization,
    generate_due_occurrences as generate_occurrences, lock_occurrence_sources, occurrence_image,
    plan_occurrence, production_enabled, production_forbidden, require_authorization_source,
    require_current_authority, require_lineage, source as occurrence_source, source_changed,
    stage_occurrence,
};
use erpx_operations_server::public::staff_tasks::{
    PgStaffTaskSources, PgStaffTasks, ProjectFact, RouteFact, RoutineTaskWrite, SaveTaskCommand,
    SaveTaskMode, StaffFact, StaffTaskActor, StaffTaskFactDelegates, StaffTaskFuture,
    StaffTaskRepository, StaffTaskSources, TaskRow, TaskScope, lock_staff_task_sources,
    notify_staff_task_approval, save_task_mode,
};
use erpx_organization_server::public::staff_department_route::{
    DepartmentPersonFact, staff_department_route,
};
use erpx_people_payroll_server::public::staff_facts::{active_staff, staff_people};
use erpx_projects_server::public::staff_facts::staff_project;
use erpx_platform_runtime_server::public::http_support::HeaderMap;
use erpx_platform_runtime_server::public::postgres::{DatabasePool, PgTransactionError};
use erpx_platform_shared_server::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
    TimestampMillis,
};
use erpx_workflow_server::public::commands::CommandJson;
use erpx_workflow_server::public::staff_approval::{
    OpenStaffApproval, StaffApprovalStep, open_staff_approval,
};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::collections::BTreeMap;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ROUTINE_SCOPE_REQUIRED",
        "Choose the current authorized Operations workspace.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_ROUTINES_UNAVAILABLE",
        "The native Operations routine command is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn actor(
    current: &AuthorizedWorkContext,
) -> Result<erpx_operations_server::public::staff_tasks::StaffTaskActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(
        erpx_operations_server::public::staff_tasks::StaffTaskActor {
            user_id: current.user_id().as_str().to_owned(),
            assignment_id: assignment.id.as_str().to_owned(),
            company_id: workspace.company_id.as_str().to_owned(),
            branch_id: workspace.branch_id.as_ref().map(|v| v.as_str().to_owned()),
            department_id: workspace
                .department_id
                .as_ref()
                .map(|v| v.as_str().to_owned()),
            permissions: current.command_iam_metadata().permissions.clone(),
            modules: current.modules().to_vec(),
            expires_at: current.expires_at(),
        },
    )
}

fn context(current: &AuthorizedWorkContext) -> serde_json::Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|v| v.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|v| v.as_str()),
        "branchSlug": workspace.branch_slug,
        "departmentSlug": workspace.department_slug,
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "modules": current.modules(),
        "permissions": current.command_iam_metadata().permissions,
        "expiresAt": current.expires_at(),
    })
}

/// Cross-owner routine facts over the caller's retained unit.
#[derive(Clone, Copy)]
struct RoutineDelegates;

impl erpx_operations_server::public::routine_commands::RoutineFactDelegates for RoutineDelegates {
    fn company_owner<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> RoutineFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            company_owner(c, company_id, user_id).await
        })
    }

    fn active_staff<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> RoutineFuture<
        'a,
        Option<erpx_operations_server::public::staff_tasks::StaffFact>,
        BusinessError,
    > {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            Ok(active_staff(c, company_id, user_id).await?.map(|fact| {
                erpx_operations_server::public::staff_tasks::StaffFact {
                    branch_id: fact.branch_id,
                    department_id: fact.department_id,
                }
            }))
        })
    }

    fn project<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        project_id: &'a str,
    ) -> RoutineFuture<
        'a,
        Option<erpx_operations_server::public::staff_tasks::ProjectFact>,
        BusinessError,
    > {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            Ok(staff_project(c, company_id, project_id).await?.map(|fact| {
                erpx_operations_server::public::staff_tasks::ProjectFact {
                    owner_user_id: fact.owner_user_id,
                    title: fact.title,
                }
            }))
        })
    }

    fn task_route<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        scope: &'a erpx_operations_server::public::staff_tasks::TaskScope,
        creator_user_id: &'a str,
        assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        service_task: bool,
    ) -> RoutineFuture<'a, erpx_operations_server::public::staff_tasks::RouteFact, BusinessError>
    {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            if approvers.len() > 20
                || approvers
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != approvers.len()
            {
                return Ok(erpx_operations_server::public::staff_tasks::RouteFact {
                    blocked_reason: Some(
                        "Choose up to twenty different ordered approvers per phase.".into(),
                    ),
                });
            }
            if active_staff(c, &scope.company_id, assignee_user_id)
                .await?
                .is_none()
            {
                return Ok(erpx_operations_server::public::staff_tasks::RouteFact {
                    blocked_reason: Some("Choose a current active staff member.".into()),
                });
            }
            for user in approvers {
                if user == assignee_user_id || (phase == "start" && user == creator_user_id) {
                    return Ok(erpx_operations_server::public::staff_tasks::RouteFact {
                        blocked_reason: Some(
                            "Assignees cannot approve their work; creators cannot approve their own start request."
                                .into(),
                        ),
                    });
                }
                if active_staff(c, &scope.company_id, user).await?.is_none() {
                    return Ok(erpx_operations_server::public::staff_tasks::RouteFact {
                        blocked_reason: Some("Choose current active staff as approvers.".into()),
                    });
                }
            }
            let department = if creator_user_id == assignee_user_id
                && scope.department_id.is_some()
                && !service_task
            {
                let people = staff_people(c, &scope.company_id).await?;
                let facts: Vec<DepartmentPersonFact> = people
                    .into_iter()
                    .map(|person| DepartmentPersonFact {
                        user_id: person.user_id,
                        person_id: person.person_id,
                        department_id: person.department_id,
                        position_id: person.position_id,
                        level_number: person.level_number,
                        manager_person_id: person.manager_person_id,
                        engagement_id: person.engagement_id,
                        engagement_version: person.engagement_version,
                    })
                    .collect();
                staff_department_route(c, &scope.company_id, creator_user_id, &facts)
                    .await?
                    .blocked_reason
            } else {
                None
            };
            Ok(erpx_operations_server::public::staff_tasks::RouteFact {
                blocked_reason: department,
            })
        })
    }
}

#[derive(Clone, Copy)]
struct RoutineSources;

impl erpx_identity_access_server::public::effective_authority::HoldingWorkSourcePreparation
    for RoutineSources
{
    fn prepare<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> erpx_platform_runtime_server::public::postgres::PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_routine_sources(c).await?;
            lock_staff_task_sources(c).await?;
            Ok(())
        })
    }
}

struct RunRoute {
    steps: Vec<StaffApprovalStep>,
    blocked_reason: Option<String>,
}

fn run_blocked(reason: impl Into<String>) -> RunRoute {
    RunRoute {
        steps: Vec::new(),
        blocked_reason: Some(reason.into()),
    }
}

/// Retained `taskRoute` for the manual run: named-only steps plus the
/// Organization department route for a self-assigned reviewed occurrence.
async fn compute_run_route(
    connection: &mut PgConnection,
    scope: &TaskScope,
    creator_user_id: &str,
    assignee_user_id: &str,
    phase: &str,
    approvers: &[String],
) -> Result<RunRoute, BusinessError> {
    use std::collections::BTreeSet;
    if approvers.len() > 20 || approvers.iter().collect::<BTreeSet<_>>().len() != approvers.len() {
        return Ok(run_blocked(
            "Choose up to twenty different ordered approvers per phase.",
        ));
    }
    if active_staff(connection, &scope.company_id, assignee_user_id)
        .await?
        .is_none()
    {
        return Ok(run_blocked("The assignee is not current active staff."));
    }
    for user in approvers {
        if active_staff(connection, &scope.company_id, user)
            .await?
            .is_none()
        {
            return Ok(run_blocked("Every approver must be current active staff."));
        }
        if user == assignee_user_id || (phase == "start" && user == creator_user_id) {
            return Ok(run_blocked(
                "Assignees cannot approve their work; creators cannot approve their own start request.",
            ));
        }
    }
    let mut steps = Vec::new();
    if !approvers.is_empty() && creator_user_id == assignee_user_id && scope.department_id.is_some()
    {
        let people = staff_people(connection, &scope.company_id).await?;
        let facts: Vec<DepartmentPersonFact> = people
            .into_iter()
            .map(|person| DepartmentPersonFact {
                user_id: person.user_id,
                person_id: person.person_id,
                department_id: person.department_id,
                position_id: person.position_id,
                level_number: person.level_number,
                manager_person_id: person.manager_person_id,
                engagement_id: person.engagement_id,
                engagement_version: person.engagement_version,
            })
            .collect();
        let route =
            staff_department_route(connection, &scope.company_id, assignee_user_id, &facts).await?;
        if let Some(reason) = route.blocked_reason {
            return Ok(run_blocked(reason));
        }
        steps.extend(route.steps.into_iter().map(|step| StaffApprovalStep {
            user_id: step.user_id,
            authority: step.authority,
            candidate_user_ids: step.candidate_user_ids,
            required_role: None,
            stage: step.stage,
            snapshot_json: step.snapshot_json,
        }));
    }
    steps.extend(approvers.iter().map(|user| StaffApprovalStep {
        user_id: user.clone(),
        authority: "named".into(),
        candidate_user_ids: vec![user.clone()],
        required_role: None,
        stage: None,
        snapshot_json: None,
    }));
    Ok(RunRoute {
        steps,
        blocked_reason: None,
    })
}

fn run_scope(task: &TaskRow) -> TaskScope {
    TaskScope {
        company_id: task.company_id.clone(),
        branch_id: task.branch_id.clone(),
        department_id: task.department_id.clone(),
    }
}

fn run_phase_approvers(task: &TaskRow, phase: &str) -> Result<Vec<String>, BusinessError> {
    match phase {
        "start" => task.start_approvers(),
        "completion" => task.completion_approvers(),
        _ => Err(BusinessError::invalid(
            "TASK_APPROVAL_PHASE_INVALID",
            "Use a start or completion task approval phase.",
        )),
    }
}

/// Staff Work ports for the manual routine run. The route and approval SQL stay
/// in their owning crates; this composition only binds the current serializable
/// unit and the retained cross-owner facts.
#[derive(Clone, Copy)]
struct RoutineRunTaskSources {
    now: i64,
}

impl StaffTaskFactDelegates for RoutineRunTaskSources {
    fn active_staff<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<StaffFact>, BusinessError> {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            Ok(active_staff(c, company_id, user_id)
                .await?
                .map(|fact| StaffFact {
                    branch_id: fact.branch_id,
                    department_id: fact.department_id,
                }))
        })
    }

    fn project<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        project_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<ProjectFact>, BusinessError> {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            Ok(staff_project(c, company_id, project_id)
                .await?
                .map(|fact| ProjectFact {
                    owner_user_id: fact.owner_user_id,
                    title: fact.title,
                }))
        })
    }

    fn task_route<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        scope: &'a TaskScope,
        creator_user_id: &'a str,
        assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        service_task: bool,
    ) -> StaffTaskFuture<'a, RouteFact, BusinessError> {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            let _ = service_task;
            if approvers.is_empty() {
                return Ok(RouteFact {
                    blocked_reason: active_staff(c, &scope.company_id, assignee_user_id)
                        .await?
                        .is_none()
                        .then(|| "Choose a current active staff member.".to_owned()),
                });
            }
            let route = compute_run_route(
                c,
                scope,
                creator_user_id,
                assignee_user_id,
                phase,
                approvers,
            )
            .await?;
            Ok(RouteFact {
                blocked_reason: route.blocked_reason,
            })
        })
    }

    fn open_approval<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        key: &'a str,
        task: &'a TaskRow,
        phase: &'a str,
        route: &'a RouteFact,
        version: i64,
    ) -> StaffTaskFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            if route.blocked_reason.is_some() {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "TASK_ROUTE_INVALID",
                    "The approval route is invalid for the selected scope.",
                ));
            }
            let approvers = run_phase_approvers(task, phase)?;
            let computed = compute_run_route(
                c,
                &run_scope(task),
                &task.creator_user_id,
                &task.assignee_user_id,
                phase,
                &approvers,
            )
            .await?;
            if computed.blocked_reason.is_some() {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "TASK_ROUTE_INVALID",
                    "The approval route is invalid for the selected scope.",
                ));
            }
            open_staff_approval(
                c,
                OpenStaffApproval {
                    key,
                    company_id: &task.company_id,
                    branch_id: task.branch_id.as_deref(),
                    department_id: task.department_id.as_deref(),
                    task_id: &task.id,
                    phase,
                    requester_user_id: &task.creator_user_id,
                    assignee_user_id: Some(&task.assignee_user_id),
                    record_version: version,
                    steps: &computed.steps,
                    now: self.now,
                },
            )
            .await?;
            if let Some(first) = computed.steps.first() {
                let users = if first.candidate_user_ids.is_empty() {
                    vec![first.user_id.clone()]
                } else {
                    first.candidate_user_ids.clone()
                };
                for user in users {
                    notify_staff_task_approval(
                        c,
                        key,
                        &task.company_id,
                        &user,
                        &task.id,
                        version,
                        phase,
                        self.now,
                    )
                    .await?;
                }
            }
            Ok(())
        })
    }
}

/// Retained development package switch for the manual routine run. Invalid
/// values and non-local enforcement fail closed; disabled keeps the command
/// path unguarded exactly like the retired TypeScript wrapper.
fn routine_purchase_mode(name: &'static str) -> Result<PackageEnforcementMode, BusinessError> {
    let raw = std::env::var(name).unwrap_or_else(|_| "disabled".to_owned());
    let mode = match raw.as_str() {
        "disabled" => PackageEnforcementMode::Disabled,
        "shadow" => PackageEnforcementMode::Shadow,
        "enforced" => PackageEnforcementMode::Enforced,
        _ => {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "PACKAGE_ENFORCEMENT_CONFIGURATION",
                "Package enforcement is unavailable.",
            ));
        }
    };
    if mode != PackageEnforcementMode::Disabled
        && std::env::var("ERPX_ENVIRONMENT").as_deref() != Ok("local")
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_ENVIRONMENT",
            "Package enforcement is unavailable in this environment.",
        ));
    }
    Ok(mode)
}

const WORK_EDITOR_CAPABILITIES: [&str; 2] = ["work.actions", "work.advanced-routines"];
const PROJECT_EDITOR_CAPABILITIES: [&str; 2] = ["projects.wbs", "projects.tasks"];

fn routine_read_authorization(standalone: bool) -> WorkAuthorizationRequest {
    let mut permissions = vec!["operations.read".into(), "module.operations.read".into()];
    if standalone {
        permissions.push("records.read".into());
    }
    WorkAuthorizationRequest {
        permission: WorkPermissionRequirement::AnyOf(permissions),
        module: (!standalone).then(|| "operations".into()),
        destination: "staff-tasks".into(),
        api: true,
    }
}

async fn authorize_standalone_routine_read(
    connection: &mut PgConnection,
    current: &AuthorizedWorkContext,
    admission_deferred: bool,
    now: TimestampMillis,
) -> Result<(), Error> {
    if current
        .modules()
        .iter()
        .any(|module| module == "operations")
        && (current.has_permission("operations.read")
            || current.has_permission("module.operations.read"))
    {
        return Ok(());
    }
    let denied = || {
        BusinessError::new(
            FailureKind::Forbidden,
            "ROUTINE_ACCESS_REQUIRED",
            "The current role cannot open standalone Routines.",
        )
    };
    if current.beta() || !current.has_permission("records.read") {
        return Err(denied().into());
    }
    if admission_deferred
        && routine_purchase_mode("ERPX_APPS_PACKAGE_ENFORCEMENT")?
            == PackageEnforcementMode::Disabled
    {
        return Ok(());
    }

    let company = &current.selection().workspace().company_id;
    let catalogue = current_package_catalogue()?;
    let facts = PgCompanyPurchaseReader::new(
        current.holding_id().clone(),
        company.clone(),
        connection,
        &catalogue,
    )
    .read(company)
    .await?;
    if !catalogue
        .purchased_capability(
            &facts.grants,
            company.as_str(),
            "work.advanced-routines",
            CapabilityPurpose::History,
            now.get(),
            &facts.history,
        )
        .allowed
    {
        return Err(denied().into());
    }
    Ok(())
}

fn grouped_editor_purchase(
    accesses: &BTreeMap<String, PurchasedAccess>,
    capabilities: [&str; 2],
) -> Result<serde_json::Value, BusinessError> {
    let first = accesses.get(capabilities[0]).ok_or_else(unavailable)?;
    let second = accesses.get(capabilities[1]).ok_or_else(unavailable)?;
    Ok(serde_json::json!({
        "newWork": first.new_work && second.new_work,
        "settleExisting": first.settle_existing && second.settle_existing,
        "history": first.history && second.history,
        "revision": format!("{}:{}", first.revision, second.revision),
    }))
}

fn routine_editor_context_digest(current: &AuthorizedWorkContext) -> Result<String, BusinessError> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Authority<'a> {
        session_id: &'a str,
        revision: i64,
        assignment_version: i64,
    }
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let iam = erpx_identity_access_server::public::command_metadata::project_command_iam(current)?;
    let wire = CommandJson::from_serializable(&(
        workspace.company_id.as_str(),
        workspace.branch_id.as_ref().map(|id| id.as_str()),
        workspace.department_id.as_ref().map(|id| id.as_str()),
        current.user_id().as_str(),
        assignment.id.as_str(),
        &assignment.role,
        Authority {
            session_id: current.session_id().as_str(),
            revision: selected.revision(),
            assignment_version: assignment.version,
        },
        iam,
    ))?;
    Ok(hex::encode(Sha256::digest(wire.as_str())))
}

async fn read_routine_editor_context(
    connection: &mut PgConnection,
    current: &AuthorizedWorkContext,
    strict_picker: bool,
    production_render: bool,
    now: TimestampMillis,
) -> Result<serde_json::Value, Error> {
    use PackageEnforcementMode::{Disabled, Enforced};
    let apps_mode = routine_purchase_mode("ERPX_APPS_PACKAGE_ENFORCEMENT")?;
    let projects_mode = routine_purchase_mode("ERPX_PROJECTS_PACKAGE_ENFORCEMENT")?;
    let company = &current.selection().workspace().company_id;
    let user = current.user_id().as_str();
    if (strict_picker || projects_mode != Disabled) && !current.has_permission("records.read") {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PROJECT_ACCESS_CHANGED",
            "Choose a current Project assignment.",
        )
        .into());
    }
    let catalogue = current_package_catalogue()?;
    let facts = if strict_picker || apps_mode != Disabled || projects_mode != Disabled {
        PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            company.clone(),
            connection,
            &catalogue,
        )
        .read(company)
        .await?
    } else {
        erpx_commercial_server::public::purchases::CompanyPurchaseFacts {
            grants: Vec::new(),
            history: Vec::new(),
        }
    };
    let project_capabilities = PROJECT_EDITOR_CAPABILITIES.map(str::to_owned);
    let project_access = purchased_accesses(
        &catalogue,
        &facts,
        company.as_str(),
        &project_capabilities,
        projects_mode,
        now,
    )?;
    let picker_history = if strict_picker {
        catalogue
            .purchased_capability(
                &facts.grants,
                company.as_str(),
                "projects.wbs",
                CapabilityPurpose::History,
                now.get(),
                &facts.history,
            )
            .allowed
    } else if projects_mode == Enforced {
        project_access
            .get("projects.wbs")
            .ok_or_else(unavailable)?
            .history
    } else {
        true
    };
    // The Apps editor has no isolated engineering attestation (only the
    // Service child can receive that exception). Projects is outside the
    // current production release scope, so its conjunction is false for a
    // strict production render. Development only excludes beta Companies.
    let picker_ready = picker_history && !(strict_picker && (production_render || current.beta()));
    let projects = if picker_ready {
        erpx_projects_server::public::native_lane_27::read_routine_project_options(
            connection,
            company.as_str(),
            user,
        )
        .await?
    } else {
        Vec::new()
    };
    let purchases = if apps_mode == Disabled {
        serde_json::Value::Null
    } else {
        let work = purchased_accesses(
            &catalogue,
            &facts,
            company.as_str(),
            &WORK_EDITOR_CAPABILITIES.map(str::to_owned),
            apps_mode,
            now,
        )?;
        serde_json::json!({
            "work": grouped_editor_purchase(&work, WORK_EDITOR_CAPABILITIES)?,
            "projects": grouped_editor_purchase(&project_access, PROJECT_EDITOR_CAPABILITIES)?,
            "context": routine_editor_context_digest(current)?,
        })
    };
    let mut data =
        crate::routine_editor_staff::routine_editor_staff(connection, company.as_str(), user)
            .await?;
    let fields = data.as_object_mut().ok_or_else(unavailable)?;
    fields.insert("projects".into(), serde_json::json!(projects));
    fields.insert("purchases".into(), purchases);
    Ok(data)
}

async fn require_routine_history(
    connection: &mut PgConnection,
    current: &AuthorizedWorkContext,
    project_id: Option<&str>,
    now: i64,
) -> Result<(), Error> {
    let apps = routine_purchase_mode("ERPX_APPS_PACKAGE_ENFORCEMENT")?;
    let projects = routine_purchase_mode("ERPX_PROJECTS_PACKAGE_ENFORCEMENT")?;
    let company = &current.selection().workspace().company_id;
    let missing = || {
        BusinessError::new(
            FailureKind::NotFound,
            "TEMPLATE_NOT_FOUND",
            "Routine template not found.",
        )
    };
    if apps == PackageEnforcementMode::Enforced || projects == PackageEnforcementMode::Enforced {
        if let Some(project_id) = project_id {
            if staff_project(connection, company.as_str(), project_id)
                .await?
                .is_none()
                || projects == PackageEnforcementMode::Enforced
                    && !current.has_permission("records.read")
            {
                return Err(missing().into());
            }
        }
    }
    let capabilities: &[&str] = match project_id {
        None if apps == PackageEnforcementMode::Enforced => {
            &["work.actions", "work.advanced-routines"]
        }
        Some(_) if projects == PackageEnforcementMode::Enforced => {
            &["projects.wbs", "projects.tasks"]
        }
        _ => return Ok(()),
    };
    let catalogue = current_package_catalogue()?;
    let facts = PgCompanyPurchaseReader::new(
        current.holding_id().clone(),
        company.clone(),
        connection,
        &catalogue,
    )
    .read(company)
    .await?;
    for capability in capabilities {
        if !catalogue
            .purchased_capability(
                &facts.grants,
                company.as_str(),
                capability,
                CapabilityPurpose::History,
                now,
                &facts.history,
            )
            .allowed
        {
            return Err(missing().into());
        }
    }
    Ok(())
}

/// The retained `getProject` visibility rule: Company scope plus current
/// accountable owner or explicit Project membership.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RoutineProjectSource {
    id: String,
    company_id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
    owner_user_id: String,
}

async fn routine_project_source(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    project_id: &str,
) -> Result<Option<RoutineProjectSource>, Error> {
    let row: Option<(String, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.owner_user_id \
         FROM staff_projects p \
         WHERE p.id=$1 AND p.company_id=$2 \
         AND (p.owner_user_id=$3 OR EXISTS(SELECT 1 FROM staff_project_members m \
          WHERE m.project_id=p.id AND m.user_id=$3)) LIMIT 1",
    )
    .bind(project_id)
    .bind(company_id)
    .bind(user_id)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(
        |(id, company_id, branch_id, department_id, owner_user_id)| RoutineProjectSource {
            id,
            company_id,
            branch_id,
            department_id,
            owner_user_id,
        },
    ))
}

async fn require_routine_project(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    project_id: &str,
) -> Result<RoutineProjectSource, Error> {
    routine_project_source(connection, company_id, user_id, project_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "PROJECT_NOT_FOUND",
                "Project not found.",
            )
            .into()
        })
}

fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn purchase_available(alias: &str, now: &str) -> String {
    format!(
        "{alias}.starts_at<={now} AND {alias}.state='active' \
         AND ({alias}.ends_at IS NULL OR {alias}.ends_at>{now})"
    )
}

/// SQL equivalent of the commercial purchased-capability predicate for the
/// new-work purpose. Definitions and dependency versions come from the current
/// sealed catalogue, never from the request.
fn purchased_capability_sql(
    catalogue: &CommercialCatalogue,
    company: &str,
    capability: &str,
    now: &str,
) -> String {
    let candidates = catalogue
        .definitions()
        .iter()
        .filter(|definition| {
            definition
                .visible_capabilities
                .iter()
                .chain(&definition.included_capabilities)
                .any(|value| value == capability)
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return "FALSE".into();
    }
    candidates
        .iter()
        .map(|definition| {
            let mut sql = format!(
                "(EXISTS(SELECT 1 FROM company_package_grants purchase \
                 WHERE purchase.company_id={company} AND purchase.package_id={} \
                 AND purchase.package_version={} AND {})",
                sql_literal(&definition.id),
                definition.version,
                purchase_available("purchase", now)
            );
            for dependency in &definition.dependencies {
                let versions = catalogue
                    .definitions()
                    .iter()
                    .filter(|parent| &parent.id == dependency)
                    .map(|parent| parent.version.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                sql.push_str(&format!(
                    " AND EXISTS(SELECT 1 FROM company_package_grants parent \
                     WHERE parent.company_id={company} AND parent.package_id={} \
                     AND parent.package_version IN ({versions}) AND {})",
                    sql_literal(dependency),
                    purchase_available("parent", now)
                ));
            }
            sql.push(')');
            sql
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Retained queued selection scope for one Holding. In development or disabled
/// package mode the scheduler keeps `TRUE`; enforced mode filters invalid
/// authority, staff, Project membership, purchase and release before LIMIT.
pub(crate) fn occurrence_policy(production: bool) -> Result<OccurrencePolicy, BusinessError> {
    let apps_mode = routine_purchase_mode("ERPX_APPS_PACKAGE_ENFORCEMENT")?;
    if apps_mode != PackageEnforcementMode::Enforced {
        return Ok(OccurrencePolicy {
            production,
            scope_sql: None,
        });
    }
    let projects_mode = routine_purchase_mode("ERPX_PROJECTS_PACKAGE_ENFORCEMENT")?;
    let catalogue = current_package_catalogue()?;
    let now = "routine_job_clock.now_ms";
    let actor = "work_routine_authorization.authority_json::jsonb";
    let user = format!("({actor}->>'userId')");
    let role = format!("({actor}->>'role')");
    let company = "work_routine_authorization.company_id";
    let assignment = "work_routine_authorization.assignment_id";
    let branch = format!("({actor}->>'branchId')");
    let department = format!("({actor}->>'departmentId')");
    let session = format!("({actor}->'authority'->>'sessionId')");
    let assignment_version = format!(
        "COALESCE(({actor}->'authority'->>'assignmentVersion')::integer,\
         ({actor}->'iam'->>'queuedAssignmentVersion')::integer,\
         ({actor}->>'assignmentVersion')::integer)"
    );
    let apps = ["work.actions", "work.advanced-routines"]
        .iter()
        .map(|capability| purchased_capability_sql(&catalogue, "t.company_id", capability, now))
        .collect::<Vec<_>>()
        .join(" AND ");
    let project_purchase = ["projects.wbs", "projects.tasks"]
        .iter()
        .map(|capability| purchased_capability_sql(&catalogue, "t.company_id", capability, now))
        .collect::<Vec<_>>()
        .join(" AND ");
    let project = if projects_mode == PackageEnforcementMode::Enforced {
        format!(
            "EXISTS(SELECT 1 FROM staff_projects routine_project \
             WHERE routine_project.id=t.project_id AND routine_project.company_id=t.company_id \
             AND iam_role_allows({role},routine_project.company_id,'records.read') \
             AND (routine_project.owner_user_id={user} OR EXISTS(SELECT 1 FROM staff_project_members routine_member \
             WHERE routine_member.project_id=routine_project.id AND routine_member.user_id={user}))) \
             AND ({project_purchase})"
        )
    } else {
        "TRUE".into()
    };
    let config = "jsonb_build_object('template',to_jsonb(t),'schedule',to_jsonb(s)-\
                  ARRAY['next_occurrence_at','last_generated_at','last_error','updated_at','paused_at','state'])::text";
    let staff = format!(
        "(SELECT COUNT(*) FROM hr_people p \
         JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
         WHERE p.company_id=s.company_id AND p.status='active' AND p.merged_into_id IS NULL \
         AND iam_same_person(p.user_id,{user},p.company_id) AND e.status='active' \
         AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
         AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))=1"
    );
    let production_predicate = if production {
        " AND EXISTS(SELECT 1 FROM company_feature_rollouts f \
         WHERE f.company_id=s.company_id AND f.module_id='operations' \
         AND f.rollout_state IN ('pilot','available'))"
    } else {
        ""
    };
    let scope = format!(
        "EXISTS(WITH RECURSIVE routine_job_clock AS MATERIALIZED(SELECT \
         floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms),\
         routine_lineage AS(SELECT a.command_id,a.initial_occurrence_at cursor,0 depth \
         FROM work_routine_execution_authorizations a \
         WHERE a.schedule_id=s.id AND a.schedule_version=s.version \
         UNION ALL SELECT lineage.command_id,step.next_occurrence_at,lineage.depth+1 \
         FROM routine_lineage lineage JOIN work_routine_execution_steps step \
         ON step.authorization_command_id=lineage.command_id AND step.occurrence_at=lineage.cursor \
         WHERE lineage.depth<10000) \
         SELECT 1 FROM routine_templates t CROSS JOIN routine_job_clock \
         JOIN work_routine_execution_authorizations work_routine_authorization \
         ON work_routine_authorization.schedule_id=s.id \
         AND work_routine_authorization.schedule_version=s.version \
         AND work_routine_authorization.template_id=t.id \
         AND work_routine_authorization.company_id=t.company_id \
         WHERE t.id=s.template_id AND t.company_id=s.company_id AND t.published=1 \
         AND work_routine_authorization.template_version=t.version \
         AND work_routine_authorization.source_hash=encode(sha256(convert_to({config},'UTF8')),'hex') \
         AND work_routine_authorization.command_id=(SELECT latest.command_id \
         FROM work_routine_execution_authorizations latest \
         WHERE latest.schedule_id=s.id AND latest.schedule_version=s.version \
         ORDER BY latest.created_at DESC,latest.command_id DESC LIMIT 1) \
         AND {actor}->>'companyId'={company} \
         AND {actor}->>'userId'=work_routine_authorization.actor_id \
         AND {actor}->>'assignmentId'={assignment} \
         AND {assignment_version}>0 \
         AND EXISTS(SELECT 1 FROM assignments queued_assignment \
         WHERE queued_assignment.id={assignment} AND queued_assignment.user_id={user} \
         AND queued_assignment.role={role} AND queued_assignment.company_id={company} \
         AND queued_assignment.version={assignment_version} \
         AND iam_assignment_active(queued_assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND iam_principal_active(queued_assignment.user_id,queued_assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND (queued_assignment.scope_kind='holding' OR queued_assignment.scope_kind='branch' \
           AND queued_assignment.scope_node_id={branch} OR queued_assignment.scope_kind='department' \
           AND queued_assignment.scope_node_id={department}) \
         AND EXISTS(SELECT 1 FROM iam_roles queued_role WHERE queued_role.id=queued_assignment.role \
           AND queued_role.company_id IN ('*',{company}) AND queued_role.status='active' \
           AND ({actor}->'iam'->>'roleRevision' IS NULL \
             OR queued_role.published_revision=({actor}->'iam'->>'roleRevision')::integer))) \
         AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r \
         WHERE r.session_id={session} AND r.company_id={company} AND r.user_id={user}) \
         AND ({actor}->'iam'->>'identityVersion' IS NULL \
           OR COALESCE((SELECT version FROM iam_members m \
             WHERE m.company_id={company} AND m.user_id={user}),0)=({actor}->'iam'->>'identityVersion')::integer) \
         AND ({actor}->'iam'->>'emergencyUntil' IS NULL \
           OR ({actor}->'iam'->>'emergencyUntil')::bigint>routine_job_clock.now_ms) \
         AND ({actor}->'authority' IS NULL OR {actor}->'authority'='null'::jsonb \
           OR EXISTS(SELECT 1 FROM active_work_contexts c WHERE c.session_id={session} \
             AND c.company_id={company} AND c.user_id={user} AND c.assignment_id={assignment} \
             AND c.revision=({actor}->'authority'->>'revision')::integer \
             AND c.assignment_version=({actor}->'authority'->>'assignmentVersion')::integer \
             AND c.assignment_version={assignment_version} AND c.expires_at>routine_job_clock.now_ms)) \
         AND ({actor}->'iam'->>'hrAppointmentVersion' IS NULL \
           OR EXISTS(SELECT 1 FROM authority_seats s \
             JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id \
             JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
             WHERE s.company_id={company} AND s.kind='hr' AND s.scope_id={company} \
             AND s.version=({actor}->'iam'->>'hrAppointmentVersion')::integer \
             AND s.effective_at<=routine_job_clock.now_ms \
             AND iam_same_person(p.user_id,{user},s.company_id))) \
         AND CASE WHEN t.project_id IS NULL THEN ({apps}) ELSE ({project}) END \
         AND {staff} \
         AND EXISTS(SELECT 1 FROM routine_lineage chain \
         WHERE chain.command_id=work_routine_authorization.command_id \
         AND chain.cursor=s.next_occurrence_at \
         AND chain.depth=(SELECT count(*) FROM work_routine_execution_steps step \
           WHERE step.authorization_command_id=chain.command_id)) \
         AND NOT EXISTS(SELECT 1 FROM work_routine_execution_steps step \
         WHERE step.authorization_command_id=work_routine_authorization.command_id \
         AND (step.source_hash<>work_routine_authorization.source_hash OR step.generated AND NOT EXISTS(\
         SELECT 1 FROM staff_tasks task \
         JOIN audit_events audit ON audit.command_id=step.command_id AND audit.record_id=task.id \
         AND audit.record_version=1 AND audit.action='routine.occurrence' AND audit.record_type='staff_task' \
         JOIN outbox_events outbox ON outbox.command_id=audit.command_id \
         AND outbox.aggregate_id=task.id AND outbox.aggregate_version=1 \
         AND outbox.event_type='task.generated' AND outbox.aggregate_type='staff_task' \
         WHERE task.id=step.task_id AND task.company_id=s.company_id AND task.schedule_id=s.id \
         AND task.occurrence_at=step.occurrence_at AND task.template_id=t.id \
         AND task.template_version=t.version)))){production_predicate})"
    );
    Ok(OccurrencePolicy {
        production,
        scope_sql: Some(scope),
    })
}

struct OccurrenceProject {
    source: RoutineProjectSource,
}

async fn occurrence_project(
    connection: &mut PgConnection,
    actor: &StaffTaskActor,
    role: &str,
    project_id: &str,
) -> Result<OccurrenceProject, PgTransactionError<BusinessError>> {
    let source = routine_project_source(connection, &actor.company_id, &actor.user_id, project_id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "PROJECT_NOT_FOUND",
                "Project not found in the current workspace.",
            )
        })?;
    let allowed: bool = sqlx::query_scalar(
        "SELECT iam_role_allows($1,$2,'records.read') \
         AND EXISTS(SELECT 1 FROM assignments g WHERE g.id=$3 AND g.user_id=$4 \
         AND g.role=$1 AND g.company_id=$2 \
         AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=$5 \
           OR g.scope_kind='department' AND g.scope_node_id=$6))",
    )
    .bind(role)
    .bind(&actor.company_id)
    .bind(&actor.assignment_id)
    .bind(&actor.user_id)
    .bind(actor.branch_id.as_deref())
    .bind(actor.department_id.as_deref())
    .fetch_one(&mut *connection)
    .await?;
    if !allowed {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PROJECT_ACCESS_CHANGED",
            "Choose a current Project assignment.",
        )
        .into());
    }
    Ok(OccurrenceProject { source })
}

fn occurrence_actor(
    authority: &erpx_operations_server::public::routine_occurrences::StoredRoutineAuthority,
) -> StaffTaskActor {
    StaffTaskActor {
        user_id: authority.user_id.clone(),
        assignment_id: authority.assignment_id.clone(),
        company_id: authority.company_id.clone(),
        branch_id: authority.branch_id.clone(),
        department_id: authority.department_id.clone(),
        permissions: authority.permissions.clone(),
        modules: Vec::new(),
        expires_at: i64::MAX,
    }
}

fn synthetic_occurrence_actor(
    template: &erpx_operations_server::public::routine_commands::RoutineTemplateRow,
) -> StaffTaskActor {
    StaffTaskActor {
        user_id: template.creator_user_id.clone(),
        assignment_id: String::new(),
        company_id: template.company_id.clone(),
        branch_id: template.branch_id.clone(),
        department_id: template.department_id.clone(),
        permissions: Vec::new(),
        modules: Vec::new(),
        expires_at: i64::MAX,
    }
}

/// One composition-owned scheduled occurrence unit. It binds the captured
/// authorization, current IAM, package fences and the native routine task write
/// in the same serializable transaction the Operations pass retries.
pub(crate) struct RoutineOccurrencePassUnit {
    holding: HoldingId,
    production: bool,
    apps_mode: PackageEnforcementMode,
    projects_mode: PackageEnforcementMode,
    catalogue: CommercialCatalogue,
}

impl RoutineOccurrencePassUnit {
    pub(crate) fn new(holding: HoldingId, production: bool) -> Result<Self, BusinessError> {
        Ok(Self {
            holding,
            production,
            apps_mode: routine_purchase_mode("ERPX_APPS_PACKAGE_ENFORCEMENT")?,
            projects_mode: routine_purchase_mode("ERPX_PROJECTS_PACKAGE_ENFORCEMENT")?,
            catalogue: current_package_catalogue()?,
        })
    }
}

impl OccurrenceUnit for RoutineOccurrencePassUnit {
    fn run<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        selected: &'a erpx_operations_server::public::routine_commands::RoutineScheduleRow,
        at: i64,
        now: i64,
    ) -> RoutineOccurrenceFuture<'a, Option<OccurrenceOutcome>> {
        Box::pin(async move {
            let Some(source) =
                occurrence_source(connection, &selected.company_id, &selected.id, at).await?
            else {
                return Ok(None);
            };
            let original = &source.schedule;
            if original.state != "active"
                || original.version != selected.version
                || original.next_occurrence_at != Some(at)
            {
                return Ok(None);
            }
            let project_mode = self.projects_mode != PackageEnforcementMode::Disabled;
            let enforced = self.apps_mode == PackageEnforcementMode::Enforced;
            let Some(template) = source.template.as_ref() else {
                if enforced {
                    return Ok(None);
                }
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "TEMPLATE_UNAVAILABLE",
                    "The published routine template is unavailable.",
                )
                .into());
            };
            if !template.is_published() {
                if enforced {
                    return Ok(None);
                }
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "TEMPLATE_UNAVAILABLE",
                    "The published routine template is unavailable.",
                )
                .into());
            }
            if self.production && !production_enabled(connection, &original.company_id).await? {
                return Err(production_forbidden().into());
            }
            if self.apps_mode == PackageEnforcementMode::Shadow {
                sqlx::query(
                    "SELECT package_id FROM company_package_grants WHERE company_id=$1 \
                     ORDER BY package_id",
                )
                .bind(&original.company_id)
                .fetch_all(&mut *connection)
                .await?;
            }
            let mut authorization: Option<RoutineExecutionAuthorization> = None;
            let mut actor = synthetic_occurrence_actor(template);
            let mut boundaries: Vec<PurchaseCommandBoundary> = Vec::new();
            let mut assertion_keys: Vec<String> = Vec::new();
            let mut project_before: Option<RoutineProjectSource> = None;
            if enforced {
                let company = CompanyId::new(original.company_id.clone())?;
                lock_company_purchase_read_sources(connection, &company).await?;
                lock_occurrence_sources(connection, &original.company_id).await?;
                let stored = occurrence_authorization(connection, &original.id, original.version)
                    .await?
                    .ok_or_else(|| {
                        BusinessError::new(
                            FailureKind::Conflict,
                            "WORK_ROUTINE_AUTHORIZATION_REQUIRED",
                            "Pause and resume this retained schedule with its original current owner before generation.",
                        )
                    })?;
                let stored_actor = stored.actor()?;
                require_authorization_source(
                    connection,
                    &stored,
                    template,
                    original,
                    &stored_actor,
                )
                .await?;
                require_lineage(connection, &stored, Some(at)).await?;
                require_current_authority(connection, &stored, &stored_actor, now).await?;
                actor = occurrence_actor(&stored_actor);
                let base = format!("occurrence-{}-{at}", original.id);
                if template.project_id.is_none() {
                    if self.apps_mode == PackageEnforcementMode::Enforced {
                        for (index, capability) in ["work.actions", "work.advanced-routines"]
                            .iter()
                            .enumerate()
                        {
                            let key = format!("{base}:apps:{index}");
                            if let Some(mut boundary) = PurchaseCommandBoundary::new(
                                self.apps_mode,
                                key.clone(),
                                PurchaseCommandScope {
                                    company_id: company.clone(),
                                    capability: (*capability).to_owned(),
                                    purpose: CapabilityPurpose::NewWork,
                                },
                            )? {
                                boundary
                                    .begin(connection, &self.holding, &self.catalogue)
                                    .await?;
                                boundaries.push(boundary);
                                assertion_keys.push(key);
                            }
                        }
                    }
                } else if project_mode {
                    let project_id = template.project_id.as_deref().expect("project checked");
                    let project =
                        occurrence_project(connection, &actor, &stored_actor.role, project_id)
                            .await?;
                    project_before = Some(project.source);
                    for (index, capability) in ["projects.wbs", "projects.tasks"].iter().enumerate()
                    {
                        let key = format!("{base}:project:{index}");
                        if let Some(mut boundary) = PurchaseCommandBoundary::new(
                            self.projects_mode,
                            key.clone(),
                            PurchaseCommandScope {
                                company_id: company.clone(),
                                capability: (*capability).to_owned(),
                                purpose: CapabilityPurpose::NewWork,
                            },
                        )? {
                            boundary
                                .begin(connection, &self.holding, &self.catalogue)
                                .await?;
                            boundaries.push(boundary);
                            assertion_keys.push(key);
                        }
                    }
                }
                authorization = Some(stored);
            }
            let mut facts = OccurrenceSourceFacts {
                pause_exists: source.pause_exists,
                pause_ended_at: source.pause_ended_at,
                prior: source.prior,
                start_blocked: None,
                completion_blocked: None,
                project_available: true,
            };
            if !facts.pause_exists && !facts.prior {
                let scope = TaskScope {
                    company_id: template.company_id.clone(),
                    branch_id: template.branch_id.clone(),
                    department_id: template.department_id.clone(),
                };
                let mut repository = PgStaffTasks::new(&mut *connection);
                let mut sources = PgStaffTaskSources::new(RoutineRunTaskSources { now });
                let start_route = sources
                    .task_route(
                        repository.connection(),
                        &scope,
                        &template.creator_user_id,
                        &template.assignee_user_id,
                        "start",
                        &template.start_approvers()?,
                        false,
                    )
                    .await?;
                let completion_route = sources
                    .task_route(
                        repository.connection(),
                        &scope,
                        &template.creator_user_id,
                        &template.assignee_user_id,
                        "completion",
                        &template.completion_approvers()?,
                        false,
                    )
                    .await?;
                facts.start_blocked = start_route.blocked_reason;
                facts.completion_blocked = completion_route.blocked_reason;
                facts.project_available = match template.project_id.as_deref() {
                    Some(project_id) => sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM staff_projects WHERE id=$1 AND company_id=$2)",
                    )
                    .bind(project_id)
                    .bind(&template.company_id)
                    .fetch_one(&mut *connection)
                    .await?,
                    None => true,
                };
            }
            let plan = plan_occurrence(template, original, &facts, at)?;
            let generated = !plan.state.is_empty();
            if generated {
                let input = SaveTaskCommand {
                    title: template.title.clone(),
                    description: template.description.clone(),
                    assignee_user_id: template.assignee_user_id.clone(),
                    project_id: template.project_id.clone(),
                    start_approvers: plan.start_approvers.clone(),
                    completion_approvers: plan.completion_approvers.clone(),
                    due_at: Some(
                        erpx_operations_server::public::routine_occurrences::due_at_iso(
                            plan.due_at,
                        )?,
                    ),
                    expected_version: 1,
                    reason: String::new(),
                    source: None,
                };
                let fingerprint = fingerprint_json(
                    "routine.occurrence",
                    &plan.task_id,
                    &serde_json::json!({
                        "scheduleId": original.id,
                        "occurrenceAt": at,
                    }),
                )?;
                let mut repository = PgStaffTasks::new(&mut *connection);
                let mut sources = PgStaffTaskSources::new(RoutineRunTaskSources { now });
                save_task_mode(
                    &actor,
                    &mut repository,
                    &mut sources,
                    None,
                    &input,
                    &plan.key,
                    false,
                    now,
                    SaveTaskMode::Routine(RoutineTaskWrite {
                        template_id: template.id.clone(),
                        template_version: template.version,
                        branch_id: template.branch_id.clone(),
                        department_id: template.department_id.clone(),
                        state: plan.state.clone(),
                        blocked_reason: plan.blocked_reason.clone(),
                        notification: plan.notification.clone(),
                        creator_notification: plan.creator_notification.clone(),
                        open_start_approval: plan.open_start_approval,
                        request_fingerprint: fingerprint,
                        creator_user_id: template.creator_user_id.clone(),
                        record_id: Some(plan.task_id.clone()),
                        schedule_id: Some(original.id.clone()),
                        occurrence_at: Some(at),
                        due_at_millis: Some(plan.due_at),
                        require_write: false,
                        require_current_staff: false,
                        write_ledger: false,
                    }),
                )
                .await?;
            }
            let completed = stage_occurrence(
                connection,
                authorization.as_ref(),
                &original.id,
                &original.company_id,
                original.version,
                &template.id,
                at,
                plan.next,
                generated,
                &plan.task_id,
                &plan.state,
                template.branch_id.as_deref(),
                template.department_id.as_deref(),
                now,
                &assertion_keys,
            )
            .await?;
            if let (Some(_authorization), Some(_completed)) =
                (authorization.as_ref(), completed.as_ref())
            {
                let after = occurrence_image(
                    connection,
                    &original.company_id,
                    &template.id,
                    &original.id,
                    &plan.task_id,
                )
                .await?;
                if after.as_deref() != completed.as_deref() {
                    return Err(source_changed_occurrence());
                }
                if let Some(before) = project_before.as_ref() {
                    let current = routine_project_source(
                        connection,
                        &actor.company_id,
                        &actor.user_id,
                        &before.id,
                    )
                    .await?
                    .ok_or_else(|| {
                        BusinessError::new(
                            FailureKind::NotFound,
                            "PROJECT_NOT_FOUND",
                            "Project not found in the current workspace.",
                        )
                    })?;
                    if &current != before {
                        return Err(source_changed_occurrence());
                    }
                }
                require_lineage(
                    connection,
                    authorization.as_ref().expect("authorization checked"),
                    plan.next,
                )
                .await?;
                let stored = authorization.as_ref().expect("authorization checked");
                let stored_actor = stored.actor()?;
                require_current_authority(connection, stored, &stored_actor, now).await?;
            }
            for mut boundary in boundaries.drain(..) {
                let receipt = boundary
                    .complete(connection, &self.holding, &self.catalogue)
                    .await?;
                if let Some(observation) = receipt.observation {
                    write_purchase_shadow_checked_observation(&mut std::io::stderr(), &observation);
                }
            }
            Ok(Some(OccurrenceOutcome {
                next: plan.next,
                generated,
            }))
        })
    }
}

fn source_changed_occurrence() -> PgTransactionError<BusinessError> {
    source_changed().into()
}

/// One bounded scheduled occurrence pass for a registered Holding. The worker
/// supervisor owns cadence and restart logging; this function owns the native
/// policy, unit and retry.
pub(crate) async fn run_routine_occurrence_pass(
    pool: DatabasePool,
    holding: HoldingId,
    now: i64,
    limit: i64,
    production: bool,
) -> Result<OccurrencePass, PgTransactionError<BusinessError>> {
    let policy = occurrence_policy(production)?;
    let unit = RoutineOccurrencePassUnit::new(holding, production)?;
    generate_occurrences(pool, now, limit, policy, unit).await
}

struct RoutineRunAdmission {
    boundaries: Vec<PurchaseCommandBoundary>,
}

impl RoutineRunAdmission {
    async fn complete(
        mut self,
        connection: &mut PgConnection,
        holding: &HoldingId,
    ) -> Result<(), Error> {
        if self.boundaries.is_empty() {
            return Ok(());
        }
        let catalogue = current_package_catalogue()?;
        for mut boundary in self.boundaries.drain(..) {
            let receipt = boundary.complete(connection, holding, &catalogue).await?;
            if let Some(observation) = receipt.observation {
                write_purchase_shadow_checked_observation(&mut std::io::stderr(), &observation);
            }
        }
        Ok(())
    }
}

/// Retained Apps and Projects purchase admission for one manual run. A replay
/// is admitted under `history`; first work requires `new-work`. Each capability
/// gets the same before/after Company package fence the retired wrapper used.
async fn begin_routine_run_admission(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    project_id: Option<&str>,
    key: &str,
    replay: bool,
) -> Result<RoutineRunAdmission, Error> {
    let apps_mode = routine_purchase_mode("ERPX_APPS_PACKAGE_ENFORCEMENT")?;
    let projects_mode = routine_purchase_mode("ERPX_PROJECTS_PACKAGE_ENFORCEMENT")?;
    if apps_mode == PackageEnforcementMode::Disabled
        && (project_id.is_none() || projects_mode == PackageEnforcementMode::Disabled)
    {
        return Ok(RoutineRunAdmission {
            boundaries: Vec::new(),
        });
    }
    let catalogue = current_package_catalogue()?;
    let purpose = if replay {
        CapabilityPurpose::History
    } else {
        CapabilityPurpose::NewWork
    };
    let mut boundaries = Vec::new();
    let begin = |mode: PackageEnforcementMode, capability: &str, suffix: &str| {
        let boundary = PurchaseCommandBoundary::new(
            mode,
            format!("{key}{suffix}"),
            PurchaseCommandScope {
                company_id: company.clone(),
                capability: capability.to_owned(),
                purpose,
            },
        )?;
        Ok::<_, BusinessError>(boundary)
    };
    if apps_mode != PackageEnforcementMode::Disabled {
        for (index, capability) in ["work.actions", "work.advanced-routines"]
            .iter()
            .enumerate()
        {
            let suffix = format!(":apps:{index}");
            if let Some(mut boundary) = begin(apps_mode, capability, &suffix)? {
                boundary.begin(connection, holding, &catalogue).await?;
                boundaries.push(boundary);
            }
        }
    }
    if let Some(_project_id) = project_id
        && projects_mode != PackageEnforcementMode::Disabled
    {
        for (index, capability) in ["projects.wbs", "projects.tasks"].iter().enumerate() {
            let suffix = format!(":project:{index}");
            if let Some(mut boundary) = begin(projects_mode, capability, &suffix)? {
                boundary.begin(connection, holding, &catalogue).await?;
                boundaries.push(boundary);
            }
        }
    }
    Ok(RoutineRunAdmission { boundaries })
}

/// The completed occurrence may not change the template snapshot or Project
/// membership between the owner write and commit.
async fn verify_routine_run_source(
    connection: &mut PgConnection,
    template: &erpx_operations_server::public::routine_commands::RoutineTemplateRow,
) -> Result<(), Error> {
    let current: Option<(
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
    )> = sqlx::query_as(
        "SELECT company_id,project_id,branch_id,creator_user_id,title,description,\
         assignee_user_id,start_approvers_json,completion_approvers_json,version,published \
         FROM routine_templates WHERE id=$1 AND company_id=$2",
    )
    .bind(&template.id)
    .bind(&template.company_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some((
        company_id,
        project_id,
        branch_id,
        creator_user_id,
        title,
        description,
        assignee_user_id,
        start_json,
        completion_json,
        version,
        published,
    )) = current
    else {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "WORK_ROUTINE_SOURCE_CHANGED",
            "The completed original routine source changed during deferred effects.",
        )
        .into());
    };
    if company_id != template.company_id
        || project_id != template.project_id
        || branch_id != template.branch_id
        || creator_user_id != template.creator_user_id
        || title != template.title
        || description != template.description
        || assignee_user_id != template.assignee_user_id
        || start_json != template.start_approvers_json
        || completion_json != template.completion_approvers_json
        || version != template.version
        || published != template.published
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "WORK_ROUTINE_SOURCE_CHANGED",
            "The completed original routine source changed during deferred effects.",
        )
        .into());
    }
    Ok(())
}

pub struct RoutineConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl RoutineConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }
    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

pub struct RoutineService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: RoutineConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> RoutineService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: RoutineConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        &self.configuration.company
    }

    async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        key: CommandKey,
        command: RoutineHttpCommand,
    ) -> Result<RoutineHttpResponse, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("operations".into()),
                    destination: "staff-tasks".into(),
                    api: true,
                },
                RoutineSources,
                move |connection, current| {
                    let command = command.clone();
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let actor = actor(current)?;
                        let now = clock.now().get();
                        let command = command.clone();
                        let routine_authority = RoutineAuthority {
                            actor_json: context(current).to_string(),
                            role: current.selection().assignment().role.clone(),
                        };
                        let data = match command {
                            RoutineHttpCommand::SaveTemplate {
                                id,
                                clone_id,
                                command,
                            } => {
                                let mut repository = PgRoutineCommands::new(connection);
                                let mut sources = PgRoutineSources::new(RoutineDelegates);
                                save_template(
                                    &actor,
                                    &mut repository,
                                    &mut sources,
                                    id.as_deref(),
                                    &command,
                                    clone_id.as_deref(),
                                    key.as_str(),
                                    now,
                                )
                                .await
                                .map(|receipt| serde_json::to_value(receipt).unwrap_or_default())?
                            }
                            RoutineHttpCommand::SaveSchedule {
                                template_id,
                                id,
                                command,
                            } => {
                                let mut repository = PgRoutineCommands::new(connection);
                                let mut sources = PgRoutineSources::new(RoutineDelegates);
                                save_schedule(
                                    &actor,
                                    &mut repository,
                                    &mut sources,
                                    &template_id,
                                    id.as_deref(),
                                    &command,
                                    key.as_str(),
                                    Some(RoutineCapture {
                                        kind: if id.is_some() { "update" } else { "create" },
                                        authority: routine_authority.clone(),
                                    }),
                                    now,
                                )
                                .await
                                .map(|receipt| serde_json::to_value(receipt).unwrap_or_default())?
                            }
                            RoutineHttpCommand::ChangeScheduleState {
                                id,
                                resume,
                                command,
                            } => {
                                let mut repository = PgRoutineCommands::new(connection);
                                let mut sources = PgRoutineSources::new(RoutineDelegates);
                                change_schedule_state(
                                    &actor,
                                    &mut repository,
                                    &mut sources,
                                    &id,
                                    &command,
                                    resume,
                                    key.as_str(),
                                    if resume {
                                        Some(RoutineCapture {
                                            kind: "resume",
                                            authority: routine_authority.clone(),
                                        })
                                    } else {
                                        None
                                    },
                                    now,
                                )
                                .await
                                .map(|receipt| serde_json::to_value(receipt).unwrap_or_default())?
                            }
                            RoutineHttpCommand::EndSchedule { id, command } => {
                                let mut repository = PgRoutineCommands::new(connection);
                                let mut sources = PgRoutineSources::new(RoutineDelegates);
                                end_schedule(
                                    &actor,
                                    &mut repository,
                                    &mut sources,
                                    &id,
                                    &command,
                                    key.as_str(),
                                    now,
                                )
                                .await
                                .map(|receipt| serde_json::to_value(receipt).unwrap_or_default())?
                            }
                            RoutineHttpCommand::RunTemplate {
                                template_id,
                                due_at,
                            } => {
                                let stored = {
                                    let mut repository = PgRoutineCommands::new(connection);
                                    repository.stored_receipt(key.as_str()).await?
                                };
                                let replay = stored.as_ref().is_some_and(|stored| {
                                    stored.operation == "routine.run"
                                        && stored.company_id == actor.company_id
                                        && stored.actor_id == actor.user_id
                                });
                                let existing_template = {
                                    let mut repository = PgRoutineCommands::new(connection);
                                    repository.template(&actor.company_id, &template_id).await?
                                };
                                let template = if replay {
                                    None
                                } else {
                                    let mut repository = PgRoutineCommands::new(connection);
                                    let mut sources = PgRoutineSources::new(RoutineDelegates);
                                    Some(
                                        run_template_source(
                                            &actor,
                                            &mut repository,
                                            &mut sources,
                                            &template_id,
                                        )
                                        .await?,
                                    )
                                };
                                let project_id = template
                                    .as_ref()
                                    .and_then(|row| row.project_id.clone())
                                    .or_else(|| {
                                        existing_template
                                            .as_ref()
                                            .and_then(|row| row.project_id.clone())
                                    });
                                let project_before = if let Some(project_id) = project_id.as_deref()
                                {
                                    Some(
                                        require_routine_project(
                                            connection,
                                            &actor.company_id,
                                            &actor.user_id,
                                            project_id,
                                        )
                                        .await?,
                                    )
                                } else {
                                    None
                                };
                                let admission = begin_routine_run_admission(
                                    connection,
                                    &holding,
                                    &company,
                                    project_id.as_deref(),
                                    key.as_str(),
                                    replay,
                                )
                                .await?;
                                let receipt = {
                                    let mut repository = PgStaffTasks::new(connection);
                                    let mut sources =
                                        PgStaffTaskSources::new(RoutineRunTaskSources { now });
                                    run_template(
                                        &actor,
                                        &mut repository,
                                        &mut sources,
                                        template.as_ref(),
                                        &template_id,
                                        &RunRoutineTemplateCommand {
                                            due_at: due_at.clone(),
                                        },
                                        key.as_str(),
                                        now,
                                    )
                                    .await?
                                };
                                if let Some(template) = template.as_ref() {
                                    verify_routine_run_source(connection, template).await?;
                                }
                                if let Some(before) = project_before.as_ref()
                                    && routine_project_source(
                                        connection,
                                        &actor.company_id,
                                        &actor.user_id,
                                        &before.id,
                                    )
                                    .await?
                                    .as_ref()
                                        != Some(before)
                                {
                                    return Err(BusinessError::new(
                                        FailureKind::Conflict,
                                        "PROJECT_SOURCE_CHANGED",
                                        "The original Project owner or workspace changed.",
                                    )
                                    .into());
                                }
                                admission.complete(connection, &holding).await?;
                                serde_json::to_value(receipt).unwrap_or_default()
                            }
                        };
                        Ok(RoutineHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct RoutineBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<RoutineService<C>>,
}

impl<C: Clock + Send + Sync + 'static> RoutineBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<RoutineService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn standalone_ssr_admission(&self, headers: &HeaderMap) -> Result<Option<bool>, BusinessError> {
        if self.policy.environment != "development"
            || unique_request_header(headers, "x-erpx-api-transport")? == Some("public")
        {
            return Ok(None);
        }
        Ok(
            match unique_request_header(headers, "x-erpx-routine-apps-admission")? {
                Some("purchased") => Some(false),
                Some("deferred") => Some(true),
                _ => None,
            },
        )
    }

    fn context(
        &self,
        headers: &HeaderMap,
        mutation: bool,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        let company =
            unique_request_header(headers, "x-erpx-context-company")?.ok_or_else(scope)?;
        if company != self.service.company().as_str() {
            return Err(scope());
        }
        let assignment =
            unique_request_header(headers, "x-erpx-context-assignment")?.ok_or_else(scope)?;
        let revision: i64 = unique_request_header(headers, "x-erpx-context-revision")?
            .ok_or_else(scope)?
            .parse()
            .map_err(|_| scope())?;
        let assignment_version: i64 =
            unique_request_header(headers, "x-erpx-context-assignment-version")?
                .ok_or_else(scope)?
                .parse()
                .map_err(|_| scope())?;
        if !(0..=9_007_199_254_740_991).contains(&revision)
            || !(1..=9_007_199_254_740_991).contains(&assignment_version)
        {
            return Err(scope());
        }
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: Some(ExpectedWorkSelection {
                    revision,
                    assignment_id: AssignmentId::new(assignment)?,
                    assignment_version,
                }),
                mutation,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> RoutineHttpBackend for RoutineBackend<C> {
    fn command<'a>(
        &'a self,
        request: RoutineHttpCommandRequest<'a>,
    ) -> RoutineHttpFuture<'a, RoutineHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, true)?;
            self.service
                .command(request.host, proof, selection, request.key, request.command)
                .await
                .map_err(business)
        })
    }

    fn schedules<'a>(
        &'a self,
        host: &'a str,
        headers: &'a HeaderMap,
        template_id: String,
    ) -> RoutineHttpFuture<'a, RoutineHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(headers, false)?;
            let standalone = self.standalone_ssr_admission(headers)?;
            let holding = self.service.configuration.holding.clone();
            let company = self.service.company().clone();
            let clock = self.service.clock.clone();
            self.service
                .authorizer
                .execute(
                    host,
                    proof,
                    selection,
                    routine_read_authorization(standalone.is_some()),
                    move |connection, current| {
                        let holding = holding.clone();
                        let company = company.clone();
                        let template_id = template_id.clone();
                        let clock = clock.clone();
                        Box::pin(async move {
                            if current.holding_id() != &holding
                                || current.selection().workspace().company_id != company
                            {
                                return Err(scope().into());
                            }
                            let now = clock.now();
                            if let Some(deferred) = standalone {
                                authorize_standalone_routine_read(
                                    connection, current, deferred, now,
                                )
                                .await?;
                            }
                            let holding_owner = current.selection().assignment().scope_kind
                                == WorkScopeKind::Holding
                                && current.has_permission("manage_erpx.access");
                            let data = read_template_schedules(
                                connection,
                                company.as_str(),
                                current.user_id().as_str(),
                                &template_id,
                                holding_owner,
                            )
                            .await?;
                            require_routine_history(
                                connection,
                                current,
                                data["template"]["project_id"].as_str(),
                                now.get(),
                            )
                            .await?;
                            Ok(RoutineHttpResponse {
                                data,
                                context: context(current),
                            })
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }

    fn editor_context<'a>(
        &'a self,
        host: &'a str,
        headers: &'a HeaderMap,
    ) -> RoutineHttpFuture<'a, RoutineHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(headers, false)?;
            let standalone = self.standalone_ssr_admission(headers)?;
            let holding = self.service.configuration.holding.clone();
            let company = self.service.company().clone();
            let clock = self.service.clock.clone();
            // In development this is the Holding Apps editor. The optional
            // private SSR hint can only narrow the picker for a compiled render.
            let strict_picker = self.policy.environment == "development";
            let production_render = std::env::var("NODE_ENV").as_deref() == Ok("production")
                || unique_request_header(headers, "x-erpx-routine-compiled-render")? == Some("1");
            self.service
                .authorizer
                .execute(
                    host,
                    proof,
                    selection,
                    routine_read_authorization(standalone.is_some()),
                    move |connection, current| {
                        let holding = holding.clone();
                        let company = company.clone();
                        let clock = clock.clone();
                        Box::pin(async move {
                            if current.holding_id() != &holding
                                || current.selection().workspace().company_id != company
                            {
                                return Err(scope().into());
                            }
                            let now = clock.now();
                            if let Some(deferred) = standalone {
                                authorize_standalone_routine_read(
                                    connection, current, deferred, now,
                                )
                                .await?;
                            }
                            let data = read_routine_editor_context(
                                connection,
                                current,
                                strict_picker,
                                production_render,
                                now,
                            )
                            .await?;
                            Ok(RoutineHttpResponse {
                                data,
                                context: context(current),
                            })
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }
}
