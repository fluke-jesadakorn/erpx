use crate::{
    CANDIDATE_PROJECTION, CANDIDATE_SNAPSHOT_EVENT, WORKFORCE_PROJECTION, WORKFORCE_SNAPSHOT_EVENT,
};
use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_talent::{TalentAggregate, TalentRealm};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TalentSnapshot {
    pub aggregate: TalentAggregate,
}
#[derive(Debug, Clone)]
pub struct TalentCandidateProjector {
    types: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct TalentWorkforceProjector {
    types: Vec<String>,
}
impl TalentCandidateProjector {
    pub fn new() -> Self {
        Self {
            types: vec![CANDIDATE_SNAPSHOT_EVENT.into()],
        }
    }
}
impl Default for TalentCandidateProjector {
    fn default() -> Self {
        Self::new()
    }
}
impl TalentWorkforceProjector {
    pub fn new() -> Self {
        Self {
            types: vec![WORKFORCE_SNAPSHOT_EVENT.into()],
        }
    }
}
impl Default for TalentWorkforceProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for TalentCandidateProjector {
    fn name(&self) -> &str {
        CANDIDATE_PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.types
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let aggregate = decode(
            ctx,
            event,
            CANDIDATE_PROJECTION,
            CANDIDATE_SNAPSHOT_EVENT,
            TalentRealm::Candidate,
        )?;
        let principal = aggregate
            .candidate_principal_id()
            .ok_or_else(|| ProjectionFailure::new("candidate projection has no owner"))?;
        let status = status(&aggregate)?;
        changed(sqlx::query("INSERT INTO erpx_talent_read.candidate_projection (projection_name,generation,partition_key,record_id,record_type,scope_key,active_scope,authority_realm,candidate_principal_id,lifecycle_status,source_aggregate_version,source_global_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,'candidate',$8,$9,$10,$11,clock_timestamp()) ON CONFLICT (holding_id,projection_name,generation,partition_key,record_id) DO UPDATE SET lifecycle_status=EXCLUDED.lifecycle_status,source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=clock_timestamp() WHERE erpx_talent_read.candidate_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(CANDIDATE_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(aggregate.id_uuid()).bind(variant(aggregate.aggregate_type())?).bind(scope_key(aggregate.scope())?).bind(serde_json::to_value(aggregate.scope()).map_err(failure)?).bind(principal).bind(status).bind(i64v(event.aggregate_version)?).bind(i64v(event.global_sequence)?).execute(&mut **tx).await.map_err(failure)?,"talent candidate snapshot version did not advance")
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(tx, ctx, "erpx_talent_read.candidate_projection").await
    }
}

#[async_trait]
impl Projector for TalentWorkforceProjector {
    fn name(&self) -> &str {
        WORKFORCE_PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.types
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let aggregate = decode(
            ctx,
            event,
            WORKFORCE_PROJECTION,
            WORKFORCE_SNAPSHOT_EVENT,
            TalentRealm::Workforce,
        )?;
        let status = status(&aggregate)?;
        changed(sqlx::query("INSERT INTO erpx_talent_read.workforce_projection (projection_name,generation,partition_key,record_id,record_type,scope_key,active_scope,authority_realm,lifecycle_status,source_aggregate_version,source_global_sequence,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,'workforce',$8,$9,$10,clock_timestamp()) ON CONFLICT (holding_id,projection_name,generation,partition_key,record_id) DO UPDATE SET lifecycle_status=EXCLUDED.lifecycle_status,source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,projected_at=clock_timestamp() WHERE erpx_talent_read.workforce_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(WORKFORCE_PROJECTION).bind(ctx.generation).bind(&event.partition).bind(aggregate.id_uuid()).bind(variant(aggregate.aggregate_type())?).bind(scope_key(aggregate.scope())?).bind(serde_json::to_value(aggregate.scope()).map_err(failure)?).bind(status).bind(i64v(event.aggregate_version)?).bind(i64v(event.global_sequence)?).execute(&mut **tx).await.map_err(failure)?,"talent workforce snapshot version did not advance")
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(tx, ctx, "erpx_talent_read.workforce_projection").await
    }
}

fn decode(
    ctx: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    name: &str,
    event_type: &str,
    realm: TalentRealm,
) -> Result<TalentAggregate, ProjectionFailure> {
    if ctx.projection != name
        || event.context != "talent"
        || event.event_type != event_type
        || event.event_version != 1
    {
        return Err(ProjectionFailure::new(
            "talent projector received an incompatible event",
        ));
    }
    let snapshot: TalentSnapshot =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let a = snapshot.aggregate;
    if a.authority_realm() != realm
        || a.id_uuid() != event.aggregate_id.as_uuid()
        || a.version() != event.aggregate_version
        || a.scope() != &event.active_scope
        || crate::aggregate_partition(a.aggregate_type(), a.id_uuid()) != event.partition
    {
        return Err(ProjectionFailure::new("talent snapshot metadata mismatch"));
    }
    Ok(a)
}
async fn reset(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &ProjectorContext<'_>,
    table: &str,
) -> Result<(), ProjectionFailure> {
    let q = format!("DELETE FROM {table} WHERE projection_name=$1 AND generation=$2");
    sqlx::query(&q)
        .bind(ctx.projection)
        .bind(ctx.generation)
        .execute(&mut **tx)
        .await
        .map_err(failure)?;
    Ok(())
}
fn changed(r: sqlx::postgres::PgQueryResult, m: &str) -> Result<(), ProjectionFailure> {
    if r.rows_affected() != 1 {
        return Err(ProjectionFailure::new(m));
    }
    Ok(())
}
fn status(a: &TalentAggregate) -> Result<String, ProjectionFailure> {
    serde_json::to_value(a)
        .map_err(failure)?
        .get("state")
        .and_then(|v| v.get("status"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ProjectionFailure::new("talent status is missing"))
}
fn scope_key(s: &erpx_kernel::AuthorityScope) -> Result<String, ProjectionFailure> {
    match s {
        erpx_kernel::AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        erpx_kernel::AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        _ => Err(ProjectionFailure::new(
            "talent projection scope must be exact",
        )),
    }
}
fn variant(k: &str) -> Result<&'static str, ProjectionFailure> {
    match k {
        "talent_requisition" => Ok("requisition"),
        "talent_candidate" => Ok("candidate"),
        "talent_application" => Ok("application"),
        "talent_offer" => Ok("offer"),
        "talent_hire_handoff" => Ok("hire_handoff"),
        "talent_proposal" => Ok("proposal"),
        "talent_interview" => Ok("interview"),
        "talent_review_cycle" => Ok("review_cycle"),
        _ => Err(ProjectionFailure::new("unsupported talent projection")),
    }
}
fn i64v(v: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(v).map_err(|_| ProjectionFailure::new("talent numeric value is too large"))
}
fn failure(e: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projectors_are_generation_managed() {
        assert_eq!(TalentCandidateProjector::new().name(), CANDIDATE_PROJECTION);
        assert_eq!(TalentWorkforceProjector::new().name(), WORKFORCE_PROJECTION);
        assert!(include_str!("runner_projection.rs").contains("ctx.generation"));
    }
}
