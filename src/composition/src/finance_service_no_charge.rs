//! Field-only Finance nonmonetary confirmation on the original selected unit.
use crate::{
    finance_journal_create::{FinanceJournalCreateConfiguration, Ids},
    finance_journal_review::{command_actor, finance_actor},
    finance_service_invoice_flow::read_service_invoice_flow,
    finance_service_invoice_post::{Sources as InvoiceSources, service_no_charge_authority},
};
use erpx_server_finance::public::service_no_charge::*;
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
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, UserId};
use erpx_server_workflow::public::{commands::*, service_invoice_flow::*};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_NO_CHARGE_SOURCE_CHANGED",
        "Review the original fully covered Service billing source.",
    )
}
#[derive(Clone)]
struct Sources;
impl HoldingWorkSourcePreparation for Sources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            lock_service_no_charge_sources(c).await?;
            crate::finance_journal_detail::ServiceInvoiceReadSources
                .prepare(c)
                .await?;
            InvoiceSources.prepare(c).await?;
            lock_service_invoice_flow_sources(c).await
        })
    }
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: ServiceNoChargeWire,
    source: Option<CurrentServiceNoCharge>,
    retained: Option<RetainedServiceNoCharge>,
    flow: Option<ServiceInvoiceNoChargeFlow>,
    record: Value,
    before: Value,
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
        let source = read_service_no_charge(c, &finance_actor(self.current), &r.record_id).await?;
        require_scope(&source, self.current)?;
        let independent = !self
            .current
            .same_person_user(&UserId::new(source.creator()?.to_owned())?)
            && !self
                .current
                .same_person_user(&UserId::new(source.releaser()?.to_owned())?);
        source.require_decision(
            self.input.input().expected_version,
            &self.input.input().context_key,
            actor,
            independent,
        )?;
        self.record = source.record();
        let flow =
            read_service_invoice_flow(c, self.current, &self.record, true, self.clock).await?;
        self.before = flow.view.clone();
        let version = source.version()? + 1;
        let name =
            erpx_server_people_payroll::public::journal_flow_people::current_journal_flow_people(
                c,
                &self.current.selection().workspace().company_id,
            )
            .await?
            .into_iter()
            .find(|p| p.user_id == self.current.user_id().as_str())
            .map(|p| p.name)
            .unwrap_or_else(|| self.current.user_id().as_str().into());
        self.flow = Some(flow.plan_no_charge(ServiceInvoiceNoChargeFlowInput {
            actor: actor.user_id.as_str(),
            actor_name: &name,
            command: &r.key,
            event: &format!("flow-event-{}", r.key),
            record_version: version,
            reason: &self.input.input().reason,
            now: self.clock.now().get(),
        })?);
        self.source = Some(source);
        Ok(WorkflowOutput {
            version,
            result: CommandJson::from_serializable(
                &ServiceNoChargeResult{id:r.record_id.clone(),status:ServiceNoChargeStatus::NoChargeConfirmed,version,confirmation_id:format!("service-no-charge-{}",r.key)},
            )?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.retained = Some(
            self.source
                .take()
                .ok_or_else(changed)?
                .apply(c, &r.key, &self.input.input().reason, self.clock.now().get())
                .await?,
        );
        self.flow.as_mut().ok_or_else(changed)?.apply(c).await
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
            .await?;
        self.flow
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await.map_err(no_charge_flow_error)?;
        let after =
            read_service_invoice_flow(c, self.current, &self.record, true, self.clock).await?;
        if after.view["canAct"] != true
            || [0].iter().any(|i| {
                after.view["steps"][*i]["candidates"] != self.before["steps"][*i]["candidates"]
            })
        {
            return Err(changed().into());
        }
        require_selected_actor_current(c, self.current, self.clock.now().get()).await
    }
}
pub async fn execute_service_no_charge<C, F, P>(
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
                permission: "finance.write".into(),
                module: Some("finance".into()),
                destination: "finance/invoices".into(),
                api: true,
            },
            Sources,
            move |c, current| {
                let config = config.clone();
                let clock = clock.clone();
                let prepare = prepare.clone();
                Box::pin(async move {
                    // The original no-charge API requires approval permission
                    // before parsing, including malformed bodies and replays.
                    current.require_permission("finance.approve")?;
                    let request = prepare()?;
                    let input = normalize_service_no_charge(request.input.as_str().as_bytes())?;
                    if input.normalized() != &request.input {
                        return Err(BusinessError::invalid(
                            "VALIDATION_FAILED",
                            "Use the normalized no-charge confirmation.",
                        )
                        .into());
                    }
                    let mut authority =
                        service_no_charge_authority(current, &config, clock.as_ref());
                    let mut effects = Effects {
                        current,
                        clock: clock.as_ref(),
                        input,
                        source: None,
                        retained: None,
                        flow: None,
                        record: Value::Null,
                        before: Value::Null,
                    };
                    let mut ids = Ids;
                    let outcome = PgCommandLedger::new(
                        config.company.clone(),
                        c,
                        &mut authority,
                        clock.as_ref(),
                        &mut ids,
                    )
                    .execute(&request, &mut effects)
                    .await?;
                    if !outcome.replayed {
                        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                            .execute(&mut *c)
                            .await
                            .map_err(no_charge_deferred_error)?;
                        require_selected_actor_current(c,current,clock.now().get()).await.map_err(no_charge_current_authority_error)?;
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

pub(crate) use crate::finance_service_no_charge_read::require_scope;

fn no_charge_deferred_error(error: sqlx::Error) -> Error {
    if error.as_database_error().and_then(|e| e.constraint())
        == Some("native_service_invoice_flow_required")
    {
        return BusinessError::new(
            FailureKind::Conflict,
            "FLOW_ROUTING_CHANGED",
            "The original invoice responsibility changed before confirmation committed.",
        )
        .into();
    }
    service_no_charge_persistence_error(error)
}

fn no_charge_current_authority_error(error:Error)->Error{
    match error {
        PgTransactionError::Business(e) if matches!(e.code,"WORK_ROLE_UNAVAILABLE"|"QUEUED_AUTHORITY_CHANGED") => BusinessError::new(FailureKind::Forbidden,"FINANCE_ACCESS_CHANGED","The current Finance authority changed before confirmation committed.").into(),
        other=>other,
    }
}

fn no_charge_flow_error(error:Error)->Error{
    match error {
        PgTransactionError::Business(e) if e.code=="FLOW_SOURCE_CHANGED" => BusinessError::new(FailureKind::Conflict,"FLOW_ROUTING_CHANGED","The original invoice responsibility or captured default changed before confirmation committed.").into(),
        other=>other,
    }
}
