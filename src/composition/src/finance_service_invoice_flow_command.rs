//! Field invoice responsibility assignment. The original owner transaction,
//! current IAM/purchase/lineage and Workflow replay order remain authoritative.
use crate::{
    finance_journal_create::{FinanceJournalCreateConfiguration, Ids},
    finance_journal_detail::ServiceInvoiceReadSources,
    finance_journal_review::{command_actor, finance_actor},
    finance_service_invoice_flow::read_service_invoice_flow,
    finance_service_invoice_post::service_invoice_flow_authority,
};
use erpx_server_collaboration::public::service_invoice_flow_notification::*;
use erpx_server_finance::public::field_service_invoice_read::{
    CurrentFinanceServiceInvoiceWorkspace, read_service_invoice_workspace,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_people_payroll::public::journal_flow_people::current_journal_flow_people;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, TimestampMillis};
use erpx_server_workflow::public::{commands::*, service_invoice_flow::*};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FLOW_SOURCE_CHANGED",
        "The current invoice workflow changed. Reload its source.",
    )
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Check the workflow responsibility, versions and reason.",
    )
}
fn whitespace(v: u16) -> bool {
    matches!(v,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
}
/// Preserve Zod's finite wire shape, JavaScript UTF-16 length/trim and original
/// normalized property order. Unknown client fields cannot select source kind.
pub fn normalize_service_invoice_flow_assignment(body:&[u8])->Result<CommandJson,BusinessError>{
    normalize_owned_finance_flow_assignment(body,"invoice")
}
pub fn normalize_field_journal_flow_assignment(body:&[u8])->Result<CommandJson,BusinessError>{
    normalize_owned_finance_flow_assignment(body,"journal")
}
fn normalize_owned_finance_flow_assignment(
    body: &[u8],kind:&'static str,
) -> Result<CommandJson, BusinessError> {
    let body = String::from_utf8_lossy(body);
    let body = body.strip_prefix('\u{feff}').unwrap_or(&body);
    let doc = CommandJson::parse(body)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Use valid JSON."))?;
    if doc.object_keys()?.is_none() {
        return Err(invalid());
    }
    let string =
        |key: &str, min: usize, max: usize, trim: bool| -> Result<CommandJson, BusinessError> {
            let value = doc.property(key)?.ok_or_else(invalid)?;
            let units = value.string_utf16_units()?.ok_or_else(invalid)?;
            let start = if trim {
                units
                    .iter()
                    .position(|u| !whitespace(*u))
                    .unwrap_or(units.len())
            } else {
                0
            };
            let end = if trim {
                units
                    .iter()
                    .rposition(|u| !whitespace(*u))
                    .map_or(start, |i| i + 1)
            } else {
                units.len()
            };
            let units = &units[start..end];
            if !(min..=max).contains(&units.len()) {
                return Err(invalid());
            }
            CommandJson::from_utf16_string(units)
        };
    let step = string("stepKey", 1, 100, false)?;
    let user = string("userId", 1, 180, false)?;
    let reason = string("reason", 3, 2000, true)?;
    let integer = |key: &str, min: f64| -> Result<i64, BusinessError> {
        let v = doc.property(key)?.ok_or_else(invalid)?;
        let value: Value = serde_json::from_str(v.as_str()).map_err(|_| invalid())?;
        value
            .as_f64()
            .filter(|n| {
                n.is_finite() && n.fract() == 0.0 && (min..=9_007_199_254_740_991.0).contains(n)
            })
            .map(|v| v as i64)
            .ok_or_else(invalid)
    };
    let expected = integer("expectedVersion", 0.0)?;
    let record = integer("recordVersion", 1.0)?;
    let save = match doc.property("saveDefault")? {
        None => false,
        Some(v) => match v.as_str() {
            "true" => true,
            "false" => false,
            _ => return Err(invalid()),
        },
    };
    CommandJson::parse(&format!(
        "{{\"kind\":\"{kind}\",\"stepKey\":{},\"userId\":{},\"expectedVersion\":{expected},\"recordVersion\":{record},\"reason\":{},\"saveDefault\":{save}}}",
        step.as_str(),
        user.as_str(),
        reason.as_str()
    ))
}
pub(crate) struct Input {
    pub(crate) step: String,
    pub(crate) user: String,
    pub(crate) expected: i64,
    pub(crate) record: i64,
    pub(crate) reason: String,
    pub(crate) save: bool,
}
pub(crate) fn physical(doc: &CommandJson) -> Result<Input, BusinessError> {
    let text = |key| {
        doc.property(key)?
            .ok_or_else(invalid)?
            .string_utf8_storage()?
            .ok_or_else(invalid)
    };
    let number = |key| -> Result<i64, BusinessError> {
        serde_json::from_str::<Value>(doc.property(key)?.ok_or_else(invalid)?.as_str())
            .map_err(|_| invalid())?
            .as_i64()
            .ok_or_else(invalid)
    };
    Ok(Input {
        step: text("stepKey")?,
        user: text("userId")?,
        expected: number("expectedVersion")?,
        record: number("recordVersion")?,
        reason: text("reason")?,
        save: doc.property("saveDefault")?.ok_or_else(invalid)?.as_str() == "true",
    })
}
#[derive(Clone)]
struct Sources;
impl HoldingWorkSourcePreparation for Sources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            ServiceInvoiceReadSources.prepare(c).await?;
            lock_service_invoice_flow_notices(c).await
        })
    }
}
struct Sample(TimestampMillis);
impl Clock for Sample {
    fn now(&self) -> TimestampMillis {
        self.0
    }
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: Input,
    now: i64,
    invoice: Option<CurrentFinanceServiceInvoiceWorkspace>,
    record: Value,
    before: Value,
    plan: Option<ServiceInvoiceFlowAssignment>,
    notice: Option<RetainedServiceInvoiceFlowNotice>,
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
        let invoice =
            read_service_invoice_workspace(c, &finance_actor(self.current), &r.record_id).await?;
        let record = serde_json::to_value(&invoice.data().invoice).map_err(|_| changed())?;
        let flow = read_service_invoice_flow(c, self.current, &record, true, self.clock).await?;
        let name = current_journal_flow_people(c, &self.current.selection().workspace().company_id)
            .await?
            .into_iter()
            .find(|p| p.user_id == self.current.user_id().as_str())
            .map(|p| p.name)
            .unwrap_or_else(|| self.current.user_id().as_str().into());
        self.before = flow.view.clone();
        let event = uuid::Uuid::new_v4().to_string();
        let plan = flow.plan_assignment(ServiceInvoiceFlowAssignmentInput {
            step: &self.input.step,
            user: &self.input.user,
            expected_flow: self.input.expected,
            expected_record: self.input.record,
            reason: &self.input.reason,
            save_default: self.input.save,
            actor: self.current.user_id().as_str(),
            actor_name: &name,
            command: &r.key,
            event: &event,
            now: self.now,
        })?;
        let output = WorkflowOutput {
            version: self.input.record,
            result: CommandJson::from_serializable(&plan.result)?,
            event_scope: None,
        };
        self.record = record;
        self.invoice = Some(invoice);
        self.plan = Some(plan);
        Ok(output)
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.invoice
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        let plan = self.plan.as_mut().ok_or_else(changed)?;
        plan.apply(c).await?;
        self.notice = Some(
            stage_service_invoice_flow_notice(
                c,
                ServiceInvoiceFlowNotice {
                    id: &format!("invoice-flow-notice-{}", r.key),
                    company: self.current.selection().workspace().company_id.as_str(),
                    recipient: &plan.recipient,
                    invoice: &r.record_id,
                    version: self.input.record,
                    message: &plan.message,
                    now: self.now,
                },
            )
            .await?,
        );
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.invoice
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        self.plan
            .as_ref()
            .ok_or_else(changed)?
            .require_after(c)
            .await?;
        self.notice
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        let after =
            read_service_invoice_flow(c, self.current, &self.record, true, self.clock).await?;
        if after.view["canManage"] != true
            || [0, 1].iter().any(|i| {
                after.view["steps"][*i]["candidates"] != self.before["steps"][*i]["candidates"]
            })
        {
            return Err(changed().into());
        }
        Ok(())
    }
}
pub async fn execute_service_invoice_flow_assignment<C, F, P>(
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
    P: Fn() -> Result<WorkflowRequest, BusinessError> + Clone + Send + Sync + 'static,
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
                permission: "finance.read".into(),
                module: Some("finance".into()),
                destination: "finance/invoices".into(),
                api: true,
            },
            Sources,
            move |c, current| {
                let (config, clock, prepare) = (config.clone(), clock.clone(), prepare.clone());
                Box::pin(async move {
                    let request = prepare()?;
                    let input = physical(&request.input)?;
                    let sample = Sample(clock.now());
                    if !(0..=9_007_199_254_740_991).contains(&sample.0.get()) {
                        return Err(invalid().into());
                    }
                    let mut authority =
                        service_invoice_flow_authority(current, &config, clock.as_ref());
                    let mut effects = Effects {
                        current,
                        clock: clock.as_ref(),
                        input,
                        now: sample.0.get(),
                        invoice: None,
                        record: Value::Null,
                        before: Value::Null,
                        plan: None,
                        notice: None,
                    };
                    let mut ids = Ids;
                    let outcome = PgCommandLedger::new(
                        config.company.clone(),
                        c,
                        &mut authority,
                        &sample,
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
                        authority
                            .verify_command(
                                c,
                                &command_actor(current)?,
                                &request,
                                CommandDisposition::Execute,
                            )
                            .await?;
                    }
                    Ok(outcome.result)
                })
            },
        )
        .await
}
