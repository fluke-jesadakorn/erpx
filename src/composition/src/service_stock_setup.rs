//! The selected Service Stock discovery unit. Owner facts are evidence, not grants.
use crate::service_parts_posting_setup::PostingPurchases;
use erpx_server_commerce::public::service_stock_masters::{
    lock_service_stock_mapping, lock_service_stock_masters, read_service_stock_mapping,
    service_stock_workspace_policy,
};
use erpx_server_commercial::public::purchases::lock_commercial_authority_sources;
use erpx_server_finance::public::stock_master_money::{
    lock_stock_master_finance_sources, read_stock_master_finance, read_stock_master_workspace_chart,
};
use erpx_server_identity_access::public::{
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    selected_command_authority::capture_service_stock_authority,
};
use erpx_server_inventory::public::service_parts_posting::{
    lock_service_initial_stock_schema, read_service_initial_stock_schema,
};
use erpx_server_organization::public::field_service_workspace::{
    lock_service_workspace_locations, read_service_stock_branches,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::service_parts_posting::{
    lock_service_posting_receipts, read_service_stock_posting_receipt,
};
use serde::Serialize;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Service stock source changed.",
    )
}
#[derive(Clone, Copy)]
pub struct ServiceStockSources;
impl HoldingWorkSourcePreparation for ServiceStockSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_service_posting_receipts(c).await?;
            lock_service_stock_masters(c).await?;
            lock_service_stock_mapping(c).await?;
            lock_service_initial_stock_schema(c).await?;
            lock_service_workspace_locations(c).await?;
            lock_stock_master_finance_sources(c).await?;
            Ok(())
        })
    }
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceStockBranch {
    pub id: String,
    pub name: String,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceStockWorkspace {
    pub company_id: String,
    pub available: bool,
    pub schema_available: bool,
    pub source_ready: bool,
    pub currency: Option<String>,
    pub can_create_warehouse: bool,
    pub can_create_product: bool,
    pub branches: Vec<ServiceStockBranch>,
    pub unavailable_reason: Option<&'static str>,
}
fn unavailable(company: &str, reason: &'static str) -> ServiceStockWorkspace {
    ServiceStockWorkspace {
        company_id: company.into(),
        available: false,
        schema_available: false,
        source_ready: false,
        currency: None,
        can_create_warehouse: false,
        can_create_product: false,
        branches: vec![],
        unavailable_reason: Some(reason),
    }
}
/// Must run inside the genuine selected Account/Holding coordinator after
/// ServiceStockSources. A configured bridge must not fall back to legacy.
pub async fn read_service_stock_workspace<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
) -> Result<ServiceStockWorkspace, Error> {
    let company = &current.selection().workspace().company_id;
    let scope = current.selection().workspace();
    if scope.department_id.is_some()
        || !current.has_permission("inventory.read")
        || !current.has_permission("inventory.prepare")
    {
        return Ok(unavailable(company.as_str(), "stock-authority-required"));
    }
    let actor = capture_service_stock_authority(c, current, clock).await?;
    let purchases = PostingPurchases::capture(c, current, clock.now()).await?;
    if !purchases.history_allowed {
        actor.require_current(c, clock).await?;
        purchases.require_current(c, clock.now()).await?;
        return Ok(unavailable(company.as_str(), "purchase-required"));
    }
    let commerce = read_service_stock_mapping(c, company).await?;
    let inventory = read_service_initial_stock_schema(c).await?;
    let branches =
        read_service_stock_branches(c, company, scope.branch_id.as_ref().map(|x| x.as_str()))
            .await?;
    let mapping = commerce
        .mapping_json()
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()
        .map_err(|_| changed())?;
    let mut receipt = None;
    let mut finance = None;
    let mut currency = None;
    let mut ready = false;
    if let Some(m) = &mapping {
        let chart = m
            .get("chart_id")
            .and_then(|v| v.as_str())
            .ok_or_else(changed)?;
        let curr = m
            .get("currency")
            .and_then(|v| v.as_str())
            .ok_or_else(changed)?;
        let source = read_service_stock_posting_receipt(
            c,
            company,
            commerce.mapping_json().ok_or_else(changed)?,
        )
        .await?;
        let financial = read_stock_master_workspace_chart(c, company, chart, curr).await?;
        ready = commerce.schema_available()
            && source.facts().is_some()
            && financial.active_chart_base_matches_mapping();
        if ready {
            currency = Some(curr.to_owned());
        }
        receipt = Some(source);
        finance = Some(financial);
    }
    let schema = commerce.schema_available() && inventory.available();
    let policy = service_stock_workspace_policy(
        purchases.history_allowed,
        purchases.new_work_allowed,
        schema,
        ready,
    );
    let data = ServiceStockWorkspace {
        company_id: company.as_str().into(),
        available: policy.available,
        schema_available: schema,
        source_ready: ready,
        currency,
        can_create_warehouse: policy.can_create,
        can_create_product: policy.can_create,
        branches: branches
            .branches()
            .map(|v| ServiceStockBranch {
                id: v.id.clone(),
                name: v.display_name.clone(),
            })
            .collect(),
        unavailable_reason: policy.unavailable_reason,
    };
    commerce.require_current(c).await?;
    inventory.require_current(c).await?;
    branches.require_current(c).await?;
    if let Some(x) = receipt {
        x.require_current(c).await?;
    }
    if let Some(x) = finance {
        x.require_current(c).await?;
    }
    actor.require_current(c, clock).await?;
    purchases.require_current(c, clock.now()).await?;
    let size = serde_json::to_vec(&data).map_err(|_| changed())?.len();
    if size > 2 * 1024 * 1024 {
        return Err(changed().into());
    }
    Ok(data)
}

use crate::service_parts_posting_setup::trimmed_units;
use erpx_server_commerce::public::service_stock_masters::{
    ServiceStockMaster, ServiceStockMasterIntent,
};
use erpx_server_shared::public::CommandKey;
use erpx_server_workflow::public::commands::CommandJson;
#[derive(Clone)]
pub struct NormalizedServiceStockMaster {
    pub master: ServiceStockMaster,
    pub workflow_input: CommandJson,
}
fn wire() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Review the Service stock master input.",
    )
}
fn text(
    v: &CommandJson,
    key: &str,
    min: usize,
    max: usize,
    trim: bool,
) -> Result<(String, CommandJson), BusinessError> {
    trimmed_units(&v.property(key)?.ok_or_else(wire)?, min, max, trim).map_err(|_| wire())
}
fn member(name: &str, value: &CommandJson) -> String {
    format!("\"{name}\":{}", value.as_str())
}
fn default_string(
    v: &CommandJson,
    name: &str,
    default: &str,
    min: usize,
    max: usize,
    trim: bool,
) -> Result<(String, CommandJson), BusinessError> {
    if v.property(name)?.is_some() {
        text(v, name, min, max, trim)
    } else {
        Ok((default.into(), CommandJson::from_serializable(&default)?))
    }
}
/// Preserve the strict Zod field order/defaults and logical UTF-16 fingerprint.
pub fn normalize_service_stock_master(
    input: &CommandJson,
    kind: &str,
) -> Result<NormalizedServiceStockMaster, BusinessError> {
    let allowed: &[&str] = match kind {
        "warehouses" => &["code", "name", "branchId", "status"],
        "products" => &[
            "itemType",
            "sku",
            "name",
            "unit",
            "trackingMode",
            "price",
            "taxBps",
            "status",
        ],
        _ => return Err(wire()),
    };
    if input
        .object_keys()?
        .ok_or_else(wire)?
        .iter()
        .any(|k| !allowed.contains(&k.as_str()))
    {
        return Err(wire());
    }
    let mut fields = Vec::new();
    let master = if kind == "warehouses" {
        let (code, c) = text(input, "code", 2, 40, true)?;
        let (name, n) = text(input, "name", 2, 160, true)?;
        let (branch_id, b) = text(input, "branchId", 1, 2 * 1024 * 1024, false)?;
        let (status, s) = default_string(input, "status", "active", 1, 16, false)?;
        if !["active", "archived"].contains(&status.as_str()) {
            return Err(wire());
        }
        fields.extend([
            member("code", &c),
            member("name", &n),
            member("branchId", &b),
            member("status", &s),
        ]);
        ServiceStockMaster::Warehouse {
            branch_id,
            code,
            name,
            status,
        }
    } else {
        let (item_type, i) = default_string(input, "itemType", "stock", 1, 16, false)?;
        if item_type != "stock" {
            return Err(wire());
        }
        let (sku, s) = text(input, "sku", 2, 40, true)?;
        let (name, n) = text(input, "name", 2, 160, true)?;
        let (unit, u) = default_string(input, "unit", "each", 1, 30, true)?;
        let (tracking_mode, t) = default_string(input, "trackingMode", "none", 1, 16, false)?;
        if !["none", "lot", "serial"].contains(&tracking_mode.as_str()) {
            return Err(wire());
        }
        let (price, p) = text(input, "price", 1, 2 * 1024 * 1024, false)?;
        let (whole, fraction) = price.split_once('.').unwrap_or((&price, ""));
        if whole.is_empty()
            || !whole.bytes().all(|x| x.is_ascii_digit())
            || (!fraction.is_empty() && !fraction.bytes().all(|x| x.is_ascii_digit()))
            || fraction.len() > 2
            || price.ends_with('.')
        {
            return Err(wire());
        }
        let significant = whole.trim_start_matches('0');
        let whole = if significant.is_empty() {
            0
        } else {
            significant.parse::<u64>().map_err(|_| wire())?
        };
        let frac = if fraction.is_empty() {
            0
        } else {
            fraction.parse::<u64>().map_err(|_| wire())? * if fraction.len() == 1 { 10 } else { 1 }
        };
        let price_minor = whole
            .checked_mul(100)
            .and_then(|v| v.checked_add(frac))
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or_else(wire)? as i64;
        let tax = input
            .property("taxBps")?
            .unwrap_or(CommandJson::parse("0")?);
        let tax_bps = tax.as_str().parse::<i64>().map_err(|_| wire())?;
        if !(0..=10_000).contains(&tax_bps) {
            return Err(wire());
        }
        let (status, st) = default_string(input, "status", "active", 1, 16, false)?;
        if !["active", "archived"].contains(&status.as_str()) {
            return Err(wire());
        }
        fields.extend([
            member("itemType", &i),
            member("sku", &s),
            member("name", &n),
            member("unit", &u),
            member("trackingMode", &t),
            member("price", &p),
            format!("\"taxBps\":{tax_bps}"),
            member("status", &st),
        ]);
        ServiceStockMaster::Product {
            sku,
            name,
            unit,
            tracking_mode,
            price_minor,
            tax_bps,
            status,
        }
    };
    Ok(NormalizedServiceStockMaster {
        master,
        workflow_input: CommandJson::parse(&format!("{{{}}}", fields.join(",")))?,
    })
}

use erpx_server_commerce::public::service_stock_masters::{
    CurrentServiceStockMapping, PgServiceStockMasterSource, PreparedServiceStockMaster,
    read_service_stock_master,
};
use erpx_server_commercial::public::purchases::CapabilityPurpose;
use erpx_server_finance::public::stock_master_money::{
    CurrentStockMasterFinance, PreparedStockMasterProductMoney, RetainedStockMasterProductMoney,
    prepare_stock_master_product_money, read_stock_master_product_money,
};
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam,
    selected_command_authority::CurrentServiceStockAuthority, work_context::WorkScopeKind,
};
use erpx_server_organization::public::field_service_workspace::CurrentServiceStockBranches;
use erpx_server_shared::public::{IdentifierSource, RecordId};
use erpx_server_workflow::public::{
    commands::{
        CommandActor, CommandDisposition, CommandField, PgCommandAuthority, PgCommandLedger,
        PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
    },
    record_origin::{
        PgRawRecordOrigin, RawRecordOriginRequest, read_raw_record_origin_result_with_limit,
        verify_raw_record_origin,
    },
    service_parts_posting::{
        CurrentServicePrerequisiteReceipt, CurrentServiceStockPostingReceipt,
        read_service_prerequisite_receipt,
    },
};

fn stock_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let s = current.selection();
    let a = s.assignment();
    let w = s.workspace();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: w.company_id.clone(),
        branch_id: w
            .branch_id
            .as_ref()
            .map(|x| CommandField::Value(x.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: CommandField::Null,
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: s.revision(),
            assignment_version: a.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match a.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|x| x.as_str().into()),
    })
}
fn require_stock_result(result: &CommandJson, id: &str) -> Result<(), BusinessError> {
    let actual = result
        .property("id")?
        .ok_or_else(changed)?
        .string_utf16_units()?
        .ok_or_else(changed)?;
    if actual != id.encode_utf16().collect::<Vec<_>>()
        || result.property("version")?.ok_or_else(changed)?.as_str() != "1"
    {
        return Err(changed());
    }
    Ok(())
}
async fn stock_receipt_result(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    request: &WorkflowRequest,
) -> Result<PgRawRecordOrigin, Error> {
    let identity = RawRecordOriginRequest {
        command_key: request.key.clone(),
        operation: request.operation.clone(),
        record_id: request.record_id.clone(),
        creator_user_id: current.user_id().as_str().into(),
        company_id: current.selection().workspace().company_id.clone(),
    };
    let (origin, result) = read_raw_record_origin_result_with_limit(c, identity, 2 * 1024 * 1024)
        .await?
        .ok_or_else(changed)?;
    require_stock_result(&result, &request.record_id)?;
    verify_raw_record_origin(c, &origin).await?;
    Ok(origin)
}
struct StockProof<'a, C> {
    intent: ServiceStockMasterIntent,
    clock: &'a C,
    authority: CurrentServiceStockAuthority<'a>,
    purchases: PostingPurchases,
    commerce: CurrentServiceStockMapping,
    receipt: CurrentServiceStockPostingReceipt,
    finance: CurrentStockMasterFinance,
    branch: Option<CurrentServiceStockBranches>,
    purpose: CapabilityPurpose,
}
impl<C: Clock + Sync> StockProof<'_, C> {
    async fn check(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.authority.require_current(c, self.clock).await?;
        self.purchases
            .require_for(c, self.purpose, self.clock.now())
            .await?;
        self.commerce.require_current(c).await?;
        self.receipt.require_current(c).await?;
        self.finance.require_current(c).await?;
        if let Some(branch) = &self.branch {
            branch.require_current(c).await?;
        }
        Ok(())
    }
}
impl<C: Clock + Sync> PgServiceStockMasterSource for StockProof<'_, C> {
    fn intent(&self) -> &ServiceStockMasterIntent {
        &self.intent
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.check(c).await
    }
    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.check(c).await
    }
}
async fn stock_proof<'a, C: Clock + Sync>(
    c: &mut PgConnection,
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    intent: ServiceStockMasterIntent,
    purpose: CapabilityPurpose,
) -> Result<StockProof<'a, C>, Error> {
    let authority = capture_service_stock_authority(c, current, clock).await?;
    let purchases = PostingPurchases::capture(c, current, clock.now()).await?;
    purchases.require_for(c, purpose, clock.now()).await?;
    let company = &current.selection().workspace().company_id;
    let commerce = read_service_stock_mapping(c, company).await?;
    let mapping_json = commerce.mapping_json().ok_or_else(|| {
        BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_INITIAL_PHYSICAL_STOCK_SETUP_REQUIRED",
            "Configure the original Service posting mapping first.",
        )
    })?;
    let mapping: serde_json::Value = serde_json::from_str(mapping_json).map_err(|_| changed())?;
    let chart = mapping
        .get("chart_id")
        .and_then(|x| x.as_str())
        .ok_or_else(changed)?;
    let currency = mapping
        .get("currency")
        .and_then(|x| x.as_str())
        .ok_or_else(changed)?;
    let receipt = read_service_stock_posting_receipt(c, company, mapping_json).await?;
    let finance = read_stock_master_finance(c, company, chart, currency).await?;
    if !commerce.schema_available()
        || receipt.facts().is_none()
        || !finance.active_chart_base_matches_mapping()
        || (purpose == CapabilityPurpose::NewWork && !finance.effective_matches_mapping())
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_INITIAL_PHYSICAL_STOCK_SETUP_REQUIRED",
            "Use the active original Service posting chart and document currency.",
        )
        .into());
    }
    let branch = if purpose == CapabilityPurpose::NewWork {
        if let ServiceStockMaster::Warehouse { branch_id, .. } = &intent.master {
            if current
                .selection()
                .workspace()
                .branch_id
                .as_ref()
                .is_some_and(|b| b.as_str() != branch_id)
            {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "SCOPE_FORBIDDEN",
                    "Choose your assigned branch.",
                )
                .into());
            }
            let source = read_service_stock_branches(c, company, Some(branch_id)).await?;
            if source.branches().next().is_none() {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "CONFLICT",
                    "Choose an active Branch in this Company.",
                )
                .into());
            }
            Some(source)
        } else {
            None
        }
    } else {
        None
    };
    let proof = StockProof {
        intent,
        clock,
        authority,
        purchases,
        commerce,
        receipt,
        finance,
        branch,
        purpose,
    };
    proof.check(c).await?;
    Ok(proof)
}
struct StockAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    intent: ServiceStockMasterIntent,
    purpose: CapabilityPurpose,
}
impl<C: Clock + Sync> PgCommandAuthority for StockAuthority<'_, C> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        let auth = capture_service_stock_authority(c, self.current, self.clock).await?;
        auth.require_current(c, self.clock).await?;
        Ok(stock_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        let proof = stock_proof(
            c,
            self.current,
            self.clock,
            self.intent.clone(),
            self.purpose,
        )
        .await?;
        if disposition == CommandDisposition::Replay {
            let origin = stock_receipt_result(c, self.current, request).await?;
            let scope = self
                .current
                .selection()
                .workspace()
                .branch_id
                .as_ref()
                .map(|x| x.as_str());
            let master = read_service_stock_master(c, &self.intent, scope)
                .await?
                .ok_or_else(changed)?;
            let money = if matches!(self.intent.master, ServiceStockMaster::Product { .. }) {
                Some(
                    read_stock_master_product_money(c, &self.intent.company, &self.intent.id())
                        .await?,
                )
            } else {
                None
            };
            master.require_current(c).await?;
            if let Some(money) = money {
                money.require_current(c).await?;
            }
            proof.check(c).await?;
            verify_raw_record_origin(c, &origin).await?;
        }
        Ok(())
    }
}
struct StockPlan<'a, C> {
    proof: StockProof<'a, C>,
    master: PreparedServiceStockMaster,
    money: Option<PreparedStockMasterProductMoney>,
    retained_money: Option<RetainedStockMasterProductMoney>,
    command_receipt: Option<CurrentServicePrerequisiteReceipt>,
    origin: Option<PgRawRecordOrigin>,
}
struct StockEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    intent: ServiceStockMasterIntent,
    plan: Option<StockPlan<'a, C>>,
}
impl<C: Clock + Sync> PgOwnerCommandEffects for StockEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let proof = stock_proof(
            c,
            self.current,
            self.clock,
            self.intent.clone(),
            CapabilityPurpose::NewWork,
        )
        .await?;
        let money = if let ServiceStockMaster::Product { price_minor, .. } = &self.intent.master {
            Some(
                prepare_stock_master_product_money(
                    c,
                    &self.intent.company,
                    &self.intent.id(),
                    proof.finance.currency(),
                    *price_minor,
                    self.clock,
                )
                .await?,
            )
        } else {
            None
        };
        let master = PreparedServiceStockMaster::prepare(c, &proof).await?;
        let result = CommandJson::from_serializable(
            &serde_json::json!({"id":request.record_id,"version":1}),
        )?;
        self.plan = Some(StockPlan {
            proof,
            master,
            money,
            retained_money: None,
            command_receipt: None,
            origin: None,
        });
        Ok(WorkflowOutput {
            version: 1,
            result,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<(), Error> {
        let p = self.plan.as_mut().ok_or_else(changed)?;
        p.proof.check(c).await?;
        let receipt = read_service_prerequisite_receipt(c, &request.key).await?;
        if !receipt.facts().is_some_and(|r| {
            r.operation == request.operation
                && r.record_id == request.record_id
                && r.actor_id == self.current.user_id().as_str()
                && r.company_id.as_deref() == Some(self.intent.company.as_str())
        }) {
            return Err(changed().into());
        }
        p.command_receipt = Some(receipt);
        p.origin = Some(stock_receipt_result(c, self.current, request).await?);
        if let Some(money) = p.money.take() {
            p.retained_money = Some(money.persist(c).await?);
        }
        p.master.apply(c, &p.proof).await
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let p = self.plan.as_ref().ok_or_else(changed)?;
        let receipt = p.command_receipt.as_ref().ok_or_else(changed)?;
        let origin = p.origin.as_ref().ok_or_else(changed)?;
        verify_raw_record_origin(c, origin).await?;
        receipt.require_current(c).await?;
        p.master.require_current(c, &p.proof).await?;
        if let Some(money) = &p.retained_money {
            money.require_current(c).await?;
        }
        sqlx::raw_sql("SET CONSTRAINTS finance_book_projection IMMEDIATE; SET CONSTRAINTS finance_book_projection DEFERRED; SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
        receipt.require_current(c).await?;
        verify_raw_record_origin(c, origin).await?;
        p.master.require_current(c, &p.proof).await?;
        if let Some(money) = &p.retained_money {
            money.require_current(c).await?;
        }
        Ok(())
    }
}
struct StockIds;
impl IdentifierSource for StockIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("UUID record identity")
    }
}
pub struct ServiceStockMasterOutcome {
    pub result: CommandJson,
    pub replayed: bool,
}
pub async fn execute_service_stock_master<C: Clock + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    key: &str,
    input: &NormalizedServiceStockMaster,
) -> Result<ServiceStockMasterOutcome, Error> {
    let command_id = CommandKey::new(key)?;
    let intent = ServiceStockMasterIntent {
        company: current.selection().workspace().company_id.clone(),
        command_id,
        master: input.master.clone(),
    };
    if !intent.valid() {
        return Err(wire().into());
    }
    let selected = capture_service_stock_authority(c, current, clock).await?;
    selected.require_current(c, clock).await?;
    let operation = match intent.master {
        ServiceStockMaster::Warehouse { .. } => "commerce.warehouses.create",
        ServiceStockMaster::Product { .. } => "commerce.products.create",
    };
    let receipt = read_service_prerequisite_receipt(c, key).await?;
    let replay = receipt.facts().is_some_and(|r| {
        r.operation == operation
            && r.actor_id == current.user_id().as_str()
            && r.company_id.as_deref() == Some(intent.company.as_str())
    });
    let purpose = if replay {
        CapabilityPurpose::History
    } else {
        CapabilityPurpose::NewWork
    };
    let request = WorkflowRequest {
        key: key.into(),
        operation: operation.into(),
        record_id: intent.id(),
        input: input.workflow_input.clone(),
    };
    let mut authority = StockAuthority {
        current,
        clock,
        intent: intent.clone(),
        purpose,
    };
    let mut effects = StockEffects {
        current,
        clock,
        intent,
        plan: None,
    };
    let result = PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        clock,
        &mut StockIds,
    )
    .execute(&request, &mut effects)
    .await?;
    require_stock_result(&result.result, &request.record_id)?;
    Ok(ServiceStockMasterOutcome {
        result: CommandJson::from_serializable(
            &serde_json::json!({"id":request.record_id,"version":1}),
        )?,
        replayed: result.replayed,
    })
}

use crate::field_service_reservation_read::{
    FieldServiceRenderedContext, context, selected_read_context,
};
use crate::service_parts_posting_setup::{ServicePostingEnvironment, ServicePostingTarget};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        HoldingWorkAuthorizer, SelectedContextRead, SelectedIngressFuture,
        SelectedTransportCallbacks, SelectedWorkIngress, WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use std::sync::Arc;
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceStockReadResponse {
    pub data: ServiceStockWorkspace,
    pub context: FieldServiceRenderedContext,
}
pub struct ServiceStockCommandResponse {
    pub data: CommandJson,
    pub context: FieldServiceRenderedContext,
}
struct StockReadIngress {
    selected: SelectedContextRead,
    holding: erpx_server_shared::public::HoldingId,
    company: erpx_server_shared::public::CompanyId,
}
enum StockReadDenial {
    View(Box<ServiceStockReadResponse>),
    Ingress(BusinessError),
}
fn stock_read_denied(
    input: &StockReadIngress,
    error: BusinessError,
) -> Result<StockReadDenial, BusinessError> {
    if error.code != "PERMISSION_REQUIRED" {
        return Err(error);
    }
    if input.selected.holding_id() != &input.holding
        || input.selected.selection().workspace().company_id != input.company
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "SERVICE_POSTING_SCOPE_REQUIRED",
            "Use the current Company.",
        ));
    }
    Ok(StockReadDenial::View(Box::new(ServiceStockReadResponse {
        data: unavailable(input.company.as_str(), "stock-authority-required"),
        context: selected_read_context(&input.selected)?,
    })))
}
pub async fn read_selected_service_stock<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServicePostingTarget<C>,
) -> Result<ServiceStockReadResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let ServicePostingTarget {
        holding,
        company,
        clock,
        environment,
    } = target;
    if environment != ServicePostingEnvironment::Development {
        return Err(changed().into());
    }
    let expected_holding = holding.clone();
    let expected_company = company.clone();
    let current_ownership = company.clone();
    let request_ownership = company.clone();
    let callbacks=SelectedTransportCallbacks::<_,StockReadIngress,StockReadDenial,_>::new::<ServiceStockReadResponse>(move|selected:&SelectedWorkIngress<'_>|->SelectedIngressFuture<StockReadIngress,StockReadDenial>{let result=selected.read_context().and_then(|selected|{let company=expected_company.selected(&expected_holding,&selected)?;Ok(StockReadIngress{selected,holding:expected_holding.clone(),company})}).map_err(StockReadDenial::Ingress);Box::pin(async move{result})},stock_read_denied,move|c,current,input:StockReadIngress|{let clock=clock.clone();let ownership=current_ownership.clone();Box::pin(async move{ownership.current(&input.holding,current)?;if current.holding_id()!=&input.holding||current.selection().workspace().company_id!=input.company{return Err(BusinessError::new(FailureKind::Forbidden,"SERVICE_POSTING_SCOPE_REQUIRED","Use the current Company.").into());}let response=ServiceStockReadResponse{data:read_service_stock_workspace(c,current,clock.as_ref()).await?,context:context(current)?};serde_json::to_writer(&mut crate::field_service_reservation_read::ReplyBudget(0),&response).map_err(|_|changed())?;Ok(Ok(response))})});
    match authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut r = request_ownership.request(request()?)?;
                r.mutation = false;
                Ok(r)
            },
            (
                WorkAuthorizationRequest {
                    permission: "inventory.read".into(),
                    module: None,
                    destination: "services/field-service".into(),
                    api: true,
                },
                ServiceStockSources,
            ),
            callbacks,
        )
        .await?
    {
        Ok(v) => Ok(v),
        Err(StockReadDenial::View(v)) => Ok(*v),
        Err(StockReadDenial::Ingress(e)) => Err(e.into()),
    }
}
pub struct ServiceStockTransportCommand {
    pub key: Result<String, BusinessError>,
    pub body: axum::body::Body,
    pub kind: &'static str,
}
pub async fn execute_selected_service_stock<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServicePostingTarget<C>,
    command: ServiceStockTransportCommand,
) -> Result<ServiceStockCommandResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let ServicePostingTarget {
        holding,
        company,
        clock,
        environment,
    } = target;
    if environment != ServicePostingEnvironment::Development {
        return Err(changed().into());
    }
    let ServiceStockTransportCommand { key, body, kind } = command;
    let request_ownership = company.clone();
    let body = Arc::new(tokio::sync::Mutex::new(Some(body)));
    let cached = Arc::new(tokio::sync::OnceCell::new());
    let callbacks=SelectedTransportCallbacks::<_,(String,NormalizedServiceStockMaster),BusinessError,_>::new::<ServiceStockCommandResponse>(move|_:&SelectedWorkIngress<'_>|->SelectedIngressFuture<(String,NormalizedServiceStockMaster),BusinessError>{let body=body.clone();let cached=cached.clone();let key=key.clone();Box::pin(async move{let key=key?;if key.is_empty(){return Err(BusinessError::invalid("COMMAND_KEY_REQUIRED","Idempotency-Key is required."));}let bytes=cached.get_or_init(||async{let body=body.lock().await.take()?;axum::body::to_bytes(body,2*1024*1024).await.ok().map(|v|v.to_vec())}).await.as_ref().ok_or_else(wire)?;let text=String::from_utf8_lossy(bytes);let text=text.strip_prefix('\u{feff}').unwrap_or(&text);let json=CommandJson::parse(text).map_err(|_|BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;let input=normalize_service_stock_master(&json,kind)?;Ok((key,input))})},|_,error|Err(error),move|c,current,(key,input):(String,NormalizedServiceStockMaster)|{let clock=clock.clone();let holding=holding.clone();let company=company.clone();Box::pin(async move{company.current(&holding,current)?;let rendered=context(current)?;let result=execute_service_stock_master(c,current,clock.as_ref(),&key,&input).await?;let size=result.result.as_str().len().saturating_add(serde_json::to_vec(&rendered).map_err(|_|changed())?.len()).saturating_add(20);if size>2*1024*1024{return Err(changed().into());}Ok(Ok(ServiceStockCommandResponse{data:result.result,context:rendered}))})});
    match authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut r = request_ownership.request(request()?)?;
                r.mutation = true;
                Ok(r)
            },
            (
                WorkAuthorizationRequest {
                    permission: "inventory.prepare".into(),
                    module: None,
                    destination: "services/field-service".into(),
                    api: true,
                },
                ServiceStockSources,
            ),
            callbacks,
        )
        .await?
    {
        Ok(v) => Ok(v),
        Err(e) => Err(e.into()),
    }
}

/// Contract-only request/result shapes; runtime uses ordered CommandJson for
/// the retained exact Workflow fingerprint.
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceStockWarehouseInput {
    pub code: String,
    pub name: String,
    pub branch_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub status: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceStockProductInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub item_type: Option<String>,
    pub sku: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub tracking_mode: Option<String>,
    pub price: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub tax_bps: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub status: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceStockMasterResult {
    pub id: String,
    pub version: u8,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceStockMasterResponse {
    pub data: ServiceStockMasterResult,
    pub context: FieldServiceRenderedContext,
}
