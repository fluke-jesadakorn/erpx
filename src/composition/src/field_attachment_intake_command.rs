//! Source-ordered Workflow composition for native Field visit set intake.
use crate::field_attachment::{AttachmentContext, Proof, proof};
use crate::field_attachment_intake::{
    FieldAttachmentIntakeAuthority, FieldAttachmentIntakeSources,
};
use erpx_server_commercial::public::purchases::CommercialCatalogue;
use erpx_server_files::public::field_visit_intake::{
    FieldIntakeAvailability, FieldSetCreationPlan, FieldSetRequest, RecordedFieldSetKind,
    WrittenFieldSet, prepare_field_intake, prepare_field_set_creation, recorded_field_set,
};
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam,
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    work_context::WorkScopeKind,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::{
    commands::*,
    field_intake_command_evidence::{
        FieldIntakeCommandClock, FieldIntakeCommandGeneratedIds, PlannedFieldIntakeCommandEvidence,
        RetainedFieldIntakeCommandEvidence, plan_field_intake_command_evidence,
        verify_historical_field_intake_command,
    },
};
use sqlx::PgConnection;
use std::{future::Future, pin::Pin};
/// Trusted runtime probe, sampled only for a fresh command after replay checks.
pub trait FieldIntakeProviderReadiness: Send + Sync {
    fn probe(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<FieldIntakeProviderStatus, BusinessError>> + Send + '_>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldIntakeProviderStatus {
    pub scanner_enabled: bool,
    pub scanner_ready: bool,
    pub preview_ready: bool,
}
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The original Service visit or selected role changed before its evidence committed.",
    )
}
#[derive(Clone)]
pub struct FieldAttachmentIntakeCommandSources;
impl HoldingWorkSourcePreparation for FieldAttachmentIntakeCommandSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            FieldAttachmentIntakeSources.prepare(c).await?;
            erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
            Ok(())
        })
    }
}

fn command_actor(current: &AuthorizedWorkContext, now: i64) -> Result<CommandActor, BusinessError> {
    current.require_permission("records.read")?;
    if now >= current.expires_at() {
        return Err(changed());
    }
    let s = current.selection();
    let w = s.workspace();
    let a = s.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: w.company_id.clone(),
        branch_id: w
            .branch_id
            .as_ref()
            .map(|v| CommandField::Value(v.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: w
            .department_id
            .as_ref()
            .map(|v| CommandField::Value(v.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: s.revision(),
            assignment_version: a.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match a.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|v| v.as_str().into()),
    })
}

struct Authority<'a, C> {
    ctx: &'a AttachmentContext<'a, C>,
    request: &'a WorkflowRequest,
    proof: Option<Proof>,
    replay: bool,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FieldAttachmentIntakeCommandSources.prepare(c).await
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.record_id != self.request.record_id
            || r.operation != self.request.operation
            || r.input.as_str() != self.request.input.as_str()
        {
            return Err(changed().into());
        }
        Ok(command_actor(self.ctx.actor, self.ctx.clock.now().get())?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        a: &CommandActor,
        _: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if command_actor(self.ctx.actor, self.ctx.clock.now().get())? != *a {
            return Err(changed().into());
        }
        let replay = disposition == CommandDisposition::Replay;
        if !replay {
            self.ctx.actor.require_permission("records.write")?;
        }
        if self.proof.is_none() || self.replay != replay {
            self.proof = Some(proof(c, self.ctx, &self.request.record_id, !replay).await?);
            self.replay = replay;
        }
        self.proof
            .as_ref()
            .ok_or_else(changed)?
            .current(
                c,
                self.ctx.actor,
                self.ctx.holding,
                self.ctx.company,
                self.ctx.catalogue,
                self.ctx.clock,
            )
            .await
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultMetadata {
    set_id: String,
    expected_target_version: i64,
    policy_version: i64,
}
struct Effects<'a, C> {
    ctx: &'a AttachmentContext<'a, C>,
    input: &'a FieldSetRequest,
    key: &'a str,
    availability: FieldIntakeAvailability,
    readiness: Option<&'a dyn FieldIntakeProviderReadiness>,
    source: &'a SourceOrderedWorkflowRequest,
    clock: FieldIntakeCommandClock,
    ids: &'a FieldIntakeCommandGeneratedIds,
    plan: Option<FieldSetCreationPlan>,
    proof: Option<Proof>,
    written: Option<WrittenFieldSet>,
    evidence: Option<PlannedFieldIntakeCommandEvidence>,
    retained: Option<RetainedFieldIntakeCommandEvidence>,
}
impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        FieldAttachmentIntakeCommandSources.prepare(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let authority = FieldAttachmentIntakeAuthority::new(
            self.ctx.actor,
            self.ctx.holding,
            self.ctx.company,
            self.ctx.catalogue,
            self.ctx.clock,
        );
        let now = self.ctx.clock.now().get();
        let read = prepare_field_intake(
            c,
            &authority,
            self.ctx.actor,
            &self.input.target_id,
            false,
            now,
        )
        .await?;
        let write = prepare_field_intake(
            c,
            &authority,
            self.ctx.actor,
            &self.input.target_id,
            true,
            now,
        )
        .await?;
        if write.scope().version != self.input.expected_target_version {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "ATTACHMENT_TARGET_CHANGED",
                "The record changed. Refresh before adding files.",
            )
            .into());
        }
        erpx_server_files::public::field_visit_intake::validate_component_ref(
            erpx_server_files::public::field_visit_intake::component_ref(
                self.input.component_ref.as_deref(),
            ),
        )?;
        if let Some(readiness) = self.readiness {
            let status = readiness.probe().await.map_err(|_| BusinessError::new(
                FailureKind::Unavailable,
                "ATTACHMENT_SCANNER_UNAVAILABLE",
                "File intake is unavailable until the malware scanner and current signatures are ready.",
            ))?;
            self.availability.scanner_ready = status.scanner_enabled && status.scanner_ready;
            self.availability.preview_ready = status.preview_ready;
        }
        let plan = prepare_field_set_creation(
            c,
            &read,
            Some(&write),
            self.input,
            self.key,
            &self.availability,
            now,
        )
        .await?;
        if plan.disposition()
            == erpx_server_files::public::field_visit_intake::FieldSetDisposition::Replay
        {
            return Err(changed().into());
        }
        let source = proof(c, self.ctx, &self.input.target_id, true).await?;
        let result = ResultMetadata {
            set_id: plan.set_id().into(),
            expected_target_version: plan.expected_target_version(),
            policy_version: plan.policy_version(),
        };
        let scope = CommandEventScope {
            company_id: self.ctx.company.clone(),
            branch_id: read
                .scope()
                .branch_id
                .clone()
                .map(CommandField::Value)
                .unwrap_or(CommandField::Null),
            department_id: read
                .scope()
                .department_id
                .clone()
                .map(CommandField::Value)
                .unwrap_or(CommandField::Null),
        };
        let output = WorkflowOutput {
            version: result.expected_target_version,
            result: CommandJson::from_serializable(&result)?,
            event_scope: Some(scope),
        };
        self.evidence = Some(
            plan_field_intake_command_evidence(
                c,
                actor,
                r,
                self.source,
                &output,
                self.clock,
                self.ids,
            )
            .await?,
        );
        self.plan = Some(plan);
        self.proof = Some(source);
        Ok(output)
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let source = self.proof.as_ref().ok_or_else(changed)?;
        source
            .current(
                c,
                self.ctx.actor,
                self.ctx.holding,
                self.ctx.company,
                self.ctx.catalogue,
                self.ctx.clock,
            )
            .await?;
        let authority = FieldAttachmentIntakeAuthority::new(
            self.ctx.actor,
            self.ctx.holding,
            self.ctx.company,
            self.ctx.catalogue,
            self.ctx.clock,
        );
        let now = self.ctx.clock.now().get();
        let read = prepare_field_intake(
            c,
            &authority,
            self.ctx.actor,
            &self.input.target_id,
            false,
            now,
        )
        .await?;
        let write = prepare_field_intake(
            c,
            &authority,
            self.ctx.actor,
            &self.input.target_id,
            true,
            now,
        )
        .await?;
        let written = self
            .plan
            .as_ref()
            .ok_or_else(changed)?
            .apply_native(
                c,
                &read,
                Some(&write),
                now,
                self.evidence.as_ref().ok_or_else(changed)?,
            )
            .await?;
        source
            .current(
                c,
                self.ctx.actor,
                self.ctx.holding,
                self.ctx.company,
                self.ctx.catalogue,
                self.ctx.clock,
            )
            .await?;
        self.written = Some(written);
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
        self.proof
            .as_ref()
            .ok_or_else(changed)?
            .current(
                c,
                self.ctx.actor,
                self.ctx.holding,
                self.ctx.company,
                self.ctx.catalogue,
                self.ctx.clock,
            )
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
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
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
pub struct FieldSetCommandResponse {
    pub data: serde_json::Value,
    pub replayed: bool,
}

/// Execute a native Files set command or return exact legacy/native history.
/// Authorization, Files origin and Workflow evidence remain in the caller's serializable unit.
pub struct FieldAttachmentIntakeCommand<'a, C> {
    pub actor: &'a AuthorizedWorkContext,
    pub holding: &'a HoldingId,
    pub company: &'a CompanyId,
    pub catalogue: &'a CommercialCatalogue,
    pub clock: &'a C,
    pub input: &'a FieldSetRequest,
    pub key: &'a str,
    pub availability: &'a FieldIntakeAvailability,
}
pub async fn execute_field_attachment_intake<C: Clock + Sync>(
    c: &mut PgConnection,
    command: FieldAttachmentIntakeCommand<'_, C>,
) -> Result<FieldSetCommandResponse, Error> {
    execute_inner(c, command, None).await
}
pub async fn execute_field_attachment_intake_with_live_readiness<C: Clock + Sync>(
    c: &mut PgConnection,
    command: FieldAttachmentIntakeCommand<'_, C>,
    readiness: &dyn FieldIntakeProviderReadiness,
) -> Result<FieldSetCommandResponse, Error> {
    execute_inner(c, command, Some(readiness)).await
}
async fn execute_inner<C: Clock + Sync>(
    c: &mut PgConnection,
    command: FieldAttachmentIntakeCommand<'_, C>,
    readiness: Option<&dyn FieldIntakeProviderReadiness>,
) -> Result<FieldSetCommandResponse, Error> {
    let FieldAttachmentIntakeCommand {
        actor,
        holding,
        company,
        catalogue,
        clock,
        input,
        key,
        availability,
    } = command;
    validate_key(key)?;
    FieldAttachmentIntakeCommandSources.prepare(c).await?;
    let ctx = AttachmentContext {
        actor,
        holding,
        company,
        catalogue,
        clock,
    };
    let input_json = CommandJson::from_serializable(&CommandInput {
        wire_key: key,
        target_type: &input.target_type,
        target_id: &input.target_id,
        component_kind: &input.component_kind,
        component_ref: erpx_server_files::public::field_visit_intake::component_ref(
            input.component_ref.as_deref(),
        ),
        purpose_key: &input.purpose_key,
        expected_target_version: input.expected_target_version,
    })?;
    let identity = WorkflowRecordIdentity::from_utf16(input.target_id.encode_utf16().collect());
    let ledger_key = erpx_server_files::public::field_visit_intake::field_set_command_id(
        company.as_str(),
        actor.user_id().as_str(),
        key,
    );
    let request = WorkflowRequest {
        key: ledger_key.clone(),
        operation: "attachment.set.create".into(),
        record_id: identity.physical_text().into(),
        input: input_json.clone(),
    };
    let ordered = SourceOrderedWorkflowRequest {
        key: ledger_key,
        operation: request.operation.clone(),
        record_id: identity,
        input: input_json,
    };
    let lookup = FieldAttachmentIntakeAuthority::new(actor, holding, company, catalogue, clock);
    let read = prepare_field_intake(
        c,
        &lookup,
        actor,
        &input.target_id,
        false,
        clock.now().get(),
    )
    .await?;
    let recorded =
        recorded_field_set(c, &read, input, key, availability, clock.now().get()).await?;
    let fixed = FieldIntakeCommandClock::sample(clock)?;
    let mut ids = Ids;
    let generated = FieldIntakeCommandGeneratedIds::reserve(&mut ids);
    let mut wrapped = generated.wrap(&mut ids);
    let mut authority = Authority {
        ctx: &ctx,
        request: &request,
        proof: None,
        replay: false,
    };
    let mut effects = Effects {
        ctx: &ctx,
        input,
        key,
        availability: availability.clone(),
        readiness,
        source: &ordered,
        clock: fixed,
        ids: &generated,
        plan: None,
        proof: None,
        written: None,
        evidence: None,
        retained: None,
    };
    if let Some(recorded) = recorded
        .as_ref()
        .filter(|r| r.kind == RecordedFieldSetKind::Native)
    {
        let metadata = ResultMetadata {
            set_id: recorded.data["id"].as_str().ok_or_else(changed)?.into(),
            expected_target_version: recorded.data["expectedTargetVersion"]
                .as_i64()
                .ok_or_else(changed)?,
            policy_version: recorded.data["policyVersion"]
                .as_i64()
                .ok_or_else(changed)?,
        };
        let historical_scope = CommandEventScope {
            company_id: company.clone(),
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
        };
        let output = WorkflowOutput {
            version: metadata.expected_target_version,
            result: CommandJson::from_serializable(&metadata)?,
            event_scope: Some(historical_scope),
        };
        let current_actor = command_actor(actor, clock.now().get())?;
        verify_historical_field_intake_command(
            c,
            &current_actor,
            &request,
            &ordered,
            &output,
            recorded.anchor.as_ref().ok_or_else(changed)?,
        )
        .await?;
        read.require_current(c, clock.now().get()).await?;
        let current = recorded_field_set(c, &read, input, key, availability, clock.now().get())
            .await?
            .ok_or_else(changed)?;
        if current.kind != RecordedFieldSetKind::Native
            || current.data != recorded.data
            || current.anchor != recorded.anchor
            || current.branch_id != recorded.branch_id
            || current.department_id != recorded.department_id
        {
            return Err(changed().into());
        }
        return Ok(FieldSetCommandResponse {
            data: current.data,
            replayed: true,
        });
    }
    let replay = PgCommandLedger::new(company.clone(), c, &mut authority, &fixed, &mut wrapped)
        .replay_source_ordered(&ordered, &mut effects)
        .await?;
    if replay.is_some() {
        return Err(changed().into());
    }
    if let Some(recorded) = recorded {
        if recorded.kind != RecordedFieldSetKind::Legacy {
            return Err(changed().into());
        }
        read.require_current(c, clock.now().get()).await?;
        return Ok(FieldSetCommandResponse {
            data: recorded.data,
            replayed: true,
        });
    }
    let outcome = PgCommandLedger::new(company.clone(), c, &mut authority, &fixed, &mut wrapped)
        .execute_source_ordered(&ordered, &mut effects)
        .await?;
    if outcome.replayed {
        return Err(changed().into());
    }
    effects
        .proof
        .as_ref()
        .ok_or_else(changed)?
        .current(c, actor, holding, company, catalogue, clock)
        .await?;
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
    read.require_current(c, clock.now().get()).await?;
    Ok(FieldSetCommandResponse {
        data: effects.written.as_ref().ok_or_else(changed)?.data().clone(),
        replayed: false,
    })
}
