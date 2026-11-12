//! Native Operations routine command assembly.
//!
//! The owner stores templates/schedules and the command ledger; composition
//! supplies the current Account/IAM selection and the People, Projects,
//! Organization and Identity facts the retained routine validation consumes.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest,
    },
    routine_owner::company_owner,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_operations::public::routine_commands::{
    PgRoutineCommands, PgRoutineSources, RoutineAuthority, RoutineCapture, RoutineCommandRepository,
    RoutineFuture, RunRoutineTemplateCommand, change_schedule_state,
    end_schedule, lock_routine_sources, run_template, run_template_source,
    save_schedule, save_template,
};
use erpx_server_operations::public::routine_commands_http::{
    RoutineHttpBackend, RoutineHttpCommand, RoutineHttpCommandRequest, RoutineHttpFuture,
    RoutineHttpResponse,
};
use erpx_server_operations::public::staff_tasks::{
    PgStaffTaskSources, PgStaffTasks, ProjectFact, RouteFact, StaffFact, StaffTaskFactDelegates,
    StaffTaskFuture, TaskRow, TaskScope, lock_staff_task_sources, notify_staff_task_approval,
};
use erpx_server_commercial::public::purchase_after::write_purchase_shadow_checked_observation;
use erpx_server_commercial::public::purchase_command_boundary::{
    PurchaseCommandBoundary, PurchaseCommandScope,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_workflow::public::staff_approval::{
    OpenStaffApproval, StaffApprovalStep, open_staff_approval,
};
use erpx_server_organization::public::staff_department_route::{
    DepartmentPersonFact, staff_department_route,
};
use erpx_server_people_payroll::public::staff_facts::{active_staff, staff_people};
use erpx_server_projects::public::staff_facts::staff_project;
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
};
use sqlx::PgConnection;
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

fn actor(current: &AuthorizedWorkContext) -> Result<erpx_server_operations::public::staff_tasks::StaffTaskActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(erpx_server_operations::public::staff_tasks::StaffTaskActor {
        user_id: current.user_id().as_str().to_owned(),
        assignment_id: assignment.id.as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|v| v.as_str().to_owned()),
        department_id: workspace.department_id.as_ref().map(|v| v.as_str().to_owned()),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
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

impl erpx_server_operations::public::routine_commands::RoutineFactDelegates for RoutineDelegates {
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
    ) -> RoutineFuture<'a, Option<erpx_server_operations::public::staff_tasks::StaffFact>, BusinessError> {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            Ok(active_staff(c, company_id, user_id)
                .await?
                .map(|fact| erpx_server_operations::public::staff_tasks::StaffFact {
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
    ) -> RoutineFuture<'a, Option<erpx_server_operations::public::staff_tasks::ProjectFact>, BusinessError> {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            Ok(staff_project(c, company_id, project_id)
                .await?
                .map(|fact| erpx_server_operations::public::staff_tasks::ProjectFact {
                    owner_user_id: fact.owner_user_id,
                    title: fact.title,
                }))
        })
    }

    fn task_route<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        scope: &'a erpx_server_operations::public::staff_tasks::TaskScope,
        creator_user_id: &'a str,
        assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        service_task: bool,
    ) -> RoutineFuture<'a, erpx_server_operations::public::staff_tasks::RouteFact, BusinessError> {
        Box::pin(async move {
            let c = unit.ok_or_else(unavailable)?;
            if approvers.len() > 20
                || approvers.iter().collect::<std::collections::BTreeSet<_>>().len() != approvers.len()
            {
                return Ok(erpx_server_operations::public::staff_tasks::RouteFact {
                    blocked_reason: Some(
                        "Choose up to twenty different ordered approvers per phase.".into(),
                    ),
                });
            }
            if active_staff(c, &scope.company_id, assignee_user_id).await?.is_none() {
                return Ok(erpx_server_operations::public::staff_tasks::RouteFact {
                    blocked_reason: Some("Choose a current active staff member.".into()),
                });
            }
            for user in approvers {
                if user == assignee_user_id || (phase == "start" && user == creator_user_id) {
                    return Ok(erpx_server_operations::public::staff_tasks::RouteFact {
                        blocked_reason: Some(
                            "Assignees cannot approve their work; creators cannot approve their own start request."
                                .into(),
                        ),
                    });
                }
                if active_staff(c, &scope.company_id, user).await?.is_none() {
                    return Ok(erpx_server_operations::public::staff_tasks::RouteFact {
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
            Ok(erpx_server_operations::public::staff_tasks::RouteFact {
                blocked_reason: department,
            })
        })
    }
}

#[derive(Clone, Copy)]
struct RoutineSources;

impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for RoutineSources
{
    fn prepare<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'c, (), BusinessError> {
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
    if approvers.len() > 20
        || approvers.iter().collect::<BTreeSet<_>>().len() != approvers.len()
    {
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
    if !approvers.is_empty()
        && creator_user_id == assignee_user_id
        && scope.department_id.is_some()
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
fn routine_purchase_mode(
    name: &'static str,
) -> Result<PackageEnforcementMode, BusinessError> {
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
        for (index, capability) in ["work.actions", "work.advanced-routines"].iter().enumerate() {
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
    template: &erpx_server_operations::public::routine_commands::RoutineTemplateRow,
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
                            RoutineHttpCommand::SaveTemplate { id, clone_id, command } => {
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
                            RoutineHttpCommand::SaveSchedule { template_id, id, command } => {
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
                            RoutineHttpCommand::ChangeScheduleState { id, resume, command } => {
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
                            RoutineHttpCommand::RunTemplate { template_id, due_at } => {
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
                                    repository
                                        .template(&actor.company_id, &template_id)
                                        .await?
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
                                let project_before = if let Some(project_id) =
                                    project_id.as_deref()
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

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."));
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: true,
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
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.key,
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}
