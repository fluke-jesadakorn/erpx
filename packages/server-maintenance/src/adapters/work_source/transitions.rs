//! Actual Work transition effects. Service retains scheduling/settlement
//! sources separately; no foreign mutation or command grant belongs here.
use super::*;
use crate::{
    application::work_commands::MaintenanceWorkCommand,
    domain::{
        work::{MaintenanceState, maintenance_state},
        work_input::MaintenanceChangeAction,
    },
};
use erpx_server_assets::public::maintenance::{
    MaintenanceAssetCondition, MaintenancePhysicalAction, MaintenancePhysicalChange,
    MaintenancePhysicalReceipt, RetainedMaintenanceAssetImage,
};

/// Owned intended rows, not a current Work/purchase/Workflow grant.
pub struct PgMaintenanceTransitionPlan {
    facts: MaintenanceWorkSourceFacts,
    transaction: Transaction,
    command: MaintenanceWorkCommand,
    state: MaintenanceState,
    at: TimestampMillis,
    version: i64,
    transition: MaintenanceTransition,
    before_digest: String,
    after_digest: String,
    event_digest: String,
    event_id: String,
    asset: RetainedMaintenanceAssetImage,
    physical: Option<MaintenancePhysicalChange>,
}
pub struct PgMaintenanceTransitionReceipt {
    facts: MaintenanceWorkSourceFacts,
    transaction: Transaction,
    version: i64,
    after_digest: String,
    event_digest: String,
    event_id: String,
    asset: RetainedMaintenanceAssetImage,
    physical: Option<MaintenancePhysicalChange>,
}
fn transition(command: &MaintenanceWorkCommand) -> Result<MaintenanceTransition, BusinessError> {
    Ok(match command.input().action {
        MaintenanceChangeAction::Start => MaintenanceTransition::Start,
        MaintenanceChangeAction::Block => MaintenanceTransition::Block,
        MaintenanceChangeAction::Resume => MaintenanceTransition::Resume,
        MaintenanceChangeAction::Complete { .. } => MaintenanceTransition::Complete,
        MaintenanceChangeAction::Cancel => MaintenanceTransition::Cancel,
        _ => {
            return Err(BusinessError::invalid(
                "MAINTENANCE_EFFECT_ACTION_UNAVAILABLE",
                "Retain the actual scheduling or parts-settlement source for this action.",
            ));
        }
    })
}
impl CurrentMaintenanceWork<'_> {
    pub async fn retain_transition_effects(
        &self,
        c: &mut PgConnection,
        command: &MaintenanceWorkCommand,
        at: TimestampMillis,
        now: TimestampMillis,
    ) -> Result<PgMaintenanceTransitionPlan, Error> {
        let transition = transition(command)?;
        if command.work_id() != &self.facts.id
            || at.get() < self.observed_at
            || at.get() > now.get()
        {
            return Err(changed().into());
        }
        self.require_action(&transition)?;
        self.require_current(c, now).await?;
        let version = command
            .input()
            .expected_version
            .checked_add(1)
            .filter(|v| *v <= 9_007_199_254_740_992)
            .ok_or_else(changed)?;
        let state = maintenance_state(
            &MaintenanceWorkFacts {
                owner_id: self.facts.owner_id.clone(),
                status: self.facts.status.clone(),
                owner_eligible: self.owner.is_eligible(),
                reviewer_eligible: self.reviewer.is_eligible(),
                scheduled_at: self.facts.scheduled_at,
                downtime_started_at: self.facts.downtime_started_at,
            },
            &transition,
            at.get(),
        );
        let blocked = if matches!(transition, MaintenanceTransition::Block) {
            Some(command.input().reason.as_str())
        } else {
            None
        };
        let(before_digest,after_digest):(String,String)=sqlx::query_as("SELECT encode(sha256(convert_to(to_jsonb(w)::text,'UTF8')),'hex'),encode(sha256(convert_to((to_jsonb(w)||jsonb_build_object('status',$2::text,'owner_id',$3::text,'scheduled_at',$4::bigint,'downtime_started_at',$5::bigint,'downtime_ended_at',$6::bigint,'blocked_reason',$7::text,'completion_evidence',$8::text,'resulting_condition',$9::text,'version',$10::bigint,'updated_at',$11::bigint))::text,'UTF8')),'hex') FROM maintenance_work_orders w WHERE id=$1")
   .bind(self.facts.id.as_str()).bind(state.status).bind(state.owner_id.as_str()).bind(state.scheduled_at).bind(state.downtime_started_at).bind(state.downtime_ended_at).bind(blocked).bind(completion(command).0).bind(completion(command).1).bind(version).bind(at.get()).fetch_one(&mut*c).await?;
        let event_id = format!("maintenance-event-{}", command.key().as_str());
        let event_digest:String=sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'work_order_id',$2::text,'version',$3::bigint,'action',$4::text,'actor_id',$5::text,'reason',$6::text,'status',$7::text,'owner_id',$8::text,'scheduled_at',$9::bigint,'downtime_started_at',$10::bigint,'downtime_ended_at',$11::bigint,'created_at',$12::bigint)::text,'UTF8')),'hex')")
   .bind(&event_id).bind(self.facts.id.as_str()).bind(version).bind(transition.action()).bind(self.current.user_id().as_str()).bind(&command.input().reason).bind(state.status).bind(state.owner_id.as_str()).bind(state.scheduled_at).bind(state.downtime_started_at).bind(state.downtime_ended_at).bind(at.get()).fetch_one(&mut*c).await?;
        let asset = self.asset.retain_unchanged_image(c).await?;
        let physical = match &command.input().action {
            MaintenanceChangeAction::Start => Some(MaintenancePhysicalAction::Start),
            MaintenanceChangeAction::Complete {
                resulting_condition,
                ..
            } => Some(MaintenancePhysicalAction::Complete {
                condition: match resulting_condition.as_str() {
                    "good" => MaintenanceAssetCondition::Good,
                    "degraded" => MaintenanceAssetCondition::Degraded,
                    "unsafe" => MaintenanceAssetCondition::Unsafe,
                    _ => return Err(changed().into()),
                },
            }),
            MaintenanceChangeAction::Cancel => Some(MaintenancePhysicalAction::Cancel {
                downtime_started_at: self.facts.downtime_started_at,
            }),
            _ => None,
        }
        .map(|action| MaintenancePhysicalChange {
            action,
            expected_asset_version: self.asset.facts().version,
            actor_id: self.current.user_id().clone(),
            command_key: command.key().clone(),
            reason: command.input().reason.clone(),
            at: at.get(),
        });
        Ok(PgMaintenanceTransitionPlan {
            facts: self.facts.clone(),
            transaction: self.transaction.clone(),
            command: command.clone(),
            state,
            at,
            version,
            transition,
            before_digest,
            after_digest,
            event_digest,
            event_id,
            asset,
            physical,
        })
    }
}
impl PgMaintenanceTransitionPlan {
    /// Intended physical data only; Service must prepare/apply it through the
    /// actual borrowed Assets source, after the Work update for Start.
    pub fn physical_change(&self) -> Option<&MaintenancePhysicalChange> {
        self.physical.as_ref()
    }
    pub fn command(&self) -> &MaintenanceWorkCommand {
        &self.command
    }
    pub fn application_at(&self) -> TimestampMillis {
        self.at
    }
    pub fn version_after(&self) -> i64 {
        self.version
    }
    pub fn status_after(&self) -> &'static str {
        self.state.status
    }
    pub async fn require_current_source(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if source.facts != self.facts || source.transaction != self.transaction {
            return Err(changed().into());
        }
        source.require_current(c, now).await?;
        if work_digest(c, &self.facts.id).await?.as_deref() != Some(self.before_digest.as_str()) {
            return Err(changed().into());
        }
        self.asset.require_current(c).await?;
        Ok(())
    }
    pub async fn require_before(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current_source(c, source, now).await?;
        source.require_action(&self.transition)?;
        source.require_expected_version(self.command.input().expected_version)?;
        Ok(())
    }
    /// Work CAS/update precedes the original late start Asset lifecycle guard.
    pub async fn write_work(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_before(c, source, now).await?;
        let blocked = if matches!(self.transition, MaintenanceTransition::Block) {
            Some(self.command.input().reason.as_str())
        } else {
            None
        };
        let n=sqlx::query("UPDATE maintenance_work_orders SET status=$2,owner_id=$3,scheduled_at=$4,downtime_started_at=$5,downtime_ended_at=$6,blocked_reason=$7,completion_evidence=$8,resulting_condition=$9,version=$10,updated_at=$11 WHERE id=$1 AND version=$12")
   .bind(self.facts.id.as_str()).bind(self.state.status).bind(self.state.owner_id.as_str()).bind(self.state.scheduled_at).bind(self.state.downtime_started_at).bind(self.state.downtime_ended_at).bind(blocked).bind(completion(&self.command).0).bind(completion(&self.command).1).bind(self.version).bind(self.at.get()).bind(self.facts.version).execute(&mut*c).await?.rows_affected();
        if n != 1 {
            return Err(changed().into());
        }
        Ok(())
    }
    /// Service applies the real Assets start effect after `write_work`, then
    /// supplies its original opaque receipt before the Work event is appended.
    pub async fn finish(
        self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        started_asset: Option<&MaintenancePhysicalReceipt>,
        now: TimestampMillis,
    ) -> Result<PgMaintenanceTransitionReceipt, Error> {
        require_external_after(c, source, &self.facts, &self.transaction, now).await?;
        require_asset_after(
            c,
            source,
            &self.asset,
            self.physical.as_ref(),
            started_asset,
        )
        .await?;
        if work_digest(c, &self.facts.id).await?.as_deref() != Some(self.after_digest.as_str()) {
            return Err(changed().into());
        }

        sqlx::query("INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,downtime_started_at,downtime_ended_at,created_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
   .bind(&self.event_id).bind(self.facts.id.as_str()).bind(self.version).bind(self.transition.action()).bind(source.current.user_id().as_str()).bind(&self.command.input().reason).bind(self.state.status).bind(self.state.owner_id.as_str()).bind(self.state.scheduled_at).bind(self.state.downtime_started_at).bind(self.state.downtime_ended_at).bind(self.at.get()).execute(&mut*c).await?;
        let receipt = PgMaintenanceTransitionReceipt {
            facts: self.facts,
            transaction: self.transaction,
            version: self.version,
            after_digest: self.after_digest,
            event_digest: self.event_digest,
            event_id: self.event_id,
            asset: self.asset,
            physical: self.physical,
        };
        receipt.require_after(c, source, started_asset, now).await?;
        Ok(receipt)
    }
}
impl PgMaintenanceTransitionReceipt {
    pub async fn require_after(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        started_asset: Option<&MaintenancePhysicalReceipt>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        require_external_after(c, source, &self.facts, &self.transaction, now).await?;
        require_asset_after(
            c,
            source,
            &self.asset,
            self.physical.as_ref(),
            started_asset,
        )
        .await?;
        if work_digest(c, &self.facts.id).await?.as_deref() != Some(self.after_digest.as_str()) {
            return Err(changed().into());
        }
        let event:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex')FROM maintenance_work_events e WHERE id=$1 AND work_order_id=$2 AND version=$3")
   .bind(&self.event_id).bind(self.facts.id.as_str()).bind(self.version).fetch_optional(c).await?;
        if event.as_deref() != Some(self.event_digest.as_str()) {
            return Err(changed().into());
        }
        Ok(())
    }
}
async fn work_digest(c: &mut PgConnection, id: &RecordId) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(w)::text,'UTF8')),'hex')FROM maintenance_work_orders w WHERE id=$1").bind(id.as_str()).fetch_optional(c).await?)
}

async fn require_external_after(
    c: &mut PgConnection,
    source: &CurrentMaintenanceWork<'_>,
    facts: &MaintenanceWorkSourceFacts,
    unit: &Transaction,
    now: TimestampMillis,
) -> Result<(), Error> {
    if &source.facts != facts
        || &source.transaction != unit
        || now.get() < source.observed_at
        || now.get() >= source.current.expires_at()
        || &transaction(c).await? != unit
    {
        return Err(changed().into());
    }
    source.owner.require_current(c, now).await?;
    source.reviewer.require_current(c, now).await
}
async fn require_asset_after(
    c: &mut PgConnection,
    source: &CurrentMaintenanceWork<'_>,
    unchanged: &RetainedMaintenanceAssetImage,
    physical: Option<&MaintenancePhysicalChange>,
    receipt: Option<&MaintenancePhysicalReceipt>,
) -> Result<(), Error> {
    match (physical, receipt) {
        (Some(change), Some(receipt)) => {
            receipt.require_applied_from(c, &source.asset, change).await
        }
        (None, None) => unchanged.require_current(c).await,
        _ => Err(changed().into()),
    }
}

fn completion(command: &MaintenanceWorkCommand) -> (Option<&str>, Option<&str>) {
    match &command.input().action {
        MaintenanceChangeAction::Complete {
            evidence,
            resulting_condition,
        } => (Some(evidence), Some(resulting_condition)),
        _ => (None, None),
    }
}
