//! Controlled Field receipt creation on the selected native Holding unit.
use crate::{
    finance_journal_create::{FinanceJournalCreateConfiguration, Ids},
    finance_journal_review::{command_actor, finance_actor},
    finance_service_invoice_post::{
        Sources, generic_payment_authority, service_receipt_create_authority,
    },
};
use erpx_server_finance::public::{
    generic_payment::{GenericPaymentInput, PreparedGenericPayment, RetainedGenericPayment},
    service_receipt::*,
    units_to_decimal,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    selected_command_authority::require_selected_actor_current,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::{
    commands::*, service_parts_posting::read_service_prerequisite_receipt,
};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
/// Logical REST input stays coupled to its physical owner DTO through retries.
#[derive(Clone)]
pub struct ServiceReceiptCreateWire {
    input: ServiceReceiptCreateInput,
    normalized: CommandJson,
}
impl ServiceReceiptCreateWire {
    pub fn input(&self) -> &ServiceReceiptCreateInput {
        &self.input
    }
    pub fn normalized(&self) -> &CommandJson {
        &self.normalized
    }
}
pub fn normalize_service_receipt_create(
    body: &[u8],
) -> Result<ServiceReceiptCreateWire, BusinessError> {
    let invalid = || BusinessError::invalid("VALIDATION_FAILED", "Invalid payment.");
    let body = String::from_utf8_lossy(body);
    let body = body.strip_prefix('\u{feff}').unwrap_or(&body);
    let doc = CommandJson::parse(body)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))?;
    if doc.object_keys()?.is_none() {
        return Err(invalid());
    }
    let mut members = Vec::new();
    let mut physical = serde_json::Map::new();
    for key in [
        "partyId",
        "kind",
        "paymentNumber",
        "paymentDate",
        "currency",
        "amount",
        "bankAccount",
        "settlementInvoiceId",
        "correctionPaymentId",
    ] {
        let Some(value) = doc.property(key)? else {
            if matches!(
                key,
                "partyId" | "bankAccount" | "settlementInvoiceId" | "correctionPaymentId"
            ) {
                continue;
            }
            return Err(invalid());
        };
        let units = value.string_utf16_units()?.ok_or_else(invalid)?;
        if (key == "currency" && units.len() != 3)
            || (matches!(key, "settlementInvoiceId" | "correctionPaymentId")
                && !(1..=128).contains(&units.len()))
        {
            return Err(invalid());
        }
        let text = value.string_utf8_storage()?.ok_or_else(invalid)?;
        if key == "kind" && !matches!(text.as_str(), "payment" | "receipt" | "refund") {
            return Err(invalid());
        }
        if key == "paymentDate" && !valid_service_receipt_date(&text) {
            return Err(invalid());
        }
        members.push(format!("\"{key}\":{}", value.as_str()));
        physical.insert(key.into(), serde_json::Value::String(text));
    }
    // Field receipt corrections are outside the currently implemented original
    // receipt flow. Never silently discard a supplied correction command.
    if physical.contains_key("correctionPaymentId") {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "SERVICE_PAYMENT_CORRECTION_UNAVAILABLE",
            "Field receipt correction is not available in this native command.",
        ));
    }
    let text = |key: &str| {
        physical
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let input = ServiceReceiptCreateInput {
        kind: text("kind"),
        party_id: text("partyId"),
        payment_number: text("paymentNumber"),
        payment_date: text("paymentDate"),
        currency: text("currency"),
        amount: text("amount"),
        bank_account: text("bankAccount"),
        settlement_invoice_id: text("settlementInvoiceId"),
    };
    Ok(ServiceReceiptCreateWire {
        input,
        normalized: CommandJson::parse(&format!("{{{}}}", members.join(",")))?,
    })
}
#[derive(Clone)]
pub(crate) struct ReceiptSources;
impl HoldingWorkSourcePreparation for ReceiptSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            lock_service_receipt_sources(c).await?;
            Sources.prepare(c).await
        })
    }
}
enum PreparedCreate {
    Field(PreparedServiceReceiptCreate),
    Generic(PreparedGenericPayment),
}
enum RetainedCreate {
    Field(RetainedServiceReceiptCreate),
    Generic(RetainedGenericPayment),
}
impl RetainedCreate {
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        match self {
            Self::Field(retained) => retained.require_current(c).await,
            Self::Generic(retained) => retained.require_current(c).await,
        }
    }
}
fn generic_input(input: &ServiceReceiptCreateInput) -> GenericPaymentInput {
    GenericPaymentInput {
        kind: input.kind.clone(),
        party_id: input.party_id.clone(),
        payment_number: input.payment_number.clone(),
        payment_date: input.payment_date.clone(),
        currency: input.currency.clone(),
        amount: input.amount.clone(),
        bank_account: input.bank_account.clone(),
        settlement_invoice_id: input.settlement_invoice_id.clone(),
    }
}
pub(crate) async fn field_invoice(
    c: &mut PgConnection,
    company: &erpx_server_shared::public::CompanyId,
    invoice: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE id=$1 AND company_id=$2 AND source_type='field_service')",
    )
    .bind(invoice)
    .bind(company.as_str())
    .fetch_one(&mut *c)
    .await?)
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: ServiceReceiptCreateInput,
    prepared: Option<PreparedCreate>,
    retained: Option<RetainedCreate>,
}
impl<C: Clock + Send + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        ReceiptSources.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let (prepared, result) = if field_invoice(
            c,
            &self.current.selection().workspace().company_id,
            &self.input.settlement_invoice_id,
        )
        .await?
        {
            let prepared = prepare_service_receipt_create(
                c,
                &finance_actor(self.current),
                &r.record_id,
                &self.input,
                self.clock,
            )
            .await?;
            let amount = units_to_decimal(&prepared.amount().into(), 2).map_err(|_| {
                BusinessError::invalid("MONEY_OVERFLOW", "Money exceeds supported precision.")
            })?;
            (
                PreparedCreate::Field(prepared),
                serde_json::json!({"id":r.record_id,"amount":amount,"outstanding":amount,"status":"draft","version":1,"settlementInvoiceId":self.input.settlement_invoice_id}),
            )
        } else {
            let prepared = erpx_server_finance::public::generic_payment::prepare_generic_payment(
                c,
                &finance_actor(self.current),
                &r.record_id,
                &generic_input(&self.input),
                self.clock,
            )
            .await?;
            let result = prepared.result()?;
            (PreparedCreate::Generic(prepared), result)
        };
        let result = CommandJson::from_serializable(&result)?;
        self.prepared = Some(prepared);
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
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let retained = match self.prepared.take().ok_or_else(unavailable)? {
            PreparedCreate::Field(plan) => RetainedCreate::Field(plan.persist(c).await?),
            PreparedCreate::Generic(plan) => RetainedCreate::Generic(plan.persist(c).await?),
        };
        self.retained = Some(retained);
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.retained
            .as_ref()
            .ok_or_else(unavailable)?
            .require_current(c)
            .await
    }
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SERVICE_RECEIPT_UNAVAILABLE",
        "The controlled receipt could not be prepared.",
    )
}
/// Typed internal entry only: HTTP raw normalization/mounting remains separate.
/// Receipt identity is retained across native transaction retries and exact replay.
pub async fn execute_service_receipt_create<C, F, P>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    selection: F,
    config: FinanceJournalCreateConfiguration,
    clock: Arc<C>,
    prepare: P,
) -> Result<CommandJson, Error>
where
    C: Clock + Send + Sync + 'static,
    F: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    P: Fn() -> Result<(String, ServiceReceiptCreateWire), BusinessError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let company = config.company.clone();
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            move || {
                let mut scope = selection()?;
                if scope.company_id != company {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "WORK_CONTEXT_CHANGED",
                        "Choose the current Company.",
                    ));
                }
                scope.mutation = true;
                Ok(scope)
            },
            WorkAuthorizationRequest {
                permission: "finance.write".into(),
                module: Some("finance".into()),
                destination: "finance/payments".into(),
                api: true,
            },
            ReceiptSources,
            move |c, current| {
                let config = config.clone();
                let clock = clock.clone();
                let prepare = prepare.clone();
                Box::pin(async move {
                    require_selected_actor_current(c, current, clock.now().get()).await?;
                    let (key, wire) = prepare()?;
                    let input = wire.input;
                    validate_key(&key)?;
                    let receipt = read_service_prerequisite_receipt(c, &key).await?;
                    if receipt.facts().is_some_and(|r| {
                        r.operation != "finance.payment.create"
                            || r.actor_id != current.user_id().as_str()
                            || r.company_id.as_deref() != Some(config.company.as_str())
                    }) {
                        return Err(BusinessError::new(
                            FailureKind::Conflict,
                            "IDEMPOTENCY_CONFLICT",
                            "This key belongs to a different original request.",
                        )
                        .into());
                    }
                    let id = receipt
                        .facts()
                        .map(|r| r.record_id.clone())
                        .unwrap_or_else(|| format!("pay-{}", uuid::Uuid::new_v4()));
                    let field = field_invoice(c, &config.company, &input.settlement_invoice_id).await?;
                    let mut authority = if field {
                        service_receipt_create_authority(
                            current,
                            &config,
                            clock.as_ref(),
                            input.settlement_invoice_id.clone(),
                        )
                    } else {
                        generic_payment_authority(
                            current,
                            &config,
                            clock.as_ref(),
                            input.settlement_invoice_id.clone(),
                            None,
                            crate::finance_service_invoice_post::ReceiptAction::Create,
                        )
                    };
                    let request = WorkflowRequest {
                        key,
                        operation: "finance.payment.create".into(),
                        record_id: id,
                        input: wire.normalized,
                    };
                    let mut effects = Effects {
                        current,
                        clock: clock.as_ref(),
                        input,
                        prepared: None,
                        retained: None,
                    };
                    let mut ids = Ids;
                    let result = PgCommandLedger::new(
                        config.company.clone(),
                        c,
                        &mut authority,
                        clock.as_ref(),
                        &mut ids,
                    )
                    .execute(&request, &mut effects)
                    .await?;
                    if !result.replayed {
                        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                            .execute(&mut *c)
                            .await?;
                        effects
                            .retain(c, &command_actor(current)?, &request)
                            .await?;
                        authority
                            .verify_command(
                                c,
                                &command_actor(current)?,
                                &request,
                                CommandDisposition::Execute,
                            )
                            .await?;
                    }
                    Ok(result.result)
                })
            },
        )
        .await
}
