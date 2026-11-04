//! Native Finance generic controlled-payment workspace read. Current IAM and
//! Commercial purchase guards are resolved here; payment source facts remain in
//! the Finance owner adapter. The Field-specific read retains its own proof path.
use crate::finance_journal_review::finance_actor;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
};
use erpx_server_finance::public::service_receipt::{
    service_payment_routing_facts,
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::record_origin::{
    read_finance_receipt_creation_origin, read_service_receipt_bank_origins,
};
use sqlx::PgConnection;
use std::collections::BTreeMap;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PAYMENT_SOURCE_CHANGED",
        "The controlled payment's original invoice changed. Refresh its source workflow.",
    )
}
async fn creation_invoice(
    c: &mut PgConnection,
    company: &str,
    id: &str,
    creator: &str,
) -> Result<String, Error> {
    let company_id = erpx_server_shared::public::CompanyId::new(company).map_err(|_| changed())?;
    let (_, result) = read_finance_receipt_creation_origin(c, &company_id, id, creator)
        .await?
        .ok_or_else(changed)?;
    let invoice = result
        .property("settlementInvoiceId")
        .map_err(|_| changed())?
        .ok_or_else(changed)?;
    let units = invoice.string_utf16_units()?.ok_or_else(changed)?;
    let invoice = String::from_utf16(&units).map_err(|_| changed())?;
    if invoice.is_empty() {
        return Err(changed().into());
    }
    Ok(invoice)
}
/// True only for a retained Field Service release. The read does not grant
/// Field authority; the Field-specific composition re-proves its own source.
pub async fn payment_is_field(
    c: &mut PgConnection,
    company: &str,
    id: &str,
) -> Result<bool, Error> {
    let creator: Option<String> =
        sqlx::query_scalar("SELECT created_by FROM finance_payments WHERE company_id=$1 AND id=$2")
            .bind(company)
            .bind(id)
            .fetch_optional(&mut *c)
            .await?;
    let Some(creator) = creator else {
        return Ok(false);
    };
    let invoice = creation_invoice(c, company, id, &creator).await?;
    let source: Option<String> = sqlx::query_scalar(
        "SELECT source_type FROM finance_invoices WHERE id=$1 AND company_id=$2",
    )
    .bind(invoice)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    Ok(source.as_deref() == Some("field_service"))
}
/// Whether a persisted native command receipt names a supported open generic
/// invoice. Absence of a receipt or an unsupported source remains legacy.
pub async fn generic_payment_supported(
    c: &mut PgConnection,
    company: &str,
    id: &str,
    creator: &str,
    kind: &str,
) -> Result<bool, Error> {
    let invoice = match creation_invoice(c, company, id, creator).await {
        Ok(invoice) => invoice,
        Err(_) => return Ok(false),
    };
    let expected = if kind == "receipt" {
        "receivable"
    } else if kind == "payment" {
        "payable"
    } else {
        return Ok(false);
    };
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=$1 AND i.company_id=$2 \
         AND i.kind=$3 AND i.status IN ('posted','partially_paid') \
         AND i.source_type IS DISTINCT FROM 'expense_claim')",
    )
    .bind(invoice)
    .bind(company)
    .bind(expected)
    .fetch_one(&mut *c)
    .await?)
}
pub async fn read_finance_generic_payment_workspace<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    config: &crate::finance_journal_create::FinanceJournalCreateConfiguration,
    clock: &C,
    id: &str,
) -> Result<
    (
        serde_json::Value,
        bool,
        BTreeMap<String, crate::finance_presentation::FinancePurchaseAccess>,
    ),
    Error,
> {
    require_selected_actor_current(c, current, clock.now().get()).await?;
    current.require_permission("module.finance.read")?;
    current.require_permission("finance.read")?;
    if current.holding_id() != &config.holding
        || current.selection().workspace().company_id != config.company
    {
        return Err(changed().into());
    }
    let facts = service_payment_routing_facts(c, &finance_actor(current), id).await?;
    let invoice = creation_invoice(c, config.company.as_str(), id, &facts.creator).await?;
    let bank = read_service_receipt_bank_origins(c, &config.company, id).await?;
    let mut reader = PgCompanyPurchaseReader::new(
        config.holding.clone(),
        config.company.clone(),
        c,
        &config.catalogue,
    );
    let base = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["finance.banking".into()],
                purpose: CapabilityPurpose::SettleExisting,
                mode: PackageEnforcementMode::Enforced,
            },
            clock.now(),
        )
        .await?;
    let history = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["finance.banking".into()],
                purpose: CapabilityPurpose::History,
                mode: PackageEnforcementMode::Enforced,
            },
            clock.now(),
        )
        .await?;
    let base_allowed = base.require_allowed().is_ok();
    let history_allowed = history.require_allowed().is_ok();
    let company_scope = current.selection().workspace().branch_id.is_none()
        && current.selection().workspace().department_id.is_none();
    let can_confirm = company_scope && current.has_permission("finance.pay") && base_allowed;
    let workspace = erpx_server_finance::public::service_receipt::read_generic_payment_workspace(
        c,
        &finance_actor(current),
        id,
        &invoice,
        can_confirm,
    )
    .await?;
    bank.require_current(c).await?;
    workspace.require_current(c).await?;
    let mut reread = PgCompanyPurchaseReader::new(
        config.holding.clone(),
        config.company.clone(),
        c,
        &config.catalogue,
    );
    for (source, expected) in [(&base, base_allowed), (&history, history_allowed)] {
        let after = reread.revalidate(&source.witness, clock.now()).await?;
        if after.require_allowed().is_ok() != expected || after.revision != source.revision {
            return Err(changed().into());
        }
    }
    let mut data = workspace.data().clone();
    data.as_object_mut().ok_or_else(changed)?.insert(
        "sourcePurchase".into(),
        serde_json::json!({
            "capability":"finance.banking",
            "settleExisting":base_allowed,
            "history":history_allowed,
            "revision":history.revision
        }),
    );
    let presentation = crate::finance_presentation::read_finance_presentation(
        c,
        &config.holding,
        &config.company,
        &config.catalogue,
        clock,
    )
    .await?;
    presentation
        .require_current(
            c,
            &config.holding,
            &config.company,
            &config.catalogue,
            clock,
        )
        .await?;
    Ok((data, !history_allowed, presentation.values().clone()))
}
