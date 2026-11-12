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
    PgRoutineCommands, PgRoutineSources, RoutineAuthority, RoutineCapture, RoutineFactDelegates,
    RoutineFuture, change_schedule_state, end_schedule, lock_routine_sources, save_schedule,
    save_template,
};
use erpx_server_operations::public::routine_commands_http::{
    RoutineHttpBackend, RoutineHttpCommand, RoutineHttpCommandRequest, RoutineHttpFuture,
    RoutineHttpResponse,
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
        Box::pin(async move { lock_routine_sources(c).await })
    }
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
                        let mut repository = PgRoutineCommands::new(connection);
                        let mut sources = PgRoutineSources::new(RoutineDelegates);
                        let routine_authority = RoutineAuthority {
                            actor_json: context(current).to_string(),
                            role: current.selection().assignment().role.clone(),
                        };
                        let data = match command {
                            RoutineHttpCommand::SaveTemplate { id, clone_id, command } => {
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
