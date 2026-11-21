//! Development-only actual Finance/Identity/Commercial assembly. This library
//! mounts no route and cannot substitute Legal acceptance for Finance release.
pub(crate) use crate::finance_field_service_lineage as field_service_parts;
mod journal_post_flow;
#[cfg(feature = "existing-business")]
mod maintenance_parts;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, lock_commercial_authority_sources,
};
use erpx_server_finance::public::{
    FinanceActor, JournalError,
    journal_review::{
        CurrentJournalReviewScope, FinanceJournalAccessPurpose, JournalReviewCommand,
        JournalReviewInput, JournalReviewResult, JournalReviewRootKind, JournalReviewerFacts,
        PgFinanceJournalReviewPolicy, PgJournalReview, ReviewJournal,
        inspect_field_service_parts_journal_target, inspect_journal_review_source,
        inspect_maintenance_parts_journal_target, lock_journal_review_sources, review_journal,
    },
    journal_review_wire::NormalizedJournalReviewCommand,
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
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
    TimestampMillis, UserId,
};
use erpx_server_workflow::public::commands::CommandActor;
use field_service_parts::FieldServicePartsSource;
#[cfg(feature = "existing-business")]
use maintenance_parts::MaintenancePartsSource;
use sqlx::PgConnection;
use std::sync::Arc;
use tokio::sync::Mutex;

type Error = PgTransactionError<BusinessError>;

pub use crate::finance_journal_environment::FinanceJournalEnvironment;
#[derive(Clone)]
pub struct FinanceJournalConfiguration {
    holding_id: HoldingId,
    company_id: CompanyId,
    purchases: Arc<CommercialCatalogue>,
    environment: FinanceJournalEnvironment,
    enforce_post_flow: bool,
    allow_manual_root_reversal: bool,
    allow_field_service_post: bool,
}
impl FinanceJournalConfiguration {
    pub fn new(
        environment: FinanceJournalEnvironment,
        holding_id: HoldingId,
        company_id: CompanyId,
        purchases: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        Ok(Self {
            holding_id,
            company_id,
            purchases,
            environment,
            enforce_post_flow: false,
            allow_manual_root_reversal: false,
            allow_field_service_post: false,
        })
    }
    /// Explicit native HTTP journal post composition; existing foundation
    /// callers retain their previous constructor and behavior.
    pub fn new_enforced_post(
        environment: FinanceJournalEnvironment,
        holding_id: HoldingId,
        company_id: CompanyId,
        purchases: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        let mut config = Self::new(environment, holding_id, company_id, purchases)?;
        config.enforce_post_flow = true;
        Ok(config)
    }
    /// Closed private HTTP policy for the explicitly enabled actions. Reversal
    /// is limited to a protected direct manual root and does not use post Flow.
    pub fn new_private_actions(
        environment: FinanceJournalEnvironment,
        holding_id: HoldingId,
        company_id: CompanyId,
        purchases: Arc<CommercialCatalogue>,
        post: bool,
        reverse: bool,
    ) -> Result<Self, BusinessError> {
        if !post && !reverse {
            return Err(BusinessError::invalid(
                "FINANCE_REVIEW_CONFIGURATION_INVALID",
                "Enable an explicit journal action.",
            ));
        }
        let mut config = Self::new(environment, holding_id, company_id, purchases)?;
        config.enforce_post_flow = post;
        config.allow_manual_root_reversal = reverse;
        Ok(config)
    }
    /// Narrow private opt-in; actual owner lineage is still required per command.
    pub fn with_field_service_post(mut self) -> Result<Self, BusinessError> {
        if !self.enforce_post_flow {
            return Err(BusinessError::invalid(
                "FINANCE_REVIEW_CONFIGURATION_INVALID",
                "Enable journal post first.",
            ));
        }
        self.allow_field_service_post = true;
        Ok(self)
    }
    async fn lock_sources(&self, c: &mut PgConnection) -> Result<(), Error> {
        lock_commercial_authority_sources(c).await?;
        lock_journal_review_sources(c).await?;
        erpx_server_inventory::public::consumption_finance::lock_consumption_finance_sources(c)
            .await?;
        #[cfg(feature = "existing-business")]
        erpx_server_maintenance::public::finance_lineage::lock_maintenance_finance_lineage_sources(
            c,
        )
        .await?;
        erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
        if self.enforce_post_flow {
            erpx_server_identity_access::public::journal_flow_participants::lock_journal_flow_participant_sources(c).await?;
            erpx_server_people_payroll::public::journal_flow_people::lock_journal_flow_people_sources(c).await?;
            erpx_server_organization::public::journal_flow_department::lock_journal_flow_department_sources(c).await?;
            erpx_server_workflow::public::journal_post_flow::lock_journal_post_flow_sources(c)
                .await?;
        }
        erpx_server_field_service::public::finance_lineage::lock_field_service_parts_finance_sources(c).await?;
        if matches!(self.environment, FinanceJournalEnvironment::Production) {
            crate::finance_production_release::require_production_release(c, &self.company_id)
                .await?;
        }
        Ok(())
    }
}
impl HoldingWorkSourcePreparation for FinanceJournalConfiguration {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move { self.lock_sources(c).await })
    }
}

pub struct FinanceJournalReviewAdministration<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FinanceJournalConfiguration,
    clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> FinanceJournalReviewAdministration<C> {
    #[cfg(any(feature = "business-http", feature = "finance-field-write"))]
    pub(crate) fn company_id(&self) -> &CompanyId {
        &self.configuration.company_id
    }

    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: FinanceJournalConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }
    /// Only the Holding callback is replayable. The actual Control request
    /// lease is retained, never retried around an already committed Holding unit.
    pub async fn review(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        command: JournalReviewCommand,
    ) -> Result<JournalReviewResult, Error> {
        self.review_prepared(
            host,
            proof,
            move || Ok(workspace),
            move || NormalizedJournalReviewCommand::from_typed(command.clone()),
            "finance.write",
        )
        .await
    }
    /// Transport parsing is pure and deferred until retained authentication and
    /// the same current Finance read context. It cannot substitute an actor or
    /// bypass the canonical write/review/source/purchase policy.
    pub async fn review_transport<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        prepare: Prepare,
    ) -> Result<JournalReviewResult, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<NormalizedJournalReviewCommand, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        self.review_prepared(host, proof, workspace, prepare, "finance.read")
            .await
    }
    async fn review_prepared<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        prepare: Prepare,
        permission: &'static str,
    ) -> Result<JournalReviewResult, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<NormalizedJournalReviewCommand, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let company = self.configuration.company_id.clone();
        let workspace = move || {
            let mut workspace = workspace()?;
            if workspace.company_id != company {
                return Err(scope_error());
            }
            workspace.mutation = true;
            Ok(workspace)
        };
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                workspace,
                WorkAuthorizationRequest {
                    permission: permission.into(),
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
                        let normalized = prepare()?;
                        let command = normalized.command().clone();
                        let actor = finance_actor(current);
                        let policy = Mutex::new(CurrentFinancePolicy {
                            current,
                            clock: clock.as_ref(),
                            configuration: &configuration,
                            source: None,
                            purchase: None,
                            flow: None,
                            post_command: matches!(command.input, JournalReviewInput::Post { .. }),
                        });
                        let mut ids = ReviewIds;
                        let mut ports = PgJournalReview::new(
                            configuration.holding_id.clone(),
                            configuration.company_id.clone(),
                            c,
                            &policy,
                            clock.as_ref(),
                            &mut ids,
                        )
                        .map_err(finance_error)?
                        .with_normalized_command(normalized)
                        .map_err(finance_error)?;
                        review_journal(&mut ports, &actor, &command)
                            .await
                            .map_err(finance_error)
                    })
                },
            )
            .await
    }
}
struct ReviewIds;
impl IdentifierSource for ReviewIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string())
            .expect("UUID is a bounded record identifier")
    }
}
fn finance_error(error: PgTransactionError<JournalError>) -> Error {
    match error {
        PgTransactionError::Persistence(error) => PgTransactionError::Persistence(error),
        PgTransactionError::Business(error) => BusinessError::new(
            error.kind,
            error.code,
            "The current Finance journal review could not be completed.",
        )
        .into(),
    }
}
fn scope_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_CURRENT_AUTHORITY_REQUIRED",
        "Choose the current authorized Finance workspace.",
    )
}
pub(crate) use crate::finance_journal_actor::{command_actor, finance_actor};
struct CurrentFinancePolicy<'a> {
    current: &'a AuthorizedWorkContext,
    clock: &'a (dyn Clock + Send + Sync),
    configuration: &'a FinanceJournalConfiguration,
    source: Option<CurrentFinanceSource>,
    purchase: Option<(CapabilityPurpose, PurchaseWitness)>,
    flow: Option<journal_post_flow::JournalPostFlow>,
    post_command: bool,
}
enum CurrentFinanceSource {
    Owned(Box<CurrentJournalReviewScope>),
    #[cfg(feature = "existing-business")]
    MaintenanceParts(Box<MaintenancePartsSource>),
    FieldServiceParts(Box<FieldServicePartsSource>),
}
impl CurrentFinanceSource {
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        match self {
            Self::Owned(s) => s.require_current(c).await,
            #[cfg(feature = "existing-business")]
            Self::MaintenanceParts(s) => s.require_current(c).await,
            Self::FieldServiceParts(s) => s.require_current(c).await,
        }
    }
}
impl CurrentFinancePolicy<'_> {
    fn current(&self, actor: &FinanceActor, now: i64) -> Result<CommandActor, BusinessError> {
        if actor != &finance_actor(self.current)
            || actor.holding_id != self.configuration.holding_id
            || actor.company_id != self.configuration.company_id
            || now >= self.current.expires_at()
        {
            return Err(scope_error());
        }
        self.current.require_permission("finance.read")?;
        self.current.require_permission("finance.write")?;
        self.current.require_permission("module.finance.read")?;
        command_actor(self.current)
    }
    async fn require_field_selected_current(
        &self,
        c: &mut PgConnection,
        now: i64,
    ) -> Result<(), Error> {
        if matches!(
            self.source,
            Some(CurrentFinanceSource::FieldServiceParts(_))
        ) {
            erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current(c,self.current,now).await.map_err(|error|match error {
                PgTransactionError::Business(e) if matches!(e.code,"WORK_ROLE_UNAVAILABLE"|"QUEUED_AUTHORITY_CHANGED") => BusinessError::new(FailureKind::Forbidden,"FINANCE_ACCESS_CHANGED","The current Finance authority changed before posting committed.").into(),
                other=>other,
            })?;
        }
        Ok(())
    }
    fn selected(&self, actor: &CommandActor, now: i64) -> Result<(), BusinessError> {
        if actor != &self.current(&finance_actor(self.current), now)? {
            return Err(scope_error());
        }
        Ok(())
    }
}
impl PgFinanceJournalReviewPolicy for CurrentFinancePolicy<'_> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        actor: &FinanceActor,
        _: &JournalReviewCommand,
    ) -> Result<(), Error> {
        if actor != &finance_actor(self.current) {
            return Err(scope_error().into());
        }
        self.configuration.lock_sources(c).await
    }
    async fn authorize_current(
        &mut self,
        _: &mut PgConnection,
        actor: &FinanceActor,
        _: &JournalReviewCommand,
        now: i64,
    ) -> Result<CommandActor, Error> {
        Ok(self.current(actor, now)?)
    }
    async fn reviewer(
        &mut self,
        _: &mut PgConnection,
        actor: &CommandActor,
        journal: Option<&ReviewJournal>,
        now: i64,
    ) -> Result<JournalReviewerFacts, Error> {
        self.selected(actor, now)?;
        Ok(JournalReviewerFacts {
            can_read: self.current.has_permission("finance.read"),
            can_approve: self.current.has_permission("finance.approve"),
            same_person: journal
                .map(|j| UserId::new(j.created_by.clone()))
                .transpose()?
                .as_ref()
                .is_some_and(|creator| self.current.same_person_user(creator)),
        })
    }
    async fn ready(
        &mut self,
        _: &mut PgConnection,
        actor: &CommandActor,
        now: i64,
    ) -> Result<(), Error> {
        // B0 development readiness is the real current Finance read/write
        // resource context. General production release is rejected at config.
        Ok(self.selected(actor, now)?)
    }
    async fn prepare_post_flow(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        journal: &ReviewJournal,
        command: &JournalReviewCommand,
        now: i64,
    ) -> Result<(), Error> {
        if self.configuration.enforce_post_flow && self.post_command {
            self.selected(actor, now)?;
            if self.flow.is_some() {
                return Err(scope_error().into());
            }
            let source = match &self.source {
                Some(CurrentFinanceSource::Owned(_)) => journal_post_flow::JournalFlowSource::Owned,
                Some(CurrentFinanceSource::FieldServiceParts(source))
                    if self.configuration.allow_field_service_post =>
                {
                    journal_post_flow::JournalFlowSource::FieldService(source)
                }
                _ => return Err(scope_error().into()),
            };
            self.flow = Some(
                journal_post_flow::JournalPostFlow::prepare(
                    c,
                    self.current,
                    journal,
                    command,
                    now,
                    source,
                )
                .await?,
            );
        }
        Ok(())
    }
    async fn apply_post_flow_before(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        if self.configuration.enforce_post_flow && self.post_command {
            self.flow
                .as_ref()
                .ok_or_else(scope_error)?
                .before(c)
                .await?;
        }
        Ok(())
    }
    async fn apply_post_flow_after(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        if self.configuration.enforce_post_flow && self.post_command {
            self.flow.as_ref().ok_or_else(scope_error)?.after(c).await?;
        }
        Ok(())
    }
    async fn retain_post_flow(&mut self, c: &mut PgConnection) -> Result<(), Error> {
        // Current Finance authority takes precedence over the derived Flow
        // primary view after deferred effects; initial primary policy is intact.
        self.require_field_selected_current(c, self.clock.now().get())
            .await?;
        if self.configuration.enforce_post_flow && self.post_command {
            self.flow
                .as_ref()
                .ok_or_else(scope_error)?
                .retain(c)
                .await?;
        }
        Ok(())
    }
    async fn require_source_access(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        command: &JournalReviewCommand,
        purpose: FinanceJournalAccessPurpose,
        now: i64,
    ) -> Result<(), Error> {
        self.selected(actor, now)?;
        match &self.source {
            Some(source) => {
                if matches!(source, CurrentFinanceSource::FieldServiceParts(_)) {
                    self.require_field_selected_current(c, now).await?;
                }
                source.require_current(c).await?;
            }
            None => {
                self.source = Some(
                    match inspect_maintenance_parts_journal_target(
                        c,
                        &finance_actor(self.current),
                        command,
                    )
                    .await?
                    {
                        Some(target) => {
                            #[cfg(feature = "existing-business")]
                            {
                                CurrentFinanceSource::MaintenanceParts(Box::new(
                                    MaintenancePartsSource::retain(c, target).await?,
                                ))
                            }
                            #[cfg(not(feature = "existing-business"))]
                            {
                                let _ = target;
                                return Err(BusinessError::new(
                                    FailureKind::Unavailable,
                                    "MAINTENANCE_RUNTIME_UNAVAILABLE",
                                    "This runtime does not include Maintenance journal review.",
                                )
                                .into());
                            }
                        }
                        None => match match inspect_field_service_parts_journal_target(
                            c,
                            &finance_actor(self.current),
                            command,
                        )
                        .await
                        {
                            Ok(target) => target,
                            Err(error) => {
                                // Preserve ordinary purchase denial for an unproved
                                // Field source; never admit it through included rights.
                                if matches!(&error,PgTransactionError::Business(e) if e.code=="FINANCE_JOURNAL_SOURCE_UNSUPPORTED")
                                {
                                    PgCompanyPurchaseReader::new(
                                        self.configuration.holding_id.clone(),
                                        self.configuration.company_id.clone(),
                                        c,
                                        &self.configuration.purchases,
                                    )
                                    .inspect(
                                        &PurchaseRequest {
                                            capabilities: vec!["finance.accounting".into()],
                                            purpose: CapabilityPurpose::History,
                                            mode: PackageEnforcementMode::Enforced,
                                        },
                                        TimestampMillis::new(now),
                                    )
                                    .await?
                                    .require_allowed()?;
                                }
                                return Err(error);
                            }
                        } {
                            Some(target) => CurrentFinanceSource::FieldServiceParts(Box::new(
                                FieldServicePartsSource::retain(c, target).await?,
                            )),
                            None => CurrentFinanceSource::Owned(Box::new(
                                inspect_journal_review_source(
                                    c,
                                    &finance_actor(self.current),
                                    command,
                                )
                                .await?,
                            )),
                        },
                    },
                );
            }
        }
        if self.configuration.enforce_post_flow || self.configuration.allow_manual_root_reversal {
            match (&self.source, &command.input) {
                (Some(CurrentFinanceSource::Owned(source)), JournalReviewInput::Post { .. })
                    if self.configuration.enforce_post_flow
                        && source.journal_id() == source.root_journal_id() => {}
                (
                    Some(CurrentFinanceSource::FieldServiceParts(_)),
                    JournalReviewInput::Post { .. },
                ) if self.configuration.enforce_post_flow
                    && self.configuration.allow_field_service_post => {}
                (Some(CurrentFinanceSource::Owned(source)), JournalReviewInput::Reverse { .. })
                    if self.configuration.allow_manual_root_reversal
                        && source.root_kind() == JournalReviewRootKind::Manual
                        && source.journal_id() == source.root_journal_id() => {}
                _ => {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "JOURNAL_FLOW_SOURCE_UNSUPPORTED",
                        "This journal action or source is not available in the private API.",
                    )
                    .into());
                }
            }
        }
        if purpose == FinanceJournalAccessPurpose::BaselineRead {
            return Ok(());
        }
        let purpose = match purpose {
            FinanceJournalAccessPurpose::ExactReplayHistory => CapabilityPurpose::History,
            FinanceJournalAccessPurpose::SettleExisting => CapabilityPurpose::SettleExisting,
            FinanceJournalAccessPurpose::BaselineRead => unreachable!(),
        };
        let mut purchases = PgCompanyPurchaseReader::new(
            self.configuration.holding_id.clone(),
            self.configuration.company_id.clone(),
            c,
            &self.configuration.purchases,
        );
        if let Some((original_purpose, witness)) = &self.purchase {
            if original_purpose != &purpose {
                return Err(scope_error().into());
            }
            purchases
                .revalidate(witness, TimestampMillis::new(now))
                .await?
                .require_allowed()?;
        } else {
            let inspection = purchases
                .inspect(
                    &PurchaseRequest {
                        capabilities: vec!["finance.accounting".into()],
                        purpose,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    TimestampMillis::new(now),
                )
                .await?;
            let witness = match inspection.require_allowed() {
                Ok(_) => inspection.witness,
                Err(base_error) => {
                    let included_capability = match &self.source {
                        #[cfg(feature = "existing-business")]
                        Some(CurrentFinanceSource::MaintenanceParts(_)) => {
                            Some("assets.parts-costs")
                        }
                        Some(CurrentFinanceSource::FieldServiceParts(_)) => {
                            Some("finance.service-invoice")
                        }
                        _ => None,
                    };
                    if let Some(capability) = included_capability
                        .filter(|_| matches!(command.input, JournalReviewInput::Post { .. }))
                    {
                        let included = purchases
                            .inspect(
                                &PurchaseRequest {
                                    capabilities: vec![capability.into()],
                                    purpose,
                                    mode: PackageEnforcementMode::Enforced,
                                },
                                TimestampMillis::new(now),
                            )
                            .await?;
                        included.require_allowed().map_err(|_| base_error)?;
                        included.witness
                    } else {
                        return Err(base_error.into());
                    }
                }
            };
            self.purchase = Some((purpose, witness));
        }
        Ok(())
    }
}
