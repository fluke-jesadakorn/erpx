//! Native Field receipt confirmation; all effects remain on the selected unit.
use crate::finance_service_receipt_files::PaymentProofOwner;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    PurchaseWitness,
};
use crate::{
    finance_journal_create::{FinanceJournalCreateConfiguration, Ids},
    finance_journal_review::{command_actor, finance_actor},
    finance_service_receipt_create::{ReceiptSources, field_invoice},
};
use erpx_server_files::public::payment_proof_binding::*;
use erpx_server_finance::public::{service_receipt::*, *};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_organization::public::{
    CompanyContext, PgOrganizationReader, read_company_context,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, UserId};
use erpx_server_workflow::public::{
    commands::*,
    finance_receipt_decision::*,
    record_origin::{
        PgRawRecordOrigin, RawRecordOriginRequest, read_finance_receipt_creation_origin,
        read_raw_record_origin_result_with_limit, verify_raw_record_origin,
    },
};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
/// Validated confirmation couples the physical owner input to its original
/// logical UTF-16 request identity; callers cannot substitute either half.
#[derive(Clone)]
pub struct ServiceReceiptConfirmWire {
    input: ServiceReceiptConfirmInput,
    normalized: CommandJson,
}
impl ServiceReceiptConfirmWire {
    pub fn input(&self) -> &ServiceReceiptConfirmInput {
        &self.input
    }
    pub fn normalized(&self) -> &CommandJson {
        &self.normalized
    }
}
pub fn normalize_service_receipt_confirm(
    body: &[u8],
) -> Result<ServiceReceiptConfirmWire, BusinessError> {
    let invalid = || BusinessError::invalid("VALIDATION_FAILED", "Invalid payment confirmation.");
    let body = String::from_utf8_lossy(body);
    let body = body.strip_prefix('\u{feff}').unwrap_or(&body);
    let doc = CommandJson::parse(body)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))?;
    if doc.object_keys()?.is_none() {
        return Err(invalid());
    }
    let string = |raw: CommandJson, trim: bool| -> Result<(CommandJson, String), BusinessError> {
        let units = raw.string_utf16_units()?.ok_or_else(invalid)?;
        let units = if trim {
            let ws = |u: &u16| matches!(*u,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff);
            let start = units.iter().position(|u| !ws(u)).unwrap_or(units.len());
            let end = units.iter().rposition(|u| !ws(u)).map_or(start, |i| i + 1);
            units[start..end].to_vec()
        } else {
            units
        };
        Ok((
            CommandJson::from_utf16_string(&units)?,
            String::from_utf16_lossy(&units),
        ))
    };
    let mut members = Vec::new();
    let mut physical = serde_json::Map::new();
    for key in [
        "expectedVersion",
        "periodId",
        "bankTransactionId",
        "bankVersion",
        "bankReference",
        "proof",
        "fxRate",
        "fxGainAccountId",
        "fxLossAccountId",
    ] {
        let Some(raw) = doc.property(key)? else {
            if matches!(key, "fxRate" | "fxGainAccountId" | "fxLossAccountId") {
                continue;
            }
            return Err(invalid());
        };
        if matches!(key, "expectedVersion" | "bankVersion") {
            let value: serde_json::Value =
                serde_json::from_str(raw.as_str()).map_err(|_| invalid())?;
            let n = value
                .as_f64()
                .filter(|n| {
                    n.is_finite() && n.fract() == 0.0 && (1.0..=9_007_199_254_740_991.0).contains(n)
                })
                .ok_or_else(invalid)? as i64;
            members.push(format!("\"{key}\":{n}"));
            physical.insert(key.into(), serde_json::json!(n));
        } else {
            let (raw, text) = string(raw, matches!(key, "bankReference" | "proof"))?;
            members.push(format!("\"{key}\":{}", raw.as_str()));
            physical.insert(key.into(), serde_json::Value::String(text));
        }
    }
    let allocations = doc
        .property("allocations")?
        .ok_or_else(invalid)?
        .array_elements()?
        .ok_or_else(invalid)?;
    if allocations.is_empty() || allocations.len() > 100 {
        return Err(invalid());
    }
    let mut logical_rows = Vec::new();
    let mut rows = Vec::new();
    for row in allocations {
        if row.object_keys()?.is_none() {
            return Err(invalid());
        }
        let mut logical = Vec::new();
        let mut data = serde_json::Map::new();
        for key in ["invoiceId", "amount"] {
            let (raw, text) = string(row.property(key)?.ok_or_else(invalid)?, false)?;
            logical.push(format!("\"{key}\":{}", raw.as_str()));
            data.insert(key.into(), serde_json::Value::String(text));
        }
        logical_rows.push(format!("{{{}}}", logical.join(",")));
        rows.push(serde_json::Value::Object(data));
    }
    members.push(format!("\"allocations\":[{}]", logical_rows.join(",")));
    physical.insert("allocations".into(), serde_json::Value::Array(rows));
    let sets = match doc.property("attachmentSetIds")? {
        Some(raw) => raw.array_elements()?.ok_or_else(invalid)?,
        None => Vec::new(),
    };
    if sets.len() > 20 {
        return Err(invalid());
    }
    let mut logical = Vec::new();
    let mut data = Vec::new();
    for raw in sets {
        let (raw, text) = string(raw, false)?;
        logical.push(raw.as_str().to_owned());
        data.push(serde_json::Value::String(text));
    }
    members.push(format!("\"attachmentSetIds\":[{}]", logical.join(",")));
    physical.insert("attachmentSetIds".into(), serde_json::Value::Array(data));
    let input =
        serde_json::from_value(serde_json::Value::Object(physical)).map_err(|_| invalid())?;
    let input = normalize_service_receipt_confirm_input(input)?;
    Ok(ServiceReceiptConfirmWire {
        input,
        normalized: CommandJson::parse(&format!("{{{}}}", members.join(",")))?,
    })
}
struct GenericConfirmAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    config: &'a FinanceJournalCreateConfiguration,
    clock: &'a C,
    invoice: String,
    payment: String,
    field: bool,
    purchase: Option<(CapabilityPurpose, PurchaseWitness)>,
}
impl<C: Clock + Send + Sync> PgCommandAuthority for GenericConfirmAuthority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Sources.prepare(c).await
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        let scope = self.current.selection().workspace();
        if self.current.holding_id() != &self.config.holding
            || scope.company_id != self.config.company
            || scope.branch_id.is_some()
            || scope.department_id.is_some()
            || request.operation != "finance.payment.confirm"
            || !request.record_id.starts_with("pay-")
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "COMPANY_TREASURY_REQUIRED",
                "Use a current Company treasury assignment to confirm a controlled payment.",
            )
            .into());
        }
        self.current.require_permission("finance.pay")?;
        self.current.require_permission("finance.write")?;
        self.current.require_permission("finance.read")?;
        self.current.require_permission("module.finance.read")?;
        if self.field {
            erpx_server_finance::public::service_receipt::require_service_receipt_finance_lineage(
                c,
                &finance_actor(self.current),
                &self.payment,
                &self.invoice,
                false,
            )
            .await?;
        }
        erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current(
            c,
            self.current,
            self.clock.now().get(),
        )
        .await?;
        Ok(command_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        self.authorize(c, request).await?;
        let mut reader = PgCompanyPurchaseReader::new(
            self.config.holding.clone(),
            self.config.company.clone(),
            c,
            &self.config.catalogue,
        );
        if let Some((purpose, witness)) = &self.purchase {
            if *purpose != CapabilityPurpose::SettleExisting {
                return Err(changed().into());
            }
            reader.revalidate(witness, self.clock.now()).await?.require_allowed()?;
            return Ok(());
        }
        let found = reader
            .inspect(
                &PurchaseRequest {
                    capabilities: vec!["finance.banking".into()],
                    purpose: CapabilityPurpose::SettleExisting,
                    mode: PackageEnforcementMode::Enforced,
                },
                self.clock.now(),
            )
            .await?;
        found.require_allowed()?;
        self.purchase = Some((CapabilityPurpose::SettleExisting, found.witness));
        Ok(())
    }
}
#[derive(Clone)]
struct Sources;
impl HoldingWorkSourcePreparation for Sources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            ReceiptSources.prepare(c).await?;
            lock_service_receipt_bank_sources(c).await?;
            lock_finance_receipt_decision_evidence(c).await?;
            lock_customer_receipt_sources(c).await?;
            lock_payment_proof_binding_sources(c).await
        })
    }
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PAYMENT_SOURCE_CHANGED",
        "The controlled payment's original Service invoice changed. Refresh its source workflow.",
    )
}
fn je(e: PgTransactionError<JournalError>) -> Error {
    match e {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => BusinessError::new(
            e.kind,
            e.code,
            "The controlled receipt journal could not be posted.",
        )
        .into(),
    }
}
struct JournalAuthorityPort<'a>(&'a AuthorizedWorkContext);
impl JournalAuthority for JournalAuthorityPort<'_> {
    type Error = PgTransactionError<JournalError>;
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        if *actor != finance_actor(self.0)
            || !self.0.has_permission("finance.pay")
            || !self.0.has_permission("finance.write")
            || !self.0.has_permission("finance.read")
            || !matches!(draft.source_type.as_str(), "payment" | "payment_fx")
            || draft.is_opening
        {
            return Err(JournalError::new(
                "FINANCE_CURRENT_AUTHORITY_REQUIRED",
                "Choose the current independent Treasury payer.",
            )
            .into());
        }
        Ok(())
    }
}
struct Effects<'a, C: Clock + Send + Sync> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: ServiceReceiptConfirmInput,
    invoice: String,
    source: Option<PreparedServiceReceiptConfirm>,
    primary: Option<JournalPreparation>,
    fx: Option<JournalPreparation>,
    journal_sources: Vec<PgManualJournalCreateSources>,
    retained: Option<RetainedServiceReceiptConfirm>,
    decision: Option<PlannedFinanceReceiptDecision>,
    bank_origin: Option<PgRawRecordOrigin>,
    company: Option<CompanyContext>,
    receipt: Option<RetainedCustomerReceipt>,
    finalized: bool,
    file_owner: &'a PaymentProofOwner<'a, C>,
    file_source: Option<PreparedPaymentProofBinding<'a, PaymentProofOwner<'a, C>>>,
    files: Option<RetainedPaymentProofBinding<'a, PaymentProofOwner<'a, C>>>,
}
impl<C: Clock + Send + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Sources.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let payment =
            read_service_receipt_review(c, &finance_actor(self.current), &r.record_id).await?;
        let row = payment.row();
        let same = |key| -> Result<bool, BusinessError> {
            Ok(match row[key].as_str() {
                Some(id) => self.current.same_person_user(&UserId::new(id)?),
                None => false,
            })
        };
        validate_service_receipt_confirmation(
            &ServiceReceiptReviewFacts {
                status: row["status"].as_str().ok_or_else(changed)?,
                version: row["version"].as_i64().ok_or_else(changed)?,
                has_bank_account: row["bank_account"].as_str().is_some_and(|v| !v.is_empty()),
                same_creator: same("created_by")?,
                same_approver: same("approved_by")?,
                same_releaser: same("released_by")?,
            },
            self.input.expected_version,
        )?;
        if !self.input.attachment_set_ids.is_empty() {
            self.file_source = Some(prepare_payment_proof_binding(c, self.file_owner).await?);
        }
        let source = prepare_service_receipt_confirm(
            c,
            &finance_actor(self.current),
            &r.record_id,
            &self.invoice,
            &r.key,
            &self.input,
            self.clock,
        )
        .await?;
        let statement = source.bank_statement();
        let statement_id = statement["id"].as_str().ok_or_else(changed)?;
        let bank_key = statement_id
            .strip_prefix("statement-")
            .ok_or_else(changed)?;
        let (origin, result) = read_raw_record_origin_result_with_limit(
            c,
            RawRecordOriginRequest {
                command_key: bank_key.into(),
                operation: "finance.bank.import".into(),
                record_id: statement_id.into(),
                creator_user_id: statement["imported_by"]
                    .as_str()
                    .ok_or_else(changed)?
                    .into(),
                company_id: finance_actor(self.current).company_id,
            },
            2 * 1024 * 1024,
        )
        .await?
        .ok_or_else(|| {
            BusinessError::invalid(
                "SERVICE_PAYMENT_BANK_SOURCE_INVALID",
                "Import the bank evidence for this exact controlled Service receipt.",
            )
        })?;
        let result: serde_json::Value =
            serde_json::from_str(result.as_str()).map_err(|_| changed())?;
        let field_import = result.get("settlementPaymentId").is_some_and(|value| !value.is_null());
        if result["id"] != statement_id
            || field_import && result["settlementPaymentId"] != r.record_id
        {
            return Err(BusinessError::invalid(
                "SERVICE_PAYMENT_BANK_SOURCE_INVALID",
                "Import the bank evidence for this exact controlled payment.",
            )
            .into());
        }
        self.bank_origin = Some(origin);
        let actor_fin = finance_actor(self.current);
        let rules = embedded_currency_rules().map_err(|_| changed())?;
        let collation = FinanceCollation::english().map_err(|_| changed())?;
        for (draft, key) in [
            (Some(source.primary_draft()), r.key.clone()),
            (source.fx_draft(), format!("{}-fx", r.key)),
        ] {
            if let Some(draft) = draft {
                self.journal_sources.push(
                    PgManualJournalCreateSources::capture(c, &actor_fin, draft)
                        .await
                        .map_err(je)?,
                );
                let mut authority = JournalAuthorityPort(self.current);
                let mut ports = PgJournalPreparation::new(
                    c,
                    &actor_fin,
                    &mut authority,
                    self.clock,
                    &rules,
                    &collation,
                )
                .map_err(je)?;
                let plan = prepare_journal(
                    &mut ports,
                    &actor_fin,
                    draft,
                    &CommandKey::new(key)?,
                    JournalStatus::Posted,
                )
                .await
                .map_err(je)?;
                if self.primary.is_none() {
                    self.primary = Some(plan);
                } else {
                    self.fx = Some(plan);
                }
            }
        }
        self.company = Some(
            read_company_context(
                &mut PgOrganizationReader::new(actor_fin.holding_id.clone(), c),
                &actor_fin.company_id,
            )
            .await?,
        );
        self.decision = Some(
            PlannedFinanceReceiptDecision::prepare(
                c,
                actor,
                FinanceReceiptDecisionFacts {
                    id: uuid::Uuid::new_v4().to_string(),
                    command_id: r.key.clone(),
                    record_id: r.record_id.clone(),
                    action: r.operation.clone(),
                    reviewed_version: self.input.expected_version,
                    input_digest: hex::encode(Sha256::digest(r.input.as_str().as_bytes())),
                    created_at: self.clock.now().get(),
                },
            )
            .await?,
        );
        let result = CommandJson::from_serializable(
            &serde_json::json!({"id":r.record_id,"status":"settled","version":self.input.expected_version+1,"journalId":self.primary.as_ref().ok_or_else(changed)?.journal_id(),"fxJournalId":self.fx.as_ref().map(|p|p.journal_id()),"realizedFx":units_to_decimal(&source.difference().into(),2).map_err(|_|changed())?,"bankTransactionId":self.input.bank_transaction_id,"withheldMinor":0,"netBankMinor":source.amount(),"withholdingDocumentId":serde_json::Value::Null}),
        )?;
        self.source = Some(source);
        Ok(WorkflowOutput {
            version: self.input.expected_version + 1,
            result,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<(), Error> {
        if let Some(files) = self.file_source.take() {
            self.files = Some(files.apply(c).await?);
        }
        self.retained = Some(
            self.source
                .take()
                .ok_or_else(changed)?
                .apply(
                    c,
                    self.primary.as_ref().ok_or_else(changed)?,
                    self.fx.as_ref(),
                    self.clock,
                )
                .await?,
        );
        self.decision.as_ref().ok_or_else(changed)?.apply(c).await?;
        let kind: String = sqlx::query_scalar(
            "SELECT kind FROM finance_payments WHERE id=$1 AND company_id=$2",
        )
        .bind(&r.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        if kind == "receipt" {
            self.receipt = Some(
                issue_customer_receipt(
                    c,
                    &finance_actor(self.current),
                    &r.record_id,
                    &self
                        .company
                        .as_ref()
                        .ok_or_else(changed)?
                        .company()
                        .display_name,
                    self.clock,
                )
                .await?,
            );
        }
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        if let Some(files) = &self.files {
            files.require_after(c).await?;
        }
        let retained = self.retained.as_ref().ok_or_else(changed)?;
        if self.finalized {
            retained.require_finalized(c).await?;
        } else {
            retained.require_current(c).await?;
        }
        if let Some(receipt) = &self.receipt {
            receipt.require_current(c).await?;
        }
        self.decision
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        for source in &self.journal_sources {
            source.require_current(c).await.map_err(je)?;
        }
        verify_raw_record_origin(c, self.bank_origin.as_ref().ok_or_else(changed)?).await?;
        let actor = finance_actor(self.current);
        let company = read_company_context(
            &mut PgOrganizationReader::new(actor.holding_id, c),
            &actor.company_id,
        )
        .await?;
        if company.company() != self.company.as_ref().ok_or_else(changed)?.company() {
            return Err(changed().into());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub async fn execute_service_receipt_confirm<C, F, P>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    selection: F,
    config: FinanceJournalCreateConfiguration,
    clock: Arc<C>,
    file_features: PaymentProofBindingFeatures,
    prepare: P,
) -> Result<CommandJson, Error>
where
    C: Clock + Send + Sync + 'static,
    F: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    P: Fn() -> Result<(String, String, ServiceReceiptConfirmWire), BusinessError>
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
                let mut s = selection()?;
                if s.company_id != company {
                    return Err(changed());
                }
                s.mutation = true;
                Ok(s)
            },
            WorkAuthorizationRequest {
                permission: "finance.pay".into(),
                module: Some("finance".into()),
                destination: "finance/payments".into(),
                api: true,
            },
            Sources,
            move |c, current| {
                let config = config.clone();
                let clock = clock.clone();
                let prepare = prepare.clone();
                let file_features = file_features.clone();
                Box::pin(async move {
                    let (key, id, wire) = prepare()?;
                    let input = wire.input;
                    validate_key(&key)?;
                    let payment =
                        read_service_receipt_review(c, &finance_actor(current), &id).await?;
                    let (origin, result) = read_finance_receipt_creation_origin(
                        c,
                        &config.company,
                        &id,
                        payment.row()["created_by"].as_str().ok_or_else(changed)?,
                    )
                    .await?
                    .ok_or_else(changed)?;
                    let result: serde_json::Value =
                        serde_json::from_str(result.as_str()).map_err(|_| changed())?;
                    if result["id"] != id || result["status"] != "draft" || result["version"] != 1 {
                        return Err(changed().into());
                    }
                    let invoice = result["settlementInvoiceId"]
                        .as_str()
                        .ok_or_else(changed)?
                        .to_owned();
                    let field = field_invoice(c, &config.company, &invoice).await?;
                    let mut auth = GenericConfirmAuthority {
                        current,
                        config: &config,
                        clock: clock.as_ref(),
                        invoice: invoice.clone(),
                        payment: id.clone(),
                        field,
                        purchase: None,
                    };
                    let request = WorkflowRequest {
                        key,
                        operation: "finance.payment.confirm".into(),
                        record_id: id,
                        input: wire.normalized,
                    };
                    let file_owner = PaymentProofOwner {
                        facts: PaymentProofBindingFacts {
                            holding_id: current.holding_id().clone(),
                            company_id: config.company.clone(),
                            actor_id: current.user_id().as_str().into(),
                            target_type: "finance_payment".into(),
                            target_id: request.record_id.clone(),
                            target_version: input.expected_version,
                            command_id: request.key.clone(),
                            set_ids: input.attachment_set_ids.clone(),
                            features: file_features,
                            now: clock.now().get(),
                        },
                        current,
                        clock: clock.as_ref(),
                        origin: &origin,
                        invoice: invoice.clone(),
                    };
                    let mut effects = Effects {
                        current,
                        clock: clock.as_ref(),
                        input,
                        invoice,
                        source: None,
                        primary: None,
                        fx: None,
                        journal_sources: Vec::new(),
                        retained: None,
                        decision: None,
                        bank_origin: None,
                        company: None,
                        receipt: None,
                        finalized: false,
                        file_owner: &file_owner,
                        file_source: None,
                        files: None,
                    };
                    let mut ids = Ids;
                    let outcome = PgCommandLedger::new(
                        config.company.clone(),
                        c,
                        &mut auth,
                        clock.as_ref(),
                        &mut ids,
                    )
                    .execute(&request, &mut effects)
                    .await?;
                    if !outcome.replayed {
                        flush_service_receipt_confirm_effects(c).await?;
                        effects.finalized = true;
                        effects
                            .retain(c, &command_actor(current)?, &request)
                            .await?;
                        auth.verify_command(
                            c,
                            &command_actor(current)?,
                            &request,
                            CommandDisposition::Execute,
                        )
                        .await?;
                    }
                    verify_raw_record_origin(c, &origin).await?;
                    Ok(outcome.result)
                })
            },
        )
        .await
        .map_err(|error|match error {
            // Keep the original Finance error contract for a revoked selected
            // Treasury assignment, including exact receipt replay. The IAM
            // denial is retained; no historical command grants current access.
            PgTransactionError::Business(e) if e.code=="WORK_ROLE_UNAVAILABLE" =>
                BusinessError::new(FailureKind::Forbidden,"FINANCE_ACCESS_CHANGED","The selected Finance access changed. Refresh the current Company role.").into(),
            other=>other,
        })
}
