//! Development-only actual Finance/Identity/Commercial assembly. This library
//! mounts no route and cannot substitute Legal acceptance for Finance release.
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, lock_commercial_authority_sources,
};
use erpx_server_finance::public::{
    FinanceActor, JournalError,
    journal_review::{
        CurrentJournalReviewScope, FinanceJournalAccessPurpose, JournalReviewCommand,
        JournalReviewResult, JournalReviewerFacts, PgFinanceJournalReviewPolicy, PgJournalReview,
        ReviewJournal, inspect_journal_review_source, lock_journal_review_sources, review_journal,
    },
};
use erpx_server_identity_access::public::{
    DeviceProof,
    command_metadata::project_command_iam,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
    TimestampMillis, UserId,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, WorkAuthority,
};
use sqlx::PgConnection;
use std::sync::Arc;
use tokio::sync::Mutex;

type Error = PgTransactionError<BusinessError>;

/// Chosen only by trusted bootstrap code, never an HTTP or serialized grant.
#[derive(Clone, Copy, Debug)]
pub enum FinanceJournalEnvironment {
    Development,
    Production,
}
#[derive(Clone)]
pub struct FinanceJournalConfiguration {
    holding_id: HoldingId,
    company_id: CompanyId,
    purchases: Arc<CommercialCatalogue>,
}
impl FinanceJournalConfiguration {
    pub fn new(
        environment: FinanceJournalEnvironment,
        holding_id: HoldingId,
        company_id: CompanyId,
        purchases: Arc<CommercialCatalogue>,
    ) -> Result<Self, BusinessError> {
        if matches!(environment, FinanceJournalEnvironment::Production) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "FINANCE_PRODUCTION_COMPOSITION_UNAVAILABLE",
                "Finance production release and Company acceptance ports are incomplete.",
            ));
        }
        Ok(Self {
            holding_id,
            company_id,
            purchases,
        })
    }
    async fn lock_sources(&self, c: &mut PgConnection) -> Result<(), Error> {
        lock_commercial_authority_sources(c).await?;
        lock_journal_review_sources(c).await
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
        mut workspace: HoldingWorkspaceRequest,
        command: JournalReviewCommand,
    ) -> Result<JournalReviewResult, Error> {
        if workspace.company_id != self.configuration.company_id {
            return Err(scope_error().into());
        }
        // A read selector cannot opt a financial mutation out of selected CAS.
        workspace.mutation = true;
        let configuration = self.configuration.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace,
                WorkAuthorizationRequest {
                    permission: "finance.write".into(),
                    module: Some("finance".into()),
                    destination: "finance/journals".into(),
                    api: true,
                },
                configuration.clone(),
                move |c, current| {
                    let configuration = configuration.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        let actor = finance_actor(current);
                        let policy = Mutex::new(CurrentFinancePolicy {
                            current,
                            configuration: &configuration,
                            source: None,
                            purchase: None,
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
fn finance_actor(current: &AuthorizedWorkContext) -> FinanceActor {
    FinanceActor {
        holding_id: current.holding_id().clone(),
        company_id: current.selection().workspace().company_id.clone(),
        user_id: current.user_id().clone(),
        assignment_id: current.selection().assignment().id.clone(),
    }
}
/// Shared Workflow wire projection, not an independently constructed grant.
fn command_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: assignment.id.clone(),
        role: assignment.role.clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: selected.revision(),
            assignment_version: assignment.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: assignment
            .scope_node_id
            .as_ref()
            .map(|id| id.as_str().into()),
    })
}
struct CurrentFinancePolicy<'a> {
    current: &'a AuthorizedWorkContext,
    configuration: &'a FinanceJournalConfiguration,
    source: Option<CurrentJournalReviewScope>,
    purchase: Option<(CapabilityPurpose, PurchaseWitness)>,
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
            Some(source) => source.require_current(c).await?,
            None => {
                self.source = Some(
                    inspect_journal_review_source(c, &finance_actor(self.current), command).await?,
                )
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
            inspection.require_allowed()?;
            self.purchase = Some((purpose, inspection.witness));
        }
        Ok(())
    }
}
