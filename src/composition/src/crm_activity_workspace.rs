//! Native CRM activity workspace/work read assembly.
//!
//! Current Account/Holding authority stays with the selected coordinator; the
//! owner port supplies the CRM rows and view/work rules. This module owns the
//! one serializable Holding unit, the current IAM scope, the `sales.crm`
//! purchase inspection and the cross-owner lock order. No TypeScript fallback
//! or business rule lives here. The owner GET router is mounted by the lead on
//! the private gateway with the exact registered Holding targets.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseInspection,
    PurchaseRequest, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_crm::public::activity_workspace::{
    ActivityActor, ActivityWorkspaceData, PgActivityDirectory, PgActivityWorkspace, workspace_view,
    work_items,
};
use erpx_server_crm::public::activity_workspace_http::{
    CrmActivityWorkspaceHttpBackend, CrmActivityWorkspaceHttpFuture,
    CrmActivityWorkspaceHttpResponse, CrmActivityWorkspaceHttpTarget,
    CrmActivityWorkspaceReadKind, CrmActivityWorkspaceReadRequest,
};
use erpx_server_identity_access::public::account_inbound_http::parse_inbound_account_request;
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
    WorkAuthorizationRequest,
};
use erpx_server_identity_access::public::work_context::{HoldingWorkspaceRequest, WorkScopeKind};
use erpx_server_runtime::public::http_support::{HeaderMap, Router};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::BTreeMap;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;
type ReadResponse = CrmActivityWorkspaceHttpResponse;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CRM_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CRM_LEAD_NOT_FOUND",
        "Lead not found in this scope.",
    )
}

fn package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "Sales CRM or retained history is required.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CRM_ACTIVITY_WORKSPACE_UNAVAILABLE",
        "The CRM activity workspace is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CrmActivitySources;

impl HoldingWorkSourcePreparation for CrmActivitySources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c)
                .await
                .map_err(business)?;
            sqlx::query(
                "LOCK TABLE public.hr_people,public.hr_engagements,public.assignments, \
                 public.crm_leads,public.crm_opportunities,public.crm_lead_activities, \
                 public.crm_lead_activity_events IN SHARE MODE",
            )
            .execute(&mut *c)
            .await?;
            Ok(())
        })
    }
}

/// Development package switch, identical to the retained TypeScript reader.
/// Invalid values and non-local enforcement fail closed; disabled keeps the
/// read unguarded exactly as the retired TypeScript wrapper did.
fn purchase_mode() -> Result<PackageEnforcementMode, BusinessError> {
    let raw = std::env::var("ERPX_COMMERCE_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned());
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

#[derive(Clone, Debug)]
struct PurchaseControls {
    new_work: bool,
    settle_existing: bool,
    history: bool,
    revision: String,
}

impl PurchaseControls {
    fn value(&self) -> Value {
        json!({
            "newWork": self.new_work,
            "settleExisting": self.settle_existing,
            "history": self.history,
            "revision": self.revision,
        })
    }
}

async fn purchase_fact(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    now: TimestampMillis,
) -> Result<Option<PurchaseControls>, Error> {
    if purchase_mode()? == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let catalogue = current_package_catalogue()?;
    let mut reader =
        PgCompanyPurchaseReader::new(holding.clone(), company.clone(), connection, &catalogue);
    let request = |purpose| PurchaseRequest {
        capabilities: vec!["sales.crm".into()],
        purpose,
        mode: PackageEnforcementMode::Enforced,
    };
    let history = reader
        .inspect(&request(CapabilityPurpose::History), now)
        .await
        .map_err(business)?;
    let new_work = reader
        .inspect(&request(CapabilityPurpose::NewWork), now)
        .await
        .map_err(business)?;
    let settle = reader
        .inspect(&request(CapabilityPurpose::SettleExisting), now)
        .await
        .map_err(business)?;
    let allowed = |inspection: &PurchaseInspection| {
        inspection
            .access
            .iter()
            .find(|access| access.capability == "sales.crm")
            .is_some_and(|access| access.allowed_by_mode)
    };
    Ok(Some(PurchaseControls {
        new_work: allowed(&new_work),
        settle_existing: allowed(&settle),
        history: allowed(&history),
        revision: history.revision.clone(),
    }))
}

fn permissions(current: &AuthorizedWorkContext) -> &[String] {
    &current.command_iam_metadata().permissions
}

fn scope_actor(current: &AuthorizedWorkContext) -> ActivityActor {
    let selected = current.selection();
    let workspace = selected.workspace();
    let granted = permissions(current);
    ActivityActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        can_sales: granted.iter().any(|permission| permission == "commerce.sales"),
        can_manage: granted.iter().any(|permission| permission == "commerce.approve"),
        has_read: granted.iter().any(|permission| permission == "commerce.read"),
        same_person_ids: Vec::new(),
    }
}

pub(crate) async fn plugin_site_measurement(
    connection: &mut PgConnection,
    current: &AuthorizedWorkContext,
) -> Result<Option<erpx_server_crm::public::activity_workspace::SiteMeasurementCard>, Error> {
    let actor = scope_actor(current);
    if !actor.has_read || !current.modules().iter().any(|module| module == "crm") {
        return Ok(None);
    }
    let now = sqlx::query_scalar::<_, i64>(
        "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp())*1000)::bigint",
    )
    .fetch_one(&mut *connection)
    .await?;
    let company = &current.selection().workspace().company_id;
    if purchase_fact(connection, current.holding_id(), company, TimestampMillis::new(now))
        .await?
        .is_some_and(|purchase| !purchase.history)
    {
        return Ok(None);
    }
    PgActivityWorkspace::new(connection)
        .next_site_measurement(&actor)
        .await
}

/// Resolve merged-person flags for the concrete rows. Composition supplies the
/// current Identity same-person decision; the owner projection only consumes
/// the resulting ids, never mints them.
fn row_actor(current: &AuthorizedWorkContext, rows: &[erpx_server_crm::public::activity_workspace::CrmActivityRow]) -> ActivityActor {
    let mut actor = scope_actor(current);
    let mut ids = BTreeMap::new();
    for row in rows {
        for id in [row.created_by.as_str(), row.owner_id.as_str()] {
            if current.same_person_user_raw(id) {
                ids.insert(id.to_owned(), ());
            }
        }
    }
    actor.same_person_ids = ids.into_keys().collect();
    actor
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
        "branchSlug": workspace.branch_slug,
        "departmentSlug": workspace.department_slug,
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "assignmentScopeKind": match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        "modules": current.modules(),
        "permissions": permissions(current),
        "expiresAt": current.expires_at(),
    })
}

#[derive(Clone)]
pub struct CrmActivityWorkspaceConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl CrmActivityWorkspaceConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }
}

pub struct CrmActivityWorkspaceService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: CrmActivityWorkspaceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CrmActivityWorkspaceService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: CrmActivityWorkspaceConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "commerce.read".into(),
            module: Some("crm".into()),
            destination: "crm".into(),
            api: true,
        }
    }

    fn input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        }
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: erpx_server_identity_access::public::DeviceProof,
        input: Input,
        kind: CrmActivityWorkspaceReadKind,
    ) -> Result<ReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                CrmActivitySources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let now = clock.now();
                        let purchase =
                            purchase_fact(&mut *connection, &holding, &company, now).await?;
                        let actor = scope_actor(&current);
                        let mut repository = PgActivityWorkspace::new(connection);
                        let data = match kind {
                            CrmActivityWorkspaceReadKind::Workspace { lead_id } => {
                                let lead = repository
                                    .lead(&actor, &lead_id)
                                    .await?
                                    .ok_or_else(not_found)?;
                                let rows = repository.activities(&actor, &lead_id).await?;
                                let history = repository.history(&actor, &lead_id).await?;
                                drop(repository);
                                let mut directory = PgActivityDirectory::new(connection);
                                let recipients = directory
                                    .recipients(company.as_str(), &lead_id)
                                    .await?;
                                let view = workspace_view(&lead, &rows, &row_actor(&current, &rows));
                                let mut value = serde_json::to_value(ActivityWorkspaceData {
                                    view,
                                    history,
                                    recipients,
                                })
                                .map_err(|_| unavailable())?;
                                if let Some(purchase) = purchase {
                                    if !purchase.history {
                                        return Err(package_required().into());
                                    }
                                    value
                                        .as_object_mut()
                                        .ok_or_else(unavailable)?
                                        .insert("purchase".to_owned(), purchase.value());
                                }
                                value
                            }
                            CrmActivityWorkspaceReadKind::Work => {
                                if !actor.has_read {
                                    return Ok(ReadResponse {
                                        data: json!([]),
                                        context: context(&current),
                                    });
                                }
                                let rows = repository.work_rows(&actor).await?;
                                if purchase
                                    .as_ref()
                                    .is_some_and(|purchase| !purchase.history)
                                {
                                    return Ok(ReadResponse {
                                        data: json!([]),
                                        context: context(&current),
                                    });
                                }
                                let mut items =
                                    serde_json::to_value(work_items(&rows, &row_actor(&current, &rows)))
                                        .map_err(|_| unavailable())?;
                                if purchase
                                    .as_ref()
                                    .is_some_and(|purchase| !purchase.settle_existing)
                                {
                                    neutralize_work(&mut items);
                                }
                                items
                            }
                        };
                        Ok(ReadResponse {
                            data,
                            context: context(&current),
                        })
                    })
                },
            )
            .await
    }
}

/// Structural purchase neutralization, identical to the retained wrapper:
/// a work item the actor cannot settle becomes a review item. The owner SQL
/// source scope is still applied by the retained Work aggregator, which is an
/// independent consumer of this projection.
fn neutralize_work(data: &mut Value) {
    let Some(items) = data.as_array_mut() else {
        return;
    };
    for item in items.iter_mut() {
        let Some(object) = item.as_object_mut() else {
            continue;
        };
        object.insert("canAct".to_owned(), json!(false));
        object.insert("actionKind".to_owned(), json!("work"));
        if object.get("queue").and_then(Value::as_str) != Some("completed") {
            object.insert("queue".to_owned(), json!("waiting"));
        }
        object.insert("nextAction".to_owned(), json!("Review retained CRM source"));
        if let Some(relationships) = object
            .get_mut("relationships")
            .and_then(|value| value.as_array_mut())
        {
            relationships.retain(|value| {
                value.as_str() != Some("assigned") && value.as_str() != Some("approval")
            });
        }
    }
}

pub struct CrmActivityWorkspaceBackend<C> {
    policy: Arc<erpx_server_identity_access::public::AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CrmActivityWorkspaceService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CrmActivityWorkspaceBackend<C> {
    pub fn new(
        policy: Arc<erpx_server_identity_access::public::AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CrmActivityWorkspaceService<C>>,
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
    ) -> Result<
        (
            erpx_server_identity_access::public::DeviceProof,
            HoldingWorkspaceRequest,
        ),
        BusinessError,
    > {
        use erpx_server_identity_access::public::RequestAudience;
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.configuration.company.clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> CrmActivityWorkspaceHttpBackend
    for CrmActivityWorkspaceBackend<C>
{
    fn read<'a>(
        &'a self,
        request: CrmActivityWorkspaceReadRequest<'a>,
    ) -> CrmActivityWorkspaceHttpFuture<'a, CrmActivityWorkspaceHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let kind = request.kind.clone();
            self.service
                .read(request.host, proof, move || Ok(selection), kind)
                .await
                .map_err(business)
        })
    }
}

/// Build the owner GET router for one registered Holding.
#[allow(dead_code)]
pub fn crm_activity_workspace_router(
    targets: Vec<CrmActivityWorkspaceHttpTarget>,
) -> Result<Router, BusinessError> {
    erpx_server_crm::public::activity_workspace_http::crm_activity_workspace_http_router(targets)
}
