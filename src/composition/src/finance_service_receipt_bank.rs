//! Native exact Field receipt bank evidence; providers and public activation are absent.
use crate::{
    finance_journal_create::{FinanceJournalCreateConfiguration, Ids},
    finance_journal_review::{command_actor, finance_actor},
    finance_service_invoice_post::service_receipt_bank_authority,
    finance_service_receipt_create::ReceiptSources,
};
use erpx_server_finance::public::service_receipt::*;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::{
    commands::*,
    record_origin::{read_finance_receipt_creation_origin, verify_raw_record_origin},
};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
/// Retain logical input independently from PostgreSQL's physical text.
#[derive(Clone)]
pub struct ServiceReceiptBankWire {
    input: ServiceReceiptBankInput,
    normalized: CommandJson,
}
impl ServiceReceiptBankWire {
    pub fn input(&self) -> &ServiceReceiptBankInput {
        &self.input
    }
    pub fn normalized(&self) -> &CommandJson {
        &self.normalized
    }
}
pub fn normalize_service_receipt_bank(
    body: &[u8],
) -> Result<ServiceReceiptBankWire, BusinessError> {
    let invalid = || BusinessError::invalid("VALIDATION_FAILED", "Invalid bank statement.");
    let body = String::from_utf8_lossy(body);
    let body = body.strip_prefix('\u{feff}').unwrap_or(&body);
    let doc = CommandJson::parse(body)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))?;
    if doc.object_keys()?.is_none() {
        return Err(invalid());
    }
    let string = |doc: &CommandJson,
                  key: &str,
                  optional: bool|
     -> Result<Option<(CommandJson, String)>, BusinessError> {
        let Some(raw) = doc.property(key)? else {
            return if optional { Ok(None) } else { Err(invalid()) };
        };
        let text = raw.string_utf8_storage()?.ok_or_else(invalid)?;
        Ok(Some((raw, text)))
    };
    let mut members = Vec::new();
    let mut values = serde_json::Map::new();
    for key in [
        "settlementPaymentId",
        "bankAccount",
        "statementNumber",
        "statementDate",
        "opening",
        "closing",
        "closingFxRate",
        "sourceHash",
    ] {
        let Some((raw, text)) = string(
            &doc,
            key,
            matches!(key, "settlementPaymentId" | "closingFxRate"),
        )?
        else {
            continue;
        };
        if (key == "settlementPaymentId" && text.is_empty())
            || (key == "sourceHash" && raw.string_utf16_units()?.ok_or_else(invalid)?.len() < 8)
            || (key == "statementDate" && !valid_service_receipt_date(&text))
        {
            return Err(invalid());
        }
        members.push(format!("\"{key}\":{}", raw.as_str()));
        values.insert(key.into(), serde_json::Value::String(text));
    }
    let rows = doc
        .property("rows")?
        .ok_or_else(invalid)?
        .array_elements()?
        .ok_or_else(invalid)?;
    if rows.len() > 500 {
        return Err(invalid());
    }
    let mut physical_rows = Vec::new();
    let mut logical_rows = Vec::new();
    for row in rows {
        if row.object_keys()?.is_none() {
            return Err(invalid());
        }
        let mut data = serde_json::Map::new();
        let mut fields = Vec::new();
        for key in [
            "externalId",
            "transactionDate",
            "description",
            "amount",
            "currency",
        ] {
            let Some((raw, text)) = string(&row, key, key == "description")? else {
                continue;
            };
            if (key == "transactionDate" && !valid_service_receipt_date(&text))
                || (key == "currency" && raw.string_utf16_units()?.ok_or_else(invalid)?.len() != 3)
            {
                return Err(invalid());
            }
            fields.push(format!("\"{key}\":{}", raw.as_str()));
            data.insert(key.into(), serde_json::Value::String(text));
        }
        physical_rows.push(serde_json::Value::Object(data));
        logical_rows.push(format!("{{{}}}", fields.join(",")));
    }
    members.push(format!("\"rows\":[{}]", logical_rows.join(",")));
    values.insert("rows".into(), serde_json::Value::Array(physical_rows));
    values
        .entry("settlementPaymentId")
        .or_insert(serde_json::Value::String(String::new()));
    let input = serde_json::from_value(serde_json::Value::Object(values)).map_err(|_| invalid())?;
    Ok(ServiceReceiptBankWire {
        input,
        normalized: CommandJson::parse(&format!("{{{}}}", members.join(",")))?,
    })
}
#[derive(Clone)]
struct Sources;
impl HoldingWorkSourcePreparation for Sources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            ReceiptSources.prepare(c).await?;
            lock_service_receipt_bank_sources(c).await
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
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: ServiceReceiptBankInput,
    prepared: Option<PreparedServiceReceiptBank>,
    retained: Option<RetainedServiceReceiptBank>,
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
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let prepared = prepare_service_receipt_bank(
            c,
            &finance_actor(self.current),
            &r.key,
            &self.input,
            self.clock,
        )
        .await?;
        if prepared.statement_id() != r.record_id {
            return Err(changed().into());
        }
        let result = CommandJson::from_serializable(
            &serde_json::json!({"id":r.record_id,"imported":1,"status":"imported","settlementPaymentId":self.input.settlement_payment_id}),
        )?;
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
        self.retained = Some(self.prepared.take().ok_or_else(changed)?.persist(c).await?);
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
            .ok_or_else(changed)?
            .require_current(c)
            .await
    }
}
pub async fn execute_service_receipt_bank<C, F, P>(
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
    P: Fn() -> Result<(String, ServiceReceiptBankWire), BusinessError>
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
                permission: "finance.write".into(),
                module: Some("finance".into()),
                destination: "finance/bank".into(),
                api: true,
            },
            Sources,
            move |c, current| {
                let config = config.clone();
                let clock = clock.clone();
                let prepare = prepare.clone();
                Box::pin(async move {
                    let (key, wire) = prepare()?;
                    let input = wire.input;
                    validate_key(&key)?;
                    let payment = read_service_receipt_review(
                        c,
                        &finance_actor(current),
                        &input.settlement_payment_id,
                    )
                    .await?;
                    let (origin, result) = read_finance_receipt_creation_origin(
                        c,
                        &config.company,
                        &input.settlement_payment_id,
                        payment.row()["created_by"].as_str().ok_or_else(changed)?,
                    )
                    .await?
                    .ok_or_else(changed)?;
                    let result: serde_json::Value =
                        serde_json::from_str(result.as_str()).map_err(|_| changed())?;
                    if result["id"] != input.settlement_payment_id
                        || result["status"] != "draft"
                        || result["version"] != 1
                    {
                        return Err(changed().into());
                    }
                    let invoice = result["settlementInvoiceId"]
                        .as_str()
                        .ok_or_else(changed)?
                        .to_owned();
                    let mut auth = service_receipt_bank_authority(
                        current,
                        &config,
                        clock.as_ref(),
                        invoice,
                        input.settlement_payment_id.clone(),
                    );
                    let request = WorkflowRequest {
                        record_id: format!("statement-{key}"),
                        key,
                        operation: "finance.bank.import".into(),
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
                        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                            .execute(&mut *c)
                            .await?;
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
}
