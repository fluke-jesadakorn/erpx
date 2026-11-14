//! Native Finance supplier-batch/instruction command mount.
//!
//! The composition resolves the current IAM actor, opens the retained Company
//! purchase boundary, takes the ordered Finance source leases and calls the
//! native owner command inside the same serializable Holding transaction. Files
//! evidence stays with the shared Records/Files port; supplier-batch staff
//! notifications are staged for the current Company Finance roles.
use crate::finance_journal_detail::FinanceJournalReadConfiguration;
use erpx_server_commercial::public::purchase_command_boundary::{
    PurchaseCommandBoundary, PurchaseCommandScope,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
};
use erpx_server_files::public::business_evidence::{
    PgBusinessEvidenceFiles, evidence_context, lock_business_evidence_sources, policy_fact,
};
use erpx_server_finance::public::sales_invoice_create::SalesInvoiceCommandActor;
use erpx_server_finance::public::supplier_batch::{
    PgSupplierBatchNotifications, PgSupplierInstructionEvidence, SUPPLIER_BATCH_EVIDENCE_COMPONENT_KIND,
    SUPPLIER_BATCH_EVIDENCE_PURPOSE_KEY, SUPPLIER_BATCH_EVIDENCE_TARGET_TYPE,
    SupplierBatchCreateInput, SupplierBatchDecisionInput, SupplierBatchExportInput,
    SupplierBatchExportResult, SupplierBatchResult, SupplierInstructionConfirmInput,
    SupplierInstructionConfirmResult, SupplierInstructionEvidenceRequest, SupplierInstructionResult,
    SupplierInstructionResultInput, confirm_supplier_instruction, create_supplier_batch,
    decide_supplier_batch, export_supplier_batch, lock_supplier_batch_read_sources,
    lock_supplier_batch_sources, recover_supplier_export, record_supplier_instruction_result,
};
use erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation;
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

const BASE_CAPABILITY: &str = "finance.banking";
const SUPPLY_CAPABILITY: &str = "finance.inventory-valuation";

/// The authorizer resolves the Holding session and selected assignment before
/// this mount takes its own Company/Finance fences; no owner table is read here.
#[derive(Clone)]
pub(crate) struct SupplierBatchSources;

impl HoldingWorkSourcePreparation for SupplierBatchSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_SUPPLIER_BATCH_UNAVAILABLE",
        "The supplier batch command is unavailable.",
    )
}

/// `ERPX_FINANCE_PACKAGE_ENFORCEMENT` mirrors the retained development rollout
/// switch; invalid values fail closed and enforcement stays local-only.
pub(crate) fn supplier_batch_purchase_mode() -> Result<PackageEnforcementMode, BusinessError> {
    let raw = std::env::var("ERPX_FINANCE_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned());
    let mode = match raw.as_str() {
        "disabled" => PackageEnforcementMode::Disabled,
        "shadow" => PackageEnforcementMode::Shadow,
        "enforced" => PackageEnforcementMode::Enforced,
        _ => return Err(unavailable()),
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

/// Whether this Company holds only the included Supply settlement capability.
/// A full Banking buyer keeps its normal owner path; the retained access codes
/// are read from the same Commercial catalogue the boundary re-decides.
async fn purchase_facts(
    c: &mut PgConnection,
    configuration: &FinanceJournalReadConfiguration,
    mode: PackageEnforcementMode,
    now: TimestampMillis,
) -> Result<(String, bool), Error> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok((BASE_CAPABILITY.to_owned(), false));
    }
    let mut reader = PgCompanyPurchaseReader::new(
        configuration.holding().clone(),
        configuration.company_id().clone(),
        c,
        configuration.purchases(),
    );
    let banking = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec![BASE_CAPABILITY.into()],
                purpose: CapabilityPurpose::History,
                mode,
            },
            now,
        )
        .await?;
    let banking_history = banking
        .access
        .first()
        .map(|access| access.decision.allowed)
        .unwrap_or(false);
    if banking_history {
        return Ok((BASE_CAPABILITY.to_owned(), false));
    }
    let supply = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec![SUPPLY_CAPABILITY.into()],
                purpose: CapabilityPurpose::SettleExisting,
                mode,
            },
            now,
        )
        .await?;
    let supply_settlement = supply
        .access
        .first()
        .map(|access| access.decision.allowed)
        .unwrap_or(false);
    Ok((
        if supply_settlement {
            SUPPLY_CAPABILITY.to_owned()
        } else {
            BASE_CAPABILITY.to_owned()
        },
        true,
    ))
}

/// A replay is admitted under `history`, matching the retired TypeScript
/// purpose switch on the retained command receipt.
async fn command_purpose(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    operation: &str,
    record_id: &str,
    default: CapabilityPurpose,
) -> Result<CapabilityPurpose, Error> {
    let replayed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation=$2 \
         AND record_id=$3 AND actor_id=$4 AND authority_json::jsonb->>'companyId'=$5)",
    )
    .bind(command.as_str())
    .bind(operation)
    .bind(record_id)
    .bind(&actor.user_id)
    .bind(&actor.company_id)
    .fetch_one(&mut *c)
    .await?;
    Ok(if replayed {
        CapabilityPurpose::History
    } else {
        default
    })
}

struct Boundary {
    holding: HoldingId,
    boundary: Option<PurchaseCommandBoundary>,
}

impl Boundary {
    async fn complete(mut self, c: &mut PgConnection, configuration: &FinanceJournalReadConfiguration) -> Result<(), Error> {
        if let Some(boundary) = self.boundary.as_mut() {
            boundary
                .complete(c, &self.holding, configuration.purchases())
                .await?;
        }
        Ok(())
    }
}

/// Begin the package boundary before any Finance source lease. A denial keeps
/// the same owner error mapping the retired TypeScript wrapper returned.
async fn begin(
    c: &mut PgConnection,
    configuration: &FinanceJournalReadConfiguration,
    mode: PackageEnforcementMode,
    key: String,
    capability: &str,
    purpose: CapabilityPurpose,
) -> Result<Boundary, Error> {
    let mut boundary = PurchaseCommandBoundary::new(
        mode,
        key,
        PurchaseCommandScope {
            company_id: configuration.company_id().clone(),
            capability: capability.to_owned(),
            purpose,
        },
    )?;
    if let Some(active) = boundary.as_mut() {
        active
            .begin(c, configuration.holding(), configuration.purchases())
            .await?;
    }
    Ok(Boundary {
        holding: configuration.holding().clone(),
        boundary,
    })
}

pub(crate) async fn create(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    configuration: &FinanceJournalReadConfiguration,
    clock: &impl Clock,
    command: &CommandKey,
    input: &SupplierBatchCreateInput,
    now: i64,
) -> Result<SupplierBatchResult, Error> {
    let mode = supplier_batch_purchase_mode()?;
    let (capability, source_only) = purchase_facts(c, configuration, mode, clock.now()).await?;
    let purpose = command_purpose(
        c,
        actor,
        command,
        "finance.supplier_batch.create",
        &format!("supplier-batch-{}", command.as_str()),
        CapabilityPurpose::SettleExisting,
    )
    .await?;
    let boundary = begin(
        c,
        configuration,
        mode,
        format!("finance-supplier-batch:{}", command.as_str()),
        &capability,
        purpose,
    )
    .await?;
    lock_supplier_batch_sources(c).await?;
    let mut notifications = FinanceSupplierBatchNotifications;
    let result = create_supplier_batch(
        c,
        actor,
        command,
        input,
        source_only,
        &mut notifications,
        now,
    )
    .await?;
    boundary.complete(c, configuration).await?;
    Ok(result)
}

pub(crate) async fn decide(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    configuration: &FinanceJournalReadConfiguration,
    clock: &impl Clock,
    command: &CommandKey,
    id: &str,
    input: &SupplierBatchDecisionInput,
    decision: &str,
    can_approve: bool,
    now: i64,
) -> Result<SupplierBatchResult, Error> {
    let mode = supplier_batch_purchase_mode()?;
    let (capability, _) = purchase_facts(c, configuration, mode, clock.now()).await?;
    let operation = format!("finance.supplier_batch.{decision}");
    let purpose = command_purpose(
        c,
        actor,
        command,
        &operation,
        id,
        CapabilityPurpose::SettleExisting,
    )
    .await?;
    let boundary = begin(
        c,
        configuration,
        mode,
        format!("finance-supplier-batch:{}", command.as_str()),
        &capability,
        purpose,
    )
    .await?;
    lock_supplier_batch_sources(c).await?;
    let mut notifications = FinanceSupplierBatchNotifications;
    let result = decide_supplier_batch(
        c,
        actor,
        command,
        id,
        input,
        decision,
        can_approve,
        &mut notifications,
        now,
    )
    .await?;
    boundary.complete(c, configuration).await?;
    Ok(result)
}

pub(crate) async fn export(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    configuration: &FinanceJournalReadConfiguration,
    clock: &impl Clock,
    command: &CommandKey,
    id: &str,
    input: &SupplierBatchExportInput,
    now: i64,
) -> Result<SupplierBatchExportResult, Error> {
    let mode = supplier_batch_purchase_mode()?;
    let (capability, _) = purchase_facts(c, configuration, mode, clock.now()).await?;
    let purpose = command_purpose(
        c,
        actor,
        command,
        "finance.supplier_batch.export",
        id,
        CapabilityPurpose::SettleExisting,
    )
    .await?;
    let boundary = begin(
        c,
        configuration,
        mode,
        format!("finance-supplier-batch:{}", command.as_str()),
        &capability,
        purpose,
    )
    .await?;
    lock_supplier_batch_sources(c).await?;
    let mut notifications = FinanceSupplierBatchNotifications;
    let result =
        export_supplier_batch(c, actor, command, id, input, &mut notifications, now).await?;
    boundary.complete(c, configuration).await?;
    Ok(result)
}

pub(crate) async fn recover_export(
    c: &mut PgConnection,
    configuration: &FinanceJournalReadConfiguration,
    id: &str,
) -> Result<SupplierBatchExportResult, Error> {
    lock_supplier_batch_read_sources(c).await?;
    recover_supplier_export(c, configuration.company_id().as_str(), id).await
}

pub(crate) async fn record_result(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    configuration: &FinanceJournalReadConfiguration,
    clock: &impl Clock,
    command: &CommandKey,
    id: &str,
    input: &SupplierInstructionResultInput,
    can_approve: bool,
    now: i64,
) -> Result<SupplierInstructionResult, Error> {
    let mode = supplier_batch_purchase_mode()?;
    let (capability, _) = purchase_facts(c, configuration, mode, clock.now()).await?;
    let purpose = command_purpose(
        c,
        actor,
        command,
        "finance.supplier_batch.result",
        id,
        CapabilityPurpose::SettleExisting,
    )
    .await?;
    let boundary = begin(
        c,
        configuration,
        mode,
        format!("finance-supplier-batch:{}", command.as_str()),
        &capability,
        purpose,
    )
    .await?;
    lock_supplier_batch_sources(c).await?;
    let mut notifications = FinanceSupplierBatchNotifications;
    let result = record_supplier_instruction_result(
        c,
        actor,
        command,
        id,
        input,
        can_approve,
        &mut notifications,
        now,
    )
    .await?;
    boundary.complete(c, configuration).await?;
    Ok(result)
}

pub(crate) async fn confirm(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    configuration: &FinanceJournalReadConfiguration,
    clock: &impl Clock,
    command: &CommandKey,
    id: &str,
    input: &SupplierInstructionConfirmInput,
    now: i64,
) -> Result<SupplierInstructionConfirmResult, Error> {
    let mode = supplier_batch_purchase_mode()?;
    let (capability, _) = purchase_facts(c, configuration, mode, clock.now()).await?;
    let purpose = command_purpose(
        c,
        actor,
        command,
        "finance.supplier_batch.confirm",
        id,
        CapabilityPurpose::SettleExisting,
    )
    .await?;
    let boundary = begin(
        c,
        configuration,
        mode,
        format!("finance-supplier-batch:{}", command.as_str()),
        &capability,
        purpose,
    )
    .await?;
    lock_supplier_batch_sources(c).await?;
    // Files owner fence before the first evidence read; the retained order for
    // this unit is Commercial -> Finance -> Files.
    lock_business_evidence_sources(c).await?;
    let mut evidence = FinanceSupplierInstructionEvidence {
        company: configuration.company_id().clone(),
        actor: actor.user_id.clone(),
        now: clock.now(),
    };
    let mut notifications = FinanceSupplierBatchNotifications;
    let result = confirm_supplier_instruction(
        c,
        actor,
        command,
        id,
        input,
        &mut evidence,
        &mut notifications,
        now,
    )
    .await?;
    boundary.complete(c, configuration).await?;
    Ok(result)
}

/// Shared Records/Files evidence for the exact supplier-payment confirmation.
pub(crate) struct FinanceSupplierInstructionEvidence {
    company: CompanyId,
    actor: String,
    now: TimestampMillis,
}

impl FinanceSupplierInstructionEvidence {
    fn files<'c>(&self, c: &'c mut PgConnection) -> PgBusinessEvidenceFiles<'c> {
        PgBusinessEvidenceFiles::new(
            c,
            self.company.clone(),
            self.actor.clone(),
            None,
            None,
            vec![policy_fact(
                SUPPLIER_BATCH_EVIDENCE_TARGET_TYPE,
                SUPPLIER_BATCH_EVIDENCE_PURPOSE_KEY,
                &[SUPPLIER_BATCH_EVIDENCE_COMPONENT_KIND],
                1,
            )],
            self.now,
        )
    }
}

impl PgSupplierInstructionEvidence for FinanceSupplierInstructionEvidence {
    type Prepared = erpx_server_files::public::business_evidence::PreparedBusinessEvidence;
    fn prepare(
        &mut self,
        c: &mut PgConnection,
        request: SupplierInstructionEvidenceRequest,
    ) -> impl std::future::Future<Output = Result<Self::Prepared, Error>> + Send {
        Box::pin(async move {
            let mut files = self.files(c);
            prepare_business_evidence(
                &mut files,
                BusinessEvidenceRequest {
                    attachment_set_ids: request.set_ids,
                    target_types: vec![SUPPLIER_BATCH_EVIDENCE_TARGET_TYPE.into()],
                    contexts: vec![evidence_context(
                        SUPPLIER_BATCH_EVIDENCE_PURPOSE_KEY,
                        SUPPLIER_BATCH_EVIDENCE_COMPONENT_KIND,
                        Some(&request.component_ref),
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

/// Collaboration-owned supplier-batch staff notification, staged for the
/// current holding-scope Finance roles from the retained source query.
pub(crate) struct FinanceSupplierBatchNotifications;

impl PgSupplierBatchNotifications for FinanceSupplierBatchNotifications {
    fn stage(
        &mut self,
        c: &mut PgConnection,
        command_id: &str,
        batch_id: &str,
        message: &str,
        actor_id: &str,
        now: i64,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        Box::pin(async move {
            if command_id.is_empty()
                || command_id.len() > 200
                || batch_id.is_empty()
                || message.is_empty()
                || message.len() > 1000
                || actor_id.is_empty()
            {
                return Err(unavailable().into());
            }
            sqlx::query(
                "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
                 record_version,message,created_at) \
                 SELECT DISTINCT $1||'-'||a.user_id,b.company_id,a.user_id,'supplier_batch',\
                 b.id,b.version,$2,$5 \
                 FROM finance_supplier_batches b \
                 JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\
                 \"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\
                 \"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" \
                 FROM assignments) a ON a.company_id=b.company_id \
                 WHERE b.id=$3 AND a.user_id<>$4 AND a.scope_kind='holding' \
                 AND a.role IN ('holding_owner','finance_approver','finance_preparer','treasury') \
                 AND a.revoked_at IS NULL AND a.valid_from<=$5 AND a.valid_until>$5 \
                 ON CONFLICT DO NOTHING",
            )
            .bind(command_id)
            .bind(message)
            .bind(batch_id)
            .bind(actor_id)
            .bind(now)
            .execute(&mut *c)
            .await?;
            Ok(())
        })
    }
}
