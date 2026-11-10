//! Preventive-plan command effects on the Workflow command ledger.
//!
//! The maintenance owner unit keeps current Company/branch/department scope,
//! plan/asset row locks, CAS, history, notification, receipt and outbox in one
//! caller-owned serializable transaction. Generation reuses the existing
//! Maintenance work-creation insert and gives it its own `:work` receipt, so no
//! second work writer exists.
use crate::adapters::calendar::PgPreventiveCalendar;
use crate::adapters::work_source::{
    MaintenanceCreationPlan, insert_maintenance_creation, maintenance_creation_vacancy,
};
use crate::application::preventive_plan::{
    PreventiveGeneration, PreventivePlanAccess, PreventivePlanCommand, PreventivePlanDecision,
    PreventivePlanInput, PreventiveSavePlan, plan_action_decision, plan_save_decision,
};
use crate::domain::preventive_plan::{
    PreventivePlanAction, PreventivePlanFacts, retained_occurrence_conflict, source_conflict,
};
use erpx_server_assets::public::maintenance::{
    inspect_maintenance_creation_asset, lock_maintenance_creation_asset_sources,
    read_current_maintenance_asset,
};
use erpx_server_identity_access::public::operational_worker::{
    OperationalWorkerPermission, RawOperationalWorkerRequest, inspect_operational_worker_raw,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, IdentifierSource, RecordId,
    TimestampMillis,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, CommandOutcome, PgCommandAuthority, PgCommandLedger,
    PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MAINTENANCE_STATE_CHANGED",
        "Refresh the preventive plan before continuing.",
    )
}
fn plan_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MAINTENANCE_PLAN_NOT_FOUND",
        "Preventive plan not found in this scope.",
    )
}
fn asset_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MAINTENANCE_PLAN_SOURCE_CHANGED",
        "Reload the current asset before generating work.",
    )
}

fn branch(actor: &CommandActor) -> Option<&str> {
    match &actor.branch_id {
        CommandField::Value(value) => Some(value.as_str()),
        CommandField::Omitted | CommandField::Null => None,
    }
}
fn department(actor: &CommandActor) -> Option<&str> {
    match &actor.department_id {
        CommandField::Value(value) => Some(value.as_str()),
        CommandField::Omitted | CommandField::Null => None,
    }
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("MAINTENANCE_PLAN_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("MAINTENANCE_PLAN_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

async fn owner_eligible(
    c: &mut PgConnection,
    company: &CompanyId,
    owner: &str,
    branch_id: &str,
    department_id: Option<&str>,
    now: TimestampMillis,
) -> Result<bool, Error> {
    let worker = inspect_operational_worker_raw(
        c,
        RawOperationalWorkerRequest {
            company_id: company.clone(),
            user_id: owner.to_owned(),
            branch_id: RecordId::new(branch_id)?,
            department_id: department_id.map(RecordId::new).transpose()?,
            permission: OperationalWorkerPermission::Write,
        },
        now,
    )
    .await?;
    Ok(worker.is_eligible())
}

#[derive(Clone, Debug)]
struct PlanAsset {
    branch_id: String,
    department_id: Option<String>,
    version: i64,
}

/// Lock and retain the physical Asset relationship for a save or generation.
/// `expected_version` is the submitted plan version; generation passes `None` to
/// read the current version under the same row lock.
async fn plan_asset(
    c: &mut PgConnection,
    access: &PreventivePlanAccess,
    company: &CompanyId,
    actor: &CommandActor,
    asset_id: &RecordId,
    expected_version: Option<i64>,
) -> Result<PlanAsset, Error> {
    if let Some(expected) = expected_version {
        let branch_id = branch(actor).map(RecordId::new).transpose()?;
        let department_id = department(actor).map(RecordId::new).transpose()?;
        let asset = inspect_maintenance_creation_asset(
            c,
            &access.holding_id,
            company,
            branch_id.as_ref(),
            department_id.as_ref(),
            asset_id,
            expected,
        )
        .await?
        .ok_or_else(source_conflict)?;
        return Ok(PlanAsset {
            branch_id: asset.facts().branch_id.as_str().to_owned(),
            department_id: asset
                .facts()
                .department_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            version: asset.facts().version,
        });
    }
    let source = read_current_maintenance_asset(c, &access.holding_id, company, asset_id)
        .await?
        .ok_or_else(asset_changed)?;
    let facts = source.facts();
    if branch(actor).is_some_and(|value| value != facts.branch_id.as_str())
        || department(actor).is_some_and(|value| {
            facts.department_id.as_ref().map(RecordId::as_str) != Some(value)
        })
    {
        return Err(asset_changed().into());
    }
    let lifecycle: String = sqlx::query_scalar(
        "SELECT lifecycle_status FROM operational_assets WHERE id=$1 AND company_id=$2",
    )
    .bind(asset_id.as_str())
    .bind(company.as_str())
    .fetch_one(&mut *c)
    .await?;
    if !matches!(
        lifecycle.as_str(),
        "registered" | "in_service" | "maintenance"
    ) {
        return Err(asset_changed().into());
    }
    Ok(PlanAsset {
        branch_id: facts.branch_id.as_str().to_owned(),
        department_id: facts
            .department_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        version: facts.version,
    })
}

async fn load_plan(
    c: &mut PgConnection,
    company: &CompanyId,
    actor: &CommandActor,
    id: &RecordId,
    lock: bool,
    now: TimestampMillis,
) -> Result<Option<PreventivePlanFacts>, Error> {
    let select = "SELECT id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,\
         interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,status,version,\
         created_by,edited_by,approved_by,approved_at,updated_at \
         FROM maintenance_preventive_plans WHERE id=$1 AND company_id=$2 \
         AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4)";
    let sql = if lock {
        format!("{select} FOR UPDATE")
    } else {
        select.to_owned()
    };
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id.as_str())
        .bind(company.as_str())
        .bind(branch(actor))
        .bind(department(actor))
        .fetch_optional(&mut *c)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let owner_id: String = row.try_get("owner_id")?;
    let branch_id: String = row.try_get("branch_id")?;
    let department_id: Option<String> = row.try_get("department_id")?;
    let owner_eligible = owner_eligible(
        c,
        company,
        &owner_id,
        &branch_id,
        department_id.as_deref(),
        now,
    )
    .await?;
    Ok(Some(PreventivePlanFacts {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id,
        department_id,
        asset_id: row.try_get("asset_id")?,
        title: row.try_get("title")?,
        priority: row.try_get("priority")?,
        owner_id,
        interval_days: row.try_get("interval_days")?,
        time_zone: row.try_get("time_zone")?,
        first_due_local: row.try_get("first_due_local")?,
        next_due_local: row.try_get("next_due_local")?,
        next_due_at: row.try_get("next_due_at")?,
        lead_days: row.try_get("lead_days")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        edited_by: row.try_get("edited_by")?,
        approved_by: row.try_get("approved_by")?,
        approved_at: row.try_get("approved_at")?,
        updated_at: row.try_get("updated_at")?,
        owner_eligible,
    }))
}

fn result_json(value: serde_json::Value) -> Result<CommandJson, BusinessError> {
    CommandJson::parse(&value.to_string())
}

enum Prepared {
    Save {
        current: Option<PreventivePlanFacts>,
        decision: PreventiveSavePlan,
        asset: PlanAsset,
        actor: CommandActor,
        now: i64,
    },
    Change {
        plan: PreventivePlanFacts,
        decision: PreventivePlanDecision,
        asset: Option<PlanAsset>,
        actor: CommandActor,
        now: i64,
    },
}

pub struct PreventivePlanEffects<'a, C> {
    access: PreventivePlanAccess,
    command: PreventivePlanCommand,
    clock: &'a C,
    prepared: Option<Prepared>,
    written: Option<Written>,
}

struct Written {
    version: i64,
    generated_work_id: Option<String>,
}

impl<C: Clock + Sync> PreventivePlanEffects<'_, C> {
    fn now(&self) -> i64 {
        self.clock.now().get()
    }
}

impl<C: Clock + Sync> PgOwnerCommandEffects for PreventivePlanEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_maintenance_creation_asset_sources(c).await?;
        // Preserve the existing work-creation insertion fence without allowing
        // an independent work writer.
        sqlx::query("LOCK TABLE maintenance_work_orders IN SHARE ROW EXCLUSIVE MODE")
            .execute(&mut *c)
            .await?;
        Ok(())
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        serializable(c).await?;
        if request.key != self.command.key().as_str()
            || request.operation != self.command.operation_name()
            || request.record_id != self.command.id().as_str()
            || request.input.as_str() != self.command.receipt_input().as_str()
            || actor.user_id.as_str() != self.access.actor_id
        {
            return Err(changed().into());
        }
        let now = self.now();
        if now >= self.access.expires_at {
            return Err(changed().into());
        }
        let company = actor.company_id.clone();
        match self.command.input() {
            PreventivePlanInput::Save(definition) => {
                let current = match self.command.operation() {
                    crate::application::preventive_plan::PreventivePlanOperation::Create => None,
                    _ => Some(
                        load_plan(
                            c,
                            &company,
                            actor,
                            self.command.id(),
                            true,
                            TimestampMillis::new(now),
                        )
                        .await?
                        .ok_or_else(plan_not_found)?,
                    ),
                };
                let mut calendar = PgPreventiveCalendar::new(&mut *c);
                let decision =
                    plan_save_decision(&mut calendar, &self.access, current.as_ref(), definition)
                        .await?;
                let asset = plan_asset(
                    c,
                    &self.access,
                    &company,
                    actor,
                    &RecordId::new(definition.asset_id.clone())?,
                    Some(definition.asset_version),
                )
                .await?;
                let owner_eligible = owner_eligible(
                    c,
                    &company,
                    &definition.owner_id,
                    &asset.branch_id,
                    asset.department_id.as_deref(),
                    TimestampMillis::new(now),
                )
                .await?;
                if !owner_eligible {
                    return Err(source_conflict().into());
                }
                let result = result_json(serde_json::json!({
                    "id": self.command.id().as_str(),
                    "version": decision.next_version,
                    "status": decision.status,
                }))?;
                let version = decision.next_version;
                self.prepared = Some(Prepared::Save {
                    current,
                    decision,
                    asset,
                    actor: actor.clone(),
                    now,
                });
                Ok(WorkflowOutput {
                    version,
                    result,
                    event_scope: None,
                })
            }
            PreventivePlanInput::Action(action) => {
                let plan = load_plan(
                    c,
                    &company,
                    actor,
                    self.command.id(),
                    true,
                    TimestampMillis::new(now),
                )
                .await?
                .ok_or_else(plan_not_found)?;
                let Some(plan_action) = self.command.operation().action() else {
                    return Err(changed().into());
                };
                let mut calendar = PgPreventiveCalendar::new(&mut *c);
                let decision = plan_action_decision(
                    &mut calendar,
                    &self.access,
                    &plan,
                    &plan_action,
                    action.expected_version,
                    now,
                )
                .await?;
                let asset_id = RecordId::new(plan.asset_id.clone())?;
                let asset = match plan_action {
                    PreventivePlanAction::Generate => Some(
                        plan_asset(c, &self.access, &company, actor, &asset_id, None).await?,
                    ),
                    // The retained activation assertion also proves the asset is
                    // still registered/in service/maintenance.
                    PreventivePlanAction::Activate => {
                        match plan_asset(c, &self.access, &company, actor, &asset_id, None).await {
                            Ok(_) => None,
                            Err(PgTransactionError::Business(_)) => {
                                return Err(source_conflict().into());
                            }
                            Err(other) => return Err(other),
                        }
                    }
                    PreventivePlanAction::Pause | PreventivePlanAction::Cancel => None,
                };
                let mut value = serde_json::json!({
                    "id": plan.id,
                    "version": decision.next_version,
                    "status": decision.status,
                });
                if let Some(generation) = &decision.generation {
                    let work_key = format!("{}:work", self.command.key().as_str());
                    value["workOrderId"] = serde_json::json!(format!("maintenance-{work_key}"));
                    value["nextDueAt"] = serde_json::json!(generation.next.at);
                    value["nextDueLocal"] = serde_json::json!(generation.next.local);
                }
                let result = result_json(value)?;
                let version = decision.next_version;
                self.prepared = Some(Prepared::Change {
                    plan,
                    decision,
                    asset,
                    actor: actor.clone(),
                    now,
                });
                Ok(WorkflowOutput {
                    version,
                    result,
                    event_scope: None,
                })
            }
        }
    }

    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let prepared = self.prepared.take().ok_or_else(changed)?;
        let (version, generated_work_id) = match &prepared {
            Prepared::Save { decision, .. } => (decision.next_version, None),
            Prepared::Change { decision, .. } => (
                decision.next_version,
                decision.generation.as_ref().map(|_| {
                    format!("maintenance-{}:work", self.command.key().as_str())
                }),
            ),
        };
        let result = match prepared {
            Prepared::Save {
                current,
                decision,
                asset,
                actor,
                now,
            } => {
                let definition = self
                    .command
                    .save_input()
                    .cloned()
                    .ok_or_else(changed)?;
                apply_save(
                    c,
                    &self.command,
                    current.as_ref(),
                    &definition,
                    &decision,
                    &asset,
                    &actor,
                    now,
                )
                .await
            }
            Prepared::Change {
                plan,
                decision,
                asset,
                actor,
                now,
            } => {
                apply_change(
                    c,
                    &self.command,
                    &plan,
                    &decision,
                    asset.as_ref(),
                    &actor,
                    now,
                )
                .await
            }
        };
        result?;
        self.written = Some(Written {
            version,
            generated_work_id,
        });
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
        let written = self.written.as_ref().ok_or_else(changed)?;
        let version: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM maintenance_preventive_plans WHERE id=$1",
        )
        .bind(self.command.id().as_str())
        .fetch_optional(&mut *c)
        .await?;
        if version != Some(written.version) {
            return Err(changed().into());
        }
        let event_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM maintenance_preventive_events e \
             JOIN maintenance_preventive_plans p ON p.id=e.plan_id AND e.version=p.version \
             WHERE p.id=$1 AND e.snapshot_json=to_jsonb(p))",
        )
        .bind(self.command.id().as_str())
        .fetch_one(&mut *c)
        .await?;
        if !event_exists {
            return Err(changed().into());
        }
        if let Some(work_id) = &written.generated_work_id {
            let occurrence: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM maintenance_preventive_occurrences \
                 WHERE id=$1 AND plan_id=$2 AND work_order_id=$3)",
            )
            .bind(format!(
                "preventive-occurrence-{}",
                self.command.key().as_str()
            ))
            .bind(self.command.id().as_str())
            .bind(work_id)
            .fetch_one(&mut *c)
            .await?;
            if !occurrence {
                return Err(changed().into());
            }
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn apply_save(
    c: &mut PgConnection,
    command: &PreventivePlanCommand,
    current: Option<&PreventivePlanFacts>,
    definition: &crate::domain::preventive_plan::PreventivePlanDefinition,
    decision: &PreventiveSavePlan,
    asset: &PlanAsset,
    actor: &CommandActor,
    now: i64,
) -> Result<(), Error> {
    let reason = definition.reason.clone();
    let event_action = if current.is_some() { "revise" } else { "create" };
    match current {
        Some(current) => {
            if current.version != decision.next_version - 1
                || !matches!(current.status.as_str(), "draft" | "paused")
                || current.asset_id != definition.asset_id
            {
                return Err(changed().into());
            }
            let retained: Option<String> = sqlx::query_scalar(
                "SELECT min(due_local) FROM maintenance_preventive_occurrences WHERE plan_id=$1",
            )
            .bind(current.id.as_str())
            .fetch_one(&mut *c)
            .await?;
            if retained_occurrence_conflict(retained.as_deref(), &decision.local) {
                return Err(source_conflict().into());
            }
            let updated = sqlx::query(
                "UPDATE maintenance_preventive_plans SET title=$2,priority=$3,owner_id=$4,\
                 interval_days=$5,time_zone=$6,first_due_local=$7,next_due_local=$7,next_due_at=$8,\
                 lead_days=$9,edited_by=$10,approved_by=NULL,approved_at=NULL,version=$11,updated_at=$12 \
                 WHERE id=$1 AND version=$13 AND status IN ('draft','paused')",
            )
            .bind(current.id.as_str())
            .bind(&definition.title)
            .bind(&definition.priority)
            .bind(&definition.owner_id)
            .bind(definition.interval_days)
            .bind(&definition.time_zone)
            .bind(&decision.local)
            .bind(decision.due_at)
            .bind(definition.lead_days)
            .bind(actor.user_id.as_str())
            .bind(decision.next_version)
            .bind(now)
            .bind(current.version)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(source_conflict().into());
            }
        }
        None => {
            let inserted = sqlx::query(
                "INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,\
                 asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,\
                 next_due_local,next_due_at,lead_days,status,version,created_by,edited_by,\
                 created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$11,$12,$13,'draft',1,$14,$14,$15,$15)",
            )
            .bind(command.id().as_str())
            .bind(actor.company_id.as_str())
            .bind(&asset.branch_id)
            .bind(&asset.department_id)
            .bind(&definition.asset_id)
            .bind(&definition.title)
            .bind(&definition.priority)
            .bind(&definition.owner_id)
            .bind(definition.interval_days)
            .bind(&definition.time_zone)
            .bind(&decision.local)
            .bind(decision.due_at)
            .bind(definition.lead_days)
            .bind(actor.user_id.as_str())
            .bind(now)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if inserted != 1 {
                return Err(source_conflict().into());
            }
        }
    }
    insert_plan_event(c, command, event_action, &reason, actor, now).await?;
    insert_plan_notification(
        c,
        command,
        "Review the preventive maintenance plan and next due date",
        now,
    )
    .await
}


#[allow(clippy::too_many_arguments)]
async fn apply_change(
    c: &mut PgConnection,
    command: &PreventivePlanCommand,
    plan: &PreventivePlanFacts,
    decision: &PreventivePlanDecision,
    asset: Option<&PlanAsset>,
    actor: &CommandActor,
    now: i64,
) -> Result<(), Error> {
    let reason = command
        .action_input()
        .map(|input| input.reason.clone())
        .ok_or_else(changed)?;
    let action = command.operation().action().ok_or_else(changed)?;
    match action {
        PreventivePlanAction::Activate => {
            let updated = sqlx::query(
                "UPDATE maintenance_preventive_plans SET approved_by=$2,approved_at=$3 \
                 WHERE id=$1 AND status IN ('draft','paused') AND edited_by<>$2",
            )
            .bind(plan.id.as_str())
            .bind(actor.user_id.as_str())
            .bind(now)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(source_conflict().into());
            }
        }
        PreventivePlanAction::Generate => {
            let generation = decision.generation.as_ref().ok_or_else(changed)?;
            let asset = asset.ok_or_else(changed)?;
            if plan.status != "active" || !plan.owner_eligible {
                return Err(source_conflict().into());
            }
            if generation.due_at != plan.next_due_at || generation.due_local != plan.next_due_local {
                return Err(changed().into());
            }
            create_generated_work(c, command, plan, asset, generation, actor, now).await?;
            let definition = serde_json::json!({
                "title": plan.title,
                "priority": plan.priority,
                "ownerId": plan.owner_id,
                "intervalDays": plan.interval_days,
                "timeZone": plan.time_zone,
                "leadDays": plan.lead_days,
                "dueLocal": generation.due_local,
                "dueAt": generation.due_at,
                "nextResolvedLocal": generation.next.resolved_local,
            });
            let inserted = sqlx::query(
                "INSERT INTO maintenance_preventive_occurrences(id,plan_id,plan_version,\
                 work_order_id,due_local,due_at,source_asset_version,definition_json,\
                 generated_by,generated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8::jsonb,$9,$10)",
            )
            .bind(format!("preventive-occurrence-{}", command.key().as_str()))
            .bind(plan.id.as_str())
            .bind(plan.version)
            .bind(format!(
                "maintenance-{}:work",
                command.key().as_str()
            ))
            .bind(&generation.due_local)
            .bind(generation.due_at)
            .bind(asset.version)
            .bind(definition.to_string())
            .bind(actor.user_id.as_str())
            .bind(now)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if inserted != 1 {
                return Err(source_conflict().into());
            }
            let advanced = sqlx::query(
                "UPDATE maintenance_preventive_plans SET next_due_local=$2,next_due_at=$3 \
                 WHERE id=$1 AND status='active' AND next_due_at=$4",
            )
            .bind(plan.id.as_str())
            .bind(&generation.next.local)
            .bind(generation.next.at)
            .bind(plan.next_due_at)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if advanced != 1 {
                return Err(source_conflict().into());
            }
        }
        PreventivePlanAction::Pause | PreventivePlanAction::Cancel => {}
    }
    let updated = sqlx::query(
        "UPDATE maintenance_preventive_plans SET status=$2,version=$3,updated_at=$4 \
         WHERE id=$1 AND version=$5",
    )
    .bind(plan.id.as_str())
    .bind(decision.status)
    .bind(decision.next_version)
    .bind(now)
    .bind(plan.version)
    .execute(&mut *c)
    .await?
    .rows_affected();
    if updated != 1 {
        return Err(source_conflict().into());
    }
    insert_plan_event(c, command, action.action(), &reason, actor, now).await?;
    insert_plan_notification(
        c,
        command,
        "Preventive maintenance plan updated: review the current due work",
        now,
    )
    .await
}

async fn insert_plan_event(
    c: &mut PgConnection,
    command: &PreventivePlanCommand,
    action: &str,
    reason: &str,
    actor: &CommandActor,
    now: i64,
) -> Result<(), Error> {
    let inserted = sqlx::query(
        "INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,\
         snapshot_json,created_at) \
         SELECT $1,id,version,$2,$3,$4,to_jsonb(p),$5 FROM maintenance_preventive_plans p \
         WHERE id=$6",
    )
    .bind(format!(
        "preventive-event-{}",
        command.key().as_str()
    ))
    .bind(action)
    .bind(actor.user_id.as_str())
    .bind(reason)
    .bind(now)
    .bind(command.id().as_str())
    .execute(&mut *c)
    .await?
    .rows_affected();
    if inserted != 1 {
        return Err(changed().into());
    }
    Ok(())
}

async fn insert_plan_notification(
    c: &mut PgConnection,
    command: &PreventivePlanCommand,
    message: &str,
    now: i64,
) -> Result<(), Error> {
    // The retained collaboration projection writes this staff notification on
    // the same owner transaction. A public Collaboration port for plan notices
    // is a tracked follow-up; the row shape is unchanged.
    let inserted = sqlx::query(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
         record_version,message,created_at) \
         SELECT $1,company_id,owner_id,'maintenance_plan',id,version,$2,$3 \
         FROM maintenance_preventive_plans WHERE id=$4",
    )
    .bind(format!(
        "preventive-notice-{}",
        command.key().as_str()
    ))
    .bind(message)
    .bind(now)
    .bind(command.id().as_str())
    .execute(&mut *c)
    .await?
    .rows_affected();
    if inserted != 1 {
        return Err(changed().into());
    }
    Ok(())
}

struct CreationAuthority {
    key: String,
    record_id: String,
    input: CommandJson,
    actor: CommandActor,
}

impl CreationAuthority {
    fn bind(&self, request: &WorkflowRequest) -> Result<(), BusinessError> {
        if request.key != self.key
            || request.operation != "maintenance.create"
            || request.record_id != self.record_id
            || request.input.as_str() != self.input.as_str()
        {
            return Err(changed());
        }
        Ok(())
    }
}

impl PgCommandAuthority for CreationAuthority {
    async fn lock_sources(&mut self, _: &mut PgConnection, _: &WorkflowRequest) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        self.bind(request)?;
        Ok(self.actor.clone())
    }
    async fn verify_command(
        &mut self,
        _: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
        _: erpx_server_workflow::public::commands::CommandDisposition,
    ) -> Result<(), Error> {
        self.bind(request)?;
        if actor != &self.actor {
            return Err(changed().into());
        }
        Ok(())
    }
}

struct CreationFacts {
    id: String,
    key: String,
    company_id: String,
    branch_id: RecordId,
    department_id: Option<RecordId>,
    asset_id: RecordId,
    asset_version: i64,
    title: String,
    priority: String,
    owner_id: String,
    scheduled_at: i64,
    reason: String,
    actor_id: String,
    now: i64,
}

struct CreationEffects {
    facts: CreationFacts,
}

impl PgOwnerCommandEffects for CreationEffects {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        if !maintenance_creation_vacancy(c, &self.facts.asset_id).await? {
            return Err(source_conflict().into());
        }
        Ok(WorkflowOutput {
            version: 1,
            result: result_json(serde_json::json!({
                "id": self.facts.id,
                "version": 1,
                "status": "scheduled",
            }))?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let facts = &self.facts;
        insert_maintenance_creation(
            c,
            &MaintenanceCreationPlan {
                id: &RecordId::new(facts.id.clone())?,
                key: &facts.key,
                company_id: &facts.company_id,
                branch_id: &facts.branch_id,
                department_id: facts.department_id.as_ref(),
                asset_id: &facts.asset_id,
                asset_version: facts.asset_version,
                title: &facts.title,
                priority: &facts.priority,
                status: "scheduled",
                owner_id: &facts.owner_id,
                scheduled_at: Some(facts.scheduled_at),
                reason: &facts.reason,
                actor_id: &facts.actor_id,
                now: facts.now,
            },
        )
        .await?;
        let notified = sqlx::query(
            "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
             record_version,message,created_at) \
             SELECT $1,company_id,owner_id,'maintenance_work',id,version,\
             'New asset maintenance work: review schedule and condition',updated_at \
             FROM maintenance_work_orders WHERE id=$2",
        )
        .bind(format!("maintenance-notice-{}", facts.key))
        .bind(&facts.id)
        .execute(&mut *c)
        .await?
        .rows_affected();
        if notified != 1 {
            return Err(changed().into());
        }
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let row: Option<(String, i64, String, String)> = sqlx::query_as(
            "SELECT company_id,version,status,owner_id FROM maintenance_work_orders WHERE id=$1",
        )
        .bind(&self.facts.id)
        .fetch_optional(&mut *c)
        .await?;
        let event: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM maintenance_work_events WHERE work_order_id=$1 AND version=1 AND action='create')",
        )
        .bind(&self.facts.id)
        .fetch_one(&mut *c)
        .await?;
        if row.is_none_or(|(company, version, status, owner)| {
            company != self.facts.company_id
                || version != 1
                || status != "scheduled"
                || owner != self.facts.owner_id
        }) || !event
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn create_generated_work(
    c: &mut PgConnection,
    command: &PreventivePlanCommand,
    plan: &PreventivePlanFacts,
    asset: &PlanAsset,
    generation: &PreventiveGeneration,
    actor: &CommandActor,
    now: i64,
) -> Result<(), Error> {
    let work_key = format!("{}:work", command.key().as_str());
    let work_key = CommandKey::new(work_key)?;
    let work_id = RecordId::new(format!("maintenance-{}", work_key.as_str()))?;
    let input = result_json(serde_json::json!({
        "assetId": plan.asset_id,
        "assetVersion": asset.version,
        "title": plan.title,
        "priority": plan.priority,
        "ownerId": plan.owner_id,
        "scheduledAt": generation.due_at,
        "reason": command.action_input().map(|input| input.reason.as_str()).unwrap_or(""),
    }))?;
    let request = WorkflowRequest {
        key: work_key.as_str().to_owned(),
        operation: "maintenance.create".to_owned(),
        record_id: work_id.as_str().to_owned(),
        input: input.clone(),
    };
    let mut authority = CreationAuthority {
        key: work_key.as_str().to_owned(),
        record_id: work_id.as_str().to_owned(),
        input,
        actor: actor.clone(),
    };
    let mut effects = CreationEffects {
        facts: CreationFacts {
            id: work_id.as_str().to_owned(),
            key: work_key.as_str().to_owned(),
            company_id: plan.company_id.clone(),
            branch_id: RecordId::new(plan.branch_id.clone())?,
            department_id: plan
                .department_id
                .clone()
                .map(RecordId::new)
                .transpose()?,
            asset_id: RecordId::new(plan.asset_id.clone())?,
            asset_version: asset.version,
            title: plan.title.clone(),
            priority: plan.priority.clone(),
            owner_id: plan.owner_id.clone(),
            scheduled_at: generation.due_at,
            reason: command
                .action_input()
                .map(|input| input.reason.clone())
                .unwrap_or_default(),
            actor_id: actor.user_id.as_str().to_owned(),
            now,
        },
    };
    // A nested ledger on the same connection and transaction creates the work
    // order, its version-1 event, the `:work` receipt/audit/outbox and the work
    // notification. This is the existing creation path, not a second writer.
    let work_clock = FixedClock(TimestampMillis::new(now));
    let mut identifiers = SequentialIdentifiers::new(format!(
        "maintenance-aux-{}",
        work_key.as_str()
    ));
    let mut ledger = PgCommandLedger::new(
        actor.company_id.clone(),
        c,
        &mut authority,
        &work_clock,
        &mut identifiers,
    );
    ledger.execute(&request, &mut effects).await?;
    Ok(())
}

struct FixedClock(TimestampMillis);
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        self.0
    }
}

struct SequentialIdentifiers {
    prefix: String,
    count: u64,
}
impl SequentialIdentifiers {
    fn new(prefix: String) -> Self {
        Self { prefix, count: 0 }
    }
}
impl IdentifierSource for SequentialIdentifiers {
    fn next(&mut self) -> RecordId {
        self.count += 1;
        RecordId::new(format!("{}-{}", self.prefix, self.count))
            .expect("generated bounded auxiliary identifier")
    }
}

/// Execute one preventive-plan command on the caller's serializable Holding
/// transaction. Composition supplies the retained authority (current Account
/// context, IAM module/permission and purchase witness), selected access facts
/// and identifier source; no independent transaction or commit occurs here.
pub async fn execute_preventive_plan_command<A, C, I>(
    company: CompanyId,
    connection: &mut PgConnection,
    access: &PreventivePlanAccess,
    authority: &mut A,
    clock: &C,
    identifiers: &mut I,
    command: &PreventivePlanCommand,
) -> Result<CommandOutcome, Error>
where
    A: PgCommandAuthority,
    C: Clock + Sync,
    I: IdentifierSource,
{
    if access.expires_at <= clock.now().get() {
        return Err(changed().into());
    }
    let request = WorkflowRequest {
        key: command.key().as_str().to_owned(),
        operation: command.operation_name().to_owned(),
        record_id: command.id().as_str().to_owned(),
        input: command.receipt_input().clone(),
    };
    let mut effects = PreventivePlanEffects {
        access: access.clone(),
        command: command.clone(),
        clock,
        prepared: None,
        written: None,
    };
    let mut ledger = PgCommandLedger::new(company, connection, authority, clock, identifiers);
    ledger.execute(&request, &mut effects).await
}

