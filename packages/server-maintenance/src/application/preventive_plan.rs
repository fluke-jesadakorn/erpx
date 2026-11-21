//! Portable preventive-plan command contract, calendar port and decision
//! dispatch. The owner adapter supplies current scope, People/IAM eligibility,
//! the serializable transaction, receipt ledger and effects; these values are
//! not authority.
use crate::domain::preventive_input::MaintenancePreventiveActionInput;
use crate::domain::preventive_plan::{
    PreventivePlanAction, PreventivePlanActor, PreventivePlanDefinition,
    PreventivePlanFacts, require_action, require_save,
};
use crate::domain::recurrence::{
    MAX_GAP_MINUTES, NextOccurrence, add_days, add_minutes, naive_millis, unavailable,
};
use erpx_server_shared::public::{BusinessError, CommandKey, HoldingId, RecordId};
use erpx_server_workflow::public::commands::CommandJson;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;

pub type PreventiveCalendarFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// IANA calendar port for the owner recurrence. The PostgreSQL adapter reads the
/// bundled `pg_timezone_names` database; tests may supply a fixed-offset probe.
/// A plan never depends on the process locale.
pub trait PreventiveCalendar: Send {
    fn valid_zone<'a>(
        &'a mut self,
        zone: &'a str,
    ) -> PreventiveCalendarFuture<'a, bool>;
    /// UTC offset in milliseconds (east positive) at an instant.
    fn utc_offset_ms<'a>(
        &'a mut self,
        zone: &'a str,
        instant: i64,
    ) -> PreventiveCalendarFuture<'a, i64>;
    /// Local `YYYY-MM-DDTHH:MM` minute (seconds truncated) at an instant.
    fn local_minute<'a>(
        &'a mut self,
        zone: &'a str,
        instant: i64,
    ) -> PreventiveCalendarFuture<'a, String>;
}

/// Selected current context facts needed by the plan policy. A caller cannot
/// construct authority from request data: composition obtains these from the
/// retained `AuthorizedWorkContext`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreventivePlanAccess {
    pub holding_id: HoldingId,
    pub actor_id: String,
    pub can_review: bool,
    pub expires_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreventivePlanOperation {
    Create,
    Revise,
    Activate,
    Pause,
    Cancel,
    Generate,
}

impl PreventivePlanOperation {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Create => "maintenance.plan.create",
            Self::Revise => "maintenance.plan.revise",
            Self::Activate => "maintenance.plan.activate",
            Self::Pause => "maintenance.plan.pause",
            Self::Cancel => "maintenance.plan.cancel",
            Self::Generate => "maintenance.plan.generate",
        }
    }
    pub fn action(&self) -> Option<PreventivePlanAction> {
        match self {
            Self::Activate => Some(PreventivePlanAction::Activate),
            Self::Pause => Some(PreventivePlanAction::Pause),
            Self::Cancel => Some(PreventivePlanAction::Cancel),
            Self::Generate => Some(PreventivePlanAction::Generate),
            Self::Create | Self::Revise => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum PreventivePlanInput {
    Save(PreventivePlanDefinition),
    Action(MaintenancePreventiveActionInput),
}

/// Canonical command identity plus the normalized receipt input bytes. The
/// Workflow ledger reparses these exact bytes for its fingerprint.
#[derive(Clone, Debug)]
pub struct PreventivePlanCommand {
    key: CommandKey,
    id: RecordId,
    operation: PreventivePlanOperation,
    input: PreventivePlanInput,
    receipt: CommandJson,
}

fn create_id(key: &CommandKey) -> Result<RecordId, BusinessError> {
    RecordId::new(format!("maintenance-plan-{}", key.as_str()))
}

impl PreventivePlanCommand {
    pub fn create(
        key: CommandKey,
        input: PreventivePlanDefinition,
        receipt: CommandJson,
    ) -> Result<Self, BusinessError> {
        let id = create_id(&key)?;
        let input = PreventivePlanDefinition {
            expected_version: None,
            ..input
        };
        Ok(Self {
            key,
            id,
            operation: PreventivePlanOperation::Create,
            input: PreventivePlanInput::Save(input),
            receipt,
        })
    }
    pub fn revise(
        key: CommandKey,
        id: RecordId,
        input: PreventivePlanDefinition,
        receipt: CommandJson,
    ) -> Self {
        Self {
            key,
            id,
            operation: PreventivePlanOperation::Revise,
            input: PreventivePlanInput::Save(input),
            receipt,
        }
    }
    pub fn action(
        key: CommandKey,
        id: RecordId,
        input: MaintenancePreventiveActionInput,
        receipt: CommandJson,
    ) -> Self {
        let operation = match input.action {
            crate::domain::preventive_input::MaintenancePreventiveAction::Activate => {
                PreventivePlanOperation::Activate
            }
            crate::domain::preventive_input::MaintenancePreventiveAction::Pause => {
                PreventivePlanOperation::Pause
            }
            crate::domain::preventive_input::MaintenancePreventiveAction::Cancel => {
                PreventivePlanOperation::Cancel
            }
            crate::domain::preventive_input::MaintenancePreventiveAction::Generate => {
                PreventivePlanOperation::Generate
            }
        };
        Self {
            key,
            id,
            operation,
            input: PreventivePlanInput::Action(input),
            receipt,
        }
    }
    pub fn create_record_id(key: &CommandKey) -> Result<RecordId, BusinessError> {
        create_id(key)
    }
    pub fn key(&self) -> &CommandKey {
        &self.key
    }
    pub fn id(&self) -> &RecordId {
        &self.id
    }
    pub fn operation(&self) -> PreventivePlanOperation {
        self.operation
    }
    pub fn operation_name(&self) -> &'static str {
        self.operation.operation()
    }
    pub fn input(&self) -> &PreventivePlanInput {
        &self.input
    }
    pub fn save_input(&self) -> Option<&PreventivePlanDefinition> {
        match &self.input {
            PreventivePlanInput::Save(input) => Some(input),
            PreventivePlanInput::Action(_) => None,
        }
    }
    pub fn action_input(&self) -> Option<&MaintenancePreventiveActionInput> {
        match &self.input {
            PreventivePlanInput::Action(input) => Some(input),
            PreventivePlanInput::Save(_) => None,
        }
    }
    pub fn receipt_input(&self) -> &CommandJson {
        &self.receipt
    }
}

/// Canonical normalized create/revise schema output. The receipt bytes are the
/// exact input the Workflow ledger fingerprints.
#[derive(Clone, Debug)]
pub struct NormalizedPreventivePlanSave {
    input: PreventivePlanDefinition,
    json: CommandJson,
}

impl NormalizedPreventivePlanSave {
    pub(crate) fn new(input: PreventivePlanDefinition, json: CommandJson) -> Self {
        Self { input, json }
    }
    pub fn input(&self) -> &PreventivePlanDefinition {
        &self.input
    }
    pub fn receipt_input(&self) -> &CommandJson {
        &self.json
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreventiveSavePlan {
    /// Wall-clock first/next due minute after truncating the submitted instant.
    pub local: String,
    /// Earlier instant represented by that minute.
    pub due_at: i64,
    pub next_version: i64,
    /// `draft` for a new plan, otherwise the unchanged current status.
    pub status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreventiveGeneration {
    pub due_local: String,
    pub due_at: i64,
    pub next: NextOccurrence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreventivePlanDecision {
    pub status: &'static str,
    pub next_version: i64,
    /// Record the independent reviewer on activation.
    pub approve: bool,
    pub generation: Option<PreventiveGeneration>,
}

/// Local wall-clock minute for an instant, truncating seconds exactly like the
/// retained `Intl` formatter used when a plan is first saved.
pub async fn wall_clock<C: PreventiveCalendar>(
    calendar: &mut C,
    instant: i64,
    zone: &str,
) -> Result<String, BusinessError> {
    calendar.local_minute(zone, instant).await
}

/// Every instant represented by a local wall-clock minute, sorted ascending.
/// A fall-back fold has two; a spring-forward gap has none. This is the retained
/// TypeScript offset-probe algorithm, with the IANA rules supplied by the port.
pub async fn local_candidates<C: PreventiveCalendar>(
    calendar: &mut C,
    local: &str,
    zone: &str,
) -> Result<Vec<i64>, BusinessError> {
    let naive = naive_millis(local).ok_or_else(unavailable)?;
    let mut offsets = BTreeSet::new();
    for hours in [-36_i64, -12, 0, 12, 36] {
        offsets.insert(
            calendar
                .utc_offset_ms(zone, naive.saturating_add(hours * 3_600_000))
                .await?,
        );
    }
    let mut candidates = Vec::new();
    for offset in offsets {
        let instant = naive.saturating_sub(offset);
        if calendar.local_minute(zone, instant).await? == local {
            candidates.push(instant);
        }
    }
    candidates.sort_unstable();
    candidates.dedup();
    Ok(candidates)
}

/// Next occurrence of a retained wall-clock definition. The stored local minute
/// is preserved; only the instant and the reported `resolvedLocal` move when the
/// requested minute does not exist.
pub async fn next_occurrence<C: PreventiveCalendar>(
    calendar: &mut C,
    local: &str,
    zone: &str,
    days: i64,
) -> Result<NextOccurrence, BusinessError> {
    if days <= 0 || !calendar.valid_zone(zone).await? {
        return Err(unavailable());
    }
    let next = add_days(local, days)?;
    for minute in 0..=MAX_GAP_MINUTES {
        let resolved = if minute == 0 {
            next.clone()
        } else {
            add_minutes(&next, minute)?
        };
        if let Some(at) = local_candidates(calendar, &resolved, zone).await?.first() {
            return Ok(NextOccurrence {
                local: next,
                at: *at,
                resolved_local: resolved,
            });
        }
    }
    Err(unavailable())
}

/// Resolve a submitted `firstDueAt` instant to the plan's stored wall-clock
/// minute and earlier instant.
pub async fn resolve_save_time<C: PreventiveCalendar>(
    calendar: &mut C,
    first_due_at: i64,
    zone: &str,
) -> Result<(String, i64), BusinessError> {
    if !calendar.valid_zone(zone).await? {
        return Err(unavailable());
    }
    let local = wall_clock(calendar, first_due_at, zone).await?;
    let due_at = local_candidates(calendar, &local, zone)
        .await?
        .first()
        .copied()
        .ok_or_else(unavailable)?;
    Ok((local, due_at))
}

/// Decide a save intent for the adapter. The adapter has already loaded the
/// current plan and its current owner eligibility.
pub async fn plan_save_decision<C: PreventiveCalendar>(
    calendar: &mut C,
    access: &PreventivePlanAccess,
    current: Option<&PreventivePlanFacts>,
    definition: &PreventivePlanDefinition,
) -> Result<PreventiveSavePlan, BusinessError> {
    let policy = require_save(
        &PreventivePlanActor {
            user_id: access.actor_id.clone(),
            can_review: access.can_review,
        },
        current,
        definition,
    )?;
    let (local, due_at) =
        resolve_save_time(calendar, definition.first_due_at, &definition.time_zone).await?;
    Ok(PreventiveSavePlan {
        local,
        due_at,
        next_version: policy.next_version,
        status: policy.status,
    })
}

/// Decide an action intent for the adapter.
pub async fn plan_action_decision<C: PreventiveCalendar>(
    calendar: &mut C,
    access: &PreventivePlanAccess,
    plan: &PreventivePlanFacts,
    action: &PreventivePlanAction,
    expected_version: i64,
    now: i64,
) -> Result<PreventivePlanDecision, BusinessError> {
    let policy = require_action(
        &PreventivePlanActor {
            user_id: access.actor_id.clone(),
            can_review: access.can_review,
        },
        plan,
        action,
        expected_version,
        now,
    )?;
    let generation = match action {
        PreventivePlanAction::Generate => {
            let next = next_occurrence(
                calendar,
                &plan.next_due_local,
                &plan.time_zone,
                i64::from(plan.interval_days),
            )
            .await?;
            Some(PreventiveGeneration {
                due_local: plan.next_due_local.clone(),
                due_at: plan.next_due_at,
                next,
            })
        }
        _ => None,
    };
    Ok(PreventivePlanDecision {
        status: policy.status,
        next_version: policy.next_version,
        approve: policy.approve,
        generation,
    })
}
