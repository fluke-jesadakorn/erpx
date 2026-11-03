//! Native Finance Files intake on the caller's selected Holding unit.
//! Concrete compiled owner authorities supply source decisions; the existing
//! Files/Workflow receipt, replay, provider and retention protocol is shared.
use crate::{
    field_attachment_intake_command::{FieldIntakeProviderReadiness, FieldSetCommandResponse},
    finance_journal_create::{FinanceJournalCreateConfiguration, Ids},
    finance_journal_review::command_actor,
    finance_service_receipt_files::{FinancePaymentIntakeAuthority, FinancePaymentIntakeSources},
};
use erpx_server_files::public::field_visit_intake::*;
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkSourcePreparation,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::{commands::*, field_intake_command_evidence::*};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
type Guard<'a, A> = CurrentFieldIntake<'a, A>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The original controlled receipt or its proof changed before intake committed.",
    )
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandInput<'a> {
    wire_key: &'a str,
    target_type: &'a str,
    target_id: &'a str,
    component_kind: &'a str,
    component_ref: Option<&'a str>,
    purpose_key: &'a str,
    expected_target_version: i64,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultMetadata {
    set_id: String,
    expected_target_version: i64,
    policy_version: i64,
}
struct Authority<
    'a,
    C: Clock + Send + Sync,
    A: FieldIntakeAuthority + Sync,
    S: HoldingWorkSourcePreparation,
> {
    sources: &'a S,
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    read: &'a Guard<'a, A>,
    write: &'a Guard<'a, A>,
}
impl<C: Clock + Send + Sync, A: FieldIntakeAuthority + Sync, S: HoldingWorkSourcePreparation>
    PgCommandAuthority for Authority<'_, C, A, S>
{
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.sources.prepare(c).await
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.operation != "attachment.set.create" {
            return Err(changed().into());
        }
        self.read.require_current(c, self.clock.now().get()).await?;
        self.write
            .require_current(c, self.clock.now().get())
            .await?;
        Ok(command_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        a: &CommandActor,
        r: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        if self.authorize(c, r).await? != *a {
            return Err(changed().into());
        }
        Ok(())
    }
}
struct Effects<
    'a,
    C: Clock + Send + Sync,
    A: FieldIntakeAuthority + Sync,
    S: HoldingWorkSourcePreparation,
> {
    sources: &'a S,
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    read: &'a Guard<'a, A>,
    write: &'a Guard<'a, A>,
    input: &'a FieldSetRequest,
    key: &'a str,
    availability: FieldIntakeAvailability,
    readiness: &'a dyn FieldIntakeProviderReadiness,
    ordered: &'a SourceOrderedWorkflowRequest,
    fixed: FieldIntakeCommandClock,
    ids: &'a FieldIntakeCommandGeneratedIds,
    plan: Option<FieldSetCreationPlan>,
    written: Option<WrittenFieldSet>,
    evidence: Option<PlannedFieldIntakeCommandEvidence>,
    retained: Option<RetainedFieldIntakeCommandEvidence>,
}
impl<C: Clock + Send + Sync, A: FieldIntakeAuthority + Sync, S: HoldingWorkSourcePreparation>
    PgOwnerCommandEffects for Effects<'_, C, A, S>
{
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.sources.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        self.read.require_current(c, self.clock.now().get()).await?;
        self.write
            .require_current(c, self.clock.now().get())
            .await?;
        if self.write.scope().version != self.input.expected_target_version {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "ATTACHMENT_TARGET_CHANGED",
                "The record changed. Refresh before adding files.",
            )
            .into());
        }
        let status=self.readiness.probe().await.map_err(|_|BusinessError::new(FailureKind::Unavailable,"ATTACHMENT_SCANNER_UNAVAILABLE","File intake is unavailable until the malware scanner and current signatures are ready."))?;
        self.availability.scanner_ready = status.scanner_enabled && status.scanner_ready;
        self.availability.preview_ready = status.preview_ready;
        let plan = prepare_field_set_creation(
            c,
            self.read,
            Some(self.write),
            self.input,
            self.key,
            &self.availability,
            self.clock.now().get(),
        )
        .await?;
        if plan.disposition() == FieldSetDisposition::Replay {
            return Err(changed().into());
        }
        let output = WorkflowOutput {
            version: plan.expected_target_version(),
            result: CommandJson::from_serializable(&ResultMetadata {
                set_id: plan.set_id().into(),
                expected_target_version: plan.expected_target_version(),
                policy_version: plan.policy_version(),
            })?,
            event_scope: Some(CommandEventScope {
                company_id: self.current.selection().workspace().company_id.clone(),
                branch_id: CommandField::Null,
                department_id: CommandField::Null,
            }),
        };
        self.evidence = Some(
            plan_field_intake_command_evidence(
                c,
                actor,
                r,
                self.ordered,
                &output,
                self.fixed,
                self.ids,
            )
            .await?,
        );
        self.plan = Some(plan);
        Ok(output)
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.read.require_current(c, self.clock.now().get()).await?;
        self.write
            .require_current(c, self.clock.now().get())
            .await?;
        self.written = Some(
            self.plan
                .as_ref()
                .ok_or_else(changed)?
                .apply_native(
                    c,
                    self.read,
                    Some(self.write),
                    self.clock.now().get(),
                    self.evidence.as_ref().ok_or_else(changed)?,
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
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        self.read.require_current(c, self.clock.now().get()).await?;
        self.write
            .require_current(c, self.clock.now().get())
            .await?;
        self.written
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        self.retained = Some(
            self.evidence
                .as_ref()
                .ok_or_else(changed)?
                .retain(c, self.ids)
                .await?,
        );
        Ok(())
    }
}
/// Authorization is provided by the surrounding native Account coordinator.
/// This function never opens/commits a transaction or discovers providers.
#[allow(clippy::too_many_arguments)]
pub async fn create_finance_payment_proof_set<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    config: &FinanceJournalCreateConfiguration,
    clock: &C,
    input: &FieldSetRequest,
    key: &str,
    availability: &FieldIntakeAvailability,
    readiness: &dyn FieldIntakeProviderReadiness,
) -> Result<FieldSetCommandResponse, Error> {
    let lookup = if current.has_permission("finance.read") {
        FinancePaymentIntakeAuthority::for_read(current, config, clock)
    } else {
        FinancePaymentIntakeAuthority::for_manage(current, config, clock)
    };
    let manage = FinancePaymentIntakeAuthority::for_manage(current, config, clock);
    create_finance_intake_set(
        c,
        current,
        config,
        clock,
        input,
        key,
        availability,
        readiness,
        &lookup,
        &manage,
        &FinancePaymentIntakeSources,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
pub async fn create_finance_field_journal_support_set<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    config: &FinanceJournalCreateConfiguration,
    clock: &C,
    input: &FieldSetRequest,
    key: &str,
    availability: &FieldIntakeAvailability,
    readiness: &dyn FieldIntakeProviderReadiness,
) -> Result<FieldSetCommandResponse, Error> {
    use crate::finance_field_journal_files::{
        FinanceFieldJournalIntakeAuthority, FinanceFieldJournalIntakeSources,
    };
    let lookup = FinanceFieldJournalIntakeAuthority::for_read(current, config, clock);
    let manage = FinanceFieldJournalIntakeAuthority::for_manage(current, config, clock);
    create_finance_intake_set(
        c,
        current,
        config,
        clock,
        input,
        key,
        availability,
        readiness,
        &lookup,
        &manage,
        &FinanceFieldJournalIntakeSources,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn create_finance_intake_set<
    C: Clock + Send + Sync,
    A: FieldIntakeAuthority + Sync,
    S: HoldingWorkSourcePreparation,
>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    config: &FinanceJournalCreateConfiguration,
    clock: &C,
    input: &FieldSetRequest,
    key: &str,
    availability: &FieldIntakeAvailability,
    readiness: &dyn FieldIntakeProviderReadiness,
    lookup: &A,
    manage: &A,
    sources: &S,
) -> Result<FieldSetCommandResponse, Error> {
    validate_key(key)?;
    sources.prepare(c).await?;
    let read = prepare_field_intake(
        c,
        lookup,
        current,
        &input.target_id,
        false,
        clock.now().get(),
    )
    .await?;
    let json = CommandJson::from_serializable(&CommandInput {
        wire_key: key,
        target_type: &input.target_type,
        target_id: &input.target_id,
        component_kind: &input.component_kind,
        component_ref: component_ref(input.component_ref.as_deref()),
        purpose_key: &input.purpose_key,
        expected_target_version: input.expected_target_version,
    })?;
    let identity = WorkflowRecordIdentity::from_utf16(input.target_id.encode_utf16().collect());
    let request = WorkflowRequest {
        key: field_set_command_id(config.company.as_str(), current.user_id().as_str(), key),
        operation: "attachment.set.create".into(),
        record_id: identity.physical_text().into(),
        input: json.clone(),
    };
    let ordered = SourceOrderedWorkflowRequest {
        key: request.key.clone(),
        operation: request.operation.clone(),
        record_id: identity,
        input: json,
    };
    let native_receipt =
        erpx_server_workflow::public::service_parts_posting::read_service_prerequisite_receipt(
            c,
            &request.key,
        )
        .await?;
    let recorded =
        recorded_field_set(c, &read, input, key, availability, clock.now().get()).await?;
    if native_receipt.facts().is_some()
        && recorded
            .as_ref()
            .is_none_or(|r| r.kind != RecordedFieldSetKind::Native)
    {
        return Err(changed().into());
    }
    if let Some(recorded) = recorded {
        if !current.has_permission("finance.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ATTACHMENT_FORBIDDEN",
                "Your current assignment cannot access files for this record.",
            )
            .into());
        }
        if recorded.kind == RecordedFieldSetKind::Native {
            let output = WorkflowOutput {
                version: recorded.data["expectedTargetVersion"]
                    .as_i64()
                    .ok_or_else(changed)?,
                result: CommandJson::from_serializable(&ResultMetadata {
                    set_id: recorded.data["id"].as_str().ok_or_else(changed)?.into(),
                    expected_target_version: recorded.data["expectedTargetVersion"]
                        .as_i64()
                        .ok_or_else(changed)?,
                    policy_version: recorded.data["policyVersion"]
                        .as_i64()
                        .ok_or_else(changed)?,
                })?,
                event_scope: Some(CommandEventScope {
                    company_id: config.company.clone(),
                    branch_id: recorded
                        .branch_id
                        .clone()
                        .map(CommandField::Value)
                        .unwrap_or(CommandField::Null),
                    department_id: recorded
                        .department_id
                        .clone()
                        .map(CommandField::Value)
                        .unwrap_or(CommandField::Null),
                }),
            };
            verify_historical_field_intake_command(
                c,
                &command_actor(current)?,
                &request,
                &ordered,
                &output,
                recorded.anchor.as_ref().ok_or_else(changed)?,
            )
            .await?;
        }
        read.require_current(c, clock.now().get()).await?;
        let after = recorded_field_set(c, &read, input, key, availability, clock.now().get())
            .await?
            .ok_or_else(changed)?;
        if after.data != recorded.data
            || after.kind != recorded.kind
            || after.anchor != recorded.anchor
            || after.branch_id != recorded.branch_id
            || after.department_id != recorded.department_id
        {
            return Err(changed().into());
        }
        return Ok(FieldSetCommandResponse {
            data: after.data,
            replayed: true,
        });
    }
    let write = prepare_field_intake(
        c,
        manage,
        current,
        &input.target_id,
        true,
        clock.now().get(),
    )
    .await?;
    let fixed = FieldIntakeCommandClock::sample(clock)?;
    let mut ids = Ids;
    let generated = FieldIntakeCommandGeneratedIds::reserve(&mut ids);
    let mut wrapped = generated.wrap(&mut ids);
    let mut authority = Authority {
        sources,
        current,
        clock,
        read: &read,
        write: &write,
    };
    let mut effects = Effects {
        sources,
        current,
        clock,
        read: &read,
        write: &write,
        input,
        key,
        availability: availability.clone(),
        readiness,
        ordered: &ordered,
        fixed,
        ids: &generated,
        plan: None,
        written: None,
        evidence: None,
        retained: None,
    };
    let outcome = PgCommandLedger::new(
        config.company.clone(),
        c,
        &mut authority,
        &fixed,
        &mut wrapped,
    )
    .execute_source_ordered(&ordered, &mut effects)
    .await?;
    if outcome.replayed {
        return Err(changed().into());
    }
    read.require_current(c, clock.now().get()).await?;
    write.require_current(c, clock.now().get()).await?;
    effects
        .written
        .as_ref()
        .ok_or_else(changed)?
        .require_current(c)
        .await?;
    effects
        .retained
        .as_ref()
        .ok_or_else(changed)?
        .require_current(c)
        .await?;
    Ok(FieldSetCommandResponse {
        data: effects.written.as_ref().ok_or_else(changed)?.data().clone(),
        replayed: false,
    })
}
