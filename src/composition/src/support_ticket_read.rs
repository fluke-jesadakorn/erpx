//! Native Support ticket workspace/detail readback. Account/Holding cookies,
//! the rendered work-context CAS, the selected assignment and the Support
//! purchase are verified again inside the owner read. This layer supplies no
//! authority of its own.
use erpx_server_assets::public::record_source::read_record_source as read_asset_source;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_pms::public::record_source::read_record_source as read_pms_source;
use erpx_server_pos::public::record_source::read_record_source as read_pos_source;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, UserId,
};
use erpx_server_support::public::support_ticket_read::{
    SupportReadScope, SupportRecipient, SupportSourceReference, parse_source_link,
    read_customer_contact_notes, read_order_recipients, read_order_sources, read_sla_policies,
    read_ticket_detail, read_ticket_history, read_ticket_incidents, read_ticket_recipients,
    read_ticket_workspace,
};
use serde_json::{json, Value};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

#[derive(Clone)]
pub struct SupportTicketReadService<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SupportTicketReadService<C> {
    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }

    pub async fn workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
    ) -> Result<Value, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let expected_company = self.company.clone();
        let holding = self.holding.clone();
        let company = self.company.clone();
        let catalogue = self.catalogue.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || read_workspace(workspace, &expected_company),
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![
                        "support.read".into(),
                        "records.read".into(),
                    ]),
                    module: None,
                    destination: "support.workspace.read".into(),
                    api: true,
                },
                SupportReadSources,
                move |c, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let catalogue = catalogue.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        let scope = support_scope(current);
                        require_purchase(
                            c,
                            current,
                            &catalogue,
                            clock.as_ref(),
                            CapabilityPurpose::History,
                        )
                        .await?;
                        let new_work = purchase_allowed(
                            c,
                            current,
                            &catalogue,
                            clock.as_ref(),
                            CapabilityPurpose::NewWork,
                        )
                        .await?;
                        let rows = read_ticket_workspace(c, &scope).await?;
                        let mut items = Vec::with_capacity(rows.len());
                        for row in rows {
                            if source_visible(
                                c,
                                current,
                                &company,
                                &catalogue,
                                clock.as_ref(),
                                row.source_record_json.as_deref(),
                            )
                            .await?
                            {
                                items.push(json!({
                                    "id": row.id,
                                    "title": row.title,
                                    "status": row.status,
                                    "due_date": row.due_date,
                                    "owner_id": row.owner_id,
                                    "version": row.version,
                                    "updated_at": row.updated_at,
                                    "source_kind": row.source_kind,
                                    "source_record_json": row.source_record_json,
                                }));
                            }
                        }
                        let can_create = current.has_permission("records.write") && new_work;
                        let can_create_order = current.has_permission("support.create")
                            && current.has_permission("commerce.read")
                            && new_work;
                        let sources = if can_create_order && new_work {
                            read_order_sources(c, &scope)
                                .await?
                                .into_iter()
                                .map(|source| {
                                    json!({
                                        "id": source.id,
                                        "number": source.number,
                                        "version": source.version,
                                        "customer_name": source.customer_name,
                                    })
                                })
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };
                        Ok(json!({
                            "items": items,
                            "sources": sources,
                            "canCreate": can_create,
                            "canCreateOrder": can_create_order,
                            "ownReportsOnly": !current.has_permission("support.read"),
                        }))
                    })
                },
            )
            .await
    }

    pub async fn detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
    ) -> Result<Value, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let expected_company = self.company.clone();
        let holding = self.holding.clone();
        let company = self.company.clone();
        let catalogue = self.catalogue.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || read_workspace(workspace, &expected_company),
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![
                        "support.read".into(),
                        "records.read".into(),
                    ]),
                    module: None,
                    destination: "support.ticket.read".into(),
                    api: true,
                },
                SupportReadSources,
                move |c, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let catalogue = catalogue.clone();
                    let clock = clock.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        let scope = support_scope(current);
                        require_purchase(
                            c,
                            current,
                            &catalogue,
                            clock.as_ref(),
                            CapabilityPurpose::History,
                        )
                        .await?;
                        let settle = purchase_allowed(
                            c,
                            current,
                            &catalogue,
                            clock.as_ref(),
                            CapabilityPurpose::SettleExisting,
                        )
                        .await?;
                        let new_work = purchase_allowed(
                            c,
                            current,
                            &catalogue,
                            clock.as_ref(),
                            CapabilityPurpose::NewWork,
                        )
                        .await?;
                        let Some(row) = read_ticket_detail(c, &scope, &id).await? else {
                            return Err(not_found().into());
                        };
                        let source_kind = row["source_kind"].as_str().unwrap_or_default();
                        let managed = source_kind == "field_service";
                        let source_raw = row["source_record_json"].as_str();
                        if !source_visible(
                            c,
                            current,
                            &company,
                            &catalogue,
                            clock.as_ref(),
                            source_raw,
                        )
                        .await?
                        {
                            return Err(not_found().into());
                        }
                        let support_read = current.has_permission("support.read");
                        let support_manage = current.has_permission("support.manage");
                        let support_work = current.has_permission("support.work");
                        let owner = row["owner_id"].as_str();
                        let resolved_by = row["resolved_by"].as_str();
                        let status = row["status"].as_str().unwrap_or_default();
                        let order_id = row["order_id"].as_str();
                        let clock_value = row["sla_clock_json"]
                            .as_str()
                            .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
                        let owner_eligible = row["owner_eligible"].as_i64().unwrap_or(0) > 0;
                        let now = clock.now().get();
                        let incidents = if support_read {
                            read_ticket_incidents(c, &id).await?
                        } else {
                            Vec::new()
                        };
                        let recipients = if support_read && settle {
                            read_ticket_recipients(c, &scope, &id).await?
                        } else {
                            Vec::new()
                        };
                        let history_events = read_ticket_history(c, &id).await?;
                        let notes = if support_manage && order_id.is_some() {
                            read_customer_contact_notes(c, &company, order_id.unwrap()).await?
                        } else {
                            Vec::new()
                        };
                        let policies = if support_manage
                            && status == "open"
                            && clock_value.is_none()
                            && new_work
                            && !managed
                        {
                            read_sla_policies(c, &company).await?
                        } else {
                            Vec::new()
                        };
                        let can_work = support_work
                            && owner == Some(current.user_id().as_str())
                            && owner_eligible
                            && settle
                            && !managed;
                        let can_manage = support_manage && settle && !managed;
                        let can_report = source_kind != "sales_order"
                            && row["created_by"].as_str() == Some(current.user_id().as_str())
                            && current.has_permission("records.write")
                            && settle
                            && !managed;
                        let can_confirm = support_manage
                            && !same_person(current, owner)
                            && !same_person(current, resolved_by)
                            && settle
                            && !managed;
                        let can_read_order =
                            order_id.is_some() && current.has_permission("commerce.read");
                        let can_reopen = support_manage && new_work && !managed;
                        Ok(json!({
                            "incidents": incidents,
                            "now": now,
                            "row": row,
                            "recipients": recipients,
                            "history": history_events,
                            "notes": notes,
                            "canWork": can_work,
                            "canManage": can_manage,
                            "canReport": can_report,
                            "canConfirm": can_confirm,
                            "canReadOrder": can_read_order,
                            "slaClock": clock_value,
                            "slaPolicies": policies,
                            "managedInFieldService": managed,
                            "canReopen": can_reopen,
                        }))
                    })
                },
            )
            .await
    }

    pub async fn recipients<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        order_id: String,
    ) -> Result<Vec<SupportRecipient>, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let expected_company = self.company.clone();
        let holding = self.holding.clone();
        let company = self.company.clone();
        let catalogue = self.catalogue.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || read_workspace(workspace, &expected_company),
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::Exact("support.read".into()),
                    module: None,
                    destination: "support.recipients.read".into(),
                    api: true,
                },
                SupportReadSources,
                move |c, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let catalogue = catalogue.clone();
                    let clock = clock.clone();
                    let order_id = order_id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        let scope = support_scope(current);
                        require_purchase(
                            c,
                            current,
                            &catalogue,
                            clock.as_ref(),
                            CapabilityPurpose::SettleExisting,
                        )
                        .await?;
                        read_order_recipients(c, &scope, &order_id)
                            .await
                            .map_err(Into::into)
                    })
                },
            )
            .await
    }
}

#[derive(Clone)]
struct SupportReadSources;
impl HoldingWorkSourcePreparation for SupportReadSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            Ok(())
        })
    }
}

fn read_workspace(
    workspace: impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError>,
    company: &CompanyId,
) -> Result<HoldingWorkspaceRequest, BusinessError> {
    let mut selected = workspace()?;
    selected.mutation = false;
    if &selected.company_id != company {
        return Err(scope_required());
    }
    Ok(selected)
}

fn support_scope(current: &AuthorizedWorkContext) -> SupportReadScope {
    let selection = current.selection();
    let workspace = selection.workspace();
    let assignment = selection.assignment();
    SupportReadScope {
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().into()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().into()),
        user_id: current.user_id().as_str().into(),
        assignment_id: assignment.id.as_str().into(),
        role: assignment.role.clone(),
        support_read: current.has_permission("support.read"),
        records_read: current.has_permission("records.read"),
        support_create: current.has_permission("support.create"),
        support_work: current.has_permission("support.work"),
        support_manage: current.has_permission("support.manage"),
        commerce_read: current.has_permission("commerce.read"),
    }
}

fn same_person(current: &AuthorizedWorkContext, id: Option<&str>) -> bool {
    id.and_then(|value| UserId::new(value).ok())
        .is_some_and(|value| current.same_person_user(&value))
}

async fn require_purchase<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    catalogue: &CommercialCatalogue,
    clock: &C,
    purpose: CapabilityPurpose,
) -> Result<(), Error> {
    let mut reader = PgCompanyPurchaseReader::new(
        current.holding_id().clone(),
        current.selection().workspace().company_id.clone(),
        c,
        catalogue,
    );
    let inspection = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["support.tickets".into()],
                purpose,
                mode: PackageEnforcementMode::Enforced,
            },
            clock.now(),
        )
        .await?;
    inspection
        .require_allowed()
        .map(|_| ())
        .map_err(Into::into)
}

async fn purchase_allowed<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    catalogue: &CommercialCatalogue,
    clock: &C,
    purpose: CapabilityPurpose,
) -> Result<bool, Error> {
    let mut reader = PgCompanyPurchaseReader::new(
        current.holding_id().clone(),
        current.selection().workspace().company_id.clone(),
        c,
        catalogue,
    );
    let inspection = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["support.tickets".into()],
                purpose,
                mode: PackageEnforcementMode::Enforced,
            },
            clock.now(),
        )
        .await?;
    Ok(inspection.require_allowed().is_ok())
}

fn assignment_visible(
    current: &AuthorizedWorkContext,
    branch: Option<&str>,
    department: Option<&str>,
) -> bool {
    let assignment = current.selection().assignment();
    match assignment.scope_kind {
        WorkScopeKind::Holding => true,
        WorkScopeKind::Branch => assignment
            .scope_node_id
            .as_ref()
            .is_some_and(|node| Some(node.as_str()) == branch),
        WorkScopeKind::Department => assignment
            .scope_node_id
            .as_ref()
            .is_some_and(|node| Some(node.as_str()) == department),
    }
}

fn source_scope_visible(
    current: &AuthorizedWorkContext,
    branch: Option<&str>,
    department: Option<&str>,
) -> bool {
    let workspace = current.selection().workspace();
    if workspace
        .branch_id
        .as_ref()
        .is_some_and(|selected| Some(selected.as_str()) != branch)
    {
        return false;
    }
    if workspace
        .department_id
        .as_ref()
        .is_some_and(|selected| Some(selected.as_str()) != department)
    {
        return false;
    }
    assignment_visible(current, branch, department)
}

/// Apply the same owner authority as the retained typed-source link. A source
/// that is absent, out of scope, denied or module-gated hides the ticket.
async fn source_visible<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    clock: &C,
    raw: Option<&str>,
) -> Result<bool, Error> {
    let Some(raw) = raw else {
        return Ok(true);
    };
    let Some(reference) = parse_source_link(raw) else {
        return Ok(false);
    };
    let SupportSourceReference { kind, id, .. } = reference;
    match kind.as_str() {
        "pos_sale" => {
            if !current.has_permission("pos.read")
                || current.selection().workspace().department_id.is_some()
            {
                return Ok(false);
            }
            if !source_purchase_allowed(
                c,
                current,
                catalogue,
                clock,
                "pos.sales",
            )
            .await?
            {
                return Ok(false);
            }
            let Some(source) = read_pos_source(c, company, &id).await? else {
                return Ok(false);
            };
            let record = source.data();
            Ok(source_scope_visible(
                current,
                record.branch_id.as_deref(),
                record.department_id.as_deref(),
            ))
        }
        "asset" => {
            if !current.has_permission("records.read")
                || !current
                    .modules()
                    .iter()
                    .any(|module| module == "assets")
            {
                return Ok(false);
            }
            if !source_purchase_allowed(
                c,
                current,
                catalogue,
                clock,
                "assets.register",
            )
            .await?
            {
                return Ok(false);
            }
            let Some(source) = read_asset_source(c, company, &id).await? else {
                return Ok(false);
            };
            let record = source.data();
            Ok(source_scope_visible(
                current,
                record.branch_id.as_deref(),
                record.department_id.as_deref(),
            ))
        }
        "pms_record" => {
            if !current.has_permission("pms.read")
                || current.selection().workspace().department_id.is_some()
            {
                return Ok(false);
            }
            if !source_purchase_allowed(
                c,
                current,
                catalogue,
                clock,
                "pms.front-desk",
            )
            .await?
            {
                return Ok(false);
            }
            let Some(source) = read_pms_source(c, company, &id).await? else {
                return Ok(false);
            };
            let record = source.data();
            if !source_scope_visible(
                current,
                record.branch_id.as_deref(),
                record.department_id.as_deref(),
            ) {
                return Ok(false);
            }
            Ok(source
                .require_reader(current.user_id().as_str(), |permission| {
                    current.has_permission(permission)
                })
                .is_ok())
        }
        _ => Ok(false),
    }
}

async fn source_purchase_allowed<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    catalogue: &CommercialCatalogue,
    clock: &C,
    capability: &str,
) -> Result<bool, Error> {
    let mut reader = PgCompanyPurchaseReader::new(
        current.holding_id().clone(),
        current.selection().workspace().company_id.clone(),
        c,
        catalogue,
    );
    let inspection = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec![capability.into()],
                purpose: CapabilityPurpose::History,
                mode: PackageEnforcementMode::Enforced,
            },
            clock.now(),
        )
        .await?;
    Ok(inspection.require_allowed().is_ok())
}

fn scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "SUPPORT_SCOPE_REQUIRED",
        "Use the current Company.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPORT_TICKET_NOT_FOUND",
        "Ticket not found in this scope.",
    )
}
