//! Native service-team owner facts and reads.
//!
//! The caller owns the Holding transaction, current IAM/Organization/People
//! authority and command receipt/audit/outbox. This adapter writes only Support
//! service-team rows and never checks out a pool or commits independently.
use crate::application::service_teams::{
    OrgNode, RecordScope, ServiceTeamActor, ServiceTeamPlan, ServiceTeamRow,
};
use crate::domain::service_teams::{Coverage, ServiceTeamInput};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn forbidden(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Forbidden, code, message).into()
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SERVICE_TEAM_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("SERVICE_TEAM_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub async fn lock_service_team_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE service_teams,service_team_revisions,service_team_members,\
         service_team_coverage,command_receipts,audit_events,outbox_events \
         IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

pub async fn current_team(
    c: &mut PgConnection,
    actor: &ServiceTeamActor,
    id: &str,
) -> Result<Option<ServiceTeamRow>, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,version,configuration_json,active \
         FROM service_teams WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(actor.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, Error>(ServiceTeamRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            version: row.try_get("version")?,
            configuration_json: row.try_get("configuration_json")?,
            active: row.try_get::<i64, _>("active")? == 1,
        })
    })
    .transpose()
}

pub async fn company_member(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM assignments WHERE company_id=$1 AND user_id=$2 \
         AND revoked_at IS NULL \
         AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) LIMIT 1",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_optional(&mut *c)
    .await?
    .is_some())
}

pub async fn organization_node(
    c: &mut PgConnection,
    company_id: &str,
    id: &str,
) -> Result<Option<OrgNode>, Error> {
    let row = sqlx::query(
        "SELECT kind,branch_id FROM organization_nodes \
         WHERE id=$1 AND company_id=$2 AND status='active'",
    )
    .bind(id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok::<_, Error>(OrgNode {
            kind: row.try_get("kind")?,
            branch_id: row.try_get("branch_id")?,
        })
    })
    .transpose()
}

pub async fn service_member(
    c: &mut PgConnection,
    scope: &RecordScope,
    user_id: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 \
         AND a.revoked_at IS NULL \
         AND a.valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND a.valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$3) \
           OR (a.scope_kind='department' AND a.scope_node_id=$4))",
    )
    .bind(scope.company_id.as_str())
    .bind(user_id)
    .bind(scope.branch_id.as_deref())
    .bind(scope.department_id.as_deref())
    .fetch_optional(&mut *c)
    .await?
    .is_some())
}

/// Exact ordered scope validation used by the TypeScript adapter.
pub async fn validate_service_scope(
    c: &mut PgConnection,
    actor: &ServiceTeamActor,
    scope: &RecordScope,
) -> Result<(), Error> {
    let workspace = actor.company_id == scope.company_id
        && actor
            .branch_id
            .as_ref()
            .is_none_or(|branch| Some(branch) == scope.branch_id.as_ref())
        && actor
            .department_id
            .as_ref()
            .is_none_or(|department| Some(department) == scope.department_id.as_ref());
    if !workspace || !service_member(c, scope, &actor.user_id).await? {
        return Err(forbidden(
            "REQUEST_SCOPE_FORBIDDEN",
            "Choose a location within your authorized workspace.",
        ));
    }
    let company = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$1 \
         AND kind='company' AND status='active'",
    )
    .bind(scope.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if company.is_none() {
        return Err(conflict(
            "COMPANY_UNAVAILABLE",
            "The company is unavailable.",
        ));
    }
    if let Some(branch) = scope.branch_id.as_deref() {
        let exists = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 \
             AND kind='branch' AND status='active'",
        )
        .bind(branch)
        .bind(scope.company_id.as_str())
        .fetch_optional(&mut *c)
        .await?;
        if exists.is_none() {
            return Err(invalid("LOCATION_INVALID", "Select an active branch."));
        }
    }
    if let Some(department) = scope.department_id.as_deref() {
        let exists = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 \
             AND kind='department' AND branch_id IS NOT DISTINCT FROM $3 AND status='active'",
        )
        .bind(department)
        .bind(scope.company_id.as_str())
        .bind(scope.branch_id.as_deref())
        .fetch_optional(&mut *c)
        .await?;
        if exists.is_none() {
            return Err(invalid(
                "LOCATION_INVALID",
                "Select a department in this branch.",
            ));
        }
    }
    Ok(())
}

fn snapshot_sql() -> &'static str {
    "SELECT jsonb_build_object(\
       'teams',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'version',version,'active',active)) \
         FROM (SELECT * FROM service_teams WHERE company_id=$1 ORDER BY id)),\
       'members',(SELECT erpx_json_array_agg(jsonb_build_object('team',team_id,'user',user_id,'role',responsibility)) \
         FROM (SELECT * FROM service_team_members WHERE company_id=$2 ORDER BY team_id,user_id)),\
       'coverage',(SELECT erpx_json_array_agg(jsonb_build_object('team',team_id,'category',category,'kind',scope_kind,'node',scope_node_id)) \
         FROM (SELECT * FROM service_team_coverage WHERE company_id=$3 ORDER BY category,scope_kind,scope_node_id)),\
       'editors',(SELECT erpx_json_array_agg(user_id) FROM (SELECT user_id FROM handbook_editors WHERE company_id=$4 ORDER BY user_id) handbook_editors)\
     )::text snapshot"
}

pub async fn authority_snapshot(c: &mut PgConnection, company_id: &str) -> Result<String, Error> {
    Ok(sqlx::query_scalar(snapshot_sql())
        .bind(company_id)
        .bind(company_id)
        .bind(company_id)
        .bind(company_id)
        .fetch_one(&mut *c)
        .await?)
}

pub async fn replay(
    c: &mut PgConnection,
    actor: &ServiceTeamActor,
    key: &str,
    operation: &str,
    id: &str,
    input: &Value,
) -> Result<Option<Value>, Error> {
    let request_hash = serde_json::to_string(input)
        .map_err(|_| invalid("SERVICE_TEAM_INPUT_INVALID", "Invalid service team."))?;
    let row = sqlx::query(
        "SELECT operation,record_id,request_hash,result_json FROM command_receipts \
         WHERE command_id=$1 AND actor_id=$2 FOR SHARE",
    )
    .bind(key)
    .bind(actor.user_id.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored_operation: String = row.try_get("operation")?;
    let stored_id: String = row.try_get("record_id")?;
    let stored_hash: String = row.try_get("request_hash")?;
    if stored_operation != operation || stored_id != id || stored_hash != request_hash {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to another request.",
        ));
    }
    let result: Option<String> = row.try_get("result_json")?;
    result
        .map(|value| {
            serde_json::from_str(&value).map_err(|_| {
                conflict(
                    "SERVICE_TEAM_SOURCE_CHANGED",
                    "The service team changed before completion.",
                )
            })
        })
        .transpose()
}

pub async fn overlapping_teams(
    c: &mut PgConnection,
    actor: &ServiceTeamActor,
    id: Option<&str>,
    coverage: &Coverage,
) -> Result<Vec<String>, Error> {
    let rows = sqlx::query(
        "SELECT t.name FROM service_team_coverage c \
         JOIN service_teams t ON t.id=c.team_id AND t.company_id=c.company_id \
         WHERE c.company_id=$1 AND c.category=$2 AND c.scope_kind=$3 \
           AND c.scope_node_id=$4 AND t.active=1 \
           AND ($5::text IS NULL OR t.id<>$5) ORDER BY t.name",
    )
    .bind(actor.company_id.as_str())
    .bind(&coverage.category)
    .bind(&coverage.scope_kind)
    .bind(&coverage.scope_node_id)
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| Ok(row.try_get::<String, _>("name")?))
        .collect()
}

/// Apply the owner mutation. The composition must use the same command key for
/// its receipt/audit/outbox retain on this connection.
pub async fn apply_plan(
    c: &mut PgConnection,
    actor: &ServiceTeamActor,
    plan: &ServiceTeamPlan,
    now: i64,
) -> Result<(), Error> {
    serializable(c).await?;
    let input: &ServiceTeamInput = &plan.input;
    if plan.create {
        sqlx::query(
            "INSERT INTO service_teams(id,company_id,name,active,version,configuration_json,\
             created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$8)",
        )
        .bind(&plan.id)
        .bind(actor.company_id.as_str())
        .bind(&input.name)
        .bind(if input.active { 1_i64 } else { 0_i64 })
        .bind(plan.version)
        .bind(serde_json::to_string(input).unwrap_or_default())
        .bind(actor.user_id.as_str())
        .bind(now)
        .execute(&mut *c)
        .await?;
    } else {
        let updated = sqlx::query(
            "UPDATE service_teams SET name=$1,active=$2,version=$3,configuration_json=$4,\
             updated_at=$5 WHERE id=$6 AND company_id=$7 AND version=$8",
        )
        .bind(&input.name)
        .bind(if input.active { 1_i64 } else { 0_i64 })
        .bind(plan.version)
        .bind(serde_json::to_string(input).unwrap_or_default())
        .bind(now)
        .bind(&plan.id)
        .bind(actor.company_id.as_str())
        .bind(input.expected_version)
        .execute(&mut *c)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(conflict(
                "VERSION_CONFLICT",
                "Team setup changed. Refresh before saving.",
            ));
        }
        sqlx::query("DELETE FROM service_team_coverage WHERE team_id=$1 AND company_id=$2")
            .bind(&plan.id)
            .bind(actor.company_id.as_str())
            .execute(&mut *c)
            .await?;
        sqlx::query("DELETE FROM service_team_members WHERE team_id=$1 AND company_id=$2")
            .bind(&plan.id)
            .bind(actor.company_id.as_str())
            .execute(&mut *c)
            .await?;
    }
    sqlx::query(
        "INSERT INTO service_team_revisions(company_id,team_id,version,configuration_json,\
         actor_id,created_at) VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(actor.company_id.as_str())
    .bind(&plan.id)
    .bind(plan.version)
    .bind(serde_json::to_string(input).unwrap_or_default())
    .bind(actor.user_id.as_str())
    .bind(now)
    .execute(&mut *c)
    .await?;
    for member in &input.members {
        sqlx::query(
            "INSERT INTO service_team_members(company_id,team_id,user_id,responsibility) \
             VALUES($1,$2,$3,$4)",
        )
        .bind(actor.company_id.as_str())
        .bind(&plan.id)
        .bind(&member.user_id)
        .bind(&member.responsibility)
        .execute(&mut *c)
        .await?;
    }
    if input.active {
        for coverage in &input.coverage {
            sqlx::query(
                "INSERT INTO service_team_coverage(company_id,team_id,category,scope_kind,scope_node_id) \
                 VALUES($1,$2,$3,$4,$5)",
            )
            .bind(actor.company_id.as_str())
            .bind(&plan.id)
            .bind(&coverage.category)
            .bind(&coverage.scope_kind)
            .bind(&coverage.scope_node_id)
            .execute(&mut *c)
            .await?;
        }
    }
    Ok(())
}

/// Caller-supplied People member directory. Organization owns those rows.
pub async fn service_team_options(
    c: &mut PgConnection,
    actor: &ServiceTeamActor,
    members: Value,
) -> Result<Value, Error> {
    serializable(c).await?;
    let nodes: String = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object(\
           'id',id,'kind',kind,'display_name',display_name,'branch_id',branch_id) \
           ORDER BY display_name),'[]'::jsonb)::text \
         FROM organization_nodes WHERE company_id=$1 AND status='active' \
           AND kind IN ('company','branch','department') \
           AND ($2::text IS NULL OR id=$2 OR branch_id=$2) \
           AND ($3::text IS NULL OR id=$3 OR kind='branch')",
    )
    .bind(actor.company_id.as_str())
    .bind(actor.branch_id.as_deref())
    .bind(actor.department_id.as_deref())
    .fetch_one(&mut *c)
    .await?;
    let teams = sqlx::query(
        "SELECT id,company_id,name,active,version,configuration_json \
         FROM service_teams WHERE company_id=$1 ORDER BY name,id",
    )
    .bind(actor.company_id.as_str())
    .fetch_all(&mut *c)
    .await?;
    let mut projected = vec![];
    for row in teams {
        let id: String = row.try_get("id")?;
        let history: String = sqlx::query_scalar(
            "SELECT coalesce(jsonb_agg(jsonb_build_object(\
               'version',version,'configuration_json',configuration_json,\
               'actor_id',actor_id,'created_at',created_at) ORDER BY version DESC),'[]'::jsonb)::text \
             FROM (SELECT * FROM service_team_revisions WHERE company_id=$1 AND team_id=$2 \
               ORDER BY version DESC LIMIT 100) revisions",
        )
        .bind(actor.company_id.as_str())
        .bind(&id)
        .fetch_one(&mut *c)
        .await?;
        let handlers: String = sqlx::query_scalar(
            "SELECT coalesce(jsonb_agg(jsonb_build_object(\
               'user_id',user_id,'responsibility',responsibility) ORDER BY user_id),'[]'::jsonb)::text \
             FROM service_team_members WHERE team_id=$1 AND company_id=$2",
        )
        .bind(&id)
        .bind(actor.company_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        let coverage: String = sqlx::query_scalar(
            "SELECT coalesce(jsonb_agg(jsonb_build_object(\
               'category',category,'scope_kind',scope_kind,'scope_node_id',scope_node_id) \
               ORDER BY category,scope_node_id),'[]'::jsonb)::text \
             FROM service_team_coverage WHERE team_id=$1 AND company_id=$2",
        )
        .bind(&id)
        .bind(actor.company_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        projected.push(json!({
            "id": id,
            "company_id": row.try_get::<String, _>("company_id")?,
            "name": row.try_get::<String, _>("name")?,
            "active": row.try_get::<i64, _>("active")? == 1,
            "version": row.try_get::<i64, _>("version")?,
            "configuration_json": row.try_get::<String, _>("configuration_json")?,
            "history": serde_json::from_str::<Value>(&history).unwrap_or(Value::Array(vec![])),
            "members": serde_json::from_str::<Value>(&handlers).unwrap_or(Value::Array(vec![])),
            "coverage": serde_json::from_str::<Value>(&coverage).unwrap_or(Value::Array(vec![])),
        }));
    }
    Ok(json!({
        "owner": true,
        "nodes": serde_json::from_str::<Value>(&nodes).unwrap_or(Value::Array(vec![])),
        "members": members,
        "teams": projected,
    }))
}
