//! Account-only work selection composition. Identity owns grants/context rows,
//! Work Projection owns saved choices, Workflow owns their audit, all in one unit.
use erpx_server_identity_access::public::{
    DeviceProof,
    account_work_roles::{
        AccountWorkRoleChoice, AccountWorkSelection, CurrentAccountWorkRoles,
        RetainedAccountWorkSelection, lock_account_work_roles, read_account_work_roles,
    },
    effective_authority::HoldingWorkSourcePreparation,
    work_context::{
        HoldingAccountWorkScope, HoldingWorkContextInspector, HoldingWorkspaceRequest,
        WorkScopeKind,
    },
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{AssignmentId, BusinessError, Clock, FailureKind};
use erpx_server_work_projection::public::{
    identity_selection_preferences::{lock_work_role_preferences, read_work_role_preference},
    work_role_store::{
        InitialWorkRole, WorkRoleInitialization, WorkRoleReference, initial_work_role,
    },
};
use erpx_server_workflow::public::work_role_audit::{
    PgWorkRoleSelectionAuditSource, WorkRoleSelectionAudit, lock_work_role_audit_sources,
    write_work_role_selection_audit,
};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your access or work role changed. Review and choose again.",
    )
}
pub enum WorkRoleAction {
    Inspect,
    Restore,
    Initialize,
    Context {
        destination: String,
        api: bool,
    },
    Select {
        assignment: AssignmentId,
        revision: i64,
        expected_version: Option<i64>,
        remember: bool,
    },
}
#[derive(Clone)]
struct RoleSources {
    context: bool,
}
impl HoldingWorkSourcePreparation for RoleSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_account_work_roles(c, true).await?;
            if self.context {
                erpx_server_identity_access::public::account_work_presentation::lock_account_work_presentation_sources(c).await?;
            }
            lock_work_role_preferences(c).await?;
            lock_work_role_audit_sources(c).await
        })
    }
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NativeWorkRoleOption {
    pub id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub version: i64,
    pub role: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub modules: Vec<String>,
    #[cfg_attr(
        feature = "contracts",
        ts(type = r#""holding" | "branch" | "department""#)
    )]
    pub scope_kind: &'static str,
    pub scope_node_id: Option<String>,
    pub label: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub valid_until: i64,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NativeWorkRoleState {
    pub holding_name: String,
    pub holding_id: String,
    pub company_id: String,
    pub user_id: String,
    pub session_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub expires_at: i64,
    pub company_name: String,
    pub company_slug: String,
    pub branch_name: Option<String>,
    pub department_name: Option<String>,
    pub workspaces:
        Vec<erpx_server_identity_access::public::account_work_roles::AccountWorkspaceChoice>,
    pub context: Option<NativeWorkPresentation>,
    pub options: Vec<NativeWorkRoleOption>,
    pub selection: Option<AccountWorkSelection>,
    pub valid: bool,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NativeIamPresentation {
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub role_revision: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub identity_version: i64,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
    pub principal_id: String,
    pub person_id: Option<String>,
    pub person_user_ids: Vec<String>,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    pub hr_appointment_version: Option<i64>,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    pub emergency_until: Option<i64>,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    pub fresh_until: Option<i64>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NativeWorkPresentation {
    pub iam: NativeIamPresentation,
    pub modules: Vec<String>,
    pub beta: bool,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub expires_at: i64,
}
async fn render_view(
    c: &mut PgConnection,
    scope: &HoldingAccountWorkScope,
    facts: &CurrentAccountWorkRoles,
    now: i64,
    context: Option<(&str, bool)>,
) -> Result<NativeWorkRoleState, Error> {
    let mut result = view(scope, facts, now)?;
    result.workspaces = facts.workspace_choices(c, scope, now).await?;
    if let Some((destination, api)) = context.filter(|_| result.valid) {
        let p=erpx_server_identity_access::public::account_work_presentation::read_account_work_presentation(c,scope,destination,api,now).await?;
        let i = p.iam;
        for v in [
            Some(i.role_revision),
            Some(i.identity_version),
            i.hr_appointment_version,
            i.emergency_until,
            i.fresh_until,
            Some(p.context.expires_at()),
        ]
        .into_iter()
        .flatten()
        {
            if !(0..=9_007_199_254_740_991).contains(&v) {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "ACCOUNT_ACCESS_UNAVAILABLE",
                    "The Work rendering exceeds this bounded read.",
                )
                .into());
            }
        }
        result.context = Some(NativeWorkPresentation {
            modules: p.context.modules().to_vec(),
            beta: p.beta,
            expires_at: p.context.expires_at(),
            iam: NativeIamPresentation {
                role_revision: i.role_revision,
                identity_version: i.identity_version,
                permissions: i.permissions,
                grantable_permissions: i.grantable_permissions,
                principal_id: i.principal_id,
                person_id: i.person_id.map(|n| n.as_str().into()),
                person_user_ids: i
                    .person_user_ids
                    .into_iter()
                    .map(|n| n.as_str().into())
                    .collect(),
                hr_appointment_version: i.hr_appointment_version,
                emergency_until: i.emergency_until,
                fresh_until: i.fresh_until,
            },
        });
    }
    Ok(result)
}
fn option(c: &AccountWorkRoleChoice) -> Result<NativeWorkRoleOption, BusinessError> {
    let a = c.assignment();
    if !(0..=9_007_199_254_740_991).contains(&a.valid_until) {
        return Err(changed());
    }
    Ok(NativeWorkRoleOption {
        id: a.id.as_str().into(),
        version: a.version,
        role: a.role.clone(),
        company_id: a.company_id.as_str().into(),
        branch_id: c.branch_id().map(|n| n.as_str().into()),
        modules: a.modules.clone(),
        scope_kind: match a.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        scope_node_id: a.scope_node_id.as_ref().map(|n| n.as_str().into()),
        label: c.label().into(),
        valid_until: a.valid_until,
        branch_slug: c.branch_slug().map(str::to_owned),
        department_slug: c.department_slug().map(str::to_owned),
    })
}
fn view(
    scope: &HoldingAccountWorkScope,
    facts: &CurrentAccountWorkRoles,
    now: i64,
) -> Result<NativeWorkRoleState, BusinessError> {
    let r = scope.request();
    let w = facts.workspace();
    let valid = facts.selection().is_some_and(|s| {
        s.expires_at > now
            && facts.eligible().any(|c| {
                s.assignment_id.as_deref() == Some(c.assignment().id.as_str())
                    && s.assignment_version == Some(c.assignment().version)
            })
    });
    Ok(NativeWorkRoleState {
        holding_name: scope.holding_name().into(),
        holding_id: r.holding_id.as_str().into(),
        company_id: r.company_id.as_str().into(),
        user_id: r.user_id.as_str().into(),
        session_id: r.session_id.as_str().into(),
        expires_at: r.session_expires_at,
        company_name: facts.company_name().into(),
        company_slug: facts.company_slug().into(),
        branch_name: facts.branch_name().map(str::to_owned),
        department_name: facts.department_name().map(str::to_owned),
        workspaces: Vec::new(),
        context: None,
        options: facts.choices().map(option).collect::<Result<_, _>>()?,
        selection: facts.selection().cloned(),
        valid,
        branch_id: w.branch_id.as_ref().map(|n| n.as_str().into()),
        department_id: w.department_id.as_ref().map(|n| n.as_str().into()),
        branch_slug: w.branch_slug.clone(),
        department_slug: w.department_slug.clone(),
    })
}
struct AuditSource<'a, C> {
    selection: &'a RetainedAccountWorkSelection,
    scope: &'a HoldingAccountWorkScope,
    clock: &'a C,
    facts: WorkRoleSelectionAudit,
}
impl<C: Clock + Sync> PgWorkRoleSelectionAuditSource for AuditSource<'_, C> {
    fn facts(&self) -> &WorkRoleSelectionAudit {
        &self.facts
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.selection
            .require_current(c, self.scope, self.clock.now().get())
            .await
    }
}
fn bounded(view: &NativeWorkRoleState) -> Result<(), BusinessError> {
    let bytes = serde_json::to_vec(view).map_err(|_| changed())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "ACCOUNT_ACCESS_UNAVAILABLE",
            "The account selection exceeds this bounded read.",
        ));
    }
    Ok(())
}
pub async fn execute<C: Clock + Send + Sync + 'static>(
    inspector: &HoldingWorkContextInspector,
    host: &str,
    proof: DeviceProof,
    request: HoldingWorkspaceRequest,
    clock: Arc<C>,
    action: WorkRoleAction,
) -> Result<NativeWorkRoleState, Error> {
    let command = uuid::Uuid::new_v4().to_string();
    let audit = uuid::Uuid::new_v4().to_string();
    let context = matches!(&action, WorkRoleAction::Context { .. });
    let action = Arc::new(action);
    inspector
        .with_account_work(
            host,
            proof,
            request,
            RoleSources { context },
            move |c, scope| {
                Box::pin(execute_in(
                    c,
                    scope,
                    clock.clone(),
                    action.clone(),
                    command.clone(),
                    audit.clone(),
                ))
            },
        )
        .await
}
async fn execute_in<C: Clock + Send + Sync + 'static>(
    c: &mut PgConnection,
    scope: &HoldingAccountWorkScope,
    clock: Arc<C>,
    action: Arc<WorkRoleAction>,
    command: String,
    audit: String,
) -> Result<NativeWorkRoleState, Error> {
    let now = clock.now().get();
    let before = read_account_work_roles(c, scope, now).await?;
    let r = scope.request();
    let preference = read_work_role_preference(c, &r.user_id, &r.company_id).await?;
    let plan = match &*action {
        WorkRoleAction::Select {
            assignment,
            revision,
            expected_version,
            remember,
        } => {
            let choice = before
                .choices()
                .find(|c| &c.assignment().id == assignment)
                .ok_or_else(|| {
                    BusinessError::new(
                        FailureKind::Forbidden,
                        "ASSIGNMENT_UNAVAILABLE",
                        "Choose one of your active assignments.",
                    )
                })?;
            if expected_version.is_some_and(|v| v != choice.assignment().version) {
                return Err(changed().into());
            }
            Some((
                WorkRoleReference {
                    id: assignment.clone(),
                    version: choice.assignment().version,
                },
                *revision,
                false,
                *remember,
            ))
        }
        WorkRoleAction::Inspect => None,
        WorkRoleAction::Restore | WorkRoleAction::Initialize | WorkRoleAction::Context { .. } => {
            let refs = before
                .choices()
                .map(|c| WorkRoleReference {
                    id: c.assignment().id.clone(),
                    version: c.assignment().version,
                })
                .collect::<Vec<_>>();
            let eligible = before
                .eligible()
                .map(|c| WorkRoleReference {
                    id: c.assignment().id.clone(),
                    version: c.assignment().version,
                })
                .collect::<Vec<_>>();
            let saved = preference.preference();
            match initial_work_role(
                if matches!(&*action, WorkRoleAction::Restore) {
                    WorkRoleInitialization::RestoreSaved
                } else {
                    WorkRoleInitialization::InitializeWorkspace
                },
                before.selection().is_some(),
                saved.as_ref(),
                &refs,
                &eligible,
            ) {
                InitialWorkRole::Select {
                    assignment,
                    restoring,
                } => Some((assignment, 0, restoring, true)),
                _ => None,
            }
        }
    };
    let Some((assignment, revision, restoring, remember)) = plan else {
        before.require_current(c, scope, clock.now().get()).await?;
        preference.require_current(c).await?;
        let result = render_view(
            c,
            scope,
            &before,
            clock.now().get(),
            match &*action {
                WorkRoleAction::Context { destination, api } => Some((destination.as_str(), *api)),
                _ => None,
            },
        )
        .await?;
        bounded(&result)?;
        return Ok(result);
    };
    let selected = before
        .select(c, scope, &assignment.id, assignment.version, revision, now)
        .await?;
    let saved = if selected.changed() && !restoring && remember {
        preference.remember(c, &assignment, now).await?
    } else {
        preference
    };
    let audit = if selected.changed() {
        let facts = WorkRoleSelectionAudit {
            id: audit,
            command_id: command,
            user_id: r.user_id.as_str().into(),
            assignment_id: assignment.id.as_str().into(),
            company_id: r.company_id.as_str().into(),
            revision: selected.current().selection().ok_or_else(changed)?.revision,
            restoring,
            created_at: now,
            authority: selected.audit_authority(),
        };
        Some(
            write_work_role_selection_audit(
                c,
                &AuditSource {
                    selection: &selected,
                    scope,
                    clock: clock.as_ref(),
                    facts,
                },
            )
            .await?,
        )
    } else {
        None
    };
    sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *c)
        .await?;
    selected
        .require_current(c, scope, clock.now().get())
        .await?;
    saved.require_current(c).await?;
    if let Some(audit) = audit {
        audit.require_current(c).await?;
    }
    let result = render_view(
        c,
        scope,
        selected.current(),
        clock.now().get(),
        match &*action {
            WorkRoleAction::Context { destination, api } => Some((destination.as_str(), *api)),
            _ => None,
        },
    )
    .await?;
    bounded(&result)?;
    Ok(result)
}

#[derive(Clone)]
struct CompanySources;
impl HoldingWorkSourcePreparation for CompanySources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_identity_access::public::account_companies::lock_account_company_sources(c)
                .await
        })
    }
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NativeAccountCompanies {
    pub holding_id: String,
    pub session_id: String,
    pub user_id: String,
    pub name: String,
    pub email: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub expires_at: i64,
    pub companies:
        Vec<erpx_server_identity_access::public::account_companies::AccountCompanyChoice>,
}
pub async fn companies<C: Clock + Send + Sync + 'static>(
    inspector: &HoldingWorkContextInspector,
    host: &str,
    proof: DeviceProof,
    clock: Arc<C>,
) -> Result<NativeAccountCompanies, Error> {
    inspector
        .with_holding_account(host, proof, CompanySources, move |c, account| {
            let clock = clock.clone();
            Box::pin(async move {
                let choices =
                    erpx_server_identity_access::public::account_companies::read_account_companies(
                        c,
                        account,
                        clock.now().get(),
                    )
                    .await?;
                account.require_current(c, clock.now().get()).await?;
                let result = NativeAccountCompanies {
                    holding_id: account.holding_id().as_str().into(),
                    session_id: account.session_id().as_str().into(),
                    user_id: account.user_id().as_str().into(),
                    name: account.user().name.clone(),
                    email: account.user().email.clone(),
                    expires_at: account.expires_at(),
                    companies: choices,
                };
                if serde_json::to_vec(&result).map_err(|_| changed())?.len() > 2 * 1024 * 1024 {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "ACCOUNT_COMPANIES_UNAVAILABLE",
                        "The Company selection exceeds this bounded read.",
                    )
                    .into());
                }
                Ok(result)
            })
        })
        .await
}

/// All values are untrusted URL/selection hints. No Company, role or actor can
/// be supplied by this struct; Identity validates the resolved live workspace.
pub struct WorkRoleWorkspace {
    pub company: Option<erpx_server_shared::public::CompanyId>,
    pub front_company_slug: Option<String>,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub expected: Option<erpx_server_identity_access::public::work_context::ExpectedWorkSelection>,
    pub captured_company: Option<erpx_server_shared::public::CompanyId>,
}
pub async fn execute_workspace<C: Clock + Send + Sync + 'static>(
    inspector: &HoldingWorkContextInspector,
    host: &str,
    proof: DeviceProof,
    input: WorkRoleWorkspace,
    clock: Arc<C>,
    action: WorkRoleAction,
) -> Result<NativeWorkRoleState, Error> {
    let command = uuid::Uuid::new_v4().to_string();
    let audit = uuid::Uuid::new_v4().to_string();
    let context = matches!(&action, WorkRoleAction::Context { .. });
    let action = Arc::new(action);
    let input = Arc::new(input);
    inspector
        .with_holding_account(host, proof, RoleSources { context }, move |c, account| {
            let input = input.clone();
            let action = action.clone();
            let clock = clock.clone();
            let command = command.clone();
            let audit = audit.clone();
            Box::pin(async move {
                use erpx_server_organization::public::{
                    PgOrganizationReader, WorkspaceRouteQuery, read_company_context,
                    resolve_workspace,
                };
                let mut organization = PgOrganizationReader::new(account.holding_id().clone(), c);
                let structural = if input.front_company_slug.is_some()
                    || input.branch_slug.is_some()
                    || input.department_slug.is_some()
                {
                    let query = if let Some(slug) = &input.front_company_slug {
                        WorkspaceRouteQuery::for_company_slug(
                            input.branch_slug.clone(),
                            input.department_slug.clone(),
                            slug.clone(),
                        )?
                    } else if let Some(company) = &input.company {
                        WorkspaceRouteQuery::for_company_id(
                            input.branch_slug.clone(),
                            input.department_slug.clone(),
                            company.clone(),
                        )?
                    } else {
                        WorkspaceRouteQuery::new(
                            input.branch_slug.clone(),
                            input.department_slug.clone(),
                            None,
                        )?
                    };
                    resolve_workspace(&mut organization, &query)
                        .await?
                        .ok_or_else(|| {
                            BusinessError::new(
                                FailureKind::NotFound,
                                "WORKSPACE_NOT_FOUND",
                                "Workspace not found.",
                            )
                        })?
                } else {
                    let company = input.company.as_ref().ok_or_else(|| {
                        BusinessError::new(
                            FailureKind::Conflict,
                            "COMPANY_REQUIRED",
                            "Choose a Company to continue.",
                        )
                    })?;
                    read_company_context(&mut organization, company).await?
                };
                let company = structural.company().company_id.clone();
                if input
                    .captured_company
                    .as_ref()
                    .is_some_and(|c| c != &company)
                {
                    return Err(changed().into());
                }
                let scope = account.workspace(HoldingWorkspaceRequest {
                    company_id: company,
                    branch_slug: structural.branch().map(|n| n.slug.clone()),
                    department_slug: structural.department().map(|n| n.slug.clone()),
                    expected: input.expected.clone(),
                    mutation: false,
                });
                execute_in(c, &scope, clock, action, command, audit).await
            })
        })
        .await
}
