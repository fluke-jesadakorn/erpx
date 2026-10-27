//! Owner-local physical effects. These opaque receipts prove facts only: the
//! Service must retain actual Work/IAM/People/purchase authority independently.
use super::*;
use crate::domain::maintenance::{
    MaintenanceAssetCondition as Condition, MaintenanceAssetLifecycle as Lifecycle,
    MaintenanceAssetState, MaintenancePhysicalAction, maintenance_asset_after,
    require_maintenance_start_source,
};
use erpx_server_shared::public::{CommandKey, UserId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenancePhysicalChange {
    pub action: MaintenancePhysicalAction,
    /// Asset version retained by the Work source, not a new caller grant.
    pub expected_asset_version: i64,
    pub actor_id: UserId,
    pub command_key: CommandKey,
    pub reason: String,
    pub at: i64,
}
pub struct PreparedMaintenancePhysicalChange {
    before: RetainedMaintenanceAssetImage,
    state: MaintenanceAssetState,
    change: MaintenancePhysicalChange,
    expected_asset: String,
    expected_event: Option<String>,
    event_id: String,
}
pub struct MaintenancePhysicalReceipt {
    change: MaintenancePhysicalChange,
    facts: MaintenanceAssetFacts,
    transaction: Transaction,
    expected_asset: String,
    expected_event: Option<String>,
    event_id: String,
}
impl CurrentMaintenanceAsset {
    /// Compute full expected images inside PostgreSQL before Work/Asset effects.
    /// Start admission is deliberately deferred until `apply`, after Work CAS.
    pub async fn prepare_physical_change(
        &self,
        c: &mut PgConnection,
        change: MaintenancePhysicalChange,
    ) -> Result<PreparedMaintenancePhysicalChange, Error> {
        if change.at < 0
            || change.at > 9_007_199_254_740_991
            || change.reason.chars().count() > 2000
            || change.reason.len() > 8192
            || change.reason.is_empty()
        {
            return Err(PersistenceError::new("ASSET_MAINTENANCE_EFFECT_INPUT_INVALID").into());
        }
        let before = self.retain_unchanged_image(c).await?;
        let row = sqlx::query("SELECT condition,lifecycle_status,placement IS NOT NULL placement_present FROM operational_assets WHERE id=$1 AND company_id=$2")
            .bind(self.facts.asset_id.as_str()).bind(self.facts.company_id.as_str()).fetch_one(&mut *c).await?;
        let condition = match row.try_get::<&str, _>("condition")? {
            "good" => Condition::Good,
            "degraded" => Condition::Degraded,
            "unsafe" => Condition::Unsafe,
            _ => return Err(invalid().into()),
        };
        let lifecycle = match row.try_get::<&str, _>("lifecycle_status")? {
            "registered" => Lifecycle::Registered,
            "in_service" => Lifecycle::InService,
            "maintenance" => Lifecycle::Maintenance,
            "retirement_pending" => Lifecycle::RetirementPending,
            "retired" => Lifecycle::Retired,
            _ => return Err(invalid().into()),
        };
        let state = MaintenanceAssetState {
            version: self.facts.version,
            condition,
            lifecycle,
            placement_present: row.try_get("placement_present")?,
        };
        let next = maintenance_asset_after(&state, &change.action)?;
        let event_id = format!("asset-event-{}", change.command_key.as_str());
        let (expected_asset, expected_event) = if let Some(ref next) = next {
            let asset:String = sqlx::query_scalar("SELECT encode(sha256(convert_to((to_jsonb(a)||jsonb_build_object('version',$3::bigint,'condition',$4::text,'lifecycle_status',$5::text,'updated_at',$6::bigint))::text,'UTF8')),'hex') FROM operational_assets a WHERE id=$1 AND company_id=$2")
                .bind(self.facts.asset_id.as_str()).bind(self.facts.company_id.as_str()).bind(next.version).bind(next.condition.as_str()).bind(next.lifecycle.as_str()).bind(change.at).fetch_one(&mut *c).await?;
            let event:String = sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$3::text,'asset_id',a.id,'version',$4::bigint,'action',$5::text,'actor_id',$6::text,'reason',$7::text,'condition',$8::text,'lifecycle_status',$9::text,'placement',a.placement,'custodian_id',a.custodian_id,'created_at',$10::bigint)::text,'UTF8')),'hex') FROM operational_assets a WHERE id=$1 AND company_id=$2")
                .bind(self.facts.asset_id.as_str()).bind(self.facts.company_id.as_str()).bind(&event_id).bind(next.version).bind(change.action.event_action()).bind(change.actor_id.as_str()).bind(&change.reason).bind(next.condition.as_str()).bind(next.lifecycle.as_str()).bind(change.at).fetch_one(&mut *c).await?;
            (asset, Some(event))
        } else {
            (before.digest.clone(), None)
        };
        Ok(PreparedMaintenancePhysicalChange {
            before,
            state,
            change,
            expected_asset,
            expected_event,
            event_id,
        })
    }
}
impl PreparedMaintenancePhysicalChange {
    /// Call only after the enclosing Work CAS/update. This does not commit or
    /// mint Work authority. The original actual source must be reborrowed.
    pub async fn apply(
        self,
        c: &mut PgConnection,
        source: &CurrentMaintenanceAsset,
    ) -> Result<MaintenancePhysicalReceipt, Error> {
        if source.facts != self.before.facts || source.transaction != self.before.transaction {
            return Err(changed().into());
        }
        self.before.require_current(c).await?;
        if matches!(self.change.action, MaintenancePhysicalAction::Start) {
            require_maintenance_start_source(&self.state, self.change.expected_asset_version)?;
        }
        let next = maintenance_asset_after(&self.state, &self.change.action)?;
        let mut facts = self.before.facts;
        if let Some(next) = next {
            let changed_rows=sqlx::query("UPDATE operational_assets SET lifecycle_status=$3,condition=$4,version=version+1,updated_at=$5 WHERE id=$1 AND company_id=$2 AND version=$6")
                .bind(facts.asset_id.as_str()).bind(facts.company_id.as_str()).bind(next.lifecycle.as_str()).bind(next.condition.as_str()).bind(self.change.at).bind(facts.version).execute(&mut *c).await?.rows_affected();
            if changed_rows != 1 {
                return Err(changed().into());
            }
            sqlx::query("INSERT INTO operational_asset_events SELECT $3,id,version,$4,$5,$6,condition,lifecycle_status,placement,custodian_id,$7 FROM operational_assets WHERE id=$1 AND company_id=$2")
                .bind(facts.asset_id.as_str()).bind(facts.company_id.as_str()).bind(&self.event_id).bind(self.change.action.event_action()).bind(self.change.actor_id.as_str()).bind(&self.change.reason).bind(self.change.at).execute(&mut *c).await?;
            facts.version = next.version;
        }
        let receipt = MaintenancePhysicalReceipt {
            change: self.change,
            facts,
            transaction: self.before.transaction,
            expected_asset: self.expected_asset,
            expected_event: self.expected_event,
            event_id: self.event_id,
        };
        receipt.require_current(c).await?;
        Ok(receipt)
    }
}
impl MaintenancePhysicalReceipt {
    /// Retain the original actual Asset identity after this specific start
    /// effect. A Complete/Cancel receipt cannot impersonate a Start witness.
    pub async fn require_started_from(
        &self,
        c: &mut PgConnection,
        before: &CurrentMaintenanceAsset,
        expected_change: &MaintenancePhysicalChange,
    ) -> Result<(), Error> {
        if !matches!(expected_change.action, MaintenancePhysicalAction::Start) {
            return Err(changed().into());
        }
        self.require_applied_from(c, before, expected_change).await
    }
    /// Exact original action/actor/key/time/source binding; another command's
    /// actual physical receipt cannot substitute for this expected action.
    pub async fn require_applied_from(
        &self,
        c: &mut PgConnection,
        before: &CurrentMaintenanceAsset,
        expected_change: &MaintenancePhysicalChange,
    ) -> Result<(), Error> {
        let mut expected = before.facts.clone();
        if !matches!(
            expected_change.action,
            MaintenancePhysicalAction::Cancel {
                downtime_started_at: None | Some(0)
            }
        ) {
            expected.version = expected.version.checked_add(1).ok_or_else(changed)?;
        }
        if &self.change != expected_change
            || self.facts != expected
            || self.transaction != before.transaction
        {
            return Err(changed().into());
        }
        self.require_current(c).await
    }

    pub fn facts(&self) -> &MaintenanceAssetFacts {
        &self.facts
    }
    /// Recheck after caller flushes deferred effects, before the same unit commits.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.transaction
            || image_digest(c, &self.facts).await?.as_deref() != Some(self.expected_asset.as_str())
        {
            return Err(changed().into());
        }
        let actual:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM operational_asset_events e WHERE id=$1 AND asset_id=$2")
            .bind(&self.event_id).bind(self.facts.asset_id.as_str()).fetch_optional(c).await?;
        if actual != self.expected_event {
            return Err(changed().into());
        }
        Ok(())
    }
}
