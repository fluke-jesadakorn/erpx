//! Original Maintenance change ordering, with pure owner ports. The actual
//! adapter must retain source, purchases, receipts and all effects in one unit.
use crate::domain::{work::*, work_input::*};
use erpx_server_shared::public::{
    BusinessError, CommandKey, FailureKind, RecordId, TimestampMillis,
};
use std::future::Future;
/// Portable immutable input representation. Only the owner adapter constructs
/// it after canonical validation; the Workflow adapter reparses these exact
/// bytes for its fingerprint. This data is not a receipt or current authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceCommandInputJson(String);
impl MaintenanceCommandInputJson {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Debug)]
pub struct NormalizedMaintenanceChange {
    input: MaintenanceChangeInput,
    json: MaintenanceCommandInputJson,
}
impl NormalizedMaintenanceChange {
    pub(crate) fn new(input: MaintenanceChangeInput, json: String) -> Self {
        Self {
            input,
            json: MaintenanceCommandInputJson(json),
        }
    }
    pub fn input(&self) -> &MaintenanceChangeInput {
        &self.input
    }
    pub fn receipt_input(&self) -> &MaintenanceCommandInputJson {
        &self.json
    }
    pub fn into_command(self, key: CommandKey, work_id: RecordId) -> MaintenanceWorkCommand {
        MaintenanceWorkCommand {
            key,
            work_id,
            input: self,
        }
    }
}
#[derive(Clone, Debug)]
pub struct NormalizedMaintenanceCreate {
    input: MaintenanceCreateInput,
    json: MaintenanceCommandInputJson,
}
impl NormalizedMaintenanceCreate {
    pub(crate) fn new(input: MaintenanceCreateInput, json: String) -> Self {
        Self {
            input,
            json: MaintenanceCommandInputJson(json),
        }
    }
    pub fn input(&self) -> &MaintenanceCreateInput {
        &self.input
    }
    pub fn receipt_input(&self) -> &MaintenanceCommandInputJson {
        &self.json
    }
}
#[derive(Clone, Debug)]
pub struct MaintenanceWorkCommand {
    key: CommandKey,
    work_id: RecordId,
    input: NormalizedMaintenanceChange,
}
impl MaintenanceWorkCommand {
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
    pub fn work_id(&self) -> &RecordId {
        &self.work_id
    }
    pub fn input(&self) -> &MaintenanceChangeInput {
        self.input.input()
    }
    pub fn receipt_input(&self) -> &MaintenanceCommandInputJson {
        self.input.receipt_input()
    }
    pub fn operation(&self) -> String {
        format!("maintenance.{}", self.input().action.action())
    }
}
/// Current selected permission/module admission supplied by the owning adapter;
/// ordinary data facts cannot construct or replace its actual native context.
pub trait MaintenanceChangeAccess {
    type Error: From<BusinessError>;
    fn authorize_write(&self) -> Result<(), Self::Error>;
    fn actor(&self) -> MaintenanceActorFacts;
}
/// A repository borrowed from the already retained original owner unit. No
/// independent transaction, provider I/O or replay grant belongs in these ports.
pub trait MaintenanceChangeRepository: Send {
    type Error: From<BusinessError> + Send;
    type Source: Send;
    type PreparedConsumption: Send;
    type Output: Send;
    fn replay(
        &mut self,
        command: &MaintenanceWorkCommand,
    ) -> impl Future<Output = Result<Option<Self::Output>, Self::Error>> + Send;
    fn work(
        &mut self,
        id: &RecordId,
    ) -> impl Future<Output = Result<Option<Self::Source>, Self::Error>> + Send;
    fn policy_facts(&self, source: &Self::Source) -> MaintenanceWorkFacts;
    fn now(&self) -> TimestampMillis;
    fn prepare_consumption(
        &mut self,
        command: &MaintenanceWorkCommand,
        source: &Self::Source,
    ) -> impl Future<Output = Result<Self::PreparedConsumption, Self::Error>> + Send;
    fn change(
        &mut self,
        command: &MaintenanceWorkCommand,
        source: Self::Source,
        state: MaintenanceState,
        now: TimestampMillis,
        consumption: Option<Self::PreparedConsumption>,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send;
}
/// Permission refusal precedes wire parsing; replay precedes Work/owner/state.
/// Inventory preparation precedes the final repository version assertion. A
/// typed prepared value remains bound to this one command rather than reused
/// mutable SQL steps. The adapter owns actual current source and after-images.
pub async fn change_maintenance_work<A, P, Normalize, Repository>(
    access: &A,
    normalize: Normalize,
    repository: Repository,
) -> Result<P::Output, P::Error>
where
    A: MaintenanceChangeAccess<Error = P::Error>,
    P: MaintenanceChangeRepository,
    Normalize: FnOnce() -> Result<MaintenanceWorkCommand, P::Error>,
    Repository: FnOnce() -> Result<P, P::Error>,
{
    access.authorize_write()?;
    let command = normalize()?;
    let mut repository = repository()?;
    if let Some(prior) = repository.replay(&command).await? {
        return Ok(prior);
    }
    let source = repository.work(command.work_id()).await?.ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "MAINTENANCE_WORK_NOT_FOUND",
            "Maintenance work order not found in this scope.",
        )
    })?;
    let facts = repository.policy_facts(&source);
    let transition = match &command.input().action {
        MaintenanceChangeAction::Schedule {
            owner_id,
            scheduled_at,
        } => MaintenanceTransition::Schedule {
            owner_id: owner_id.clone(),
            scheduled_at: *scheduled_at,
        },
        MaintenanceChangeAction::Start => MaintenanceTransition::Start,
        MaintenanceChangeAction::Block => MaintenanceTransition::Block,
        MaintenanceChangeAction::Resume => MaintenanceTransition::Resume,
        MaintenanceChangeAction::Complete { .. } => MaintenanceTransition::Complete,
        MaintenanceChangeAction::IssuePart { .. } => MaintenanceTransition::IssuePart,
        MaintenanceChangeAction::Cancel => MaintenanceTransition::Cancel,
    };
    require_maintenance_transition(&access.actor(), &facts, &transition)?;
    let now = repository.now();
    let state = maintenance_state(&facts, &transition, now.get());
    let consumption = if matches!(transition, MaintenanceTransition::IssuePart) {
        Some(repository.prepare_consumption(&command, &source).await?)
    } else {
        None
    };
    repository
        .change(&command, source, state, now, consumption)
        .await
}
