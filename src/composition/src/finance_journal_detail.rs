//! Development read assembly. Rendered context is output data from the SAME
//! retained authorization unit, never a deserialized actor or a command grant.
use crate::finance_journal_environment::FinanceJournalEnvironment;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, lock_commercial_authority_sources,
};
use erpx_server_finance::public::{FinanceActor, finance_invoice_read::*, finance_reports::*, finance_setup::*, journal_detail::*, journal_list::*, journal_workspace::*, sales_defaults::*, sales_invoice_create::*, sales_invoice_edit::*, sales_invoice_list::*, sales_receipts::*};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    command_metadata::project_command_iam,
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_people_payroll::public::finance_journal_names::{
    lock_finance_journal_name_sources, read_finance_journal_names,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone)]
pub struct FinanceJournalReadConfiguration {
    holding: HoldingId,
    company: CompanyId,
    purchases: Arc<CommercialCatalogue>,
    field_service_parts: bool,
    field_service_invoices: bool,
}
impl FinanceJournalReadConfiguration {
    pub fn new(
        environment: FinanceJournalEnvironment,
        holding: HoldingId,
        company: CompanyId,
        purchases: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        if matches!(environment, FinanceJournalEnvironment::Production) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "FINANCE_PRODUCTION_COMPOSITION_UNAVAILABLE",
                "Finance release and Company acceptance ports are incomplete.",
            ));
        }
        Ok(Self {
            holding,
            company,
            purchases,
            field_service_parts: false,
            field_service_invoices: false,
        })
    }
    pub fn with_field_service_invoices(mut self) -> Self {
        self.field_service_invoices = true;
        self
    }
    pub fn with_field_service_parts(mut self) -> Self {
        self.field_service_parts = true;
        self
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }
}
impl HoldingWorkSourcePreparation for FinanceJournalReadConfiguration {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_journal_detail_sources(c).await?;
            lock_finance_journal_name_sources(c).await
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceRenderedWorkContext {
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub assignment_id: String,
    pub assignment_version: FinanceReadInteger,
    pub revision: FinanceReadInteger,
    pub role: String,
    pub assignment_scope_kind: String,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
    pub expires_at: FinanceReadInteger,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceJournalReadResponse {
    pub data: FinanceJournalDetail,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceJournalWorkspaceReadResponse {
    pub data: FinanceJournalRecordWorkspace,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceJournalListReadResponse {
    pub data: JournalList,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSalesInvoiceListReadResponse {
    pub data: SalesInvoiceList,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSalesInvoiceCreateResponse {
    pub data: SalesInvoiceCreateResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSalesInvoiceEditResponse {
    pub data: SalesInvoiceEditResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSalesInvoiceVoidResponse {
    pub data: SalesInvoiceVoidResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceInvoicePageResponse {
    pub data: InvoicePage,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceInvoiceAgingResponse {
    pub data: Vec<serde_json::Value>,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceChartAccountsResponse {
    pub data: FinanceChartAccounts,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceChartResponse { pub data: ChartResult }
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceAccountResponse { pub data: AccountResult }
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceAccountImportResponse { pub data: AccountImportResult }
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceAccountStatusResponse { pub data: AccountStatusResult }
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinancePeriodResponse { pub data: PeriodResult }
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceFinancialReportResponse {
    pub data: FinancialReport,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSalesDefaultsWorkspaceResponse {
    pub data: SalesDefaultsWorkspace,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSalesDefaultsResultResponse {
    pub data: SalesDefaultsResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSalesReceiptReadResponse {
    pub data: SalesReceiptRead,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSalesReceiptExportResponse {
    pub data: SalesReceiptExport,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSalesReceiptIssueResponse {
    pub data: SalesReceiptIssueResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceServiceInvoiceWorkspaceResponse {
    pub data:
        erpx_server_finance::public::field_service_invoice_read::FinanceServiceInvoiceWorkspace,
    pub context: FinanceRenderedWorkContext,
    #[serde(rename = "sourceOnly")]
    pub source_only: bool,
    #[serde(rename="purchaseControls")]
    pub purchase_controls:std::collections::BTreeMap<String,crate::finance_presentation::FinancePurchaseAccess>,
    #[serde(rename="sourcePurchase")]
    #[cfg_attr(feature="contracts",ts(type="Record<string, unknown>"),schema(value_type=Object))]
    pub source_purchase:serde_json::Value,
    #[serde(rename="canPost")]
    pub can_post:bool,
    #[cfg_attr(feature="contracts",ts(type="Record<string, unknown>"),schema(value_type=Object))]
    pub flow:serde_json::Value,
    #[serde(rename="noCharge")]
    pub no_charge:Option<erpx_server_finance::public::service_no_charge::ServiceNoChargeControls>,
}
#[derive(Clone)]
pub(crate) struct ServiceInvoiceReadSources;
impl HoldingWorkSourcePreparation for ServiceInvoiceReadSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_finance::public::field_service_invoice_read::lock_service_invoice_sources(
                c,
            )
            .await?;
            lock_commercial_authority_sources(c).await?;
            erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
            erpx_server_field_service::public::finance_lineage::lock_field_service_invoice_finance_sources(c).await?;
            erpx_server_people_payroll::public::journal_flow_people::lock_journal_flow_people_sources(c).await?;
            erpx_server_organization::public::journal_flow_department::lock_journal_flow_department_sources(c).await?;
            erpx_server_workflow::public::service_invoice_flow::lock_service_invoice_flow_sources(c).await
        })
    }
}
fn setup_company_scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMPANY_SCOPE_REQUIRED",
        "Use a Company Finance assignment for Finance setup.",
    )
}

fn command_actor(
    current: &AuthorizedWorkContext,
) -> Result<SalesInvoiceCommandActor, BusinessError> {
    let selection = current.selection();
    let workspace = selection.workspace();
    let assignment = selection.assignment();
    let iam = serde_json::to_value(project_command_iam(current)?)
        .map_err(|_| BusinessError::new(
            FailureKind::Unavailable,
            "COMMAND_IAM_METADATA_UNAVAILABLE",
            "The current command authority metadata is unavailable.",
        ))?;
    let object = iam.as_object().ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "COMMAND_IAM_METADATA_UNAVAILABLE",
            "The current command authority metadata is unavailable.",
        )
    })?;
    let mut iam_json = String::from("{");
    iam_json.push_str("\"personUserIds\":");
    iam_json.push_str(&object.get("person_user_ids").cloned().unwrap_or(serde_json::json!([])).to_string());
    iam_json.push_str(",\"emergencyUntil\":");
    iam_json.push_str(&object.get("emergency_until").cloned().unwrap_or(serde_json::Value::Null).to_string());
    iam_json.push_str(",\"roleRevision\":");
    iam_json.push_str(&object.get("role_revision").cloned().unwrap_or(serde_json::json!(0)).to_string());
    iam_json.push_str(",\"identityVersion\":");
    iam_json.push_str(&object.get("identity_version").cloned().unwrap_or(serde_json::json!(0)).to_string());
    iam_json.push_str(",\"permissions\":");
    iam_json.push_str(&object.get("permissions").cloned().unwrap_or(serde_json::json!([])).to_string());
    if let Some(value) = object.get("hr_appointment_version") {
        iam_json.push_str(",\"hrAppointmentVersion\":");
        iam_json.push_str(&value.to_string());
    }
    iam_json.push_str(",\"grantablePermissions\":");
    iam_json.push_str(&object.get("grantable_permissions").cloned().unwrap_or(serde_json::json!([])).to_string());
    iam_json.push_str(",\"principalId\":");
    iam_json.push_str(&object.get("principal_id").cloned().unwrap_or(serde_json::json!("")).to_string());
    iam_json.push_str(",\"personId\":");
    iam_json.push_str(&object.get("person_id").cloned().unwrap_or(serde_json::Value::Null).to_string());
    iam_json.push_str(",\"freshUntil\":");
    iam_json.push_str(&object.get("fresh_until").cloned().unwrap_or(serde_json::Value::Null).to_string());
    iam_json.push('}');
    Ok(SalesInvoiceCommandActor {
        user_id: current.user_id().as_str().into(),
        assignment_id: assignment.id.as_str().into(),
        role: assignment.role.clone(),
        session_id: current.session_id().as_str().into(),
        revision: selection.revision(),
        assignment_version: assignment.version,
        company_id: workspace.company_id.as_str().into(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().into()),
        department_id: workspace.department_id.as_ref().map(|value| value.as_str().into()),
        scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        scope_node_id: assignment
            .scope_node_id
            .as_ref()
            .map(|value| value.as_str().into())
            .unwrap_or_default(),
        iam_json,
    })
}

#[derive(Clone)]
struct FinanceReportSources;
impl HoldingWorkSourcePreparation for FinanceReportSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_finance_report_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct FinanceSetupSources;
impl HoldingWorkSourcePreparation for FinanceSetupSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_finance_setup_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct FinanceInvoiceReadSources;
impl HoldingWorkSourcePreparation for FinanceInvoiceReadSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_finance_invoice_read_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct SalesInvoiceEditSources;
impl HoldingWorkSourcePreparation for SalesInvoiceEditSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_sales_invoice_edit_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct SalesReceiptSources;
impl HoldingWorkSourcePreparation for SalesReceiptSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_sales_receipt_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct SalesDefaultsSources;
impl HoldingWorkSourcePreparation for SalesDefaultsSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_sales_defaults_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct SalesInvoiceCreateSources;
impl HoldingWorkSourcePreparation for SalesInvoiceCreateSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_sales_invoice_create_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct SalesInvoiceListSources;
impl HoldingWorkSourcePreparation for SalesInvoiceListSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_sales_invoice_list_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct JournalListPreparation;
impl HoldingWorkSourcePreparation for JournalListPreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_journal_list_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone, Copy)]
enum ReadKind {
    Detail,
    Workspace,
}
#[derive(Clone)]
struct ReadPreparation {
    kind: ReadKind,
    field_service_parts: bool,
    field_service_invoices: bool,
}
impl HoldingWorkSourcePreparation for ReadPreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            // Take Finance's retained writer/source gate before shared table
            // reads so concurrent Field invoice posting cannot meet an upgrade.
            if self.field_service_parts || self.field_service_invoices {
                erpx_server_finance::public::journal_review::lock_journal_review_sources(c).await?;
            }
            if self.field_service_invoices {
                crate::finance_service_invoice_history::lock_service_invoice_history_sources(c).await?;
            }
            match self.kind {
                ReadKind::Detail => lock_journal_detail_sources(c).await?,
                ReadKind::Workspace => lock_journal_workspace_sources(c).await?,
            }
            if self.field_service_parts {
                erpx_server_inventory::public::field_service_consumption_finance::lock_consumption_finance_sources(c).await?;
                erpx_server_field_service::public::finance_lineage::lock_field_service_parts_finance_sources(c).await?;
            }
            lock_finance_journal_name_sources(c).await
        })
    }
}
enum ReadSource {
    Detail(Box<CurrentJournalDetail>),
    Workspace(Box<CurrentJournalRecordWorkspace>),
    FieldDetail(
        Box<CurrentFieldServiceJournalDetail>,
        Box<crate::finance_field_service_lineage::FieldServicePartsSource>,
    ),
    FieldWorkspace(
        Box<CurrentFieldServiceJournalWorkspace>,
        Box<crate::finance_field_service_lineage::FieldServicePartsSource>,
    ),
    InvoiceDetail(Box<CurrentFieldServiceJournalDetail>,Box<crate::finance_service_invoice_history::CurrentServiceInvoiceHistory>),
    InvoiceWorkspace(Box<CurrentFieldServiceJournalWorkspace>,Box<crate::finance_service_invoice_history::CurrentServiceInvoiceHistory>),
}
impl ReadSource {
    fn journal(&self) -> &FinanceLedgerRow {
        match self {
            Self::Detail(s) => &s.detail().journal,
            Self::Workspace(s) => &s.data().journal,
            Self::FieldDetail(s, _) | Self::InvoiceDetail(s, _) => &s.detail().journal,
            Self::FieldWorkspace(s, _) | Self::InvoiceWorkspace(s, _) => &s.data().journal,
        }
    }
    async fn require_current<C:Clock>(&self, c: &mut PgConnection, ctx:&crate::field_service_source_record::SourceRecordContext<'_,C>) -> Result<(), Error> {
        match self {
            Self::Detail(s) => s.require_current(c).await,
            Self::Workspace(s) => s.require_current(c).await,
            Self::FieldDetail(s, lineage) => {
                s.require_current(c).await?;
                lineage.require_current(c).await
            }
            Self::FieldWorkspace(s, lineage) => {
                s.require_current(c).await?;
                lineage.require_current(c).await
            }
            Self::InvoiceDetail(s,history)=>{s.require_current(c).await?;history.require_current(c,ctx).await}
            Self::InvoiceWorkspace(s,history)=>{s.require_current(c).await?;history.require_current(c,ctx).await}
        }
    }
    fn response(
        &self,
        context: FinanceRenderedWorkContext,
        creator: &str,
        poster: &str,
    ) -> ReadResponse {
        let names = |row: &mut FinanceLedgerRow| {
            row.insert(
                "created_by_name".into(),
                FinanceLedgerValue::Text(creator.into()),
            );
            row.insert(
                "posted_by_name".into(),
                FinanceLedgerValue::Text(poster.into()),
            );
        };
        match self {
            Self::Detail(s) => {
                let mut data = s.detail().clone();
                names(&mut data.journal);
                ReadResponse::Detail(FinanceJournalReadResponse { data, context })
            }
            Self::FieldDetail(s, _) | Self::InvoiceDetail(s, _) => {
                let mut data = s.detail().clone();
                names(&mut data.journal);
                ReadResponse::Detail(FinanceJournalReadResponse { data, context })
            }
            Self::FieldWorkspace(s, _) | Self::InvoiceWorkspace(s, _) => {
                let mut data = s.data().clone();
                names(&mut data.journal);
                ReadResponse::Workspace(FinanceJournalWorkspaceReadResponse { data, context })
            }
            Self::Workspace(s) => {
                let mut data = s.data().clone();
                names(&mut data.journal);
                ReadResponse::Workspace(FinanceJournalWorkspaceReadResponse { data, context })
            }
        }
    }
}
#[derive(Serialize)]
#[serde(untagged)]
enum ReadResponse {
    Detail(FinanceJournalReadResponse),
    Workspace(FinanceJournalWorkspaceReadResponse),
}
pub(crate) fn rendered(current: &AuthorizedWorkContext) -> Result<FinanceRenderedWorkContext, BusinessError> {
    let selection = current.selection();
    let workspace = selection.workspace();
    let grant = selection.assignment();
    Ok(FinanceRenderedWorkContext {
        user_id: current.user_id().as_str().into(),
        holding_id: current.holding_id().as_str().into(),
        company_id: workspace.company_id.as_str().into(),
        branch_id: workspace.branch_id.as_ref().map(|v| v.as_str().into()),
        department_id: workspace.department_id.as_ref().map(|v| v.as_str().into()),
        branch_slug: workspace.branch_slug.clone(),
        department_slug: workspace.department_slug.clone(),
        assignment_id: grant.id.as_str().into(),
        assignment_version: FinanceReadInteger::new(grant.version)?,
        revision: FinanceReadInteger::new(selection.revision())?,
        role: grant.role.clone(),
        assignment_scope_kind: match grant.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: FinanceReadInteger::new(current.expires_at())?,
    })
}
struct ReplyBudget(usize);
impl std::io::Write for ReplyBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= 2 * 1024 * 1024)
            .ok_or_else(|| std::io::Error::other("bounded Finance reply"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn admit_finance_transport_response(reply:&impl Serialize)->Result<(),BusinessError>{
    serde_json::to_writer(ReplyBudget(0),reply).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FINANCE_RESPONSE_LIMIT","The Finance response exceeds this bounded read."))
}
fn admit_reply(reply: &ReadResponse) -> Result<(), BusinessError> {
    serde_json::to_writer(ReplyBudget(0), reply).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "JOURNAL_DETAIL_RESPONSE_LIMIT",
            "The journal exceeds this bounded read.",
        )
    })
}
fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "JOURNAL_NOT_FOUND",
        "Journal not found.",
    )
}
pub struct FinanceJournalDetailService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FinanceJournalReadConfiguration,
    clock: Arc<C>,
}
#[cfg(feature = "finance-field-write")]
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
#[cfg_attr(feature="contracts",derive(ts_rs::TS,utoipa::ToSchema))]
pub struct FinanceFieldWorkResponse {
    #[cfg_attr(feature="contracts",ts(type="Array<Record<string, unknown>>"),schema(value_type=Vec<Object>))]
    pub data: Vec<serde_json::Value>,
    pub context: FinanceRenderedWorkContext,
}
#[cfg(feature = "finance-field-write")]
#[derive(Clone)]
struct FieldFinanceWorkSources;
#[cfg(feature = "finance-field-write")]
impl HoldingWorkSourcePreparation for FieldFinanceWorkSources {
    fn prepare<'c>(&'c self,c:&'c mut PgConnection)->PgWorkFuture<'c,(),BusinessError>{Box::pin(async move{
        ServiceInvoiceReadSources.prepare(c).await?;
        ReadPreparation{kind:ReadKind::Workspace,field_service_parts:true,field_service_invoices:true}.prepare(c).await?;
        erpx_server_workflow::public::field_journal_flow::lock_field_journal_flow_sources(c).await
    })}
}
impl<C: Clock + Send + Sync + 'static> FinanceJournalDetailService<C> {
    #[cfg(feature = "finance-field-write")]
    pub async fn read_field_work<Input>(&self,host:&str,proof:DeviceProof,input:Input)->Result<FinanceFieldWorkResponse,Error>
    where Input:FnOnce()->Result<HoldingWorkspaceRequest,BusinessError>+Send+'static {
        if !self.configuration.field_service_parts||!self.configuration.field_service_invoices{return Err(scope().into());}
        let config=self.configuration.clone();let clock=self.clock.clone();let company=config.company.clone();
        self.authorizer.execute_with_sources_deferred(host,proof,move||{let mut s=input()?;if s.company_id!=company{return Err(scope());}s.mutation=false;Ok(s)},WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/work".into(),api:true},FieldFinanceWorkSources,move|c,current|{
            let(config,clock)=(config.clone(),clock.clone());Box::pin(async move{
                if current.holding_id()!=&config.holding||current.selection().workspace().company_id!=config.company{return Err(scope().into());}
                let source_config=crate::finance_journal_create::FinanceJournalCreateConfiguration{holding:config.holding,company:config.company,catalogue:config.purchases};
                let data=crate::finance_field_work::read(c,current,&source_config,clock.as_ref()).await?;
                erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current(c,current,clock.now().get()).await?;
                let reply=FinanceFieldWorkResponse{data,context:rendered(current)?};admit_finance_transport_response(&reply)?;Ok(reply)
            })
        }).await
    }
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: FinanceJournalReadConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }
    pub fn company_id(&self) -> &CompanyId {
        &self.configuration.company
    }
    pub async fn read_service_invoice<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
        payment_evidence: bool,
    ) -> Result<FinanceServiceInvoiceWorkspaceResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if !self.configuration.field_service_invoices {
            return Err(scope().into());
        }
        let company = self.configuration.company.clone();
        let input = move || {
            let mut v = input()?;
            if v.company_id != company {
                return Err(scope());
            }
            v.mutation = false;
            Ok(v)
        };
        let config = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(host,proof,input,WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/invoices".into(),api:true},ServiceInvoiceReadSources,move |c,current|{
            let config=config.clone();let clock=clock.clone();let id=id.clone();Box::pin(async move {
                if current.holding_id()!=&config.holding || current.selection().workspace().company_id!=config.company{return Err(scope().into())}
                current.require_permission("finance.read")?;current.require_permission("module.finance.read")?;
                let source_context=crate::field_service_source_record::SourceRecordContext{actor:current,holding:&config.holding,company:&config.company,catalogue:&config.purchases,clock:clock.as_ref()};
                let source=crate::finance_service_invoice_history::read_service_invoice_history(c,&source_context,&id).await?;
                if payment_evidence {
                    source.require_current(c,&source_context).await?;
                    source.require_payment_evidence()?;
                    return Err(BusinessError::new(FailureKind::Unavailable,"NATIVE_SUPPLIER_BILL_EVIDENCE_UNAVAILABLE","Supplier bill evidence is outside the Field invoice scope.").into());
                }
                let source_only=source.source_only;
                let presentation=crate::finance_presentation::read_finance_presentation(c,&config.holding,&config.company,&config.purchases,clock.as_ref()).await?;
                let mut data=source.data().clone();
                // Retained Sales receipt-list scope: Field receipts are Company
                // history; a narrowed Sales branch does not disclose this list.
                if current.selection().workspace().branch_id.is_some()||current.selection().workspace().department_id.is_some(){data.customer_receipts.clear();}
                let flow_record=serde_json::to_value(&data.invoice).map_err(|_|scope())?;
                let flow=crate::finance_service_invoice_flow::read_service_invoice_flow(c,current,&flow_record,presentation.can_post_invoice(),clock.as_ref()).await?;
                let no_charge=crate::finance_service_no_charge_read::controls(c,current,&id,presentation.service_purchase()["settleExisting"]==true,clock.as_ref()).await?;
                let reply=FinanceServiceInvoiceWorkspaceResponse{no_charge,data,context:rendered(current)?,source_only,purchase_controls:presentation.values().clone(),source_purchase:presentation.service_purchase(),can_post:presentation.can_post_invoice(),flow:flow.view.clone()};
                serde_json::to_writer(ReplyBudget(0),&reply).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FINANCE_INVOICE_READ_LIMIT","The invoice workspace exceeds this bounded read."))?;
                source.require_current(c,&source_context).await?;
                flow.require_current(c,current,&flow_record,presentation.can_post_invoice(),clock.as_ref()).await?;
                presentation.require_current(c,&config.holding,&config.company,&config.purchases,clock.as_ref()).await?;
                if clock.now().get()>=current.expires_at(){return Err(scope().into())}Ok(reply)
            })
        }).await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn list<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        chart_id: Option<String>,
        status: String,
        cursor: Option<String>,
    ) -> Result<FinanceJournalListReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/journals".into(),
                    api: true,
                },
                JournalListPreparation,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let chart_id = chart_id.clone();
                    let status = status.clone();
                    let cursor = cursor.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        PgCompanyPurchaseReader::new(
                            configuration.holding.clone(),
                            configuration.company.clone(),
                            c,
                            &configuration.purchases,
                        )
                        .inspect(
                            &PurchaseRequest {
                                capabilities: vec!["finance.accounting".into()],
                                purpose: CapabilityPurpose::History,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            clock.now(),
                        )
                        .await?
                        .require_allowed()?;
                        lock_journal_list_sources(c).await?;
                        let data = read_journal_list(
                            c,
                            configuration.company.as_str(),
                            chart_id.as_deref(),
                            &status,
                            cursor.as_deref(),
                        )
                        .await?;
                        Ok(FinanceJournalListReadResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    async fn require_sales_purchase(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        configuration: &FinanceJournalReadConfiguration,
        purpose: CapabilityPurpose,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        let mut allowed = false;
        let mut denied = None;
        for capability in ["finance.accounting", "finance.sales-posting"] {
            let mut reader = PgCompanyPurchaseReader::new(
                current.holding_id().clone(),
                configuration.company.clone(),
                c,
                &configuration.purchases,
            );
            let inspection = reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: vec![capability.into()],
                        purpose,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    now,
                )
                .await?;
            match inspection.require_allowed() {
                Ok(_) => {
                    allowed = true;
                    break;
                }
                Err(error) => {
                    denied.get_or_insert(error);
                }
            }
        }
        if !allowed {
            return Err(denied
                .unwrap_or_else(|| {
                    BusinessError::new(
                        FailureKind::Forbidden,
                        "PACKAGE_REQUIRED",
                        "The Company purchase does not permit this capability purpose.",
                    )
                })
                .into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn finance_report<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        chart_id: Option<String>,
        period_id: Option<String>,
        start_date: Option<String>,
        end_date: Option<String>,
    ) -> Result<FinanceFinancialReportResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company { return Err(scope()); }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(
            host, proof, input,
            WorkAuthorizationRequest { permission: "finance.read".into(), module: Some("finance".into()), destination: "finance/reports".into(), api: true },
            FinanceReportSources,
            move |c, current| {
                let configuration = configuration.clone();
                let clock = clock.clone();
                let chart_id = chart_id.clone();
                let period_id = period_id.clone();
                let start_date = start_date.clone();
                let end_date = end_date.clone();
                Box::pin(async move {
                    if current.holding_id() != &configuration.holding || current.selection().workspace().company_id != configuration.company { return Err(scope().into()); }
                    current.require_permission("finance.read")?;
                    current.require_permission("module.finance.read")?;
                    let workspace = current.selection().workspace();
                    if workspace.branch_id.is_some() || workspace.department_id.is_some() { return Err(setup_company_scope().into()); }
                    Self::require_sales_purchase(c, current, &configuration, CapabilityPurpose::History, clock.now()).await?;
                    let data = read_financial_report(c, configuration.company.as_str(), chart_id.as_deref(), period_id.as_deref(), start_date.as_deref(), end_date.as_deref()).await?;
                    Ok(FinanceFinancialReportResponse { data, context: rendered(current)? })
                })
            },
        ).await
    }

    pub async fn finance_chart_accounts<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        chart_id: String,
    ) -> Result<FinanceChartAccountsResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company { return Err(scope()); }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(
            host, proof, input,
            WorkAuthorizationRequest { permission: "finance.read".into(), module: Some("finance".into()), destination: "finance/setup".into(), api: true },
            FinanceSetupSources,
            move |c, current| {
                let configuration = configuration.clone();
                let clock = clock.clone();
                let chart_id = chart_id.clone();
                Box::pin(async move {
                    if current.holding_id() != &configuration.holding || current.selection().workspace().company_id != configuration.company { return Err(scope().into()); }
                    current.require_permission("finance.read")?;
                    current.require_permission("module.finance.read")?;
                    let workspace = current.selection().workspace();
                    if workspace.branch_id.is_some() || workspace.department_id.is_some() { return Err(setup_company_scope().into()); }
                    Self::require_sales_purchase(c, current, &configuration, CapabilityPurpose::History, clock.now()).await?;
                    let data = read_chart_accounts(c, configuration.company.as_str(), &chart_id).await?;
                    Ok(FinanceChartAccountsResponse { data, context: rendered(current)? })
                })
            },
        ).await
    }

    pub async fn create_chart<Input>(
        &self, host: &str, proof: DeviceProof, input: Input, command: CommandKey, body: CreateChartInput,
    ) -> Result<FinanceChartResponse, Error>
    where Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || { let mut request = input()?; if request.company_id != company { return Err(scope()); } request.mutation = true; Ok(request) };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(
            host, proof, input,
            WorkAuthorizationRequest { permission: "finance.write".into(), module: Some("finance".into()), destination: "finance/setup".into(), api: true },
            FinanceSetupSources,
            move |c, current| {
                let configuration = configuration.clone();
                let clock = clock.clone();
                let command = command.clone();
                let body = body.clone();
                Box::pin(async move {
                    if current.holding_id() != &configuration.holding || current.selection().workspace().company_id != configuration.company { return Err(scope().into()); }
                    current.require_permission("finance.write")?;
                    current.require_permission("module.finance.read")?;
                    let workspace = current.selection().workspace();
                    if workspace.branch_id.is_some() || workspace.department_id.is_some() { return Err(setup_company_scope().into()); }
                    let now = clock.now();
                    Self::require_sales_purchase(c, current, &configuration, CapabilityPurpose::NewWork, now).await?;
                    let actor = command_actor(current)?;
                    let data = create_chart(c, &actor, &command, &body, now.get()).await?;
                    Ok(FinanceChartResponse { data })
                })
            },
        ).await
    }

    pub async fn create_account<Input>(
        &self, host: &str, proof: DeviceProof, input: Input, command: CommandKey, chart_id: String, mut body: CreateAccountInput,
    ) -> Result<FinanceAccountResponse, Error>
    where Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        body.chart_id = chart_id;
        let company = self.configuration.company.clone();
        let input = move || { let mut request = input()?; if request.company_id != company { return Err(scope()); } request.mutation = true; Ok(request) };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(
            host, proof, input,
            WorkAuthorizationRequest { permission: "finance.write".into(), module: Some("finance".into()), destination: "finance/setup".into(), api: true },
            FinanceSetupSources,
            move |c, current| {
                let configuration = configuration.clone();
                let clock = clock.clone();
                let command = command.clone();
                let body = body.clone();
                Box::pin(async move {
                    if current.holding_id() != &configuration.holding || current.selection().workspace().company_id != configuration.company { return Err(scope().into()); }
                    current.require_permission("finance.write")?;
                    current.require_permission("module.finance.read")?;
                    let workspace = current.selection().workspace();
                    if workspace.branch_id.is_some() || workspace.department_id.is_some() { return Err(setup_company_scope().into()); }
                    let now = clock.now();
                    Self::require_sales_purchase(c, current, &configuration, CapabilityPurpose::NewWork, now).await?;
                    let actor = command_actor(current)?;
                    let data = create_account(c, &actor, &command, &body, now.get()).await?;
                    Ok(FinanceAccountResponse { data })
                })
            },
        ).await
    }

    pub async fn import_accounts<Input>(
        &self, host: &str, proof: DeviceProof, input: Input, command: CommandKey, body: ImportAccountsInput,
    ) -> Result<FinanceAccountImportResponse, Error>
    where Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || { let mut request = input()?; if request.company_id != company { return Err(scope()); } request.mutation = true; Ok(request) };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(
            host, proof, input,
            WorkAuthorizationRequest { permission: "finance.write".into(), module: Some("finance".into()), destination: "finance/setup".into(), api: true },
            FinanceSetupSources,
            move |c, current| {
                let configuration = configuration.clone();
                let clock = clock.clone();
                let command = command.clone();
                let body = body.clone();
                Box::pin(async move {
                    if current.holding_id() != &configuration.holding || current.selection().workspace().company_id != configuration.company { return Err(scope().into()); }
                    current.require_permission("finance.write")?;
                    current.require_permission("module.finance.read")?;
                    let workspace = current.selection().workspace();
                    if workspace.branch_id.is_some() || workspace.department_id.is_some() { return Err(setup_company_scope().into()); }
                    let now = clock.now();
                    Self::require_sales_purchase(c, current, &configuration, CapabilityPurpose::NewWork, now).await?;
                    let actor = command_actor(current)?;
                    let data = import_accounts(c, &actor, &command, &body, now.get()).await?;
                    Ok(FinanceAccountImportResponse { data })
                })
            },
        ).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn change_account_status<Input>(
        &self, host: &str, proof: DeviceProof, input: Input, command: CommandKey, chart_id: String, account_id: String, body: ChangeAccountStatusInput,
    ) -> Result<FinanceAccountStatusResponse, Error>
    where Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let purpose = if body.status == "active" { CapabilityPurpose::NewWork } else { CapabilityPurpose::History };
        let company = self.configuration.company.clone();
        let input = move || { let mut request = input()?; if request.company_id != company { return Err(scope()); } request.mutation = true; Ok(request) };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(
            host, proof, input,
            WorkAuthorizationRequest { permission: "finance.write".into(), module: Some("finance".into()), destination: "finance/setup".into(), api: true },
            FinanceSetupSources,
            move |c, current| {
                let configuration = configuration.clone();
                let clock = clock.clone();
                let command = command.clone();
                let chart_id = chart_id.clone();
                let account_id = account_id.clone();
                let body = body.clone();
                Box::pin(async move {
                    if current.holding_id() != &configuration.holding || current.selection().workspace().company_id != configuration.company { return Err(scope().into()); }
                    current.require_permission("finance.write")?;
                    current.require_permission("module.finance.read")?;
                    let workspace = current.selection().workspace();
                    if workspace.branch_id.is_some() || workspace.department_id.is_some() { return Err(setup_company_scope().into()); }
                    let now = clock.now();
                    Self::require_sales_purchase(c, current, &configuration, purpose, now).await?;
                    let actor = command_actor(current)?;
                    let data = change_account_status(c, &actor, &command, &chart_id, &account_id, &body, now.get()).await?;
                    Ok(FinanceAccountStatusResponse { data })
                })
            },
        ).await
    }

    pub async fn create_period<Input>(
        &self, host: &str, proof: DeviceProof, input: Input, command: CommandKey, body: CreatePeriodInput,
    ) -> Result<FinancePeriodResponse, Error>
    where Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || { let mut request = input()?; if request.company_id != company { return Err(scope()); } request.mutation = true; Ok(request) };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer.execute_with_sources_deferred(
            host, proof, input,
            WorkAuthorizationRequest { permission: "finance.write".into(), module: Some("finance".into()), destination: "finance/setup".into(), api: true },
            FinanceSetupSources,
            move |c, current| {
                let configuration = configuration.clone();
                let clock = clock.clone();
                let command = command.clone();
                let body = body.clone();
                Box::pin(async move {
                    if current.holding_id() != &configuration.holding || current.selection().workspace().company_id != configuration.company { return Err(scope().into()); }
                    current.require_permission("finance.write")?;
                    current.require_permission("module.finance.read")?;
                    let workspace = current.selection().workspace();
                    if workspace.branch_id.is_some() || workspace.department_id.is_some() { return Err(setup_company_scope().into()); }
                    let now = clock.now();
                    Self::require_sales_purchase(c, current, &configuration, CapabilityPurpose::NewWork, now).await?;
                    let actor = command_actor(current)?;
                    let data = create_period(c, &actor, &command, &body, now.get()).await?;
                    Ok(FinancePeriodResponse { data })
                })
            },
        ).await
    }

    pub async fn finance_invoices_page<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: InvoicePageQuery,
    ) -> Result<FinanceInvoicePageResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/invoices".into(),
                    api: true,
                },
                FinanceInvoiceReadSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for the Company invoice view.",
                            )
                            .into());
                        }
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            clock.now(),
                        )
                        .await?;
                        let data = read_invoice_page(c, configuration.company.as_str(), &query).await?;
                        Ok(FinanceInvoicePageResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn finance_invoice_aging<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        kind: String,
        as_of: String,
        chart_id: Option<String>,
    ) -> Result<FinanceInvoiceAgingResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/invoices".into(),
                    api: true,
                },
                FinanceInvoiceReadSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    let as_of = as_of.clone();
                    let chart_id = chart_id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for the Company invoice view.",
                            )
                            .into());
                        }
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            clock.now(),
                        )
                        .await?;
                        let data = read_invoice_aging(
                            c,
                            configuration.company.as_str(),
                            &kind,
                            &as_of,
                            chart_id.as_deref(),
                        )
                        .await?;
                        Ok(FinanceInvoiceAgingResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    pub async fn edit_sales_invoice<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
        body: SalesInvoiceEditInput,
    ) -> Result<Option<FinanceSalesInvoiceEditResponse>, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.write".into(),
                    module: Some("finance".into()),
                    destination: "finance/sales".into(),
                    api: true,
                },
                SalesInvoiceEditSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let id = id.clone();
                    let body = body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.write")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for sales configuration.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::NewWork,
                            now,
                        )
                        .await?;
                        let actor = command_actor(current)?;
                        let data =
                            edit_sales_invoice(c, &actor, &command, &id, &body, now.get()).await?;
                        Ok(data.map(|data| FinanceSalesInvoiceEditResponse { data }))
                    })
                },
            )
            .await
    }

    pub async fn void_sales_invoice<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
        body: SalesInvoiceVoidInput,
    ) -> Result<Option<FinanceSalesInvoiceVoidResponse>, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.write".into(),
                    module: Some("finance".into()),
                    destination: "finance/sales".into(),
                    api: true,
                },
                SalesInvoiceEditSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let id = id.clone();
                    let body = body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.write")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for sales configuration.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            now,
                        )
                        .await?;
                        let actor = command_actor(current)?;
                        let data =
                            void_sales_invoice(c, &actor, &command, &id, &body, now.get()).await?;
                        Ok(data.map(|data| FinanceSalesInvoiceVoidResponse { data }))
                    })
                },
            )
            .await
    }

    pub async fn create_sales_invoice<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        body: SalesInvoiceCreateInput,
    ) -> Result<FinanceSalesInvoiceCreateResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.write".into(),
                    module: Some("finance".into()),
                    destination: "finance/sales".into(),
                    api: true,
                },
                SalesInvoiceCreateSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let body = body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.write")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for sales configuration.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::NewWork,
                            now,
                        )
                        .await?;
                        let actor = command_actor(current)?;
                        let data = create_sales_invoice(c, &actor, &command, &body, now.get()).await?;
                        Ok(FinanceSalesInvoiceCreateResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn sales_defaults_workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<FinanceSalesDefaultsWorkspaceResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/sales/settings".into(),
                    api: true,
                },
                SalesDefaultsSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for sales configuration.",
                            )
                            .into());
                        }
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            clock.now(),
                        )
                        .await?;
                        let data = read_sales_defaults(c, configuration.company.as_str()).await?;
                        Ok(FinanceSalesDefaultsWorkspaceResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    pub async fn prepare_sales_defaults<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        body: SalesDefaultsPrepareInput,
    ) -> Result<FinanceSalesDefaultsResultResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.write".into(),
                    module: Some("finance".into()),
                    destination: "finance/sales/settings".into(),
                    api: true,
                },
                SalesDefaultsSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let body = body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.write")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for sales configuration.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::NewWork,
                            now,
                        )
                        .await?;
                        let actor = command_actor(current)?;
                        let data = prepare_sales_defaults(c, &actor, &command, &body, now.get()).await?;
                        Ok(FinanceSalesDefaultsResultResponse { data })
                    })
                },
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn decide_sales_defaults<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
        body: SalesDefaultsDecisionInput,
    ) -> Result<FinanceSalesDefaultsResultResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.approve".into(),
                    module: Some("finance".into()),
                    destination: "finance/sales/settings".into(),
                    api: true,
                },
                SalesDefaultsSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let id = id.clone();
                    let body = body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.approve")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for sales configuration.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::NewWork,
                            now,
                        )
                        .await?;
                        let actor = command_actor(current)?;
                        let data =
                            decide_sales_defaults(c, &actor, &command, &id, &body, now.get()).await?;
                        Ok(FinanceSalesDefaultsResultResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn sales_receipt<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<Option<FinanceSalesReceiptReadResponse>, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/sales/receipts".into(),
                    api: true,
                },
                SalesReceiptSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        let now = clock.now();
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            now,
                        )
                        .await?;
                        let workspace = current.selection().workspace();
                        let data = read_customer_receipt(
                            c,
                            configuration.company.as_str(),
                            workspace.branch_id.as_ref().map(|value| value.as_str()),
                            workspace.department_id.as_ref().map(|value| value.as_str()),
                            &id,
                        )
                        .await?;
                        let context = rendered(current)?;
                        Ok(data.map(|data| FinanceSalesReceiptReadResponse { data, context }))
                    })
                },
            )
            .await
    }

    pub async fn export_sales_receipt<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<Option<FinanceSalesReceiptExportResponse>, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/sales/receipts".into(),
                    api: true,
                },
                SalesReceiptSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            clock.now(),
                        )
                        .await?;
                        let workspace = current.selection().workspace();
                        let data = export_customer_receipt(
                            c,
                            configuration.company.as_str(),
                            workspace.branch_id.as_ref().map(|value| value.as_str()),
                            workspace.department_id.as_ref().map(|value| value.as_str()),
                            &id,
                        )
                        .await?;
                        Ok(data.map(|data| FinanceSalesReceiptExportResponse { data }))
                    })
                },
            )
            .await
    }

    pub async fn issue_sales_receipt<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        payment_id: String,
    ) -> Result<FinanceSalesReceiptIssueResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.write".into(),
                    module: Some("finance".into()),
                    destination: "finance/sales/receipts".into(),
                    api: true,
                },
                SalesReceiptSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let payment_id = payment_id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.write")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment to issue a customer receipt.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            now,
                        )
                        .await?;
                        let actor = command_actor(current)?;
                        let data = issue_customer_receipt(
                            c,
                            &actor,
                            &command,
                            &payment_id,
                            now.get(),
                        )
                        .await?;
                        Ok(FinanceSalesReceiptIssueResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn sales_invoice_list<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: SalesInvoiceListQuery,
    ) -> Result<FinanceSalesInvoiceListReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/invoices".into(),
                    api: true,
                },
                SalesInvoiceListSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        lock_sales_invoice_list_sources(c).await?;
                        let workspace = current.selection().workspace();
                        let data = read_sales_invoice_list(
                            c,
                            configuration.company.as_str(),
                            workspace.branch_id.as_ref().map(|value| value.as_str()),
                            workspace.department_id.as_ref().map(|value| value.as_str()),
                            &query,
                        )
                        .await?;
                        Ok(FinanceSalesInvoiceListReadResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: RecordId,
    ) -> Result<FinanceJournalReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        match self
            .read_kind(host, proof, input, id, ReadKind::Detail)
            .await?
        {
            ReadResponse::Detail(reply) => Ok(reply),
            _ => Err(scope().into()),
        }
    }
    pub async fn read_workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: RecordId,
    ) -> Result<FinanceJournalWorkspaceReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        match self
            .read_kind(host, proof, input, id, ReadKind::Workspace)
            .await?
        {
            ReadResponse::Workspace(reply) => Ok(reply),
            _ => Err(scope().into()),
        }
    }
    async fn read_kind<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: RecordId,
        kind: ReadKind,
    ) -> Result<ReadResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        let preparation = ReadPreparation {
            kind,
            field_service_parts: configuration.field_service_parts,
            field_service_invoices: configuration.field_service_invoices,
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/journals".into(),
                    api: true,
                },
                preparation,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        let inspection = PgCompanyPurchaseReader::new(
                            configuration.holding.clone(),
                            configuration.company.clone(),
                            c,
                            &configuration.purchases,
                        )
                        .inspect(
                            &PurchaseRequest {
                                capabilities: vec!["finance.accounting".into()],
                                purpose: CapabilityPurpose::History,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            clock.now(),
                        )
                        .await?;
                        let purchase = inspection.witness;
                        let actor = FinanceActor {
                            holding_id: current.holding_id().clone(),
                            company_id: configuration.company.clone(),
                            user_id: current.user_id().clone(),
                            assignment_id: current.selection().assignment().id.clone(),
                        };
                        let source_context=crate::field_service_source_record::SourceRecordContext{actor:current,holding:&configuration.holding,company:&configuration.company,catalogue:&configuration.purchases,clock:clock.as_ref()};
                        // A client label cannot choose this admission. Finance
                        // proves the exact invoice/book edge, then the existing
                        // same-unit Field history port proves lineage/purchase.
                        let invoice_source=if configuration.field_service_invoices {
                            match kind {
                                ReadKind::Detail=>match read_field_service_invoice_journal_detail(c,&actor,&id).await?{
                                    Some((invoice,detail))=>Some(ReadSource::InvoiceDetail(Box::new(detail),Box::new(crate::finance_service_invoice_history::read_service_invoice_history(c,&source_context,&invoice).await?))),None=>None,
                                },
                                ReadKind::Workspace=>match read_field_service_invoice_journal_workspace(c,&actor,&id).await?{
                                    Some((invoice,workspace))=>Some(ReadSource::InvoiceWorkspace(Box::new(workspace),Box::new(crate::finance_service_invoice_history::read_service_invoice_history(c,&source_context,&invoice).await?))),None=>None,
                                },
                            }
                        }else{None};
                        let invoice_admission=invoice_source.is_some();
                        let base_access=PgCompanyPurchaseReader::new(configuration.holding.clone(),configuration.company.clone(),c,&configuration.purchases).revalidate(&purchase,clock.now()).await?;
                        let target = if configuration.field_service_parts {
                            use erpx_server_finance::public::journal_review::{inspect_field_service_parts_journal_target, JournalReviewCommand, JournalReviewInput};
                            let command = JournalReviewCommand { key: erpx_server_shared::public::CommandKey::new("read-only-source")?, journal_id: id.clone(), input: JournalReviewInput::Post { expected_version: 1 } };
                            match inspect_field_service_parts_journal_target(c, &actor, &command).await {
                                Ok(target)=>target,
                                Err(error)=>{
                                    // A source label alone cannot expose owner facts to
                                    // an actor who lacks the ordinary Finance purchase.
                                    if matches!(&error,PgTransactionError::Business(e) if e.code=="FINANCE_JOURNAL_SOURCE_UNSUPPORTED") {base_access.require_allowed()?;}
                                    return Err(error);
                                }
                            }
                        } else { None };
                        // The existing target is only a Finance fact. Actual
                        // Inventory/Field/receipt lineage is retained below
                        // before any payload can leave this authorized unit.
                        let parts_purchase=if !invoice_admission && target.is_some() && base_access.require_allowed().is_err() {
                            let included=PgCompanyPurchaseReader::new(configuration.holding.clone(),configuration.company.clone(),c,&configuration.purchases).inspect(&PurchaseRequest{capabilities:vec!["finance.service-invoice".into(),"service.parts-assets".into()],purpose:CapabilityPurpose::History,mode:PackageEnforcementMode::Enforced},clock.now()).await?;
                            included.require_allowed()?;Some(included.witness)
                        }else{None};
                        if !invoice_admission && parts_purchase.is_none(){base_access.require_allowed()?;}
                        let source = if let Some(source)=invoice_source {source} else if let Some(target) = target {
                            match kind {
                                ReadKind::Detail => {
                                    let detail = read_field_service_journal_detail(c, &actor, &id, &target).await?;
                                    let lineage = crate::finance_field_service_lineage::FieldServicePartsSource::retain(c, target).await?;
                                    ReadSource::FieldDetail(Box::new(detail), Box::new(lineage))
                                }
                                ReadKind::Workspace => {
                                    let workspace = read_field_service_journal_workspace(c, &actor, &id, &target).await?;
                                    let lineage = crate::finance_field_service_lineage::FieldServicePartsSource::retain(c, target).await?;
                                    ReadSource::FieldWorkspace(Box::new(workspace), Box::new(lineage))
                                }
                            }
                        } else {
                            match kind {
                                ReadKind::Detail => ReadSource::Detail(Box::new(read_journal_detail(c, &actor, &id).await?)),
                                ReadKind::Workspace => ReadSource::Workspace(Box::new(read_journal_record_workspace(c, &actor, &id).await?)),
                            }
                        };
                        let creator = match source.journal().get("created_by") {
                            Some(FinanceLedgerValue::Text(v)) => UserId::new(v)?,
                            _ => return Err(scope().into()),
                        };
                        let poster = match source.journal().get("posted_by") {
                            Some(FinanceLedgerValue::Text(v)) => Some(UserId::new(v)?),
                            Some(FinanceLedgerValue::Null) => None,
                            _ => return Err(scope().into()),
                        };
                        let mut users = vec![creator.clone()];
                        if let Some(user) = &poster {
                            users.push(user.clone());
                        }
                        let names =
                            read_finance_journal_names(c, &configuration.company, &users).await?;
                        let response = source.response(
                            rendered(current)?,
                            names.name(&creator)?,
                            match &poster {
                                Some(p) => names.name(p)?,
                                None => "Unlinked account",
                            },
                        );
                        admit_reply(&response)?;
                        source.require_current(c,&source_context).await?;
                        names.require_current(c).await?;
                        let base=PgCompanyPurchaseReader::new(
                            configuration.holding.clone(),configuration.company.clone(),c,&configuration.purchases,
                        ).revalidate(&purchase, TimestampMillis::new(clock.now().get())).await?;
                        if !invoice_admission && parts_purchase.is_none(){base.require_allowed()?;}
                        if let Some(witness)=parts_purchase {
                            PgCompanyPurchaseReader::new(configuration.holding.clone(),configuration.company.clone(),c,&configuration.purchases).revalidate(&witness,clock.now()).await?.require_allowed()?;
                        }
                        // Invoice history above also retains denied-base versus
                        // included-Service status and its selected witness.

                        if clock.now().get() >= current.expires_at() {
                            return Err(scope().into());
                        }
                        Ok(response)
                    })
                },
            )
            .await
    }
}
