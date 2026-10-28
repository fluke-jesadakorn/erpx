//! Explicit development-only manual Finance journal creation composition.
use crate::finance_journal_review::{FinanceJournalEnvironment, command_actor, finance_actor};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, lock_commercial_authority_sources,
};
use erpx_server_finance::public::{
    FinanceActor, FinanceCollation, JournalAuthority, JournalDraft, JournalError,
    JournalPreparation, JournalStatus, PgJournalPreparation, PgJournalStore,
    PgManualJournalCreateSources, embedded_currency_rules,
    journal_create_wire::ManualJournalCreateWire,
    journal_review::{
        CurrentJournalReviewScope, JournalReviewCommand, JournalReviewInput, JournalReviewRootKind,
        inspect_journal_review_source, lock_journal_review_sources,
    },
    prepare_journal,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
};
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
pub(crate) fn denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_CURRENT_AUTHORITY_REQUIRED",
        "Choose the current authorized Finance workspace.",
    )
}
pub(crate) fn finance_error(error: PgTransactionError<JournalError>) -> Error {
    match error {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => BusinessError::new(
            e.kind,
            e.code,
            "The current Finance journal could not be created.",
        )
        .into(),
    }
}
#[derive(Clone)]
pub struct FinanceJournalCreateConfiguration {
    pub(crate) holding: HoldingId,
    pub(crate) company: CompanyId,
    pub(crate) catalogue: Arc<CommercialCatalogue>,
}
impl FinanceJournalCreateConfiguration {
    pub fn new(
        environment: FinanceJournalEnvironment,
        holding: HoldingId,
        company: CompanyId,
        catalogue: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        if matches!(environment, FinanceJournalEnvironment::Production) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "FINANCE_PRODUCTION_COMPOSITION_UNAVAILABLE",
                "Finance production release and Company acceptance ports are incomplete.",
            ));
        }
        Ok(Self {
            holding,
            company,
            catalogue,
        })
    }
}
impl HoldingWorkSourcePreparation for FinanceJournalCreateConfiguration {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_journal_review_sources(c).await
        })
    }
}
pub struct FinanceJournalCreateService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FinanceJournalCreateConfiguration,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> FinanceJournalCreateService<C> {
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
    pub async fn create<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, ManualJournalCreateWire), BusinessError>
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
                    Box::pin(async move {
                        let (key, wire) = prepare()?;
                        let journal_id = RecordId::new(format!("journal-{}", key.as_str()))?;
                        let request = WorkflowRequest {
                            key: key.as_str().into(),
                            operation: "finance.journal.create".into(),
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
pub(crate) struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("UUID is bounded")
    }
}
pub(crate) struct Authority<'a, C> {
    pub(crate) current: &'a AuthorizedWorkContext,
    pub(crate) configuration: &'a FinanceJournalCreateConfiguration,
    pub(crate) clock: &'a C,
    pub(crate) purchase: Option<(CapabilityPurpose, PurchaseWitness)>,
    pub(crate) source: Option<CurrentJournalReviewScope>,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.configuration.prepare(c).await
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if self.current.holding_id() != &self.configuration.holding
            || self.current.selection().workspace().company_id != self.configuration.company
            || self.clock.now().get() >= self.current.expires_at()
        {
            return Err(denied().into());
        }
        self.current.require_permission("finance.read")?;
        self.current.require_permission("finance.write")?;
        self.current.require_permission("module.finance.read")?;
        Ok(command_actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if disposition == CommandDisposition::Replay {
            if let Some(source) = &self.source {
                source.require_current(c).await?;
            } else {
                let command = JournalReviewCommand {
                    key: CommandKey::new(&request.key)?,
                    journal_id: RecordId::new(&request.record_id)?,
                    input: JournalReviewInput::Post {
                        expected_version: 1,
                    },
                };
                let source =
                    inspect_journal_review_source(c, &finance_actor(self.current), &command)
                        .await?;
                if source.root_kind() != JournalReviewRootKind::Manual
                    || source.journal_id() != source.root_journal_id()
                {
                    return Err(denied().into());
                }
                self.source = Some(source);
            }
        }
        let purpose = if disposition == CommandDisposition::Replay {
            CapabilityPurpose::History
        } else {
            CapabilityPurpose::NewWork
        };
        let mut reader = PgCompanyPurchaseReader::new(
            self.configuration.holding.clone(),
            self.configuration.company.clone(),
            c,
            &self.configuration.catalogue,
        );
        if let Some((original, witness)) = &self.purchase {
            if original != &purpose {
                return Err(denied().into());
            }
            reader
                .revalidate(witness, self.clock.now())
                .await?
                .require_allowed()?;
        } else {
            let inspection = reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: vec!["finance.accounting".into()],
                        purpose,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    self.clock.now(),
                )
                .await?;
            inspection.require_allowed()?;
            self.purchase = Some((purpose, inspection.witness));
        }
        Ok(())
    }
}
pub(crate) struct PreparationAuthority<'a> {
    pub(crate) current: &'a AuthorizedWorkContext,
}
impl JournalAuthority for PreparationAuthority<'_> {
    type Error = PgTransactionError<JournalError>;
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        if actor != &finance_actor(self.current)
            || !self.current.has_permission("finance.write")
            || !self.current.has_permission("finance.read")
            || !self.current.has_permission("module.finance.read")
            || draft.source_type != "manual"
            || draft.source_id.is_some()
            || draft.is_opening
        {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "FINANCE_CURRENT_AUTHORITY_REQUIRED",
                message: "Choose the current authorized Finance workspace.".into(),
            }
            .into());
        }
        Ok(())
    }
}
struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    actor: FinanceActor,
    draft: JournalDraft,
    clock: &'a C,
    plan: Option<JournalPreparation>,
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
        let result = CommandJson::from_serializable(plan.result())?;
        self.plan = Some(plan);
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
        let plan = self.plan.as_ref().ok_or_else(denied)?;
        PgJournalStore::new(
            self.actor.holding_id.clone(),
            self.actor.company_id.clone(),
            c,
        )
        .persist(plan)
        .await
        .map_err(finance_error)
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
        PgJournalStore::new(
            self.actor.holding_id.clone(),
            self.actor.company_id.clone(),
            &mut *c,
        )
        .retain_manual_draft(plan)
        .await
        .map_err(finance_error)?;
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
