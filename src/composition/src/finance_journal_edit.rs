//! Explicit development-only manual Finance journal edit composition.
use crate::finance_journal_create::{
    Authority, FinanceJournalCreateConfiguration, Ids, PreparationAuthority, denied, finance_error,
};
use crate::finance_journal_review::{command_actor, finance_actor};
use erpx_server_finance::public::{
    FinanceActor, FinanceCollation, JournalDraft, JournalPreparation, JournalStatus,
    ManualJournalEditResult, PgJournalPreparation, PgManualJournalCreateSources,
    embedded_currency_rules,
    journal_create_wire::ManualJournalEditWire,
    journal_review::lock_journal_review_sources,
    manual_draft_edit::{
        AppliedManualDraftEdit, CapturedManualDraftEdit, apply_manual_draft_edit,
        capture_manual_draft_edit, retain_manual_draft_edit,
    },
    prepare_journal,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest},
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, RecordId};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, PgCommandAuthority, PgCommandLedger,
    PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use erpx_server_workflow::public::record_origin::{
    RecordOriginRequest, read_record_origin, verify_record_origin,
};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
pub struct FinanceJournalEditService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FinanceJournalCreateConfiguration,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> FinanceJournalEditService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: FinanceJournalCreateConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }
    pub async fn edit<Input, Prepare>(
        &self,
        journal_id: String,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, ManualJournalEditWire), BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let company = self.configuration.company.clone();
        let scope = move || {
            let mut selection = input()?;
            if selection.company_id != company {
                return Err(denied());
            }
            selection.mutation = true;
            Ok(selection)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                scope,
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/journals".into(),
                    api: true,
                },
                configuration.clone(),
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let prepare = prepare.clone();
                    let journal_id = journal_id.clone();
                    Box::pin(async move {
                        let (key, wire) = prepare()?;
                        let journal_id = RecordId::new(&journal_id)?;
                        let request = WorkflowRequest {
                            key: key.as_str().into(),
                            operation: "finance.manual-draft.edit".into(),
                            record_id: journal_id.as_str().into(),
                            input: wire.normalized().clone(),
                        };
                        let actor = finance_actor(current);
                        let mut authority = Authority {
                            current,
                            configuration: &configuration,
                            clock: clock.as_ref(),
                            purchase: None,
                            source: None,
                        };
                        let mut effects = Effects {
                            current,
                            actor,
                            draft: wire.draft().clone(),
                            journal_id: journal_id.as_str().into(),
                            expected_version: wire.expected_version(),
                            reason: wire.reason().into(),
                            captured: None,
                            applied: None,
                            clock: clock.as_ref(),
                            plan: None,
                            sources: None,
                        };
                        let mut ids = Ids;
                        let outcome = PgCommandLedger::new(
                            configuration.company.clone(),
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
                                .await?;
                            let actor = command_actor(current)?;
                            effects.retain(c, &actor, &request).await?;
                            authority
                                .verify_command(c, &actor, &request, CommandDisposition::Execute)
                                .await?;
                            let origin = read_record_origin(
                                c,
                                RecordOriginRequest {
                                    command_key: request.key.clone(),
                                    operation: request.operation.clone(),
                                    record_id: request.record_id.clone(),
                                    creator_user_id: current.user_id().clone(),
                                    company_id: configuration.company.clone(),
                                },
                            )
                            .await?
                            .ok_or_else(denied)?;
                            if origin.result() != &outcome.result {
                                return Err(denied().into());
                            }
                            verify_record_origin(c, &origin).await?;
                        }
                        Ok(outcome.result)
                    })
                },
            )
            .await
    }
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    actor: FinanceActor,
    draft: JournalDraft,
    clock: &'a C,
    plan: Option<JournalPreparation>,
    journal_id: String,
    expected_version: i64,
    reason: String,
    captured: Option<CapturedManualDraftEdit>,
    applied: Option<AppliedManualDraftEdit>,
    sources: Option<PgManualJournalCreateSources>,
}
impl<C: Clock + Sync + Send> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_journal_review_sources(c).await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        self.captured = Some(
            capture_manual_draft_edit(
                &mut *c,
                &self.actor,
                &self.journal_id,
                &self.draft,
                self.expected_version,
                &self.reason,
            )
            .await?,
        );
        self.sources = Some(
            PgManualJournalCreateSources::capture(&mut *c, &self.actor, &self.draft)
                .await
                .map_err(finance_error)?,
        );
        let rules = embedded_currency_rules().map_err(|_| {
            BusinessError::invalid(
                "CURRENCY_CATALOGUE_UNAVAILABLE",
                "Currency rules are unavailable.",
            )
        })?;
        let collation = FinanceCollation::english().map_err(|_| {
            BusinessError::invalid(
                "FINANCE_COLLATION_UNAVAILABLE",
                "Finance collation is unavailable.",
            )
        })?;
        let mut authority = PreparationAuthority {
            current: self.current,
        };
        let mut ports = PgJournalPreparation::new(
            c,
            &self.actor,
            &mut authority,
            self.clock,
            &rules,
            &collation,
        )
        .map_err(finance_error)?;
        let plan = prepare_journal(
            &mut ports,
            &self.actor,
            &self.draft,
            &CommandKey::new(&request.key)?,
            JournalStatus::Draft,
        )
        .await
        .map_err(finance_error)?;
        let version = self.expected_version + 1;
        let result = CommandJson::from_serializable(&ManualJournalEditResult {
            id: self.journal_id.clone(),
            status: JournalStatus::Draft,
            version,
        })?;
        self.plan = Some(plan);
        Ok(WorkflowOutput {
            version,
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
        let plan = self.plan.as_ref().ok_or_else(denied)?;
        let captured = self.captured.take().ok_or_else(denied)?;
        self.applied = Some(apply_manual_draft_edit(c, captured, plan).await?);
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<(), Error> {
        let plan = self.plan.as_ref().ok_or_else(denied)?;
        self.sources
            .as_ref()
            .ok_or_else(denied)?
            .require_current(&mut *c)
            .await
            .map_err(finance_error)?;
        let applied = self.applied.as_ref().ok_or_else(denied)?;
        let proof =
            retain_manual_draft_edit(&mut *c, applied, &CommandKey::new(&request.key)?).await?;
        proof.require_current(&mut *c).await?;
        // Re-read chart, period, scale, account and FX evidence after all owner
        // effects. The same locks and transaction witness are retained.
        let rules = embedded_currency_rules().map_err(|_| denied())?;
        let collation = FinanceCollation::english().map_err(|_| denied())?;
        let mut authority = PreparationAuthority {
            current: self.current,
        };
        let mut ports = PgJournalPreparation::new(
            c,
            &self.actor,
            &mut authority,
            self.clock,
            &rules,
            &collation,
        )
        .map_err(finance_error)?;
        let fresh = prepare_journal(
            &mut ports,
            &self.actor,
            &self.draft,
            &CommandKey::new(&request.key)?,
            JournalStatus::Draft,
        )
        .await
        .map_err(finance_error)?;
        let same = plan.chart().id == fresh.chart().id
            && plan.chart().base_currency == fresh.chart().base_currency
            && plan.fx() == fresh.fx()
            && serde_json::to_value(plan.prepared_fx_evidence()).map_err(|_| denied())?
                == serde_json::to_value(fresh.prepared_fx_evidence()).map_err(|_| denied())?
            && serde_json::to_value(&plan.amounts().lines).map_err(|_| denied())?
                == serde_json::to_value(&fresh.amounts().lines).map_err(|_| denied())?
            && plan.amounts().total == fresh.amounts().total;
        if !same {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "JOURNAL_SOURCE_CHANGED",
                "The prepared manual journal changed.",
            )
            .into());
        }
        Ok(())
    }
}
