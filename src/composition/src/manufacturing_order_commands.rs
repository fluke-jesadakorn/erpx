//! Native Manufacturing order material-issue and QC-pass composition.
//!
//! Material issue (`issue`) and independent QC acceptance (`pass-qc`) own the
//! Inventory lot posting through `erpx_server_manufacturing::public::lots`,
//! the retained Finance production-costing guards/journal and the Files
//! evidence binding inside one serializable Holding transaction. The remaining
//! order actions stay on the retained Manufacturing TypeScript command until
//! their native ports exist.
use crate::finance_journal_actor::{command_actor, finance_actor};
use crate::manufacturing::manufacturing_purchase_mode;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue,
    lock_commercial_authority_sources,
};
use erpx_server_finance::public::{
    FinanceActor, FinanceCollation, JournalAuthority, JournalDraft, JournalError, JournalStatus,
    PgJournalPreparation, PgJournalStore, embedded_currency_rules, money_snapshot::{
        MoneySnapshotInput, PreparedMoneySnapshot, prepare_money_snapshot,
    }, prepare_journal,
};
use erpx_server_files::public::business_evidence::{
    PgBusinessEvidenceFiles, evidence_context, lock_business_evidence_sources, policy_fact,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_manufacturing::public::lots::{
    LotPosting, MaterialAllocation, MaterialLotRequest, OutputAllocation, OutputLotInput,
    OutputLotRequest, PgManufacturingLots, TrackingMode, plan_material_lots, plan_output_lots,
    post_material_lots, post_output_lots, scaled_amount,
};
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, CommandOutcome, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const ISSUE_CAPABILITIES: &[&str] = &[
    "manufacturing.execution",
    "manufacturing.materials",
    "inventory.stock-core",
    "manufacturing.costs",
    "finance.production-costing",
];
const PASS_QC_CAPABILITIES: &[&str] = &[
    "manufacturing.execution",
    "quality.production",
    "manufacturing.costs",
    "finance.production-costing",
    "inventory.stock-core",
];

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn journal_error(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => BusinessError::new(
            e.kind,
            e.code,
            "The production Finance posting is unavailable.",
        )
        .into(),
    }
}

/// Whole-day UTC date from epoch milliseconds (civil-from-days).
fn utc_date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.is_ascii()
        && bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
        && value[5..7]
            .parse::<u8>()
            .is_ok_and(|month| (1..=12).contains(&month))
        && value[8..10]
            .parse::<u8>()
            .is_ok_and(|day| (1..=31).contains(&day))
}

fn decimal_minor(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let value = value.abs();
    format!("{sign}{}.{:02}", value / 100, value % 100)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManufacturingOrderAction {
    Issue,
    PassQc,
}

impl ManufacturingOrderAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "issue" => Some(Self::Issue),
            "pass-qc" => Some(Self::PassQc),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::PassQc => "pass-qc",
        }
    }

    fn status(self) -> &'static str {
        match self {
            Self::Issue => "in_progress",
            Self::PassQc => "completed",
        }
    }

    fn permission(self) -> &'static str {
        match self {
            Self::Issue => "records.write",
            Self::PassQc => "records.review",
        }
    }

    fn capabilities(self) -> &'static [&'static str] {
        match self {
            Self::Issue => ISSUE_CAPABILITIES,
            Self::PassQc => PASS_QC_CAPABILITIES,
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMaterialAllocation {
    product_id: String,
    lot_id: String,
    quantity: String,
    balance_version: i64,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireOutputAllocation {
    quantity: String,
    #[serde(default)]
    lot_code: Option<String>,
    #[serde(default)]
    serial_number: Option<String>,
    #[serde(default)]
    expiry_date: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingOrderInput {
    pub expected_version: i64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub entry_date: Option<String>,
    #[serde(default)]
    pub material_lot_allocations: Vec<WireMaterialAllocation>,
    #[serde(default)]
    pub output_lot_allocations: Vec<WireOutputAllocation>,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
}

impl ManufacturingOrderInput {
    fn validate(&self, action: ManufacturingOrderAction) -> Result<(), BusinessError> {
        if self.expected_version < 1 {
            return Err(invalid(
                "MANUFACTURING_ORDER_INVALID",
                "Use the current order version.",
            ));
        }
        let reason = self.reason.as_deref().unwrap_or("").trim();
        if reason.chars().count() < 3 || reason.chars().count() > 3000 {
            return Err(invalid(
                "MANUFACTURING_REASON_REQUIRED",
                "Record a reason of 3 to 3000 characters.",
            ));
        }
        if let Some(entry) = &self.entry_date
            && !valid_date(entry)
        {
            return Err(invalid(
                "MANUFACTURING_ORDER_INVALID",
                "Use a valid entry date.",
            ));
        }
        if self.attachment_set_ids.len() > 10 {
            return Err(invalid(
                "MANUFACTURING_ATTACHMENT_LIMIT",
                "Attach at most ten evidence sets.",
            ));
        }
        if action == ManufacturingOrderAction::Issue
            && self.material_lot_allocations.len() > 500
        {
            return Err(invalid(
                "MANUFACTURING_LOT_LIMIT",
                "Select at most five hundred material lots.",
            ));
        }
        if action == ManufacturingOrderAction::PassQc && self.output_lot_allocations.len() > 500 {
            return Err(invalid(
                "MANUFACTURING_LOT_LIMIT",
                "Select at most five hundred output lots.",
            ));
        }
        Ok(())
    }

    fn material_allocations(&self) -> Result<Vec<MaterialAllocation>, BusinessError> {
        self.material_lot_allocations
            .iter()
            .map(|allocation| {
                erpx_server_manufacturing::public::lots::parse_material_allocation(
                    &allocation.product_id,
                    &allocation.lot_id,
                    &allocation.quantity,
                    allocation.balance_version,
                )
                .ok_or_else(|| {
                    invalid(
                        "MANUFACTURING_LOT_INVALID",
                        "Select current material lots and quantities.",
                    )
                })
            })
            .collect()
    }

    fn output_allocations(&self) -> Result<Vec<OutputAllocation>, BusinessError> {
        self.output_lot_allocations
            .iter()
            .map(|allocation| {
                erpx_server_manufacturing::public::lots::parse_output_allocation(
                    &allocation.quantity,
                    allocation.lot_code.as_deref(),
                    allocation.serial_number.as_deref(),
                    allocation.expiry_date.as_deref(),
                )
                .ok_or_else(|| {
                    invalid(
                        "MANUFACTURING_LOT_INVALID",
                        "Select output lot codes and quantities.",
                    )
                })
            })
            .collect()
    }

    fn entry_date(&self, now: i64) -> String {
        self.entry_date.clone().unwrap_or_else(|| utc_date(now))
    }
}

#[derive(Serialize)]
struct ManufacturingOrderResult {
    id: String,
    status: String,
    version: i64,
    #[serde(rename = "journalId")]
    journal_id: Option<String>,
}

pub struct ManufacturingOrderOutcome {
    pub result: CommandJson,
    pub context: Value,
}

#[derive(Clone)]
pub struct ManufacturingOrderCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub clock: Arc<C>,
}

#[derive(Clone)]
struct OrderSources;

impl HoldingWorkSourcePreparation for OrderSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            sqlx::query(
                "LOCK TABLE finance_journals,finance_journal_lines,finance_periods,\
                 finance_accounts,finance_charts,manufacturing_orders,\
                 manufacturing_material_reservations,manufacturing_order_postings,\
                 manufacturing_cost_movements,commerce_stock,inventory_lots,\
                 inventory_lot_balances IN SHARE ROW EXCLUSIVE MODE",
            )
            .execute(&mut *c)
            .await?;
            lock_business_evidence_sources(c).await?;
            Ok(())
        })
    }
}

struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    request: &'a WorkflowRequest,
    clock: &'a C,
    catalogue: CommercialCatalogue,
    purchase: Option<PurchaseWitness>,
    mode: PackageEnforcementMode,
    capabilities: &'static [&'static str],
    purpose: CapabilityPurpose,
    permission: &'static str,
}

impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&OrderSources, c).await
    }

    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.operation != self.request.operation
            || r.record_id != self.request.record_id
            || r.input.as_str() != self.request.input.as_str()
        {
            return Err(conflict(
                "MANUFACTURING_ORDER_CONTEXT_CHANGED",
                "Review the production command before retrying.",
            )
            .into());
        }
        Ok(command_actor(self.current)?)
    }

    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        self.current.require_permission(self.permission)?;
        if &command_actor(self.current)? != actor {
            return Err(conflict(
                "MANUFACTURING_ORDER_CONTEXT_CHANGED",
                "Your work role changed. Reload and review before retrying.",
            )
            .into());
        }
        if self.mode == PackageEnforcementMode::Disabled {
            return Ok(());
        }
        let now = self.clock.now();
        let purpose = match disposition {
            CommandDisposition::Replay => CapabilityPurpose::History,
            CommandDisposition::Execute => self.purpose,
        };
        let mut reader = PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            actor.company_id.clone(),
            c,
            &self.catalogue,
        );
        if let Some(witness) = &self.purchase {
            reader.revalidate(witness, now).await?.require_allowed()?;
        } else {
            self.purchase = Some(
                reader
                    .inspect(
                        &PurchaseRequest {
                            capabilities: self
                                .capabilities
                                .iter()
                                .map(|capability| (*capability).to_owned())
                                .collect(),
                            purpose,
                            mode: self.mode,
                        },
                        now,
                    )
                    .await?
                    .require_allowed()?
                    .clone(),
            );
        }
        Ok(())
    }
}

#[derive(Clone)]
struct OrderFacts {
    id: String,
    warehouse_id: String,
    specification_id: String,
    planned_quantity_milli: i64,
    batch_quantity_milli: i64,
    output_product_id: String,
    output_tracking_mode: String,
    status: String,
    material_cost_minor: i64,
    labor_cost_minor: i64,
    overhead_cost_minor: i64,
    reported_output_milli: Option<i64>,
    version: i64,
}

#[derive(Clone)]
struct LineFacts {
    product_id: String,
    quantity_milli: i64,
    tracking_mode: String,
}

#[derive(Clone)]
struct IssueLinePlan {
    product_id: String,
    reservation_id: String,
    amount_milli: i64,
    stock_version: i64,
    value_minor: i64,
}

async fn read_order(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
) -> Result<Option<OrderFacts>, Error> {
    let row = sqlx::query(
        "SELECT o.id,o.warehouse_id,o.specification_id,o.planned_quantity_milli,\
         s.batch_quantity_milli,s.output_product_id,p.tracking_mode output_tracking_mode,\
         o.status,o.material_cost_minor,o.labor_cost_minor,o.overhead_cost_minor,\
         o.reported_output_milli,o.version \
         FROM manufacturing_orders o \
         JOIN manufacturing_specifications s ON s.id=o.specification_id \
         JOIN commerce_products p ON p.id=s.output_product_id \
         WHERE o.id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3) \
         FOR UPDATE OF o",
    )
    .bind(id)
    .bind(company)
    .bind(branch)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, Error>(OrderFacts {
            id: row.try_get("id")?,
            warehouse_id: row.try_get("warehouse_id")?,
            specification_id: row.try_get("specification_id")?,
            planned_quantity_milli: row.try_get("planned_quantity_milli")?,
            batch_quantity_milli: row.try_get("batch_quantity_milli")?,
            output_product_id: row.try_get("output_product_id")?,
            output_tracking_mode: row.try_get("output_tracking_mode")?,
            status: row.try_get("status")?,
            material_cost_minor: row.try_get("material_cost_minor")?,
            labor_cost_minor: row.try_get("labor_cost_minor")?,
            overhead_cost_minor: row.try_get("overhead_cost_minor")?,
            reported_output_milli: row.try_get("reported_output_milli")?,
            version: row.try_get("version")?,
        })
    })
    .transpose()
}

async fn read_lines(
    c: &mut PgConnection,
    specification_id: &str,
) -> Result<Vec<LineFacts>, Error> {
    let rows = sqlx::query(
        "SELECT l.product_id,l.quantity_milli,p.tracking_mode \
         FROM manufacturing_specification_lines l \
         JOIN commerce_products p ON p.id=l.product_id \
         WHERE l.specification_id=$1 ORDER BY l.ordinal",
    )
    .bind(specification_id)
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok::<_, Error>(LineFacts {
                product_id: row.try_get("product_id")?,
                quantity_milli: row.try_get("quantity_milli")?,
                tracking_mode: row.try_get("tracking_mode")?,
            })
        })
        .collect()
}

struct ApprovedProfile {
    id: String,
    chart_id: String,
    inventory_account_id: String,
    wip_account_id: String,
    version: i64,
    currency: String,
    account_snapshot: String,
}

async fn read_approved_profile(
    c: &mut PgConnection,
    company: &str,
) -> Result<ApprovedProfile, Error> {
    let row = sqlx::query(
        "SELECT id,chart_id,inventory_account_id,wip_account_id,labor_account_id,\
         overhead_account_id,scrap_account_id,version,currency,\
         json_build_object('inventory',inventory_account_id,'wip',wip_account_id,\
         'labor',labor_account_id,'overhead',overhead_account_id,\
         'scrap',scrap_account_id)::text account_snapshot \
         FROM manufacturing_accounting_profiles WHERE company_id=$1 AND status='approved'",
    )
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, Error>(ApprovedProfile {
            id: row.try_get("id")?,
            chart_id: row.try_get("chart_id")?,
            inventory_account_id: row.try_get("inventory_account_id")?,
            wip_account_id: row.try_get("wip_account_id")?,
            version: row.try_get("version")?,
            currency: row.try_get("currency")?,
            account_snapshot: row.try_get("account_snapshot")?,
        })
    })
    .transpose()?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::InvalidInput,
            "MANUFACTURING_ACCOUNTING_REQUIRED",
            "Finance must independently approve a production account map before materials or costs can be posted.",
        )
    })
    .map_err(Into::into)
}

async fn read_period(c: &mut PgConnection, company: &str, date: &str) -> Result<String, Error> {
    let periods: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' \
         AND start_date<=$2 AND end_date>=$2",
    )
    .bind(company)
    .bind(date)
    .fetch_all(&mut *c)
    .await?;
    if periods.len() != 1 {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "MANUFACTURING_PERIOD_REQUIRED",
            "Open a Finance period covering the production entry date.",
        )
        .into());
    }
    Ok(periods.into_iter().next().expect("one period"))
}

async fn functional_currency_guard(
    c: &mut PgConnection,
    company: &str,
    chart_id: &str,
    inventory_account_id: &str,
) -> Result<(), Error> {
    let matches: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_charts c \
         JOIN finance_accounts a ON a.chart_id=c.id AND a.company_id=c.company_id \
         WHERE c.id=$1 AND c.company_id=$2 AND c.base_currency='THB' \
         AND a.id=$3 AND a.currency=c.base_currency AND a.status='active')",
    )
    .bind(chart_id)
    .bind(company)
    .bind(inventory_account_id)
    .fetch_one(&mut *c)
    .await?;
    if !matches {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "MANUFACTURING_CURRENCY_ADAPTER",
            "The production accounting adapter requires a THB functional-currency chart.",
        )
        .into());
    }
    Ok(())
}

struct BoundJournal {
    actor: FinanceActor,
    draft: JournalDraft,
}

impl JournalAuthority for BoundJournal {
    type Error = PgTransactionError<JournalError>;

    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        let lines = draft.lines.len() == self.draft.lines.len()
            && draft.lines.iter().zip(&self.draft.lines).all(|(a, b)| {
                a.account_id == b.account_id
                    && a.debit == b.debit
                    && a.credit == b.credit
                    && a.description == b.description
                    && a.dimensions == b.dimensions
            });
        if actor != &self.actor
            || !lines
            || draft.chart_id != self.draft.chart_id
            || draft.period_id != self.draft.period_id
            || draft.entry_date != self.draft.entry_date
            || draft.currency != self.draft.currency
            || draft.fx_rate != self.draft.fx_rate
            || draft.memo != self.draft.memo
            || draft.source_type != self.draft.source_type
            || draft.source_id != self.draft.source_id
            || draft.is_opening
        {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "MANUFACTURING_FINANCE_BINDING_REQUIRED",
                message: "Use the currently authorized production posting.".into(),
            }
            .into());
        }
        Ok(())
    }
}

fn journal_lines(
    profile: &ApprovedProfile,
    action: ManufacturingOrderAction,
    material: i64,
    total: i64,
) -> Vec<erpx_server_finance::public::JournalLineInput> {
    let mut lines = Vec::new();
    fn add(
        lines: &mut Vec<erpx_server_finance::public::JournalLineInput>,
        account: &str,
        debit: i64,
        credit: i64,
        description: &str,
    ) {
        if debit != 0 || credit != 0 {
            lines.push(erpx_server_finance::public::JournalLineInput {
                account_id: account.to_owned(),
                description: Some(description.to_owned()),
                debit: decimal_minor(debit),
                credit: decimal_minor(credit),
                dimensions: None,
            });
        }
    }
    match action {
        ManufacturingOrderAction::Issue => {
            add(
                &mut lines,
                &profile.wip_account_id,
                total,
                0,
                "Production work in progress",
            );
            add(
                &mut lines,
                &profile.inventory_account_id,
                0,
                material,
                "Materials issued from inventory",
            );
        }
        ManufacturingOrderAction::PassQc => {
            add(
                &mut lines,
                &profile.inventory_account_id,
                total,
                0,
                "Finished output capitalized",
            );
            add(
                &mut lines,
                &profile.wip_account_id,
                0,
                total,
                "WIP relieved at independent disposition",
            );
        }
    }
    lines
}

struct OrderEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    id: &'a str,
    action: ManufacturingOrderAction,
    input: &'a ManufacturingOrderInput,
    order: Option<OrderFacts>,
    lines: Vec<LineFacts>,
    issue_plan: Vec<IssueLinePlan>,
    profile: Option<ApprovedProfile>,
    entry_date: String,
    journal: Option<erpx_server_finance::public::JournalPreparation>,
    money: Vec<PreparedMoneySnapshot>,
    evidence: Option<erpx_server_files::public::business_evidence::PreparedBusinessEvidence>,
}

fn evidence_binding(
    input: &ManufacturingOrderInput,
    id: &str,
    key: &str,
) -> Option<BusinessEvidenceRequest> {
    if input.attachment_set_ids.is_empty() {
        return None;
    }
    Some(BusinessEvidenceRequest {
        attachment_set_ids: input.attachment_set_ids.clone(),
        target_types: vec!["production_run".into(), "manufacturing_order".into()],
        contexts: vec![evidence_context("qc_evidence", "action", Some("qc-review"))],
        target_id: id.to_owned(),
        expected_target_version: input.expected_version,
        command_id: key.to_owned(),
        binding_type: "manufacturing_pass-qc".into(),
        binding_id: format!("mfg-event-{key}"),
    })
}

impl<C: Clock + Sync> PgOwnerCommandEffects for OrderEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        HoldingWorkSourcePreparation::prepare(&OrderSources, c).await
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let now = self.clock.now().get();
        let workspace = self.current.selection().workspace();
        let branch = workspace.branch_id.as_ref().map(|id| id.as_str());
        let order = read_order(c, self.company.as_str(), branch, self.id)
            .await?
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "MANUFACTURING_ORDER_NOT_FOUND",
                    "Order not found.",
                )
            })?;
        if order.version != self.input.expected_version {
            return Err(conflict("CONFLICT", "Order changed. Reload and retry.").into());
        }
        let next = self.input.expected_version + 1;
        let lines = read_lines(c, &order.specification_id).await?;
        let entry_date = self.input.entry_date(now);
        let mut journal = None;
        match self.action {
            ManufacturingOrderAction::Issue => {
                if order.status != "reserved" {
                    return Err(conflict(
                        "INVALID_MANUFACTURING_TRANSITION",
                        "Issue a reserved order.",
                    )
                    .into());
                }
                let allocations = self.input.material_allocations()?;
                let mut plan = Vec::with_capacity(lines.len());
                let mut material = 0_i64;
                for line in &lines {
                    let reservation = sqlx::query(
                        "SELECT id,quantity_milli,consumed_milli,released_milli,expires_at \
                         FROM manufacturing_material_reservations \
                         WHERE order_id=$1 AND product_id=$2 FOR UPDATE",
                    )
                    .bind(self.id)
                    .bind(&line.product_id)
                    .fetch_optional(&mut *c)
                    .await?
                    .ok_or_else(|| {
                        conflict(
                            "MATERIAL_RESERVATION_REQUIRED",
                            "Material reservation is missing.",
                        )
                    })?;
                    let reserved: i64 = reservation.try_get("quantity_milli")?;
                    let consumed: i64 = reservation.try_get("consumed_milli")?;
                    let released: i64 = reservation.try_get("released_milli")?;
                    let expires_at: i64 = reservation.try_get("expires_at")?;
                    if consumed != 0 || released != 0 || expires_at <= now {
                        return Err(conflict(
                            "MATERIAL_RESERVATION_REQUIRED",
                            "The material reservation is no longer current.",
                        )
                        .into());
                    }
                    let stock = sqlx::query(
                        "SELECT quantity_milli,value_minor,version FROM commerce_stock \
                         WHERE warehouse_id=$1 AND product_id=$2 FOR UPDATE",
                    )
                    .bind(&order.warehouse_id)
                    .bind(&line.product_id)
                    .fetch_optional(&mut *c)
                    .await?
                    .ok_or_else(|| {
                        conflict(
                            "MATERIAL_RESERVATION_REQUIRED",
                            "Material stock is missing.",
                        )
                    })?;
                    let stock_quantity: i64 = stock.try_get("quantity_milli")?;
                    let stock_value: i64 = stock.try_get("value_minor")?;
                    let stock_version: i64 = stock.try_get("version")?;
                    if stock_quantity < reserved || stock_value < 0 {
                        return Err(conflict(
                            "MATERIAL_RESERVATION_REQUIRED",
                            "Material stock is insufficient.",
                        )
                        .into());
                    }
                    let tracking_mode = TrackingMode::parse(&line.tracking_mode).ok_or_else(|| {
                        invalid(
                            "MANUFACTURING_TRACKING_MODE",
                            "Use a supported inventory tracking mode.",
                        )
                    })?;
                    let line_allocations: Vec<MaterialAllocation> = allocations
                        .iter()
                        .filter(|allocation| allocation.product_id == line.product_id)
                        .cloned()
                        .collect();
                    let mut lots = PgManufacturingLots::new(&mut *c);
                    let facts = {
                        let mut values = Vec::with_capacity(line_allocations.len());
                        for allocation in &line_allocations {
                            values.push(
                                erpx_server_manufacturing::public::lots::ManufacturingLotRepository::material_lot(
                                    &mut lots,
                                    self.company.as_str(),
                                    &line.product_id,
                                    &order.warehouse_id,
                                    &allocation.lot_id,
                                )
                                .await?,
                            );
                        }
                        values
                    };
                    let planned = plan_material_lots(
                        tracking_mode,
                        reserved,
                        &entry_date,
                        &line_allocations,
                        &facts,
                    )?;
                    let value = match &planned {
                        Some(planned) => planned.value_minor,
                        None => {
                            scaled_amount(stock_value, reserved, stock_quantity).ok_or_else(|| {
                                invalid("MONEY_OVERFLOW", "Material value exceeds ledger precision.")
                            })?
                        }
                    };
                    if value < 0 || value > stock_value {
                        return Err(conflict(
                            "MATERIAL_RESERVATION_REQUIRED",
                            "Material value is unavailable.",
                        )
                        .into());
                    }
                    material = material.checked_add(value).ok_or_else(|| {
                        invalid("MONEY_OVERFLOW", "Production costs exceed ledger precision.")
                    })?;
                    plan.push(IssueLinePlan {
                        product_id: line.product_id.clone(),
                        reservation_id: reservation.try_get("id")?,
                        amount_milli: reserved,
                        stock_version,
                        value_minor: value,
                    });
                }
                let profile = read_approved_profile(c, self.company.as_str()).await?;
                functional_currency_guard(
                    c,
                    self.company.as_str(),
                    &profile.chart_id,
                    &profile.inventory_account_id,
                )
                .await?;
                let period = read_period(c, self.company.as_str(), &entry_date).await?;
                let total = material;
                if total > 0 {
                    let lines = journal_lines(
                        &profile,
                        ManufacturingOrderAction::Issue,
                        material,
                        total,
                    );
                    journal = Some(
                        prepare_order_journal(
                            c,
                            self.current,
                            self.clock,
                            self.request,
                            &entry_date,
                            &period,
                            &profile,
                            "issue",
                            next,
                            lines,
                        )
                        .await?,
                    );
                    self.money.push(
                        prepare_money_snapshot(
                            c,
                            self.company,
                            MoneySnapshotInput {
                                source_type: "manufacturing_cost_movement".into(),
                                source_id: format!("mfg-movement-{}-0", self.request.key),
                                amount_kind: "material_issue".into(),
                                transaction_date: entry_date.clone(),
                                currency: profile.currency.clone(),
                                currency_scale: Some(2),
                                amount_units: material,
                            },
                            self.clock,
                        )
                        .await?,
                    );
                }
                self.issue_plan = plan;
                let result = ManufacturingOrderResult {
                    id: order.id.clone(),
                    status: self.action.status().into(),
                    version: next,
                    journal_id: journal.as_ref().map(|plan| plan.journal_id().to_owned()),
                };
                self.order = Some(order);
                self.lines = lines;
                self.profile = Some(profile);
                self.entry_date = entry_date;
                self.journal = journal;
                let result = CommandJson::from_serializable(&result)?;
                return Ok(WorkflowOutput {
                    version: next,
                    result,
                    event_scope: None,
                });
            }
            ManufacturingOrderAction::PassQc => {
                if order.status != "awaiting_qc" {
                    return Err(conflict(
                        "INVALID_MANUFACTURING_TRANSITION",
                        "QC reviews reported output.",
                    )
                    .into());
                }
                let reporter: Option<String> = sqlx::query_scalar(
                    "SELECT actor_id FROM manufacturing_order_events \
                     WHERE order_id=$1 AND action='report' ORDER BY version DESC LIMIT 1",
                )
                .bind(self.id)
                .fetch_optional(&mut *c)
                .await?;
                if reporter.as_deref() == Some(self.current.user_id().as_str()) {
                    return Err(conflict(
                        "CONFLICT",
                        "QC must be independent from output reporting.",
                    )
                    .into());
                }
                let output = order.reported_output_milli.unwrap_or(0);
                if output <= 0 {
                    return Err(conflict(
                        "INVALID_MANUFACTURING_TRANSITION",
                        "Report positive output before QC.",
                    )
                    .into());
                }
                let total = order
                    .material_cost_minor
                    .checked_add(order.labor_cost_minor)
                    .and_then(|value| value.checked_add(order.overhead_cost_minor))
                    .ok_or_else(|| {
                        invalid("MONEY_OVERFLOW", "Production costs exceed ledger precision.")
                    })?;
                let tracking_mode =
                    TrackingMode::parse(&order.output_tracking_mode).ok_or_else(|| {
                        invalid(
                            "MANUFACTURING_TRACKING_MODE",
                            "Use a supported inventory tracking mode.",
                        )
                    })?;
                let allocations = self.input.output_allocations()?;
                let output_input = OutputLotInput {
                    key: self.request.key.as_str(),
                    order_id: self.id,
                    order_version: next,
                    tracking_mode,
                    output_milli: output,
                    total_minor: total,
                    entry_date: entry_date.as_str(),
                };
                if tracking_mode == TrackingMode::None {
                    plan_output_lots(&output_input, &allocations, &[])?;
                } else {
                    let mut lots = PgManufacturingLots::new(&mut *c);
                    let issues = erpx_server_manufacturing::public::lots::ManufacturingLotRepository::material_lot_issues(
                        &mut lots,
                        self.id,
                    )
                    .await?;
                    plan_output_lots(&output_input, &allocations, &issues)?;
                }
                let profile = read_approved_profile(c, self.company.as_str()).await?;
                functional_currency_guard(
                    c,
                    self.company.as_str(),
                    &profile.chart_id,
                    &profile.inventory_account_id,
                )
                .await?;
                let period = read_period(c, self.company.as_str(), &entry_date).await?;
                if total > 0 {
                    let lines = journal_lines(
                        &profile,
                        ManufacturingOrderAction::PassQc,
                        0,
                        total,
                    );
                    journal = Some(
                        prepare_order_journal(
                            c,
                            self.current,
                            self.clock,
                            self.request,
                            &entry_date,
                            &period,
                            &profile,
                            "output",
                            next,
                            lines,
                        )
                        .await?,
                    );
                    self.money.push(
                        prepare_money_snapshot(
                            c,
                            self.company,
                            MoneySnapshotInput {
                                source_type: "manufacturing_cost_movement".into(),
                                source_id: format!("mfg-movement-{}-0", self.request.key),
                                amount_kind: "output".into(),
                                transaction_date: entry_date.clone(),
                                currency: profile.currency.clone(),
                                currency_scale: Some(2),
                                amount_units: total,
                            },
                            self.clock,
                        )
                        .await?,
                    );
                }
                if let Some(binding) =
                    evidence_binding(self.input, self.id, self.request.key.as_str())
                {
                    let mut files = PgBusinessEvidenceFiles::new(
                        c,
                        self.company.clone(),
                        self.current.user_id().as_str().to_owned(),
                        branch.map(str::to_owned),
                        None,
                        vec![policy_fact(
                            "production_run",
                            "qc_evidence",
                            &["action"],
                            1,
                        )],
                        erpx_server_shared::public::TimestampMillis::new(now),
                    );
                    self.evidence = prepare_business_evidence(&mut files, binding).await?;
                }
                let result = ManufacturingOrderResult {
                    id: order.id.clone(),
                    status: self.action.status().into(),
                    version: next,
                    journal_id: journal.as_ref().map(|plan| plan.journal_id().to_owned()),
                };
                self.order = Some(order);
                self.lines = lines;
                self.profile = Some(profile);
                self.entry_date = entry_date;
                self.journal = journal;
                let result = CommandJson::from_serializable(&result)?;
                return Ok(WorkflowOutput {
                    version: next,
                    result,
                    event_scope: None,
                });
            }
        }
    }

    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let now = self.clock.now().get();
        let actor = self.current.user_id().as_str().to_owned();
        let order = self.order.take().ok_or_else(|| {
            conflict(
                "MANUFACTURING_ORDER_CONTEXT_CHANGED",
                "Reload the order before retrying.",
            )
        })?;
        let next = self.input.expected_version + 1;
        let status = self.action.status();
        match self.action {
            ManufacturingOrderAction::Issue => {
                let mut material = 0_i64;
                for (index, line) in self.issue_plan.iter().enumerate() {
                    let reservation = sqlx::query(
                        "UPDATE manufacturing_material_reservations \
                         SET consumed_milli=quantity_milli \
                         WHERE id=$1 AND consumed_milli=0 AND released_milli=0 AND expires_at>$2",
                    )
                    .bind(&line.reservation_id)
                    .bind(now)
                    .execute(&mut *c)
                    .await?;
                    if reservation.rows_affected() != 1 {
                        return Err(conflict(
                            "MATERIAL_RESERVATION_REQUIRED",
                            "The material reservation changed. Reload and retry.",
                        )
                        .into());
                    }
                    let stock = sqlx::query(
                        "UPDATE commerce_stock SET quantity_milli=quantity_milli-$1,\
                         value_minor=value_minor-$2,version=version+1 \
                         WHERE warehouse_id=$3 AND product_id=$4 AND version=$5 \
                         AND quantity_milli>=$1 AND value_minor>=$2",
                    )
                    .bind(line.amount_milli)
                    .bind(line.value_minor)
                    .bind(&order.warehouse_id)
                    .bind(&line.product_id)
                    .bind(line.stock_version)
                    .execute(&mut *c)
                    .await?;
                    if stock.rows_affected() != 1 {
                        return Err(conflict(
                            "MATERIAL_RESERVATION_REQUIRED",
                            "Material stock changed. Reload and retry.",
                        )
                        .into());
                    }
                    sqlx::query(
                        "INSERT INTO manufacturing_inventory_moves(id,order_id,warehouse_id,\
                         product_id,phase,source_order_version,quantity_milli,value_minor,\
                         movement_date,created_by,created_at) \
                         VALUES($1,$2,$3,$4,'issue',$5,$6,$7,$8,$9,$10)",
                    )
                    .bind(format!(
                        "mfg-stock-{}-{index}",
                        self.request.key
                    ))
                    .bind(self.id)
                    .bind(&order.warehouse_id)
                    .bind(&line.product_id)
                    .bind(next)
                    .bind(-line.amount_milli)
                    .bind(-line.value_minor)
                    .bind(&self.entry_date)
                    .bind(&actor)
                    .bind(now)
                    .execute(&mut *c)
                    .await?;
                    let tracking_mode = {
                        let line = self.lines.get(index).ok_or_else(|| {
                            conflict(
                                "MANUFACTURING_ORDER_CONTEXT_CHANGED",
                                "Reload the order before retrying.",
                            )
                        })?;
                        TrackingMode::parse(&line.tracking_mode).ok_or_else(|| {
                            invalid(
                                "MANUFACTURING_TRACKING_MODE",
                                "Use a supported inventory tracking mode.",
                            )
                        })?
                    };
                    let request = MaterialLotRequest {
                        tracking_mode,
                        required_milli: line.amount_milli,
                        allocations: self
                            .input
                            .material_lot_allocations
                            .iter()
                            .filter(|allocation| allocation.product_id == line.product_id)
                            .map(|allocation| {
                                erpx_server_manufacturing::public::lots::parse_material_allocation(
                                    &allocation.product_id,
                                    &allocation.lot_id,
                                    &allocation.quantity,
                                    allocation.balance_version,
                                )
                                .ok_or_else(|| {
                                    invalid(
                                        "MANUFACTURING_LOT_INVALID",
                                        "Select current material lots and quantities.",
                                    )
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    };
                    let posting = {
                        let mut lots = PgManufacturingLots::new(&mut *c);
                        post_material_lots(
                            &mut lots,
                            LotPosting {
                                key: self.request.key.as_str(),
                                order_id: self.id,
                                order_version: next,
                                warehouse_id: &order.warehouse_id,
                                product_id: &line.product_id,
                                entry_date: &self.entry_date,
                                company_id: self.company.as_str(),
                                user_id: &actor,
                                now,
                            },
                            request,
                        )
                        .await?
                    };
                    if let Some(posting) = posting
                        && posting.value_minor != line.value_minor
                    {
                        return Err(conflict(
                            "MANUFACTURING_ORDER_CONTEXT_CHANGED",
                            "The material lot value changed. Reload and retry.",
                        )
                        .into());
                    }
                    material = material.checked_add(line.value_minor).ok_or_else(|| {
                        invalid("MONEY_OVERFLOW", "Production costs exceed ledger precision.")
                    })?;
                }
                sqlx::query("UPDATE manufacturing_orders SET material_cost_minor=$1 WHERE id=$2")
                    .bind(material)
                    .bind(self.id)
                    .execute(&mut *c)
                    .await?;
            }
            ManufacturingOrderAction::PassQc => {
                let output = order.reported_output_milli.unwrap_or(0);
                let total = order
                    .material_cost_minor
                    .checked_add(order.labor_cost_minor)
                    .and_then(|value| value.checked_add(order.overhead_cost_minor))
                    .ok_or_else(|| {
                        invalid("MONEY_OVERFLOW", "Production costs exceed ledger precision.")
                    })?;
                sqlx::query(
                    "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,\
                     value_minor,version) VALUES($1,$2,$3,$4,1) \
                     ON CONFLICT(warehouse_id,product_id) DO UPDATE SET \
                     quantity_milli=commerce_stock.quantity_milli+excluded.quantity_milli,\
                     value_minor=commerce_stock.value_minor+excluded.value_minor,\
                     version=commerce_stock.version+1",
                )
                .bind(&order.warehouse_id)
                .bind(&order.output_product_id)
                .bind(output)
                .bind(total)
                .execute(&mut *c)
                .await?;
                let backdated: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=$1 \
                     AND product_id=$2 AND movement_date>$3)",
                )
                .bind(&order.warehouse_id)
                .bind(&order.output_product_id)
                .bind(&self.entry_date)
                .fetch_one(&mut *c)
                .await?;
                if backdated {
                    return Err(conflict(
                        "MANUFACTURING_OUTPUT_DATE",
                        "Record output on or after its latest movement date.",
                    )
                    .into());
                }
                sqlx::query(
                    "INSERT INTO manufacturing_inventory_moves(id,order_id,warehouse_id,\
                     product_id,phase,source_order_version,quantity_milli,value_minor,\
                     movement_date,created_by,created_at) \
                     VALUES($1,$2,$3,$4,'output',$5,$6,$7,$8,$9,$10)",
                )
                .bind(format!("mfg-stock-{}", self.request.key))
                .bind(self.id)
                .bind(&order.warehouse_id)
                .bind(&order.output_product_id)
                .bind(next)
                .bind(output)
                .bind(total)
                .bind(&self.entry_date)
                .bind(&actor)
                .bind(now)
                .execute(&mut *c)
                .await?;
                let tracking_mode =
                    TrackingMode::parse(&order.output_tracking_mode).ok_or_else(|| {
                        invalid(
                            "MANUFACTURING_TRACKING_MODE",
                            "Use a supported inventory tracking mode.",
                        )
                    })?;
                let posting = {
                    let mut lots = PgManufacturingLots::new(&mut *c);
                    post_output_lots(
                        &mut lots,
                        LotPosting {
                            key: self.request.key.as_str(),
                            order_id: self.id,
                            order_version: next,
                            warehouse_id: &order.warehouse_id,
                            product_id: &order.output_product_id,
                            entry_date: &self.entry_date,
                            company_id: self.company.as_str(),
                            user_id: &actor,
                            now,
                        },
                        OutputLotRequest {
                            tracking_mode,
                            output_milli: output,
                            total_minor: total,
                            allocations: self.input.output_allocations()?,
                        },
                    )
                    .await?
                };
                sqlx::query(
                    "INSERT INTO manufacturing_cost_snapshots(id,order_id,source_order_version,\
                     material_minor,labor_minor,overhead_minor,total_minor,\
                     output_quantity_milli,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                )
                .bind(format!("mfg-cost-{}", self.request.key))
                .bind(self.id)
                .bind(next)
                .bind(order.material_cost_minor)
                .bind(order.labor_cost_minor)
                .bind(order.overhead_cost_minor)
                .bind(total)
                .bind(output)
                .bind(now)
                .execute(&mut *c)
                .await?;
                if posting.as_ref().is_some_and(|posting| posting.lots == 0) {
                    return Err(conflict(
                        "MANUFACTURING_ORDER_CONTEXT_CHANGED",
                        "Output lots changed. Reload and retry.",
                    )
                    .into());
                }
                let factor = if order.batch_quantity_milli > 0 {
                    order.planned_quantity_milli as f64 / order.batch_quantity_milli as f64
                } else {
                    0.0
                };
                for (index, line) in self.lines.iter().enumerate() {
                    let input_quantity =
                        ((line.quantity_milli as f64 * factor).ceil() as i64).max(0);
                    sqlx::query(
                        "INSERT INTO manufacturing_genealogy(id,order_id,input_product_id,\
                         output_product_id,input_quantity_milli,output_quantity_milli,\
                         source_order_version,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
                    )
                    .bind(format!("mfg-genealogy-{}-{index}", self.request.key))
                    .bind(self.id)
                    .bind(&line.product_id)
                    .bind(&order.output_product_id)
                    .bind(input_quantity)
                    .bind(output)
                    .bind(next)
                    .bind(now)
                    .execute(&mut *c)
                    .await?;
                }
            }
        }
        let (phase, amount, quantity) = match self.action {
            ManufacturingOrderAction::Issue => {
                let material = self
                    .issue_plan
                    .iter()
                    .map(|line| line.value_minor)
                    .sum::<i64>();
                ("issue", material, 0_i64)
            }
            ManufacturingOrderAction::PassQc => (
                "output",
                order
                    .material_cost_minor
                    .saturating_add(order.labor_cost_minor)
                    .saturating_add(order.overhead_cost_minor),
                order.reported_output_milli.unwrap_or(0),
            ),
        };
        let journal_id = self
            .journal
            .as_ref()
            .map(|plan| plan.journal_id().to_owned());
        if amount > 0 {
            let profile = self.profile.as_ref().ok_or_else(|| {
                conflict(
                    "MANUFACTURING_ACCOUNTING_REQUIRED",
                    "Finance must approve the production account map.",
                )
            })?;
            let journal_id = journal_id.as_deref().ok_or_else(|| {
                conflict(
                    "MANUFACTURING_FINANCE_BINDING_REQUIRED",
                    "The production Finance posting is unavailable.",
                )
            })?;
            sqlx::query(
                "INSERT INTO manufacturing_order_postings(id,company_id,order_id,\
                 source_order_version,phase,profile_id,profile_version,journal_id,currency,\
                 amount_minor,account_snapshot_json,created_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
            )
            .bind(format!("mfg-posting-{}", self.request.key))
            .bind(self.company.as_str())
            .bind(self.id)
            .bind(next)
            .bind(phase)
            .bind(&profile.id)
            .bind(profile.version)
            .bind(journal_id)
            .bind(&profile.currency)
            .bind(amount)
            .bind(&profile.account_snapshot)
            .bind(&actor)
            .bind(now)
            .execute(&mut *c)
            .await
            .map_err(|_| {
                conflict(
                    "MANUFACTURING_FINANCE_BINDING_REQUIRED",
                    "The production posting could not be recorded.",
                )
            })?;
            sqlx::query(
                "INSERT INTO manufacturing_cost_movements(id,company_id,order_id,\
                 movement_kind,quantity_milli,amount_minor,journal_id,source_type,\
                 source_id,occurred_on,version,created_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,'production_event',$8,$9,1,$10,$11)",
            )
            .bind(format!("mfg-movement-{}-0", self.request.key))
            .bind(self.company.as_str())
            .bind(self.id)
            .bind(phase)
            .bind(quantity)
            .bind(amount)
            .bind(journal_id)
            .bind(format!("mfg-event-{}", self.request.key))
            .bind(&self.entry_date)
            .bind(&actor)
            .bind(now)
            .execute(&mut *c)
            .await?;
        }
        if let Some(journal) = self.journal.take() {
            PgJournalStore::new(self.current.holding_id().clone(), self.company.clone(), c)
                .persist(&journal)
                .await
                .map_err(journal_error)?;
        }
        for snapshot in self.money.drain(..) {
            snapshot.persist(&mut *c).await?;
        }
        let transition = sqlx::query(
            "UPDATE manufacturing_orders SET status=$1,version=version+1,updated_at=$2 \
             WHERE id=$3 AND version=$4 AND status=$5",
        )
        .bind(status)
        .bind(now)
        .bind(self.id)
        .bind(self.input.expected_version)
        .bind(&order.status)
        .execute(&mut *c)
        .await?;
        if transition.rows_affected() != 1 {
            return Err(conflict("CONFLICT", "Order changed. Reload and retry.").into());
        }
        if self.action == ManufacturingOrderAction::PassQc {
            sqlx::query(
                "UPDATE planning_resource_allocations SET released_at=$1 WHERE scenario_id=\
                 (SELECT r.scenario_id FROM planning_recommendations r \
                  JOIN manufacturing_orders o ON o.recommendation_id=r.id WHERE o.id=$2) \
                 AND released_at IS NULL",
            )
            .bind(now)
            .bind(self.id)
            .execute(&mut *c)
            .await?;
        }
        sqlx::query(
            "INSERT INTO manufacturing_order_events(id,order_id,version,action,actor_id,\
             reason,status,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(format!("mfg-event-{}", self.request.key))
        .bind(self.id)
        .bind(next)
        .bind(self.action.as_str())
        .bind(&actor)
        .bind(self.input.reason.as_deref().unwrap_or("").trim())
        .bind(status)
        .bind(self.request.input.as_str())
        .bind(now)
        .execute(&mut *c)
        .await?;
        Ok(())
    }

    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        if let Some(evidence) = &self.evidence {
            let mut files = PgBusinessEvidenceFiles::new(
                c,
                self.company.clone(),
                self.current.user_id().as_str().to_owned(),
                self.current
                    .selection()
                    .workspace()
                    .branch_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned()),
                None,
                vec![],
                self.clock.now(),
            );
            files.require_current(evidence).await?;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn prepare_order_journal<C: Clock + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &C,
    request: &WorkflowRequest,
    entry_date: &str,
    period_id: &str,
    profile: &ApprovedProfile,
    phase: &str,
    order_version: i64,
    lines: Vec<erpx_server_finance::public::JournalLineInput>,
) -> Result<erpx_server_finance::public::JournalPreparation, Error> {
    let actor = finance_actor(current);
    let draft = JournalDraft {
        chart_id: profile.chart_id.clone(),
        period_id: period_id.to_owned(),
        entry_date: entry_date.to_owned(),
        currency: profile.currency.clone(),
        fx_rate: None,
        is_opening: false,
        memo: format!(
            "Production {phase} · {} v{order_version}",
            request.record_id
        ),
        source_type: format!("manufacturing_{phase}"),
        source_id: Some(request.record_id.clone()),
        lines,
    };
    let mut bound = BoundJournal {
        actor: actor.clone(),
        draft: draft.clone(),
    };
    let rules = embedded_currency_rules().map_err(|_| {
        conflict(
            "MANUFACTURING_FINANCE_BINDING_REQUIRED",
            "The production Finance posting is unavailable.",
        )
    })?;
    let collation = FinanceCollation::english().map_err(|_| {
        conflict(
            "MANUFACTURING_FINANCE_BINDING_REQUIRED",
            "The production Finance posting is unavailable.",
        )
    })?;
    let mut ports = PgJournalPreparation::new(c, &actor, &mut bound, clock, &rules, &collation)
        .map_err(journal_error)?;
    prepare_journal(
        &mut ports,
        &actor,
        &draft,
        &CommandKey::new(request.key.clone())?,
        JournalStatus::Posted,
    )
    .await
    .map_err(journal_error)
}

#[cfg(test)]
mod tests {
    use super::{utc_date, valid_date};

    #[test]
    fn utc_date_derives_the_retained_iso_day() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(1_700_000_000_000), "2023-11-14");
    }

    #[test]
    fn entry_date_validation_rejects_malformed_dates() {
        assert!(valid_date("2026-11-13"));
        assert!(!valid_date("2026-13-13"));
        assert!(!valid_date("2026-11-3"));
        assert!(!valid_date("13/11/2026"));
    }
}

struct Ids;

impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}

fn context_value(current: &AuthorizedWorkContext) -> Value {
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
            erpx_server_identity_access::public::work_context::WorkScopeKind::Holding => "holding",
            erpx_server_identity_access::public::work_context::WorkScopeKind::Branch => "branch",
            erpx_server_identity_access::public::work_context::WorkScopeKind::Department => "department",
        },
        "modules": current.modules(),
        "permissions": current.command_iam_metadata().permissions,
        "expiresAt": current.expires_at(),
    })
}

impl<C: Clock + Send + Sync + 'static> ManufacturingOrderCommand<C> {
    #[allow(clippy::too_many_arguments)]
    pub async fn change<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        action: ManufacturingOrderAction,
        input: ManufacturingOrderInput,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<ManufacturingOrderOutcome, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        input.validate(action)?;
        if id.is_empty() || id.len() > 200 {
            return Err(invalid(
                "MANUFACTURING_ORDER_INVALID",
                "Use a current production order.",
            )
            .into());
        }
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let operation = format!("manufacturing.order.{}", action.as_str());
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: operation.clone(),
            record_id: id.clone(),
            input: wire,
        };
        let purpose = CapabilityPurpose::SettleExisting;
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![action.permission().into()]),
                    module: Some("manufacturing".into()),
                    destination: operation,
                    api: true,
                },
                OrderSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let request = request.clone();
                    let input = input.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "MANUFACTURING_SCOPE_REQUIRED",
                                "Choose the current authorized Company workspace.",
                            )
                            .into());
                        }
                        current.require_permission(action.permission())?;
                        let mode = manufacturing_purchase_mode()?;
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock.as_ref(),
                            catalogue: current_package_catalogue()?,
                            purchase: None,
                            mode,
                            capabilities: action.capabilities(),
                            purpose,
                            permission: action.permission(),
                        };
                        let mut effects = OrderEffects {
                            current,
                            company: &company,
                            clock: clock.as_ref(),
                            request: &request,
                            id: &id,
                            action,
                            input: &input,
                            order: None,
                            lines: Vec::new(),
                            issue_plan: Vec::new(),
                            profile: None,
                            entry_date: String::new(),
                            journal: None,
                            money: Vec::new(),
                            evidence: None,
                        };
                        let mut ids = Ids;
                        let outcome: CommandOutcome = PgCommandLedger::new(
                            company.clone(),
                            c,
                            &mut authority,
                            clock.as_ref(),
                            &mut ids,
                        )
                        .execute(&request, &mut effects)
                        .await?;
                        Ok(ManufacturingOrderOutcome {
                            result: outcome.result,
                            context: context_value(current),
                        })
                    })
                },
            )
            .await
    }
}
