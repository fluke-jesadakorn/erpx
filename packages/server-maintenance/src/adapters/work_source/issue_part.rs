//! Actual Work/event effects, bound to the original retained native Work source.
//! Inventory/Finance, purchases, notifications, receipts, audit and outbox remain
//! mandatory separate effects in the same enclosing command transaction.
use super::*;
use crate::application::work_commands::MaintenanceWorkCommand;
use crate::domain::work::{MaintenanceState, maintenance_state};
use crate::domain::work_input::MaintenanceChangeAction;
use erpx_server_assets::public::maintenance::RetainedMaintenanceAssetImage;

/// No public constructor or Clone. The complete intended Work/event images are
/// derived before effects, never reconstructed from trigger-returned rows.
pub struct PgMaintenanceIssuePartEffects<'source, 'authority> {
    source: &'source CurrentMaintenanceWork<'authority>,
    asset_image: RetainedMaintenanceAssetImage,
    command: &'source MaintenanceWorkCommand,
    state: MaintenanceState,
    at: TimestampMillis,
    version: i64,
    event_id: String,
    before_digest: String,
    after_digest: String,
    event_digest: Option<String>,
}
impl<'authority> CurrentMaintenanceWork<'authority> {
    /// Derive expected data before Finance preparation without asserting the
    /// requested version early. require_before is the original late apply gate.
    pub async fn retain_issue_part_effects<'s>(
        &'s self,
        c: &mut PgConnection,
        command: &'s MaintenanceWorkCommand,
        application_at: TimestampMillis,
        now: TimestampMillis,
    ) -> Result<PgMaintenanceIssuePartEffects<'s, 'authority>, Error> {
        if application_at.get() < self.observed_at || application_at.get() > now.get() {
            return Err(changed().into());
        }
        if !matches!(
            command.input().action,
            MaintenanceChangeAction::IssuePart { .. }
        ) || command.work_id() != &self.facts.id
        {
            return Err(changed().into());
        }
        self.require_action(&MaintenanceTransition::IssuePart)?;
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
            &MaintenanceTransition::IssuePart,
            application_at.get(),
        );
        // Hash complete owned rows in PostgreSQL. Huge unrelated title/evidence
        // bodies stay in the database, while any unintended rewrite is detected.
        let (before_digest, after_digest):(String,String)=sqlx::query_as("SELECT encode(sha256(convert_to(to_jsonb(w)::text,'UTF8')),'hex'),encode(sha256(convert_to((to_jsonb(w)||jsonb_build_object('status','in_progress','owner_id',$2::text,'scheduled_at',$3::bigint,'downtime_started_at',$4::bigint,'downtime_ended_at',NULL,'blocked_reason',NULL,'completion_evidence',NULL,'resulting_condition',NULL,'version',$5::bigint,'updated_at',$6::bigint))::text,'UTF8')),'hex') FROM maintenance_work_orders w WHERE id=$1")
            .bind(self.facts.id.as_str()).bind(state.owner_id.as_str()).bind(state.scheduled_at).bind(state.downtime_started_at).bind(version).bind(application_at.get()).fetch_one(&mut *c).await?;
        let event_id = format!("maintenance-event-{}", command.key().as_str());
        let event_digest = None;
        let asset_image = self.asset.retain_unchanged_image(c).await?;
        Ok(PgMaintenanceIssuePartEffects {
            source: self,
            asset_image,
            command,
            state,
            at: application_at,
            version,
            event_id,
            before_digest,
            after_digest,
            event_digest,
        })
    }
}
impl PgMaintenanceIssuePartEffects<'_, '_> {
    pub async fn finalize_event(
        &mut self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current_source(c, now).await?;
        if self.event_digest.is_some() {
            return Err(changed().into());
        }
        self.event_digest = Some(
            issue_event_digest(
                c,
                self.source,
                self.command,
                &self.state,
                self.at,
                self.version,
                &self.event_id,
            )
            .await?,
        );
        Ok(())
    }
    pub fn version_after(&self) -> i64 {
        self.version
    }
    pub fn source(&self) -> &CurrentMaintenanceWork<'_> {
        self.source
    }
    /// Called before Inventory/Finance effects. Original selected current IAM
    /// remains owned by the outer authorizer until the actual commit.
    pub async fn require_current_source(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.source.require_current(c, now).await?;
        self.asset_image.require_current(c).await?;
        if work_digest(c, &self.source.facts.id).await?.as_deref() != Some(&self.before_digest) {
            return Err(changed().into());
        }
        Ok(())
    }
    /// Original apply admission: current worker/source first, then the late
    /// requested-version assertion after Finance/Money preparation.
    pub async fn require_before(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current_source(c, now).await?;
        self.source
            .require_action(&MaintenanceTransition::IssuePart)?;
        self.source
            .require_expected_version(self.command.input().expected_version)?;
        Ok(())
    }
    /// After Inventory stock and Money. Do not call require_current on the old
    /// Work image after this intentional version change; retain_external plus
    /// these predetermined full-row images are the final protocol instead.
    pub async fn write(&self, c: &mut PgConnection, now: TimestampMillis) -> Result<(), Error> {
        self.require_before(c, now).await?;
        if self.event_digest.is_none() {
            return Err(changed().into());
        }
        let n=sqlx::query("UPDATE maintenance_work_orders SET status='in_progress',owner_id=$2,scheduled_at=$3,downtime_started_at=$4,downtime_ended_at=NULL,blocked_reason=NULL,completion_evidence=NULL,resulting_condition=NULL,version=$5,updated_at=$6 WHERE id=$1 AND version=$7")
            .bind(self.source.facts.id.as_str()).bind(self.state.owner_id.as_str()).bind(self.state.scheduled_at).bind(self.state.downtime_started_at).bind(self.version).bind(self.at.get()).bind(self.source.facts.version).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed().into());
        }
        sqlx::query("INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,downtime_started_at,downtime_ended_at,created_at)VALUES($1,$2,$3,'issue-part',$4,$5,'in_progress',$6,$7,$8,NULL,$9)")
            .bind(&self.event_id).bind(self.source.facts.id.as_str()).bind(self.version).bind(self.source.current.user_id().as_str()).bind(&self.command.input().reason).bind(self.state.owner_id.as_str()).bind(self.state.scheduled_at).bind(self.state.downtime_started_at).bind(self.at.get()).execute(&mut *c).await?;
        self.require_after(c, now).await
    }
    /// Detach only predetermined owned images for Workflow's final retain
    /// phase after audit/outbox. The receipt still requires the original sealed
    /// Work source; it cannot reconstruct current IAM/People/Asset authority.
    pub async fn into_retention(
        self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<PgMaintenanceIssuePartReceipt, Error> {
        self.require_after(c, now).await?;
        Ok(PgMaintenanceIssuePartReceipt {
            facts: self.source.facts.clone(),
            transaction: self.source.transaction.clone(),
            asset_image: self.asset_image,
            after_digest: self.after_digest,
            event_digest: self.event_digest,
            event_id: self.event_id,
            version: self.version,
        })
    }
    /// Repeat after remaining owner effects and deferred constraint processing.
    pub async fn require_after(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.source.require_external(c, now).await?;
        self.asset_image.require_current(c).await?;
        if work_digest(c, &self.source.facts.id).await?.as_deref() != Some(&self.after_digest) {
            return Err(changed().into());
        }
        let event:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM maintenance_work_events e WHERE work_order_id=$1 AND version=$2 AND id=$3")
            .bind(self.source.facts.id.as_str()).bind(self.version).bind(&self.event_id).fetch_optional(c).await?;
        if event.as_deref() != self.event_digest.as_deref() || self.event_digest.is_none() {
            return Err(changed().into());
        }
        Ok(())
    }
}
async fn issue_event_digest(
    c: &mut PgConnection,
    source: &CurrentMaintenanceWork<'_>,
    command: &MaintenanceWorkCommand,
    state: &MaintenanceState,
    at: TimestampMillis,
    version: i64,
    event_id: &str,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'work_order_id',$2::text,'version',$3::bigint,'action','issue-part','actor_id',$4::text,'reason',$5::text,'status','in_progress','owner_id',$6::text,'scheduled_at',$7::bigint,'downtime_started_at',$8::bigint,'downtime_ended_at',NULL,'created_at',$9::bigint)::text,'UTF8')),'hex')")
        .bind(event_id).bind(source.facts.id.as_str()).bind(version).bind(source.current.user_id().as_str()).bind(&command.input().reason).bind(&state.owner_id).bind(state.scheduled_at).bind(state.downtime_started_at).bind(at.get()).fetch_one(c).await?)
}
async fn work_digest(c: &mut PgConnection, id: &RecordId) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(w)::text,'UTF8')),'hex') FROM maintenance_work_orders w WHERE id=$1").bind(id.as_str()).fetch_optional(c).await?)
}

/// Predetermined own effects, not an independently current owner grant.
pub struct PgMaintenanceIssuePartReceipt {
    facts: MaintenanceWorkSourceFacts,
    transaction: Transaction,
    asset_image: RetainedMaintenanceAssetImage,
    after_digest: String,
    event_digest: Option<String>,
    event_id: String,
    version: i64,
}
impl PgMaintenanceIssuePartReceipt {
    pub async fn require_after(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if source.facts != self.facts || source.transaction != self.transaction {
            return Err(changed().into());
        }
        source.require_external(c, now).await?;
        self.asset_image.require_current(c).await?;
        if work_digest(c, &self.facts.id).await?.as_deref() != Some(&self.after_digest) {
            return Err(changed().into());
        }
        let event:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM maintenance_work_events e WHERE work_order_id=$1 AND version=$2 AND id=$3")
            .bind(self.facts.id.as_str()).bind(self.version).bind(&self.event_id).fetch_optional(c).await?;
        if event.as_deref() != self.event_digest.as_deref() || self.event_digest.is_none() {
            return Err(changed().into());
        }
        Ok(())
    }
}

/// Owned readonly preparation for Workflow's prepare/apply boundary. It retains
/// intended images only and always requires the original sealed Work source.
pub struct PgMaintenanceIssuePartPlan {
    expected: PgMaintenanceIssuePartReceipt,
    before_digest: String,
    command: MaintenanceWorkCommand,
    state: MaintenanceState,
    at: TimestampMillis,
}
impl PgMaintenanceIssuePartEffects<'_, '_> {
    pub fn into_plan(self) -> PgMaintenanceIssuePartPlan {
        PgMaintenanceIssuePartPlan {
            expected: PgMaintenanceIssuePartReceipt {
                facts: self.source.facts.clone(),
                transaction: self.source.transaction.clone(),
                asset_image: self.asset_image,
                after_digest: self.after_digest,
                event_digest: self.event_digest,
                event_id: self.event_id,
                version: self.version,
            },
            before_digest: self.before_digest,
            command: self.command.clone(),
            state: self.state,
            at: self.at,
        }
    }
}
impl PgMaintenanceIssuePartPlan {
    /// Finalize from immutable command data before any effect is inserted.
    pub async fn finalize_event(
        &mut self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current_source(c, source, now).await?;
        if self.expected.event_digest.is_some() {
            return Err(changed().into());
        }
        self.expected.event_digest = Some(
            issue_event_digest(
                c,
                source,
                &self.command,
                &self.state,
                self.at,
                self.expected.version,
                &self.expected.event_id,
            )
            .await?,
        );
        Ok(())
    }
    /// Immutable original normalized command data, not current authority.
    pub fn command(&self) -> &MaintenanceWorkCommand {
        &self.command
    }
    pub fn application_at(&self) -> TimestampMillis {
        self.at
    }
    pub fn version_after(&self) -> i64 {
        self.expected.version
    }
    pub async fn require_current_source(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        if source.facts != self.expected.facts || source.transaction != self.expected.transaction {
            return Err(changed().into());
        }
        source.require_current(c, now).await?;
        self.expected.asset_image.require_current(c).await?;
        if work_digest(c, &source.facts.id).await?.as_deref() != Some(&self.before_digest) {
            return Err(changed().into());
        }
        Ok(())
    }
    pub async fn require_before(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_current_source(c, source, now).await?;
        source.require_action(&MaintenanceTransition::IssuePart)?;
        source.require_expected_version(self.command.input().expected_version)?;
        Ok(())
    }
    pub async fn write(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.require_before(c, source, now).await?;
        if self.expected.event_digest.is_none() {
            return Err(changed().into());
        }
        let n=sqlx::query("UPDATE maintenance_work_orders SET status='in_progress',owner_id=$2,scheduled_at=$3,downtime_started_at=$4,downtime_ended_at=NULL,blocked_reason=NULL,completion_evidence=NULL,resulting_condition=NULL,version=$5,updated_at=$6 WHERE id=$1 AND version=$7")
            .bind(source.facts.id.as_str()).bind(self.state.owner_id.as_str()).bind(self.state.scheduled_at).bind(self.state.downtime_started_at).bind(self.expected.version).bind(self.at.get()).bind(source.facts.version).execute(&mut *c).await?.rows_affected();
        if n != 1 {
            return Err(changed().into());
        }
        sqlx::query("INSERT INTO maintenance_work_events(id,work_order_id,version,action,actor_id,reason,status,owner_id,scheduled_at,downtime_started_at,downtime_ended_at,created_at)VALUES($1,$2,$3,'issue-part',$4,$5,'in_progress',$6,$7,$8,NULL,$9)")
            .bind(&self.expected.event_id).bind(source.facts.id.as_str()).bind(self.expected.version).bind(source.current.user_id().as_str()).bind(&self.command.input().reason).bind(self.state.owner_id.as_str()).bind(self.state.scheduled_at).bind(self.state.downtime_started_at).bind(self.at.get()).execute(&mut *c).await?;
        self.require_after(c, source, now).await
    }
    pub async fn require_after(
        &self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        self.expected.require_after(c, source, now).await
    }
    pub async fn into_retention(
        self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceWork<'_>,
        now: TimestampMillis,
    ) -> Result<PgMaintenanceIssuePartReceipt, Error> {
        self.require_after(c, source, now).await?;
        Ok(self.expected)
    }
}
