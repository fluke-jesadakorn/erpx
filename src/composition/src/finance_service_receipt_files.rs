//! Genuine Finance/IAM/Workflow source for the finite Files payment binding port.
use crate::finance_journal_review::finance_actor;
use erpx_server_files::public::payment_proof_binding::*;
use erpx_server_finance::public::service_receipt::{
    read_service_receipt_review, require_service_receipt_finance_lineage,
};
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    selected_command_authority::require_selected_actor_current,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, UserId};
use erpx_server_workflow::public::record_origin::{PgRawRecordOrigin, verify_raw_record_origin};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
pub(crate) struct PaymentProofOwner<'a, C> {
    pub facts: PaymentProofBindingFacts,
    pub current: &'a AuthorizedWorkContext,
    pub clock: &'a C,
    pub origin: &'a PgRawRecordOrigin,
    pub invoice: String,
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PAYMENT_SOURCE_CHANGED",
        "The controlled payment's original Service invoice changed. Refresh its source workflow.",
    )
}
impl<C: Clock + Send + Sync> PaymentProofOwner<'_, C> {
    async fn require(&self, c: &mut PgConnection, after: bool) -> Result<(), Error> {
        let f = &self.facts;
        let current = self.current;
        require_selected_actor_current(c, current, self.clock.now().get()).await?;
        current.require_permission("finance.pay")?;
        current.require_permission("module.finance.read")?;
        let scope = current.selection().workspace();
        if f.holding_id != *current.holding_id()
            || f.company_id != scope.company_id
            || f.actor_id != current.user_id().as_str()
            || scope.branch_id.is_some()
            || scope.department_id.is_some()
        {
            return Err(changed().into());
        }
        verify_raw_record_origin(c, self.origin).await?;
        require_service_receipt_finance_lineage(
            c,
            &finance_actor(current),
            &f.target_id,
            &self.invoice,
            false,
        )
        .await?;
        let source = read_service_receipt_review(c, &finance_actor(current), &f.target_id).await?;
        let row = source.row();
        let same = |key| -> Result<bool, BusinessError> {
            Ok(row[key]
                .as_str()
                .map(|u| UserId::new(u).map(|id| current.same_person_user(&id)))
                .transpose()?
                .unwrap_or(false))
        };
        if row["version"].as_i64() != f.target_version.checked_add(i64::from(after))
            || row["status"].as_str() != Some(if after { "settled" } else { "released" })
            || same("created_by")?
            || same("approved_by")?
            || !same("released_by")?
        {
            return Err(changed().into());
        }
        Ok(())
    }
}
impl<C: Clock + Send + Sync> PgPaymentProofOwner for PaymentProofOwner<'_, C> {
    fn facts(&self) -> &PaymentProofBindingFacts {
        &self.facts
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require(c, false).await
    }
    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require(c, true).await
    }
}

// Finance proof intake is a different compiled authority from Visit intake.
// It retains actual owner sources and current purchase/IAM proofs; it does not
// accept a request's target label as evidence of ownership.
use crate::{
    finance_journal_create::FinanceJournalCreateConfiguration,
    finance_service_receipt_create::ReceiptSources,
};
use erpx_server_commercial::public::purchases::*;
use erpx_server_field_service::public::finance_lineage::*;
use erpx_server_files::public::field_visit_intake::{
    FieldIntakeAuthority, FieldIntakeScope, IntakeOwner,
};
use erpx_server_finance::public::{
    field_service_invoice_read::{
        CurrentFinanceServiceInvoiceWorkspace, read_service_invoice_workspace,
    },
    service_receipt::CurrentServiceReceiptReview,
};
use erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation;
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_workflow::public::record_origin::read_finance_receipt_creation_origin;
use std::collections::BTreeMap;
use tokio::sync::Mutex;
#[derive(Clone)]
pub struct FinancePaymentIntakeSources;
impl HoldingWorkSourcePreparation for FinancePaymentIntakeSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            ReceiptSources.prepare(c).await?;
            erpx_server_files::public::field_visit_link::lock_field_visit_link_sources(c).await
        })
    }
}
struct PaymentIntakeProof {
    finance: CurrentServiceReceiptReview,
    invoice: CurrentFinanceServiceInvoiceWorkspace,
    lineage: CurrentFieldServiceInvoiceFinanceLineage,
    origin: PgRawRecordOrigin,
    base: PurchaseWitness,
    base_allowed: bool,
    included: Option<PurchaseWitness>,
}
impl PaymentIntakeProof {
    async fn require<C: Clock>(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        config: &FinanceJournalCreateConfiguration,
        clock: &C,
    ) -> Result<(), Error> {
        require_selected_actor_current(c, current, clock.now().get()).await?;
        self.finance.require_current(c).await?;
        self.invoice.require_current(c).await?;
        self.lineage.require_current(c).await?;
        verify_raw_record_origin(c, &self.origin).await?;
        let mut reader = PgCompanyPurchaseReader::new(
            config.holding.clone(),
            config.company.clone(),
            c,
            &config.catalogue,
        );
        if reader
            .revalidate(&self.base, clock.now())
            .await?
            .require_allowed()
            .is_ok()
            != self.base_allowed
        {
            return Err(changed().into());
        }
        if let Some(w) = &self.included {
            reader.revalidate(w, clock.now()).await?.require_allowed()?;
        }
        let id = self.finance.row()["id"].as_str().ok_or_else(changed)?;
        let invoice = self.invoice.data().invoice["id"]
            .as_str()
            .ok_or_else(changed)?;
        require_service_receipt_finance_lineage(c, &finance_actor(current), id, invoice, false)
            .await?;
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum EvidenceAccess {
    Read,
    Manage,
}
pub struct FinancePaymentIntakeAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    proofs: Mutex<BTreeMap<(String, bool), PaymentIntakeProof>>,
    access: EvidenceAccess,
}
impl<'a, C: Clock + Send + Sync> FinancePaymentIntakeAuthority<'a, C> {
    pub fn for_read(
        current: &'a AuthorizedWorkContext,
        config: &'a FinanceJournalCreateConfiguration,
        clock: &'a C,
    ) -> Self {
        Self {
            current,
            config,
            clock,
            proofs: Mutex::new(BTreeMap::new()),
            access: EvidenceAccess::Read,
        }
    }
    pub fn for_manage(
        current: &'a AuthorizedWorkContext,
        config: &'a FinanceJournalCreateConfiguration,
        clock: &'a C,
    ) -> Self {
        Self {
            current,
            config,
            clock,
            proofs: Mutex::new(BTreeMap::new()),
            access: EvidenceAccess::Manage,
        }
    }
    fn actor(&self, current: &AuthorizedWorkContext, write: bool) -> Result<(), BusinessError> {
        if !std::ptr::eq(self.current, current)
            || current.holding_id() != &self.config.holding
            || current.selection().workspace().company_id != self.config.company
            || self.clock.now().get() >= current.expires_at()
        {
            return Err(changed());
        }
        current.require_permission("module.finance.read")?;
        let manage = write || matches!(self.access, EvidenceAccess::Manage);
        if !current.has_permission(if manage {
            "finance.pay"
        } else {
            "finance.read"
        }) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ATTACHMENT_FORBIDDEN",
                "Your current assignment cannot access files for this record.",
            ));
        }
        if manage
            && (current.selection().workspace().branch_id.is_some()
                || current.selection().workspace().department_id.is_some())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ATTACHMENT_FORBIDDEN",
                "Use a Company Finance assignment to manage this evidence.",
            ));
        }
        Ok(())
    }
}
impl<C: Clock + Send + Sync> FieldIntakeAuthority for FinancePaymentIntakeAuthority<'_, C> {
    fn owner_kind(&self) -> IntakeOwner {
        IntakeOwner::FinancePayment
    }
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        current: &'a AuthorizedWorkContext,
        id: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, FieldIntakeScope, BusinessError> {
        Box::pin(async move {
            self.actor(current, write)?;
            require_selected_actor_current(c, current, self.clock.now().get()).await?;
            let actor = finance_actor(current);
            let finance = read_service_receipt_review(c, &actor, id).await?;
            let creator = finance.row()["created_by"].as_str().ok_or_else(changed)?;
            let (origin, result) =
                read_finance_receipt_creation_origin(c, &self.config.company, id, creator)
                    .await?
                    .ok_or_else(changed)?;
            let result: serde_json::Value =
                serde_json::from_str(result.as_str()).map_err(|_| changed())?;
            if result["id"] != id || result["status"] != "draft" || result["version"] != 1 {
                return Err(changed().into());
            }
            let invoice_id = result["settlementInvoiceId"].as_str().ok_or_else(changed)?;
            require_service_receipt_finance_lineage(c, &actor, id, invoice_id, false).await?;
            let invoice = read_service_invoice_workspace(c, &actor, invoice_id).await?;
            let row = &invoice.data().invoice;
            let src = &invoice.data().source;
            let text = |k| {
                row.get(k)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(changed)
            };
            let lineage = read_field_service_invoice_finance_lineage(
                c,
                FieldServiceInvoiceFinanceRequest {
                    company_id: self.config.company.clone(),
                    order_id: src.order_id.clone(),
                    release_id: src.release_id.clone(),
                    key: src.command_key.clone(),
                    actor_id: src.actor_id.clone(),
                    currency: src.currency.clone(),
                    amount_units: src.amount_units.clone(),
                    customer_id: text("party_id")?,
                    pricing_date: text("invoice_date")?,
                },
            )
            .await?
            .ok_or_else(changed)?;
            let purpose = if write || matches!(self.access, EvidenceAccess::Manage) {
                CapabilityPurpose::SettleExisting
            } else {
                CapabilityPurpose::History
            };
            let mut reader = PgCompanyPurchaseReader::new(
                self.config.holding.clone(),
                self.config.company.clone(),
                c,
                &self.config.catalogue,
            );
            let base = reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: vec!["finance.banking".into()],
                        purpose,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    self.clock.now(),
                )
                .await?;
            let base_allowed = base.require_allowed().is_ok();
            let included = if base_allowed {
                None
            } else {
                let included = reader
                    .inspect(
                        &PurchaseRequest {
                            capabilities: vec!["finance.service-invoice".into()],
                            purpose,
                            mode: PackageEnforcementMode::Enforced,
                        },
                        self.clock.now(),
                    )
                    .await?;
                included.require_allowed()?;
                Some(included.witness)
            };
            let version = finance.row()["version"]
                .as_i64()
                .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                .ok_or_else(changed)?;
            let proof = PaymentIntakeProof {
                finance,
                invoice,
                lineage,
                origin,
                base: base.witness,
                base_allowed,
                included,
            };
            proof.require(c, current, self.config, self.clock).await?;
            let mut proofs = self.proofs.lock().await;
            let key = (id.to_owned(), write);
            if proofs.contains_key(&key) {
                return Err(changed().into());
            }
            proofs.insert(key, proof);
            Ok(FieldIntakeScope {
                company_id: self.config.company.clone(),
                branch_id: None,
                department_id: None,
                version,
            })
        })
    }
    fn revalidate<'a>(
        &'a self,
        c: &'a mut PgConnection,
        current: &'a AuthorizedWorkContext,
        id: &'a str,
        write: bool,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.actor(current, write)?;
            let proofs = self.proofs.lock().await;
            let proof = proofs.get(&(id.to_owned(), write)).ok_or_else(changed)?;
            proof.require(c, current, self.config, self.clock).await
        })
    }
}

/// Native Field-origin payment readback using the same retained Finance,
/// Workflow, IAM and purchase sources already required by its evidence intake.
/// The caller supplies a current selected actor on its existing transaction.
pub async fn read_finance_service_receipt_workspace<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    config: &FinanceJournalCreateConfiguration,
    clock: &C,
    id: &str,
) -> Result<(serde_json::Value,bool,std::collections::BTreeMap<String,crate::finance_presentation::FinancePurchaseAccess>), Error> {
    let authority = FinancePaymentIntakeAuthority::for_read(current, config, clock);
    authority.authorize(c, current, id, false).await?;
    let proofs = authority.proofs.lock().await;
    let proof = proofs.get(&(id.into(), false)).ok_or_else(changed)?;
    let invoice = proof.invoice.data().invoice["id"]
        .as_str()
        .ok_or_else(changed)?;
    let bank = erpx_server_workflow::public::record_origin::read_service_receipt_bank_origins(
        c,
        &config.company,
        id,
    )
    .await?;
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
    let banking_history = reader.inspect(&PurchaseRequest {
        capabilities:vec!["finance.banking".into()],purpose:CapabilityPurpose::History,
        mode:PackageEnforcementMode::Enforced,
    },clock.now()).await?;
    let banking_history_allowed=banking_history.require_allowed().is_ok();
    let included = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["finance.service-invoice".into()],
                purpose: CapabilityPurpose::SettleExisting,
                mode: PackageEnforcementMode::Enforced,
            },
            clock.now(),
        )
        .await?;
    let history = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["finance.service-invoice".into()],
                purpose: CapabilityPurpose::History,
                mode: PackageEnforcementMode::Enforced,
            },
            clock.now(),
        )
        .await?;
    let base_allowed = base.require_allowed().is_ok();
    let included_allowed = included.require_allowed().is_ok();
    let history_allowed = history.require_allowed().is_ok();
    let company_scope = current.selection().workspace().branch_id.is_none()
        && current.selection().workspace().department_id.is_none();
    let can_confirm = company_scope
        && current.has_permission("finance.pay")
        && (base_allowed || included_allowed);
    let workspace = erpx_server_finance::public::service_receipt::read_service_receipt_workspace(
        c,
        &finance_actor(current),
        id,
        invoice,
        can_confirm,
        &bank.statement_ids(),
    )
    .await?;
    proof.require(c, current, config, clock).await?;
    bank.require_current(c).await?;
    workspace.require_current(c).await?;
    let mut reader = PgCompanyPurchaseReader::new(
        config.holding.clone(),
        config.company.clone(),
        c,
        &config.catalogue,
    );
    for (source, expected) in [
        (&base, base_allowed),
        (&banking_history, banking_history_allowed),
        (&included, included_allowed),
        (&history, history_allowed),
    ] {
        let after = reader.revalidate(&source.witness, clock.now()).await?;
        if after.require_allowed().is_ok() != expected || after.revision != source.revision {
            return Err(changed().into());
        }
    }
    let mut data = workspace.data().clone();
    data.as_object_mut().ok_or_else(changed)?.insert("sourcePurchase".into(),serde_json::json!({"capability":"finance.service-invoice","settleExisting":included_allowed,"history":history_allowed,"revision":history.revision}));
    let presentation=crate::finance_presentation::read_finance_presentation(c,&config.holding,&config.company,&config.catalogue,clock).await?;
    presentation.require_current(c,&config.holding,&config.company,&config.catalogue,clock).await?;
    Ok((data,!banking_history_allowed,presentation.values().clone()))
}

/// Persisted-source transport observation only; never a writer or a grant.
/// Unknown/corrupt receipts close instead of masquerading as non-Field Finance.
pub async fn finance_service_receipt_runtime<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    config: &FinanceJournalCreateConfiguration,
    clock: &C,
    id: &str,
) -> Result<serde_json::Value, Error> {
    require_selected_actor_current(c, current, clock.now().get()).await?;
    current.require_permission("module.finance.read")?;
    if current.holding_id() != &config.holding
        || current.selection().workspace().company_id != config.company
    {
        return Err(changed().into());
    }
    let facts = erpx_server_finance::public::service_receipt::service_payment_routing_facts(
        c,
        &finance_actor(current),
        id,
    )
    .await?;
    let origin =
        erpx_server_workflow::public::record_origin::observe_finance_payment_creation_origin(
            c,
            &config.company,
            id,
            &facts.creator,
        )
        .await?;
    let native = if let Some((origin, result)) = origin {
        let expected = id.encode_utf16().collect::<Vec<_>>();
        if result
            .property("id")?
            .and_then(|v| v.string_utf16_units().ok().flatten())
            .as_deref()
            != Some(expected.as_slice())
        {
            return Err(changed().into());
        }
        let invoice = result.property("settlementInvoiceId")?;
        let native = if let Some(invoice) = invoice {
            let units = invoice.string_utf16_units()?.ok_or_else(changed)?;
            let invoice = String::from_utf16(&units).map_err(|_| changed())?;
            if invoice.is_empty() {
                return Err(changed().into());
            }
            erpx_server_finance::public::service_invoice_posting::service_invoice_post_runtime(
                c,
                &finance_actor(current),
                &invoice,
            )
            .await?
        } else {
            false
        };
        verify_raw_record_origin(c, &origin).await?;
        native || facts.field_allocation_marker
    } else if facts.field_allocation_marker {
        true
    } else if facts.kind == "payment" {
        false
    } else {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PAYMENT_OWNER_UNKNOWN",
            "The payment's original owner could not be established.",
        )
        .into());
    };
    require_selected_actor_current(c, current, clock.now().get()).await?;
    Ok(serde_json::json!({"id":id,"runtime":if native{"rust"}else{"legacy"}}))
}

/// Issued customer receipt shares the retained native payment authority.
/// Runtime observation reveals only a persisted Company source classification.
pub async fn read_finance_service_customer_receipt<C:Clock+Send+Sync>(
 c:&mut PgConnection,current:&AuthorizedWorkContext,config:&FinanceJournalCreateConfiguration,clock:&C,id:&str,runtime:bool
)->Result<serde_json::Value,Error>{
 use erpx_server_finance::public::service_receipt::{customer_receipt_payment,read_customer_receipt};
 current.require_permission("finance.read")?;
 let payment=customer_receipt_payment(c,&finance_actor(current),id).await?;
 let owner=finance_service_receipt_runtime(c,current,config,clock,&payment).await?;
 if runtime{return Ok(serde_json::json!({"data":{"id":id,"runtime":owner["runtime"]}}));}
 if owner["runtime"]!="rust"{return Err(BusinessError::new(FailureKind::Unavailable,"RECEIPT_OWNER_CHANGED","The receipt is not a native Field receipt.").into());}
 let scope=current.selection().workspace();
 if scope.branch_id.is_some()||scope.department_id.is_some(){
  return Err(BusinessError::new(FailureKind::NotFound,"RECEIPT_NOT_FOUND","Receipt not found in this workspace.").into());
 }
 let authority=FinancePaymentIntakeAuthority::for_read(current,config,clock);
 authority.authorize(c,current,&payment,false).await?;
 let receipt=read_customer_receipt(c,&finance_actor(current),id).await?;
 if receipt.payment_id!=payment{return Err(changed().into());}
 receipt.require_current(c).await?;
 authority.revalidate(c,current,&payment,false).await?;
 Ok(serde_json::json!({"data":receipt.detail,"export":receipt.export,"context":crate::finance_journal_detail::rendered(current)?}))
}
