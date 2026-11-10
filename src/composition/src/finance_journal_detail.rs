//! Development read assembly. Rendered context is output data from the SAME
//! retained authorization unit, never a deserialized actor or a command grant.
use crate::finance_journal_actor::finance_actor;
use crate::finance_journal_environment::FinanceJournalEnvironment;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, lock_commercial_authority_sources,
};
use erpx_server_files::public::business_evidence::{
    PgBusinessEvidenceFiles, evidence_context, lock_business_evidence_sources, policy_fact,
};
use erpx_server_finance::public::{
    FinanceActor, finance_invoice_read::*, finance_opening::*, finance_reports::*,
    finance_setup::*, journal_detail::*, journal_list::*, journal_workspace::*, period_close::*,
    sales_defaults::*, sales_invoice_create::*, sales_invoice_edit::*, sales_invoice_list::*,
    sales_receipts::*, supplier_bill::*,
};
use erpx_server_finance::public::{
    JournalAuthority as FinanceJournalAuthority, JournalDraft as FinanceJournalDraft,
    JournalError as FinanceJournalError,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    command_metadata::project_command_iam,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_people_payroll::public::finance_journal_names::{
    lock_finance_journal_name_sources, read_finance_journal_names,
};
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
    UserId,
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
    environment: FinanceJournalEnvironment,
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
        Ok(Self {
            holding,
            company,
            purchases,
            environment,
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
            lock_finance_journal_name_sources(c).await?;
            if matches!(self.environment, FinanceJournalEnvironment::Production) {
                crate::finance_production_release::require_production_release(c, &self.company)
                    .await?;
            }
            Ok(())
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
pub struct FinanceSupplierBillCreateResponse {
    pub data: SupplierBillResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSupplierBillEditResponse {
    pub data: SupplierBillResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSupplierBillVoidResponse {
    pub data: SupplierBillResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSupplierBillPostResponse {
    pub data: SupplierBillPostResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceSupplierBillDetailResponse {
    pub data: SupplierBillDetail,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinancePeriodCloseResponse {
    pub data: PeriodCloseResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinancePeriodCloseWorkspaceResponse {
    pub data: PeriodCloseWorkspace,
    pub context: FinanceRenderedWorkContext,
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
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
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
pub struct FinanceSetupWorkspaceResponse {
    pub data: FinanceSetupWorkspace,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceChartResponse {
    pub data: ChartResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceAccountResponse {
    pub data: AccountResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceAccountImportResponse {
    pub data: AccountImportResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceAccountStatusResponse {
    pub data: AccountStatusResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinancePeriodResponse {
    pub data: PeriodResult,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceFinancialReportResponse {
    pub data: FinancialReport,
    pub context: FinanceRenderedWorkContext,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceOpeningPreviewResponse {
    pub data: OpeningPreview,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct FinanceOpeningImportResponse {
    pub data: OpeningImportResult,
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
    #[serde(rename = "purchaseControls")]
    pub purchase_controls:
        std::collections::BTreeMap<String, crate::finance_presentation::FinancePurchaseAccess>,
    #[serde(rename = "sourcePurchase")]
    #[cfg_attr(feature="contracts",ts(type="Record<string, unknown>"),schema(value_type=Object))]
    pub source_purchase: serde_json::Value,
    #[serde(rename = "canPost")]
    pub can_post: bool,
    #[cfg_attr(feature="contracts",ts(type="Record<string, unknown>"),schema(value_type=Object))]
    pub flow: serde_json::Value,
    #[serde(rename = "noCharge")]
    pub no_charge: Option<erpx_server_finance::public::service_no_charge::ServiceNoChargeControls>,
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
            erpx_server_workflow::public::service_invoice_flow::lock_service_invoice_flow_sources(c)
                .await
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
    let iam = serde_json::to_value(project_command_iam(current)?).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "COMMAND_IAM_METADATA_UNAVAILABLE",
            "The current command authority metadata is unavailable.",
        )
    })?;
    let object = iam.as_object().ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "COMMAND_IAM_METADATA_UNAVAILABLE",
            "The current command authority metadata is unavailable.",
        )
    })?;
    let mut iam_json = String::from("{");
    iam_json.push_str("\"personUserIds\":");
    iam_json.push_str(
        &object
            .get("person_user_ids")
            .cloned()
            .unwrap_or(serde_json::json!([]))
            .to_string(),
    );
    iam_json.push_str(",\"emergencyUntil\":");
    iam_json.push_str(
        &object
            .get("emergency_until")
            .cloned()
            .unwrap_or(serde_json::Value::Null)
            .to_string(),
    );
    iam_json.push_str(",\"roleRevision\":");
    iam_json.push_str(
        &object
            .get("role_revision")
            .cloned()
            .unwrap_or(serde_json::json!(0))
            .to_string(),
    );
    iam_json.push_str(",\"identityVersion\":");
    iam_json.push_str(
        &object
            .get("identity_version")
            .cloned()
            .unwrap_or(serde_json::json!(0))
            .to_string(),
    );
    iam_json.push_str(",\"permissions\":");
    iam_json.push_str(
        &object
            .get("permissions")
            .cloned()
            .unwrap_or(serde_json::json!([]))
            .to_string(),
    );
    if let Some(value) = object.get("hr_appointment_version") {
        iam_json.push_str(",\"hrAppointmentVersion\":");
        iam_json.push_str(&value.to_string());
    }
    iam_json.push_str(",\"grantablePermissions\":");
    iam_json.push_str(
        &object
            .get("grantable_permissions")
            .cloned()
            .unwrap_or(serde_json::json!([]))
            .to_string(),
    );
    iam_json.push_str(",\"principalId\":");
    iam_json.push_str(
        &object
            .get("principal_id")
            .cloned()
            .unwrap_or(serde_json::json!(""))
            .to_string(),
    );
    iam_json.push_str(",\"personId\":");
    iam_json.push_str(
        &object
            .get("person_id")
            .cloned()
            .unwrap_or(serde_json::Value::Null)
            .to_string(),
    );
    iam_json.push_str(",\"freshUntil\":");
    iam_json.push_str(
        &object
            .get("fresh_until")
            .cloned()
            .unwrap_or(serde_json::Value::Null)
            .to_string(),
    );
    iam_json.push('}');
    Ok(SalesInvoiceCommandActor {
        user_id: current.user_id().as_str().into(),
        assignment_id: assignment.id.as_str().into(),
        role: assignment.role.clone(),
        session_id: current.session_id().as_str().into(),
        revision: selection.revision(),
        assignment_version: assignment.version,
        company_id: workspace.company_id.as_str().into(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|value| value.as_str().into()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().into()),
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
struct OpeningSources;
impl HoldingWorkSourcePreparation for OpeningSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_finance_opening_sources(c).await?;
            Ok(())
        })
    }
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
struct SupplierBillWriteSources;
impl HoldingWorkSourcePreparation for SupplierBillWriteSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_supplier_bill_write_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct SupplierBillVoidSources;
impl HoldingWorkSourcePreparation for SupplierBillVoidSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            // Match post and void take the same owner fences in the same order:
            // Commerce -> Procurement -> Commercial -> Finance -> Files.
            erpx_server_commerce::public::finance_payable_source::lock_commerce_payable_sources(c)
                .await?;
            erpx_server_procurement::public::finance_supplier_bill_match::lock_procurement_bill_match_sources(c)
                .await?;
            lock_commercial_authority_sources(c).await?;
            lock_supplier_bill_write_sources(c).await?;
            // Posted voids insert and reverse journals in this unit; the table
            // fence precedes the first invoice/journal read.
            sqlx::query(
                "LOCK TABLE finance_journals,finance_journal_lines,finance_periods IN SHARE ROW EXCLUSIVE MODE",
            )
            .execute(&mut *c)
            .await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct SupplierBillPostSources;
impl HoldingWorkSourcePreparation for SupplierBillPostSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            // Source owners fence before the first Finance read. The global
            // order for this unit is Commerce -> Commercial -> Finance -> Files.
            erpx_server_commerce::public::finance_payable_source::lock_commerce_payable_sources(c)
                .await?;
            erpx_server_procurement::public::finance_supplier_bill_match::lock_procurement_bill_match_sources(c)
                .await?;
            lock_commercial_authority_sources(c).await?;
            lock_supplier_bill_post_sources(c).await?;
            lock_business_evidence_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct SupplierBillReadSources;
impl HoldingWorkSourcePreparation for SupplierBillReadSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_supplier_bill_read_sources(c).await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct PeriodCloseSources;
impl HoldingWorkSourcePreparation for PeriodCloseSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            sqlx::query(
                "LOCK TABLE finance_periods,finance_accounts,finance_charts,finance_journals,finance_journal_lines,finance_invoices,finance_bank_transactions,finance_bank_statements,finance_close_checks,finance_close_reconciliations IN SHARE ROW EXCLUSIVE MODE",
            )
            .execute(&mut *c)
            .await?;
            // Files owner fence before the first snapshot; the global order for
            // this unit is Commercial -> Finance -> Files.
            lock_business_evidence_sources(c).await?;
            Ok(())
        })
    }
}

struct FinanceSupplierBillSource;
impl PgSupplierBillSource for FinanceSupplierBillSource {
    fn load(
        &mut self,
        c: &mut PgConnection,
        company_id: &str,
        source_type: &str,
        source_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<SupplierBillSourceFacts>, Error>> + Send
    {
        Box::pin(async move {
            match source_type {
                "commerce_fulfillment" => {
                    let source = erpx_server_commerce::public::finance_payable_source::load_commerce_payable_source(
                        c, company_id, source_id,
                    )
                    .await
                    .map_err(PgTransactionError::Business)?;
                    match source {
                        None => Ok(None),
                        Some(source) => {
                            let posted = if source.completion_kind == "service" {
                                true
                            } else if let Some(journal_id) = source.journal_id.as_deref() {
                                sqlx::query_scalar::<_, bool>(
                                    "SELECT EXISTS(SELECT 1 FROM finance_journals \
                                     WHERE id=$1 AND company_id=$2 AND status='posted')",
                                )
                                .bind(journal_id)
                                .bind(company_id)
                                .fetch_one(&mut *c)
                                .await?
                            } else {
                                false
                            };
                            Ok(Some(SupplierBillSourceFacts {
                                source_type: "commerce_fulfillment".into(),
                                source_id: source.fulfillment_id,
                                source_version: source.fulfillment_version,
                                status: source.order_kind,
                                invoice_id: source.invoice_id,
                                posted,
                            }))
                        }
                    }
                }
                _ => Ok(None),
            }
        })
    }
}

struct FinanceSupplierBillMatch;
impl PgSupplierBillMatch for FinanceSupplierBillMatch {
    fn load(
        &mut self,
        c: &mut PgConnection,
        company_id: &str,
        invoice_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<SupplierBillMatchFacts>, Error>> + Send
    {
        Box::pin(async move {
            let matched = erpx_server_procurement::public::finance_supplier_bill_match::load_procurement_supplier_bill_match(
                c, company_id, invoice_id,
            )
            .await
            .map_err(PgTransactionError::Business)?;
            Ok(matched.map(|matched| SupplierBillMatchFacts {
                match_id: matched.match_id,
                match_version: matched.match_version,
                status: matched.status,
                invoice_id: matched.invoice_id,
                currency: matched.currency,
                chart_id: matched.chart_id,
                supplier_id: matched.supplier_id,
                invoice_date: matched.invoice_date,
                due_date: matched.due_date,
                expected_subtotal_minor: matched.expected_subtotal_minor,
                subtotal_minor: matched.subtotal_minor,
                tax_minor: matched.tax_minor,
                clearing_account_id: matched.clearing_account_id,
                payable_account_id: matched.payable_account_id,
                tax_account_id: matched.tax_account_id,
                variance_account_id: matched.variance_account_id,
                quantity_matches: matched.quantity_matches,
                fulfillment_id: matched.fulfillment_id,
                receipts: matched
                    .receipts
                    .into_iter()
                    .map(|receipt| SupplierBillMatchReceiptFacts {
                        fulfillment_id: receipt.fulfillment_id,
                        captured_version: receipt.captured_version,
                        current_version: receipt.current_version,
                        active: receipt.active,
                    })
                    .collect(),
            }))
        })
    }
    fn stage_post(
        &mut self,
        c: &mut PgConnection,
        company_id: &str,
        match_id: &str,
        expected_version: i64,
        invoice_id: &str,
        actor_id: &str,
        command_id: &str,
        now: i64,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        Box::pin(async move {
            erpx_server_procurement::public::finance_supplier_bill_match::stage_procurement_supplier_bill_match_post(
                c,
                company_id,
                match_id,
                expected_version,
                invoice_id,
                actor_id,
                command_id,
                now,
            )
            .await
            .map_err(PgTransactionError::Business)
        })
    }
    fn stage_return(
        &mut self,
        c: &mut PgConnection,
        company_id: &str,
        match_id: &str,
        expected_version: i64,
        invoice_id: &str,
        actor_id: &str,
        command_id: &str,
        now: i64,
        reason: &str,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        Box::pin(async move {
            erpx_server_procurement::public::finance_supplier_bill_match::stage_procurement_supplier_bill_match_return(
                c,
                company_id,
                match_id,
                expected_version,
                invoice_id,
                actor_id,
                command_id,
                now,
                reason,
            )
            .await
            .map_err(PgTransactionError::Business)
        })
    }
}

#[derive(Clone)]
struct FinanceSupplierBillEvidence {
    company: CompanyId,
    actor: String,
    now: TimestampMillis,
}
impl FinanceSupplierBillEvidence {
    fn files<'c>(&self, c: &'c mut PgConnection) -> PgBusinessEvidenceFiles<'c> {
        PgBusinessEvidenceFiles::new(
            c,
            self.company.clone(),
            self.actor.clone(),
            None,
            None,
            vec![policy_fact(
                SUPPLIER_BILL_EVIDENCE_TARGET_TYPE,
                SUPPLIER_BILL_EVIDENCE_PURPOSE_KEY,
                &[SUPPLIER_BILL_EVIDENCE_COMPONENT_KIND],
                1,
            )],
            self.now,
        )
    }
}
impl PgSupplierBillEvidence for FinanceSupplierBillEvidence {
    type Prepared = erpx_server_files::public::business_evidence::PreparedBusinessEvidence;
    fn prepare(
        &mut self,
        c: &mut PgConnection,
        request: SupplierBillEvidenceRequest,
    ) -> impl std::future::Future<Output = Result<Self::Prepared, Error>> + Send {
        Box::pin(async move {
            let mut files = self.files(c);
            prepare_business_evidence(
                &mut files,
                BusinessEvidenceRequest {
                    attachment_set_ids: request.set_ids,
                    target_types: vec![SUPPLIER_BILL_EVIDENCE_TARGET_TYPE.into()],
                    contexts: vec![evidence_context(
                        SUPPLIER_BILL_EVIDENCE_PURPOSE_KEY,
                        SUPPLIER_BILL_EVIDENCE_COMPONENT_KIND,
                        Some(SUPPLIER_BILL_EVIDENCE_COMPONENT_REF),
                    )],
                    target_id: request.target_id,
                    expected_target_version: request.expected_target_version,
                    command_id: request.command_id,
                    binding_type: request.binding_type,
                    binding_id: request.binding_id,
                },
            )
            .await?
            .ok_or_else(|| {
                BusinessError::invalid(
                    "ATTACHMENT_SETS_REQUIRED",
                    "Choose one to twenty file sets.",
                )
                .into()
            })
        })
    }
    fn require_current(
        &mut self,
        c: &mut PgConnection,
        prepared: &Self::Prepared,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        Box::pin(async move {
            let mut files = self.files(c);
            files.require_current(prepared).await
        })
    }
}

/// Finance-owned adapter for the shared Records/Files close-workpaper port.
/// The Files implementation binds and re-reads the exact sets in this
/// serializable Finance unit; it never commits and never authorizes Finance.
struct FinancePeriodCloseEvidence {
    company: CompanyId,
    actor: String,
    now: TimestampMillis,
}
fn require_period_close_attachment_write() -> Result<(), Error> {
    if std::env::var("ERPX_ATTACHMENTS_V2").as_deref() != Ok("true") {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "ATTACHMENTS_DISABLED",
            "The attachment platform is not enabled for this release.",
        )
        .into());
    }
    if std::env::var("ERPX_ATTACHMENT_UPLOAD_FREEZE").as_deref() == Ok("true") {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "ATTACHMENT_UPLOAD_FREEZE",
            "File intake is briefly paused for storage reconciliation.",
        )
        .into());
    }
    Ok(())
}

impl FinancePeriodCloseEvidence {
    fn files<'c>(&self, c: &'c mut PgConnection) -> PgBusinessEvidenceFiles<'c> {
        PgBusinessEvidenceFiles::new(
            c,
            self.company.clone(),
            self.actor.clone(),
            None,
            None,
            vec![policy_fact(
                PERIOD_CLOSE_WORKPAPER_TARGET_TYPE,
                PERIOD_CLOSE_WORKPAPER_PURPOSE_KEY,
                &[PERIOD_CLOSE_WORKPAPER_COMPONENT_KIND],
                1,
            )],
            self.now,
        )
    }
}
impl PgPeriodCloseEvidence for FinancePeriodCloseEvidence {
    type Prepared = erpx_server_files::public::business_evidence::PreparedBusinessEvidence;
    fn prepare(
        &mut self,
        c: &mut PgConnection,
        request: PeriodCloseEvidenceRequest,
    ) -> impl std::future::Future<Output = Result<Self::Prepared, Error>> + Send {
        Box::pin(async move {
            require_period_close_attachment_write()?;
            let mut files = self.files(c);
            prepare_business_evidence(
                &mut files,
                BusinessEvidenceRequest {
                    attachment_set_ids: request.set_ids,
                    target_types: vec![PERIOD_CLOSE_WORKPAPER_TARGET_TYPE.into()],
                    contexts: vec![evidence_context(
                        PERIOD_CLOSE_WORKPAPER_PURPOSE_KEY,
                        PERIOD_CLOSE_WORKPAPER_COMPONENT_KIND,
                        Some(PERIOD_CLOSE_WORKPAPER_COMPONENT_REF),
                    )],
                    target_id: request.target_id,
                    expected_target_version: request.expected_target_version,
                    command_id: request.command_id,
                    binding_type: request.binding_type,
                    binding_id: request.binding_id,
                },
            )
            .await?
            .ok_or_else(|| {
                BusinessError::invalid(
                    "ATTACHMENT_SETS_REQUIRED",
                    "Choose one to twenty file sets.",
                )
                .into()
            })
        })
    }
    fn require_current(
        &mut self,
        c: &mut PgConnection,
        prepared: &Self::Prepared,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        Box::pin(async move { self.files(c).require_current(prepared).await })
    }
}

struct SupplierBillJournalAuthority<'a>(&'a AuthorizedWorkContext);
impl FinanceJournalAuthority for SupplierBillJournalAuthority<'_> {
    type Error = PgTransactionError<FinanceJournalError>;
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &FinanceJournalDraft,
    ) -> Result<(), Self::Error> {
        if actor != &finance_actor(self.0)
            || !self.0.has_permission("finance.approve")
            || !self.0.has_permission("finance.write")
            || draft.source_type != "invoice"
            || draft.is_opening
        {
            return Err(FinanceJournalError::new(
                "FINANCE_CURRENT_AUTHORITY_REQUIRED",
                "Choose the current Finance approver.",
            )
            .into());
        }
        Ok(())
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum ReadKind {
    Detail,
    Workspace,
}
#[derive(Clone)]
struct ReadPreparation {
    kind: ReadKind,
    field_service_parts: bool,
    field_service_invoices: bool,
    field_finance_history: bool,
    generic_only: bool,
}
impl HoldingWorkSourcePreparation for ReadPreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            // Generic Field History owns its complete Finance/Workflow/
            // Inventory/Field source order. Acquire it before the narrower
            // invoice/parts gates so a generic-only route cannot interleave
            // the shared Finance tables in a second order.
            if self.field_finance_history {
                crate::finance_generic_history::lock_generic_field_finance_history_sources(c)
                    .await?;
            }
            if !self.generic_only {
                // Take Finance's retained writer/source gate before shared table
                // reads so concurrent Field invoice posting cannot meet an upgrade.
                if self.field_service_parts || self.field_service_invoices {
                    erpx_server_finance::public::journal_review::lock_journal_review_sources(c)
                        .await?;
                }
                if self.field_service_invoices {
                    crate::finance_service_invoice_history::lock_service_invoice_history_sources(c)
                        .await?;
                }
                match self.kind {
                    ReadKind::Detail => lock_journal_detail_sources(c).await?,
                    ReadKind::Workspace => lock_journal_workspace_sources(c).await?,
                }
                if self.field_service_parts {
                    erpx_server_inventory::public::field_service_consumption_finance::lock_consumption_finance_sources(c).await?;
                    erpx_server_field_service::public::finance_lineage::lock_field_service_parts_finance_sources(c).await?;
                }
            } else if self.kind == ReadKind::Workspace {
                lock_journal_workspace_sources(c).await?;
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
    InvoiceDetail(
        Box<CurrentFieldServiceJournalDetail>,
        Box<crate::finance_service_invoice_history::CurrentServiceInvoiceHistory>,
    ),
    InvoiceWorkspace(
        Box<CurrentFieldServiceJournalWorkspace>,
        Box<crate::finance_service_invoice_history::CurrentServiceInvoiceHistory>,
    ),
    GenericFieldDetail(Box<crate::finance_generic_history::CurrentGenericFieldFinanceHistory>),
    GenericFieldWorkspace(Box<crate::finance_generic_history::CurrentGenericFieldFinanceHistory>),
}
impl ReadSource {
    fn journal(&self) -> &FinanceLedgerRow {
        match self {
            Self::Detail(s) => &s.detail().journal,
            Self::Workspace(s) => &s.data().journal,
            Self::FieldDetail(s, _) | Self::InvoiceDetail(s, _) => &s.detail().journal,
            Self::FieldWorkspace(s, _) | Self::InvoiceWorkspace(s, _) => &s.data().journal,
            Self::GenericFieldDetail(s) | Self::GenericFieldWorkspace(s) => s.journal(),
        }
    }
    async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        ctx: &crate::field_service_source_record::SourceRecordContext<'_, C>,
    ) -> Result<(), Error> {
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
            Self::InvoiceDetail(s, history) => {
                s.require_current(c).await?;
                history.require_current(c, ctx).await
            }
            Self::InvoiceWorkspace(s, history) => {
                s.require_current(c).await?;
                history.require_current(c, ctx).await
            }
            Self::GenericFieldDetail(s) | Self::GenericFieldWorkspace(s) => {
                s.require_current(c, ctx).await
            }
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
            Self::GenericFieldDetail(s) => {
                let mut data = s
                    .detail()
                    .expect("generic Field detail source")
                    .detail()
                    .clone();
                names(&mut data.journal);
                ReadResponse::Detail(FinanceJournalReadResponse { data, context })
            }
            Self::GenericFieldWorkspace(s) => {
                let mut data = s
                    .workspace()
                    .expect("generic Field workspace source")
                    .data()
                    .clone();
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
pub(crate) fn rendered(
    current: &AuthorizedWorkContext,
) -> Result<FinanceRenderedWorkContext, BusinessError> {
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
pub(crate) fn admit_finance_transport_response(
    reply: &impl Serialize,
) -> Result<(), BusinessError> {
    serde_json::to_writer(ReplyBudget(0), reply).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "FINANCE_RESPONSE_LIMIT",
            "The Finance response exceeds this bounded read.",
        )
    })
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
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
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
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            ServiceInvoiceReadSources.prepare(c).await?;
            ReadPreparation {
                kind: ReadKind::Workspace,
                field_service_parts: true,
                field_service_invoices: true,
                field_finance_history: false,
                generic_only: false,
            }
            .prepare(c)
            .await?;
            erpx_server_workflow::public::field_journal_flow::lock_field_journal_flow_sources(c)
                .await
        })
    }
}
impl<C: Clock + Send + Sync + 'static> FinanceJournalDetailService<C> {
    #[cfg(feature = "finance-field-write")]
    pub async fn read_field_work<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<FinanceFieldWorkResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if !self.configuration.field_service_parts || !self.configuration.field_service_invoices {
            return Err(scope().into());
        }
        let config = self.configuration.clone();
        let clock = self.clock.clone();
        let company = config.company.clone();
        self.authorizer.execute_with_sources_deferred(host,proof,move||{let mut s=input()?;if s.company_id!=company{return Err(scope());}s.mutation=false;Ok(s)},WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/work".into(),api:true},FieldFinanceWorkSources,move|c,current|{
            let(config,clock)=(config.clone(),clock.clone());Box::pin(async move{
                if current.holding_id()!=&config.holding||current.selection().workspace().company_id!=config.company{return Err(scope().into());}
                let source_config=crate::finance_journal_create::FinanceJournalCreateConfiguration::new(config.environment,config.holding.clone(),config.company.clone(),config.purchases.clone())?;
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
                ServiceInvoiceReadSources,
                move |c, current| {
                    let config = config.clone();
                    let clock = clock.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &config.holding
                            || current.selection().workspace().company_id != config.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("module.finance.read")?;
                        let source_context =
                            crate::field_service_source_record::SourceRecordContext {
                                purchase_locks: None,
                                actor: current,
                                holding: &config.holding,
                                company: &config.company,
                                catalogue: &config.purchases,
                                clock: clock.as_ref(),
                            };
                        let source =
                            crate::finance_service_invoice_history::read_service_invoice_history(
                                c,
                                &source_context,
                                &id,
                            )
                            .await?;
                        if payment_evidence {
                            source.require_current(c, &source_context).await?;
                            source.require_payment_evidence()?;
                            return Err(BusinessError::new(
                                FailureKind::Unavailable,
                                "NATIVE_SUPPLIER_BILL_EVIDENCE_UNAVAILABLE",
                                "Supplier bill evidence is outside the Field invoice scope.",
                            )
                            .into());
                        }
                        let source_only = source.source_only;
                        let presentation = crate::finance_presentation::read_finance_presentation(
                            c,
                            &config.holding,
                            &config.company,
                            &config.purchases,
                            clock.as_ref(),
                        )
                        .await?;
                        let mut data = source.data().clone();
                        // Retained Sales receipt-list scope: Field receipts are Company
                        // history; a narrowed Sales branch does not disclose this list.
                        if current.selection().workspace().branch_id.is_some()
                            || current.selection().workspace().department_id.is_some()
                        {
                            data.customer_receipts.clear();
                        }
                        let flow_record =
                            serde_json::to_value(&data.invoice).map_err(|_| scope())?;
                        let flow = crate::finance_service_invoice_flow::read_service_invoice_flow(
                            c,
                            current,
                            &flow_record,
                            presentation.can_post_invoice(),
                            clock.as_ref(),
                        )
                        .await?;
                        let no_charge = crate::finance_service_no_charge_read::controls(
                            c,
                            current,
                            &id,
                            presentation.service_purchase()["settleExisting"] == true,
                            clock.as_ref(),
                        )
                        .await?;
                        let reply = FinanceServiceInvoiceWorkspaceResponse {
                            no_charge,
                            data,
                            context: rendered(current)?,
                            source_only,
                            purchase_controls: presentation.values().clone(),
                            source_purchase: presentation.service_purchase(),
                            can_post: presentation.can_post_invoice(),
                            flow: flow.view.clone(),
                        };
                        serde_json::to_writer(ReplyBudget(0), &reply).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "FINANCE_INVOICE_READ_LIMIT",
                                "The invoice workspace exceeds this bounded read.",
                            )
                        })?;
                        source.require_current(c, &source_context).await?;
                        flow.require_current(
                            c,
                            current,
                            &flow_record,
                            presentation.can_post_invoice(),
                            clock.as_ref(),
                        )
                        .await?;
                        presentation
                            .require_current(
                                c,
                                &config.holding,
                                &config.company,
                                &config.purchases,
                                clock.as_ref(),
                            )
                            .await?;
                        if clock.now().get() >= current.expires_at() {
                            return Err(scope().into());
                        }
                        Ok(reply)
                    })
                },
            )
            .await
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

    async fn require_accounting_purchase(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        configuration: &FinanceJournalReadConfiguration,
        purpose: CapabilityPurpose,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        Self::require_purchase(
            c,
            current,
            configuration,
            "finance.accounting",
            purpose,
            now,
        )
        .await
    }

    async fn require_close_purchase(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        configuration: &FinanceJournalReadConfiguration,
        purpose: CapabilityPurpose,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        Self::require_purchase(c, current, configuration, "finance.close", purpose, now).await
    }

    async fn require_purchase(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        configuration: &FinanceJournalReadConfiguration,
        capability: &str,
        purpose: CapabilityPurpose,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            configuration.company.clone(),
            c,
            &configuration.purchases,
        )
        .inspect(
            &PurchaseRequest {
                capabilities: vec![capability.into()],
                purpose,
                mode: PackageEnforcementMode::Enforced,
            },
            now,
        )
        .await?
        .require_allowed()?;
        Ok(())
    }

    pub async fn opening_balance_preview<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        body: OpeningInput,
    ) -> Result<FinanceOpeningPreviewResponse, Error>
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
                    permission: "finance.write".into(),
                    module: Some("finance".into()),
                    destination: "finance/setup".into(),
                    api: true,
                },
                OpeningSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let body = body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("finance.write")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(setup_company_scope().into());
                        }
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            clock.now(),
                        )
                        .await?;
                        let data =
                            read_opening_preview(c, configuration.company.as_str(), &body).await?;
                        Ok(FinanceOpeningPreviewResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn import_opening_balance<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        body: OpeningImportInput,
    ) -> Result<FinanceOpeningImportResponse, Error>
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
                    destination: "finance/setup".into(),
                    api: true,
                },
                OpeningSources,
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
                        current.require_permission("finance.read")?;
                        current.require_permission("finance.write")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(setup_company_scope().into());
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
                        let finance = finance_actor(current);
                        let data = import_opening_balance(
                            c,
                            &actor,
                            &finance,
                            clock.as_ref(),
                            &command,
                            &body.definition(),
                            &body.preview_token,
                            now.get(),
                        )
                        .await?;
                        Ok(FinanceOpeningImportResponse { data })
                    })
                },
            )
            .await
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
                    destination: "finance/reports".into(),
                    api: true,
                },
                FinanceReportSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let chart_id = chart_id.clone();
                    let period_id = period_id.clone();
                    let start_date = start_date.clone();
                    let end_date = end_date.clone();
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
                            return Err(setup_company_scope().into());
                        }
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            clock.now(),
                        )
                        .await?;
                        let data = read_financial_report(
                            c,
                            configuration.company.as_str(),
                            chart_id.as_deref(),
                            period_id.as_deref(),
                            start_date.as_deref(),
                            end_date.as_deref(),
                        )
                        .await?;
                        Ok(FinanceFinancialReportResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
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
                    destination: "finance/setup".into(),
                    api: true,
                },
                FinanceSetupSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
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
                            return Err(setup_company_scope().into());
                        }
                        Self::require_sales_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            clock.now(),
                        )
                        .await?;
                        let data =
                            read_chart_accounts(c, configuration.company.as_str(), &chart_id)
                                .await?;
                        Ok(FinanceChartAccountsResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    pub async fn finance_setup_workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<FinanceSetupWorkspaceResponse, Error>
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
                    destination: "finance/setup".into(),
                    api: true,
                },
                FinanceSetupSources,
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
                            return Err(setup_company_scope().into());
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
                        let data = read_finance_setup(c, configuration.company.as_str(), now.get())
                            .await?;
                        Ok(FinanceSetupWorkspaceResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    pub async fn create_chart<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        body: CreateChartInput,
    ) -> Result<FinanceChartResponse, Error>
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
                    destination: "finance/setup".into(),
                    api: true,
                },
                FinanceSetupSources,
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
                            return Err(setup_company_scope().into());
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
                        let data = create_chart(c, &actor, &command, &body, now.get()).await?;
                        Ok(FinanceChartResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn create_account<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        chart_id: String,
        mut body: CreateAccountInput,
    ) -> Result<FinanceAccountResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        body.chart_id = chart_id;
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
                    destination: "finance/setup".into(),
                    api: true,
                },
                FinanceSetupSources,
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
                            return Err(setup_company_scope().into());
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
                        let data = create_account(c, &actor, &command, &body, now.get()).await?;
                        Ok(FinanceAccountResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn import_accounts<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        body: ImportAccountsInput,
    ) -> Result<FinanceAccountImportResponse, Error>
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
                    destination: "finance/setup".into(),
                    api: true,
                },
                FinanceSetupSources,
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
                            return Err(setup_company_scope().into());
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
                        let data = import_accounts(c, &actor, &command, &body, now.get()).await?;
                        Ok(FinanceAccountImportResponse { data })
                    })
                },
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn change_account_status<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        chart_id: String,
        account_id: String,
        body: ChangeAccountStatusInput,
    ) -> Result<FinanceAccountStatusResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let purpose = if body.status == "active" {
            CapabilityPurpose::NewWork
        } else {
            CapabilityPurpose::History
        };
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
                    destination: "finance/setup".into(),
                    api: true,
                },
                FinanceSetupSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let chart_id = chart_id.clone();
                    let account_id = account_id.clone();
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
                            return Err(setup_company_scope().into());
                        }
                        let now = clock.now();
                        Self::require_sales_purchase(c, current, &configuration, purpose, now)
                            .await?;
                        let actor = command_actor(current)?;
                        let data = change_account_status(
                            c,
                            &actor,
                            &command,
                            &chart_id,
                            &account_id,
                            &body,
                            now.get(),
                        )
                        .await?;
                        Ok(FinanceAccountStatusResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn create_period<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        body: CreatePeriodInput,
    ) -> Result<FinancePeriodResponse, Error>
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
                    destination: "finance/setup".into(),
                    api: true,
                },
                FinanceSetupSources,
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
                            return Err(setup_company_scope().into());
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
                        let data = create_period(c, &actor, &command, &body, now.get()).await?;
                        Ok(FinancePeriodResponse { data })
                    })
                },
            )
            .await
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
                        let data =
                            read_invoice_page(c, configuration.company.as_str(), &query).await?;
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
                        let data =
                            create_sales_invoice(c, &actor, &command, &body, now.get()).await?;
                        Ok(FinanceSalesInvoiceCreateResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn create_supplier_bill<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        body: SupplierBillCreateInput,
    ) -> Result<FinanceSupplierBillCreateResponse, Error>
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
                    destination: "finance/payables".into(),
                    api: true,
                },
                SupplierBillWriteSources,
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
                                "Use a Company Finance assignment for supplier bills.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_accounting_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::NewWork,
                            now,
                        )
                        .await?;
                        let actor = command_actor(current)?;
                        let data =
                            create_supplier_bill(c, &actor, &command, &body, now.get()).await?;
                        Ok(FinanceSupplierBillCreateResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn edit_supplier_bill<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
        body: SupplierBillEditInput,
    ) -> Result<Option<FinanceSupplierBillEditResponse>, Error>
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
                    destination: "finance/payables".into(),
                    api: true,
                },
                SupplierBillWriteSources,
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
                                "Use a Company Finance assignment for supplier bills.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_accounting_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::NewWork,
                            now,
                        )
                        .await?;
                        let actor = command_actor(current)?;
                        let data =
                            edit_supplier_bill(c, &actor, &command, &id, &body, now.get()).await?;
                        Ok(data.map(|data| FinanceSupplierBillEditResponse { data }))
                    })
                },
            )
            .await
    }

    pub async fn void_supplier_bill<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
        body: SupplierBillVoidInput,
    ) -> Result<FinanceSupplierBillVoidResponse, Error>
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
                    destination: "finance/payables".into(),
                    api: true,
                },
                SupplierBillVoidSources,
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
                                "Use a Company Finance assignment for supplier bills.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_accounting_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::SettleExisting,
                            now,
                        )
                        .await?;
                        let facts: Option<(String, Option<String>, String)> = sqlx::query_as(
                            "SELECT created_by,approved_by,status FROM finance_invoices                              WHERE company_id=$1 AND id=$2",
                        )
                        .bind(configuration.company.as_str())
                        .bind(&id)
                        .fetch_optional(&mut *c)
                        .await?;
                        let (creator, approver, status) = facts.ok_or_else(|| {
                            BusinessError::new(
                                FailureKind::NotFound,
                                "INVOICE_NOT_FOUND",
                                "Supplier bill not found.",
                            )
                        })?;
                        let actor = command_actor(current)?;
                        let matched = erpx_server_procurement::public::finance_supplier_bill_match::load_procurement_supplier_bill_match(
                            c,
                            &actor.company_id,
                            &id,
                        )
                        .await
                        .map_err(PgTransactionError::Business)?;
                        let data = if status == "posted" {
                            current.require_permission("finance.approve")?;
                            let mut same_person =
                                current.same_person_user(&UserId::new(creator.clone())?);
                            if let Some(approver) = approver {
                                same_person |=
                                    current.same_person_user(&UserId::new(approver.clone())?);
                            }
                            let journal_actor = finance_actor(current);
                            void_posted_supplier_bill(
                                c,
                                &actor,
                                &journal_actor,
                                &command,
                                &id,
                                same_person,
                                &body,
                                now.get(),
                            )
                            .await?
                        } else {
                            void_supplier_bill(c, &actor, &command, &id, &body, now.get()).await?
                        };
                        erpx_server_commerce::public::finance_payable_source::release_commerce_payable_source(
                            c,
                            &actor.company_id,
                            &id,
                        )
                        .await
                        .map_err(PgTransactionError::Business)?;
                        if let Some(matched) = matched {
                            erpx_server_procurement::public::finance_supplier_bill_match::stage_procurement_supplier_bill_match_return(
                                c,
                                &actor.company_id,
                                &matched.match_id,
                                matched.match_version,
                                &id,
                                actor.user_id.as_str(),
                                command.as_str(),
                                now.get(),
                                "Matched payable voided",
                            )
                            .await
                            .map_err(PgTransactionError::Business)?;
                        }
                        Ok(FinanceSupplierBillVoidResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn post_supplier_bill<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
        body: SupplierBillPostInput,
    ) -> Result<FinanceSupplierBillPostResponse, Error>
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
                    destination: "finance/payables".into(),
                    api: true,
                },
                SupplierBillPostSources,
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
                        current.require_permission("finance.approve")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment to post a supplier bill.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_accounting_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::SettleExisting,
                            now,
                        )
                        .await?;
                        let creator: Option<String> = sqlx::query_scalar(
                            "SELECT created_by FROM finance_invoices WHERE company_id=$1 AND id=$2",
                        )
                        .bind(configuration.company.as_str())
                        .bind(&id)
                        .fetch_optional(&mut *c)
                        .await?;
                        let same_person = match creator {
                            Some(creator) => current.same_person_user(&UserId::new(creator)?),
                            None => false,
                        };
                        let actor = command_actor(current)?;
                        let journal_actor = finance_actor(current);
                        let mut authority = SupplierBillJournalAuthority(current);
                        let mut evidence = FinanceSupplierBillEvidence {
                            company: configuration.company.clone(),
                            actor: actor.user_id.clone(),
                            now,
                        };
                        let mut source = FinanceSupplierBillSource;
                        let mut matches = FinanceSupplierBillMatch;
                        let data = post_supplier_bill_with_evidence(
                            c,
                            &actor,
                            &journal_actor,
                            &command,
                            &id,
                            same_person,
                            &body,
                            clock.as_ref(),
                            &mut authority,
                            &mut evidence,
                            &mut source,
                            &mut matches,
                        )
                        .await?;
                        Ok(FinanceSupplierBillPostResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn supplier_bill_detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<FinanceSupplierBillDetailResponse, Error>
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
                    destination: "finance/payables".into(),
                    api: true,
                },
                SupplierBillReadSources,
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
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for the supplier bill view.",
                            )
                            .into());
                        }
                        Self::require_accounting_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            clock.now(),
                        )
                        .await?;
                        let data =
                            read_supplier_bill(c, configuration.company.as_str(), &id).await?;
                        Ok(FinanceSupplierBillDetailResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    pub async fn period_close_workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<FinancePeriodCloseWorkspaceResponse, Error>
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
                    destination: "finance/close".into(),
                    api: true,
                },
                PeriodCloseSources,
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
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for period close.",
                            )
                            .into());
                        }
                        Self::require_close_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::History,
                            clock.now(),
                        )
                        .await?;
                        let data = read_period_close(c, &command_actor(current)?, &id).await?;
                        Ok(FinancePeriodCloseWorkspaceResponse {
                            data,
                            context: rendered(current)?,
                        })
                    })
                },
            )
            .await
    }

    pub async fn begin_period_close<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
    ) -> Result<FinancePeriodCloseResponse, Error>
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
                    destination: "finance/close".into(),
                    api: true,
                },
                PeriodCloseSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let id = id.clone();
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
                                "Use a Company Finance assignment for period close.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_close_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::SettleExisting,
                            now,
                        )
                        .await?;
                        let data = begin_period_close(
                            c,
                            &command_actor(current)?,
                            &command,
                            &id,
                            now.get(),
                        )
                        .await?;
                        Ok(FinancePeriodCloseResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn cancel_period_close<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
        body: PeriodCloseCancelInput,
    ) -> Result<FinancePeriodCloseResponse, Error>
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
                    destination: "finance/close".into(),
                    api: true,
                },
                PeriodCloseSources,
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
                                "Use a Company Finance assignment for period close.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_close_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::SettleExisting,
                            now,
                        )
                        .await?;
                        let can_approve = current.has_permission("finance.approve");
                        let data = cancel_period_close(
                            c,
                            &command_actor(current)?,
                            &command,
                            &id,
                            &body,
                            can_approve,
                            now.get(),
                        )
                        .await?;
                        Ok(FinancePeriodCloseResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn close_period<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
        body: PeriodCloseProofInput,
    ) -> Result<FinancePeriodCloseResponse, Error>
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
                    destination: "finance/close".into(),
                    api: true,
                },
                PeriodCloseSources,
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
                        current.require_permission("finance.approve")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for period close.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_close_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::SettleExisting,
                            now,
                        )
                        .await?;
                        let actor = command_actor(current)?;
                        let mut evidence = FinancePeriodCloseEvidence {
                            company: configuration.company.clone(),
                            actor: actor.user_id.clone(),
                            now,
                        };
                        let data = close_period_with_evidence(
                            c,
                            &actor,
                            &command,
                            &id,
                            &body,
                            true,
                            now.get(),
                            &mut evidence,
                        )
                        .await?;
                        Ok(FinancePeriodCloseResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn reopen_period<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CommandKey,
        id: String,
        body: PeriodCloseProofInput,
    ) -> Result<FinancePeriodCloseResponse, Error>
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
                    destination: "finance/close".into(),
                    api: true,
                },
                PeriodCloseSources,
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
                        current.require_permission("finance.approve")?;
                        current.require_permission("module.finance.read")?;
                        let workspace = current.selection().workspace();
                        if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "COMPANY_SCOPE_REQUIRED",
                                "Use a Company Finance assignment for period close.",
                            )
                            .into());
                        }
                        let now = clock.now();
                        Self::require_close_purchase(
                            c,
                            current,
                            &configuration,
                            CapabilityPurpose::SettleExisting,
                            now,
                        )
                        .await?;
                        let data = reopen_period(
                            c,
                            &command_actor(current)?,
                            &command,
                            &id,
                            &body,
                            true,
                            now.get(),
                        )
                        .await?;
                        Ok(FinancePeriodCloseResponse { data })
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
                        let data =
                            prepare_sales_defaults(c, &actor, &command, &body, now.get()).await?;
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
                            decide_sales_defaults(c, &actor, &command, &id, &body, now.get())
                                .await?;
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
                        let data =
                            issue_customer_receipt(c, &actor, &command, &payment_id, now.get())
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
            .read_kind(host, proof, input, id, ReadKind::Detail, false)
            .await?
        {
            ReadResponse::Detail(reply) => Ok(reply),
            _ => Err(scope().into()),
        }
    }
    /// Generic Field Finance History detail. This route is a real product read
    /// for journals whose owner lineage is the joined Field source, including
    /// payment/reversal journals that the manual/opening detail reader rejects.
    pub async fn read_field_history<Input>(
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
            .read_kind(host, proof, input, id, ReadKind::Detail, true)
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
            .read_kind(host, proof, input, id, ReadKind::Workspace, false)
            .await?
        {
            ReadResponse::Workspace(reply) => Ok(reply),
            _ => Err(scope().into()),
        }
    }
    /// Generic Field Finance History workspace. Retains the same bounded
    /// Finance references and joined Field decision as the detail route.
    pub async fn read_field_history_workspace<Input>(
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
            .read_kind(host, proof, input, id, ReadKind::Workspace, true)
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
        generic_only: bool,
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
            field_service_parts: configuration.field_service_parts && !generic_only,
            field_service_invoices: configuration.field_service_invoices && !generic_only,
            field_finance_history: generic_only,
            generic_only,
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
                        let source_context=crate::field_service_source_record::SourceRecordContext{purchase_locks:None,actor:current,holding:&configuration.holding,company:&configuration.company,catalogue:&configuration.purchases,clock:clock.as_ref()};
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
                        // Generic Field History is a distinct product read. It
                        // proves the joined Field source before any payload is
                        // exposed and never turns a request label into admission.
                        let generic_source = if generic_only {
                            let read_kind = match kind {
                                ReadKind::Detail => crate::finance_generic_history::GenericHistoryReadKind::Detail,
                                ReadKind::Workspace => crate::finance_generic_history::GenericHistoryReadKind::Workspace,
                            };
                            crate::finance_generic_history::read_generic_field_finance_history(
                                c,
                                &source_context,
                                read_kind,
                                id.as_str(),
                            )
                            .await?
                        } else {
                            None
                        };
                        if generic_only && generic_source.is_none() {
                            return Err(scope().into());
                        }
                        let generic_admission = generic_source.is_some();
                        let generic_purchase = if generic_admission
                            && base_access.require_allowed().is_err()
                        {
                            let included = PgCompanyPurchaseReader::new(
                                configuration.holding.clone(),
                                configuration.company.clone(),
                                c,
                                &configuration.purchases,
                            )
                            .inspect(
                                &PurchaseRequest {
                                    capabilities: vec!["finance.service-invoice".into()],
                                    purpose: CapabilityPurpose::History,
                                    mode: PackageEnforcementMode::Enforced,
                                },
                                clock.now(),
                            )
                            .await?;
                            included.require_allowed()?;
                            Some(included.witness)
                        } else {
                            None
                        };
                        if !invoice_admission && !generic_admission && parts_purchase.is_none(){base_access.require_allowed()?;}
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
                        } else if let Some(generic) = generic_source {
                            match kind {
                                ReadKind::Detail => ReadSource::GenericFieldDetail(Box::new(generic)),
                                ReadKind::Workspace => ReadSource::GenericFieldWorkspace(Box::new(generic)),
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
                        if !invoice_admission && !generic_admission && parts_purchase.is_none(){base.require_allowed()?;}
                        if let Some(witness)=parts_purchase {
                            PgCompanyPurchaseReader::new(configuration.holding.clone(),configuration.company.clone(),c,&configuration.purchases).revalidate(&witness,clock.now()).await?.require_allowed()?;
                        }
                        if let Some(witness)=generic_purchase {
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
