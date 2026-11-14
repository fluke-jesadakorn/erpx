//! PostgreSQL adapter for the native position-move commands.
//!
//! The caller owns the Holding transaction, current session proof and Company
//! lease. This adapter re-checks the retained IAM source, writes the reviewed
//! preview/version through the shared command receipt, audit and outbox, and
//! never applies the scheduled move. Applying it stays a separate worker task.
use crate::{
    application::{
        organization_nodes::OrganizationCommandActor,
        position_moves::{
            drain_once, ClaimedPositionMove, DrainOutcome, PositionMoveDrainRepository,
            PositionMoveFuture, PositionMoveRepository,
        },
    },
    domain::organization_nodes::{OrganizationActor, OrganizationScopeKind},
    domain::position_moves::{
        ExistingLeaveFact, GpoRule, Leave, LeaveAssignmentPlan, LeaveDefault, LeavePolicyFact,
        MoveLevel, MoveMember, MoveNode, MovePerson, MovePosition, PositionMoveFacts,
        PositionMoveInput, PositionMovePersonWire, PositionMovePreview, PositionMoveRecord,
        PreparedPositionMove,
    },
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PersistenceError, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use erpx_server_workflow::public::commands::{
    command_fingerprint, CommandActor, CommandEventScope, CommandField, CommandJson, WorkAuthority,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, Connection, PgConnection, Row};
use std::sync::{Arc, Mutex};

type Error = PgTransactionError<BusinessError>;

const ROW_LIMIT: usize = 4_000;

fn command_actor(actor: &OrganizationCommandActor) -> Result<CommandActor, BusinessError> {
    let command = &actor.command;
    Ok(CommandActor {
        user_id: command.user_id.clone(),
        assignment_id: command.assignment_id.clone(),
        role: command.role.clone(),
        company_id: command.company_id.clone(),
        branch_id: command
            .branch_id
            .as_ref()
            .map(|value| CommandField::Value(value.clone()))
            .unwrap_or(CommandField::Null),
        department_id: command
            .department_id
            .as_ref()
            .map(|value| CommandField::Value(value.clone()))
            .unwrap_or(CommandField::Null),
        authority: command
            .authority
            .as_ref()
            .map(|authority| {
                CommandField::Value(WorkAuthority {
                    session_id: authority.session_id.clone(),
                    revision: authority.revision,
                    assignment_version: authority.assignment_version,
                })
            })
            .unwrap_or(CommandField::Null),
        iam: CommandField::Value(CommandJson::parse(&command.iam_json)?),
        assignment_scope_kind: match command.assignment_scope_kind.as_str() {
            "branch" => "branch".into(),
            "department" => "department".into(),
            _ => "holding".into(),
        },
        assignment_scope_node_id: command.assignment_scope_node_id.clone(),
    })
}

fn fingerprint(
    actor: &OrganizationCommandActor,
    operation: &str,
    record_id: &str,
    input: &Value,
) -> Result<String, BusinessError> {
    command_fingerprint(
        &command_actor(actor)?,
        operation,
        record_id,
        &CommandJson::from_serializable(input)?,
    )
}

fn aggregate(operation: &str) -> String {
    operation.split('.').take(2).collect::<Vec<_>>().join("_")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POSITION_MOVE_UNAVAILABLE",
        "Position-move facts are unavailable.",
    )
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORGANIZATION_AUTHORITY_CHANGED",
        "Your current organization authority is unavailable.",
    )
}

fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "23505")
    {
        return conflict(
            "CONFLICT",
            "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into();
    }
    error.into()
}

fn person(row: &PgRow) -> Result<MovePerson, Error> {
    Ok(MovePerson {
        person_id: row.try_get("person_id")?,
        engagement_id: row.try_get("engagement_id")?,
        legal_name: row.try_get("legal_name")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        position_id: row.try_get("position_id")?,
        manager_person_id: row.try_get("manager_person_id")?,
        level_id: row.try_get("level_id")?,
        level_number: row.try_get("level_number")?,
        status: row.try_get("status")?,
        person_status: row.try_get("person_status")?,
        start_date: row.try_get("start_date")?,
        end_date: row.try_get("end_date")?,
    })
}

fn position(row: &PgRow) -> Result<MovePosition, Error> {
    Ok(MovePosition {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        title: row.try_get("title")?,
        level_id: row.try_get("level_id")?,
        level_number: row.try_get("level_number")?,
        reports_to_position_id: row.try_get("reports_to_position_id")?,
        is_ceo: row.try_get("is_ceo")?,
        active: row.try_get("active")?,
        version: row.try_get("version")?,
    })
}

fn node(row: &PgRow) -> Result<MoveNode, Error> {
    Ok(MoveNode {
        id: row.try_get("id")?,
        kind: row.try_get("kind")?,
        branch_id: row.try_get("branch_id")?,
        status: row.try_get("status")?,
        hierarchy_revision: row.try_get("hierarchy_revision")?,
    })
}

fn level(row: &PgRow) -> Result<MoveLevel, Error> {
    Ok(MoveLevel {
        id: row.try_get("id")?,
        level_number: row.try_get("level_number")?,
        active: row.try_get("active")?,
        version: row.try_get("version")?,
    })
}

fn member(row: &PgRow) -> Result<MoveMember, Error> {
    Ok(MoveMember {
        id: row.try_get("id")?,
        person_id: row.try_get("person_id")?,
        version: row.try_get("version")?,
        status: row.try_get("status")?,
        start_date: row.try_get("start_date")?,
        end_date: row.try_get("end_date")?,
        currency: row.try_get("currency")?,
    })
}

fn leave(row: &PgRow) -> Result<Leave, Error> {
    Ok(Leave {
        engagement_id: row.try_get("engagement_id")?,
        id: row.try_get("id")?,
        effective_from: row.try_get("effective_from")?,
        effective_to: row.try_get("effective_to")?,
        version: row.try_get("version")?,
        policy_key: row.try_get("policy_key")?,
    })
}

fn default(row: &PgRow) -> Result<LeaveDefault, Error> {
    Ok(LeaveDefault {
        level_id: row.try_get("level_id")?,
        policy_version_id: row.try_get("policy_version_id")?,
        effective_from: row.try_get("effective_from")?,
        version: row.try_get("version")?,
    })
}

fn rule(row: &PgRow) -> Result<GpoRule, Error> {
    Ok(GpoRule {
        id: row.try_get("id")?,
        target_kind: row.try_get("target_kind")?,
        target_id: row.try_get("target_id")?,
        version: row.try_get("version")?,
        revision: row.try_get("revision")?,
        connection_id: row.try_get("connection_id")?,
        gpo_id: row.try_get("gpo_id")?,
        user_filter_group_id: row.try_get("user_filter_group_id")?,
        computer_target_ou_id: row.try_get("computer_target_ou_id")?,
        active: row.try_get("active")?,
    })
}

fn record(row: &PgRow) -> Result<PositionMoveRecord, Error> {
    let definition: String = row.try_get("definition")?;
    let definition = PositionMoveInput::parse(
        &serde_json::from_str::<Value>(&definition).map_err(|_| unavailable())?,
    )?;
    let preview: String = row.try_get("preview")?;
    let preview =
        serde_json::from_str::<PositionMovePreview>(&preview).map_err(|_| unavailable())?;
    Ok(PositionMoveRecord {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        position_id: row.try_get("position_id")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        effective_at: row.try_get("effective_at")?,
        definition,
        preview,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        applied_at: row.try_get("applied_at")?,
        last_error: row.try_get("last_error")?,
    })
}

/// Actor JSON compatible with the retained drain worker's `WorkContext`
/// decode. It carries no authority token and grants nothing by itself.
fn saved_actor(actor: &OrganizationCommandActor) -> Value {
    let command = &actor.command;
    let kind = match command.assignment_scope_kind.as_str() {
        "branch" => "branch",
        "department" => "department",
        _ => "holding",
    };
    json!({
        "userId": command.user_id.as_str(),
        "name": "",
        "email": "",
        "assignmentId": command.assignment_id.as_str(),
        "role": command.role,
        "companyId": command.company_id.as_str(),
        "branchId": command.branch_id,
        "departmentId": command.department_id,
        "holdingId": actor.holding_id.as_str(),
        "holdingSlug": "",
        "holdingName": "",
        "scope": {
            "kind": kind,
            "holdingId": actor.holding_id.as_str(),
            "holdingSlug": "",
            "holdingName": "",
            "companyId": command.company_id.as_str(),
            "branchId": command.branch_id,
            "branchSlug": "",
            "branchName": "",
            "departmentId": command.department_id,
            "departmentSlug": "",
            "departmentName": ""
        },
        "assignmentScope": kind,
        "inherited": false,
        "modules": actor.modules,
        "permissions": actor.permissions,
        "iam": {"permissions": actor.permissions},
        "assignments": [],
        "workspaces": [],
        "authority": null
    })
}

pub struct PgPositionMoves<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPositionMoves<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn transaction(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("POSITION_MOVE_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn authorize(
        &mut self,
        actor: &crate::domain::organization_nodes::OrganizationActor,
        permission: &str,
    ) -> Result<(), Error> {
        self.transaction()?;
        let enabled: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments g \
             JOIN organization_nodes c ON c.id=g.company_id AND c.kind='company' AND c.status='active' \
             WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
             AND g.revoked_at IS NULL \
             AND (g.scope_kind='holding' \
                  OR (g.scope_kind='branch' AND g.scope_node_id=$5) \
                  OR (g.scope_kind='department' AND g.scope_node_id=$6)) \
             AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_role_allows(g.role,g.company_id,$7))",
        )
        .bind(actor.assignment_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(&actor.role)
        .bind(actor.company_id.as_str())
        .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
        .bind(actor.department_id.as_ref().map(|value| value.as_str()))
        .bind(permission)
        .fetch_one(&mut *self.connection)
        .await?;
        if !enabled {
            return Err(authority_changed().into());
        }
        Ok(())
    }

    async fn receipt(&mut self, key: &str, hash: &str) -> Result<Option<Value>, Error> {
        let row = sqlx::query(
            "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let saved: String = row.try_get("request_hash")?;
        if saved != hash {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let raw: Option<String> = row.try_get("result_json")?;
        raw.and_then(|value| serde_json::from_str(&value).ok())
            .map(Some)
            .ok_or_else(|| {
                conflict(
                    "IDEMPOTENCY_CONFLICT",
                    "The retained result is unavailable.",
                )
                .into()
            })
    }

    async fn save_receipt(
        &mut self,
        actor: &OrganizationCommandActor,
        key: &str,
        operation: &str,
        record_id: &str,
        hash: &str,
        result: &Value,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        let command_actor = command_actor(actor)?;
        let authority = command_actor.receipt_authority()?;
        let inserted = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (command_id) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .bind(actor.command.user_id.as_str())
        .bind(hash)
        .bind(record_id)
        .bind(result.to_string())
        .bind(now)
        .bind(authority.as_str())
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?
        .rows_affected()
            == 1;
        if !inserted {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let aggregate = aggregate(operation);
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,(SELECT authority_json FROM command_receipts WHERE command_id=$2))",
        )
        .bind(format!("audit-{key}"))
        .bind(key)
        .bind(actor.command.user_id.as_str())
        .bind(actor.command.assignment_id.as_str())
        .bind(operation)
        .bind(&aggregate)
        .bind(record_id)
        .bind(version)
        .bind(now)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        let scope = CommandEventScope::from(&command_actor);
        let state = result
            .get("state")
            .or_else(|| result.get("status"))
            .and_then(Value::as_str);
        let payload = command_actor.outbox_payload(record_id, version, state, &scope)?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(format!("outbox-{key}"))
        .bind(key)
        .bind(operation)
        .bind(&aggregate)
        .bind(record_id)
        .bind(version)
        .bind(payload.as_str())
        .bind(now)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        Ok(())
    }

    pub async fn preview(
        &mut self,
        actor: &crate::domain::organization_nodes::OrganizationActor,
        input: &PositionMoveInput,
        clock: &(dyn erpx_server_shared::public::Clock + Sync),
    ) -> Result<PositionMovePreview, Error> {
        crate::application::position_moves::preview_position_move(self, actor, input, clock).await
    }

    pub async fn submit(
        &mut self,
        actor: &OrganizationCommandActor,
        input: &PositionMoveInput,
        preview_token: &str,
        expires_at: i64,
        key: &str,
        clock: &(dyn erpx_server_shared::public::Clock + Sync),
    ) -> Result<Value, Error> {
        crate::application::position_moves::submit_position_move(
            self,
            actor,
            input,
            preview_token,
            expires_at,
            key,
            clock,
        )
        .await
    }

    pub async fn list(
        &mut self,
        actor: &crate::domain::organization_nodes::OrganizationActor,
        id: Option<&str>,
    ) -> Result<Vec<PositionMoveRecord>, Error> {
        crate::application::position_moves::position_moves(self, actor, id).await
    }

    pub async fn cancel(
        &mut self,
        actor: &OrganizationCommandActor,
        id: &str,
        expected_version: i64,
        key: &str,
        clock: &(dyn erpx_server_shared::public::Clock + Sync),
    ) -> Result<Value, Error> {
        crate::application::position_moves::cancel_position_move(
            self,
            actor,
            id,
            expected_version,
            key,
            clock,
        )
        .await
    }
}

impl PositionMoveRepository for PgPositionMoves<'_> {
    type Error = Error;

    fn authorize<'a>(
        &'a mut self,
        actor: &'a crate::domain::organization_nodes::OrganizationActor,
        permission: &'a str,
    ) -> PositionMoveFuture<'a, (), Self::Error> {
        Box::pin(async move { PgPositionMoves::authorize(self, actor, permission).await })
    }

    fn local_date<'a>(
        &'a mut self,
        timestamp: i64,
        timezone: &'a str,
    ) -> PositionMoveFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            sqlx::query_scalar(
                "SELECT to_char(to_timestamp($1::double precision / 1000.0) AT TIME ZONE $2, 'YYYY-MM-DD')",
            )
            .bind(timestamp as f64)
            .bind(timezone)
            .fetch_one(&mut *self.connection)
            .await
            .map_err(Into::into)
        })
    }

    fn facts<'a>(
        &'a mut self,
        actor: &'a crate::domain::organization_nodes::OrganizationActor,
        position_id: &'a str,
        date: &'a str,
        exclude_move_id: Option<&'a str>,
    ) -> PositionMoveFuture<'a, PositionMoveFacts, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let rows = sqlx::query(
                "SELECT p.id person_id,e.id engagement_id,p.legal_name,e.company_id,e.branch_id, \
                 e.department_id,e.position_id,e.manager_person_id,e.level_id, \
                 l.level_number::bigint level_number,p.status person_status,e.status,e.start_date, \
                 e.end_date \
                 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1 \
                 WHERE p.company_id=$1 AND p.merged_into_id IS NULL AND p.status<>'archived' \
                 AND e.worker_type='employee' AND e.status<>'ended' \
                 ORDER BY p.legal_name,e.id LIMIT $2",
            )
            .bind(actor.company_id.as_str())
            .bind((ROW_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > ROW_LIMIT {
                return Err(unavailable().into());
            }
            let people = rows.iter().map(person).collect::<Result<Vec<_>, _>>()?;
            let rows = sqlx::query(
                "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id, \
                 l.level_number::bigint level_number,p.reports_to_position_id,p.is_ceo::bigint is_ceo, \
                 p.active::bigint active,p.version::bigint version \
                 FROM organization_positions p \
                 JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id \
                 WHERE p.company_id=$1 ORDER BY l.level_number,p.title,p.id LIMIT $2",
            )
            .bind(actor.company_id.as_str())
            .bind((ROW_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > ROW_LIMIT {
                return Err(unavailable().into());
            }
            let positions = rows.iter().map(position).collect::<Result<Vec<_>, _>>()?;
            let rows = sqlx::query(
                "SELECT id,kind,branch_id,status,hierarchy_revision::bigint hierarchy_revision \
                 FROM organization_nodes WHERE company_id=$1 ORDER BY id LIMIT $2",
            )
            .bind(actor.company_id.as_str())
            .bind((ROW_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > ROW_LIMIT {
                return Err(unavailable().into());
            }
            let nodes = rows.iter().map(node).collect::<Result<Vec<_>, _>>()?;
            let rows = sqlx::query(
                "SELECT id,level_number::bigint level_number,active::bigint active,version::bigint version \
                 FROM staff_levels WHERE company_id=$1 ORDER BY id LIMIT 101",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > 100 {
                return Err(unavailable().into());
            }
            let levels = rows.iter().map(level).collect::<Result<Vec<_>, _>>()?;
            let rows = sqlx::query(
                "SELECT id,person_id,version::bigint version,status,start_date,end_date,currency \
                 FROM hr_engagements WHERE company_id=$1 AND position_id=$2 AND status<>'ended' \
                 ORDER BY id LIMIT $3",
            )
            .bind(actor.company_id.as_str())
            .bind(position_id)
            .bind((ROW_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > ROW_LIMIT {
                return Err(unavailable().into());
            }
            let members = rows.iter().map(member).collect::<Result<Vec<_>, _>>()?;
            let open_placements = sqlx::query_scalar::<_, String>(
                "SELECT c.id FROM people_lifecycle_cases c WHERE c.company_id=$1 \
                 AND c.status IN ('waiting_it','pending_review','scheduled') AND \
                 (c.definition->>'positionId'=$2 OR c.engagement_id IN (SELECT id FROM hr_engagements WHERE company_id=$1 AND position_id=$2 AND status<>'ended') \
                  OR c.person_id IN (SELECT person_id FROM hr_engagements WHERE company_id=$1 AND position_id=$2 AND status<>'ended')) \
                 ORDER BY c.id LIMIT 101",
            )
            .bind(actor.company_id.as_str())
            .bind(position_id)
            .fetch_all(&mut *self.connection)
            .await?;
            if open_placements.len() > 100 {
                return Err(unavailable().into());
            }
            let pending: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM organization_position_moves m WHERE m.company_id=$1 AND m.status='scheduled' \
                 AND m.id IS DISTINCT FROM $3 \
                 AND (m.position_id=$2 OR EXISTS(SELECT 1 FROM organization_position_move_members h \
                  JOIN hr_engagements e ON e.person_id=h.person_id WHERE h.move_id=m.id AND e.company_id=$1 \
                  AND e.position_id=$2 AND e.status<>'ended')))",
            )
            .bind(actor.company_id.as_str())
            .bind(position_id)
            .bind(exclude_move_id)
            .fetch_one(&mut *self.connection)
            .await?;
            let rows = sqlx::query(
                "SELECT DISTINCT ON(d.level_id) d.level_id,d.policy_version_id,p.effective_from, \
                 d.version::bigint version \
                 FROM level_leave_defaults d \
                 JOIN leave_policy_versions p ON p.id=d.policy_version_id AND p.company_id=d.company_id \
                 WHERE d.company_id=$1 ORDER BY d.level_id,d.version DESC LIMIT 101",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            let defaults = rows.iter().map(default).collect::<Result<Vec<_>, _>>()?;
            let rows = sqlx::query(
                "SELECT a.engagement_id,a.id,a.effective_from,a.effective_to,a.version::bigint version,p.policy_key \
                 FROM leave_policy_assignments a \
                 JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id \
                 WHERE a.company_id=$1 AND a.engagement_id IN (SELECT id FROM hr_engagements WHERE company_id=$1 AND position_id=$2 AND status<>'ended') \
                 AND (a.effective_to IS NULL OR a.effective_to>=$3) \
                 ORDER BY a.engagement_id,a.effective_from DESC,a.id LIMIT 501",
            )
            .bind(actor.company_id.as_str())
            .bind(position_id)
            .bind(date)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > 500 {
                return Err(unavailable().into());
            }
            let leave = rows.iter().map(leave).collect::<Result<Vec<_>, _>>()?;
            let rows = sqlx::query(
                "SELECT id,target_kind,target_id,version::bigint version,revision::bigint revision, \
                 connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,active \
                 FROM organization_gpo_rules WHERE company_id=$1 ORDER BY id LIMIT $2",
            )
            .bind(actor.company_id.as_str())
            .bind((ROW_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > ROW_LIMIT {
                return Err(unavailable().into());
            }
            let rules = rows.iter().map(rule).collect::<Result<Vec<_>, _>>()?;
            Ok(PositionMoveFacts {
                people,
                positions,
                nodes,
                levels,
                members,
                open_placements,
                pending,
                defaults,
                leave,
                rules,
                date: date.to_owned(),
            })
        })
    }

    fn replay<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        key: &'a str,
        operation: &'a str,
        record_id: &'a str,
        input: &'a Value,
    ) -> PositionMoveFuture<'a, Option<Value>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let hash = fingerprint(actor, operation, record_id, input)?;
            PgPositionMoves::receipt(self, key, &hash).await
        })
    }

    fn schedule<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        input: &'a PositionMoveInput,
        key: &'a str,
        payload: &'a Value,
        prepared: &'a PreparedPositionMove,
        now: i64,
    ) -> PositionMoveFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let id = format!("position-move-{key}");
            let result = json!({
                "id": id,
                "version": 1,
                "status": "scheduled",
                "effectiveAt": input.effective_at,
            });
            sqlx::query(
                "INSERT INTO organization_position_moves(id,company_id,position_id,status,effective_at,definition,preview,snapshot_hash,authorizing_assignment_id,actor_json,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,'scheduled',$4,$5,$6,$7,$8,$9,$10,$11,$11)",
            )
            .bind(&id)
            .bind(actor.command.company_id.as_str())
            .bind(&input.position_id)
            .bind(input.effective_at)
            .bind(input.to_value().to_string())
            .bind(prepared.preview_json()?)
            .bind(&prepared.snapshot_hash)
            .bind(actor.command.assignment_id.as_str())
            .bind(saved_actor(actor).to_string())
            .bind(actor.command.user_id.as_str())
            .bind(now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            for member in &prepared.members {
                sqlx::query(
                    "INSERT INTO organization_position_move_members(move_id,engagement_id,person_id) VALUES($1,$2,$3)",
                )
                .bind(&id)
                .bind(&member.id)
                .bind(&member.person_id)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            }
            let hash = fingerprint(actor, "organization.position-move.submit", &id, payload)?;
            PgPositionMoves::save_receipt(
                self,
                actor,
                key,
                "organization.position-move.submit",
                &id,
                &hash,
                &result,
                1,
                now,
            )
            .await?;
            Ok(result)
        })
    }

    fn rows<'a>(
        &'a mut self,
        actor: &'a crate::domain::organization_nodes::OrganizationActor,
        id: Option<&'a str>,
    ) -> PositionMoveFuture<'a, Vec<PositionMoveRecord>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let rows = sqlx::query(
                "SELECT m.id,m.company_id,m.position_id,m.status,m.version::bigint version, \
                 m.effective_at,m.definition,m.preview,m.created_by,m.created_at,m.updated_at, \
                 m.applied_at,m.last_error \
                 FROM organization_position_moves m \
                 JOIN organization_positions p ON p.company_id=m.company_id AND p.id=m.position_id \
                 WHERE m.company_id=$1 AND ($2::text IS NULL OR m.id=$2) \
                 AND ($3::text IS NULL OR p.branch_id=$3) \
                 AND ($4::text IS NULL OR p.department_id=$4) \
                 ORDER BY m.created_at DESC,m.id LIMIT 100",
            )
            .bind(actor.company_id.as_str())
            .bind(id)
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter().map(record).collect()
        })
    }

    fn cancel<'a>(
        &'a mut self,
        actor: &'a OrganizationCommandActor,
        id: &'a str,
        expected_version: i64,
        key: &'a str,
        now: i64,
    ) -> PositionMoveFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let version = expected_version + 1;
            let result = json!({"id": id, "version": version, "status": "cancelled"});
            let updated = sqlx::query(
                "UPDATE organization_position_moves SET status='cancelled',version=$1,updated_at=$2 \
                 WHERE id=$3 AND company_id=$4 AND status='scheduled' AND version=$5 \
                 AND effective_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .bind(version)
            .bind(now)
            .bind(id)
            .bind(actor.command.company_id.as_str())
            .bind(expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if updated != 1 {
                return Err(conflict(
                    "POSITION_MOVE_LOCKED",
                    "Only a scheduled move before its effective time can be cancelled.",
                )
                .into());
            }
            let hash = fingerprint(
                actor,
                "organization.position-move.cancel",
                id,
                &json!({"expectedVersion": expected_version}),
            )?;
            PgPositionMoves::save_receipt(
                self,
                actor,
                key,
                "organization.position-move.cancel",
                id,
                &hash,
                &result,
                version,
                now,
            )
            .await?;
            Ok(result)
        })
    }
}

impl PreparedPositionMove {
    fn preview_json(&self) -> Result<String, BusinessError> {
        serde_json::to_string(&self.preview)
            .map_err(|_| BusinessError::invalid("POSITION_MOVE_INVALID", "The preview is invalid."))
    }
}

fn scope_kind(value: &str) -> OrganizationScopeKind {
    match value {
        "branch" => OrganizationScopeKind::Branch,
        "department" => OrganizationScopeKind::Department,
        _ => OrganizationScopeKind::Holding,
    }
}

fn actor_json(value: &Value) -> Result<OrganizationActor, Error> {
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(unavailable)
    };
    let optional = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    let permissions = value
        .get("permissions")
        .and_then(Value::as_array)
        .ok_or_else(unavailable)?
        .iter()
        .map(|permission| {
            permission
                .as_str()
                .map(str::to_owned)
                .ok_or_else(unavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let scope = value
        .get("scope")
        .and_then(|scope| scope.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("holding");
    let assignment_scope = value
        .get("assignmentScope")
        .and_then(Value::as_str)
        .unwrap_or(scope);
    Ok(OrganizationActor {
        holding_id: HoldingId::new(text("holdingId")?).map_err(|_| unavailable())?,
        company_id: CompanyId::new(text("companyId")?).map_err(|_| unavailable())?,
        user_id: UserId::new(text("userId")?).map_err(|_| unavailable())?,
        assignment_id: AssignmentId::new(text("assignmentId")?).map_err(|_| unavailable())?,
        role: text("role")?.to_owned(),
        branch_id: optional("branchId")
            .map(RecordId::new)
            .transpose()
            .map_err(|_| unavailable())?,
        department_id: optional("departmentId")
            .map(RecordId::new)
            .transpose()
            .map_err(|_| unavailable())?,
        scope_kind: scope_kind(scope),
        assignment_scope_kind: scope_kind(assignment_scope),
        permissions,
        authority: None,
    })
}

fn claimed_move(row: &PgRow) -> Result<ClaimedPositionMove, Error> {
    let actor: String = row.try_get("actor_json")?;
    Ok(ClaimedPositionMove {
        record: record(row)?,
        actor: actor_json(&serde_json::from_str(&actor).map_err(|_| unavailable())?)?,
        authorizing_assignment_id: row.try_get("authorizing_assignment_id")?,
        created_by: row.try_get("created_by")?,
        snapshot_hash: row.try_get("snapshot_hash")?,
    })
}

impl PositionMoveDrainRepository for PgPositionMoves<'_> {
    fn claim<'a>(
        &'a mut self,
        now: i64,
    ) -> PositionMoveFuture<'a, Option<ClaimedPositionMove>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let acquired: bool =
                sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(821039320) AS acquired")
                    .fetch_one(&mut *self.connection)
                    .await?;
            if !acquired {
                return Ok(None);
            }
            let row = sqlx::query(
                "SELECT m.id,m.company_id,m.position_id,m.status,m.version::bigint version, \
                 m.effective_at,m.definition,m.preview,m.snapshot_hash,m.authorizing_assignment_id, \
                 m.actor_json,m.created_by,m.created_at,m.updated_at,m.applied_at,m.last_error \
                 FROM organization_position_moves m WHERE m.status='scheduled' AND m.effective_at<=$1 \
                 ORDER BY m.effective_at,m.id FOR UPDATE SKIP LOCKED LIMIT 1",
            )
            .bind(now)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(claimed_move).transpose()
        })
    }

    fn assignment_valid<'a>(
        &'a mut self,
        claimed: &'a ClaimedPositionMove,
        affected_count: usize,
        now: i64,
    ) -> PositionMoveFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let valid: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM assignments a \
                 WHERE a.id=$1 AND a.company_id=$2 AND a.user_id=$3 AND a.role=$4 \
                 AND iam_assignment_active(a.id,$5) AND iam_principal_active(a.user_id,a.company_id,$5) \
                 AND iam_role_allows(a.role,a.company_id,'organization.manage') \
                 AND ($6=0 OR iam_role_allows(a.role,a.company_id,'people.manage')) \
                 AND (a.scope_kind='holding' \
                      OR (a.scope_kind='branch' AND a.scope_node_id=$7) \
                      OR (a.scope_kind='department' AND a.scope_node_id=$8)))",
            )
            .bind(&claimed.authorizing_assignment_id)
            .bind(&claimed.record.company_id)
            .bind(&claimed.created_by)
            .bind(&claimed.actor.role)
            .bind(now)
            .bind(affected_count as i64)
            .bind(claimed.actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(
                claimed
                    .actor
                    .department_id
                    .as_ref()
                    .map(|value| value.as_str()),
            )
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(valid)
        })
    }

    fn set_payroll_date<'a>(
        &'a mut self,
        date: &'a str,
    ) -> PositionMoveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            sqlx::query("SELECT set_config('erpx.payroll_assignment_date',$1,true)")
                .bind(date)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn apply_position<'a>(
        &'a mut self,
        company_id: &'a str,
        candidate: &'a MovePosition,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            sqlx::query(
                "UPDATE organization_positions SET branch_id=$1,department_id=$2,level_id=$3, \
                 reports_to_position_id=$4,version=version+1,updated_at=$5 \
                 WHERE company_id=$6 AND id=$7",
            )
            .bind(&candidate.branch_id)
            .bind(&candidate.department_id)
            .bind(&candidate.level_id)
            .bind(&candidate.reports_to_position_id)
            .bind(now)
            .bind(company_id)
            .bind(&candidate.id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn apply_holder<'a>(
        &'a mut self,
        claimed: &'a ClaimedPositionMove,
        input: &'a PositionMoveInput,
        holder: &'a PositionMovePersonWire,
        original: &'a MoveMember,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            sqlx::query(
                "UPDATE hr_engagements SET branch_id=$1,department_id=$2,level_id=$3, \
                 manager_person_id=$4,version=version+1,updated_at=$5 \
                 WHERE company_id=$6 AND id=$7",
            )
            .bind(&holder.after.branch_id)
            .bind(&holder.after.department_id)
            .bind(&holder.after.level_id)
            .bind(&holder.after.manager_person_id)
            .bind(now)
            .bind(&claimed.record.company_id)
            .bind(&holder.engagement_id)
            .execute(&mut *self.connection)
            .await?;
            // Employment status, start/end dates, salary and pay group are
            // deliberately preserved.
            sqlx::query(
                "UPDATE employees SET branch_id=$1,department_id=$2 WHERE company_id=$3 AND person_id=$4",
            )
            .bind(&holder.after.branch_id)
            .bind(&holder.after.department_id)
            .bind(&claimed.record.company_id)
            .bind(&holder.person_id)
            .execute(&mut *self.connection)
            .await?;
            let before = json!({
                "branchId": holder.before.branch_id,
                "departmentId": holder.before.department_id,
                "levelId": holder.before.level_id,
                "managerPersonId": holder.before.manager_person_id,
                "status": original.status,
                "startDate": original.start_date,
                "endDate": original.end_date,
            });
            let after = json!({
                "branchId": holder.after.branch_id,
                "departmentId": holder.after.department_id,
                "levelId": holder.after.level_id,
                "managerPersonId": holder.after.manager_person_id,
                "status": original.status,
                "startDate": original.start_date,
                "endDate": original.end_date,
                "effectiveAt": claimed.record.effective_at,
            });
            sqlx::query(
                "INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(format!(
                "position-move-history-{}-{}",
                claimed.record.id, holder.engagement_id
            ))
            .bind(&claimed.record.company_id)
            .bind(&holder.engagement_id)
            .bind(before.to_string())
            .bind(after.to_string())
            .bind(&claimed.created_by)
            .bind(&input.reason)
            .bind(&claimed.record.id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            if holder.before.level_id != holder.after.level_id {
                sqlx::query(
                    "INSERT INTO staff_level_history(id,engagement_id,previous_level_id,level_id,actor_id,reason,command_id,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(format!(
                    "position-move-level-{}-{}",
                    claimed.record.id, holder.engagement_id
                ))
                .bind(&holder.engagement_id)
                .bind(&holder.before.level_id)
                .bind(&holder.after.level_id)
                .bind(&claimed.created_by)
                .bind(&input.reason)
                .bind(&claimed.record.id)
                .bind(now)
                .execute(&mut *self.connection)
                .await?;
            }
            Ok(())
        })
    }

    fn leave_policy<'a>(
        &'a mut self,
        company_id: &'a str,
        level_id: &'a str,
    ) -> PositionMoveFuture<'a, Option<LeavePolicyFact>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let row = sqlx::query(
                "SELECT d.policy_version_id,p.effective_from FROM level_leave_defaults d \
                 JOIN leave_policy_versions p ON p.id=d.policy_version_id AND p.company_id=d.company_id \
                 WHERE d.company_id=$1 AND d.level_id=$2 ORDER BY d.version DESC LIMIT 1",
            )
            .bind(company_id)
            .bind(level_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(LeavePolicyFact {
                    policy_version_id: row.try_get("policy_version_id")?,
                    effective_from: row.try_get("effective_from")?,
                })
            })
            .transpose()
        })
    }

    fn existing_leave<'a>(
        &'a mut self,
        company_id: &'a str,
        engagement_id: &'a str,
        date: &'a str,
    ) -> PositionMoveFuture<'a, Option<ExistingLeaveFact>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let row = sqlx::query(
                "SELECT a.id,a.effective_from,a.effective_to,p.policy_key \
                 FROM leave_policy_assignments a \
                 JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id \
                 WHERE a.company_id=$1 AND a.engagement_id=$2 \
                 AND (a.effective_to IS NULL OR a.effective_to>=$3) \
                 ORDER BY a.effective_from DESC LIMIT 1",
            )
            .bind(company_id)
            .bind(engagement_id)
            .bind(date)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(ExistingLeaveFact {
                    id: row.try_get("id")?,
                    effective_from: row.try_get("effective_from")?,
                    effective_to: row.try_get("effective_to")?,
                    policy_key: row.try_get("policy_key")?,
                })
            })
            .transpose()
        })
    }

    fn stage_leave<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        command_id: &'a str,
        engagement_id: &'a str,
        plan: &'a LeaveAssignmentPlan,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            if let Some(end_assignment_id) = &plan.end_assignment_id {
                sqlx::query(
                    "UPDATE leave_policy_assignments SET effective_to=$1,version=version+1 WHERE id=$2",
                )
                .bind(&plan.end)
                .bind(end_assignment_id)
                .execute(&mut *self.connection)
                .await?;
            }
            sqlx::query(
                "INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(format!("leave-default-{command_id}"))
            .bind(actor.company_id.as_str())
            .bind(engagement_id)
            .bind(&plan.policy_version_id)
            .bind(&plan.effective)
            .bind(actor.user_id.as_str())
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn complete<'a>(
        &'a mut self,
        claimed: &'a ClaimedPositionMove,
        input: &'a PositionMoveInput,
        affected_count: usize,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let record = &claimed.record;
            let version = record.version + 1;
            sqlx::query(
                "UPDATE organization_position_moves SET status='applied',version=version+1, \
                 applied_at=$1,updated_at=$1,last_error=NULL WHERE id=$2",
            )
            .bind(now)
            .bind(&record.id)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) \
                 VALUES($1,$2,$3,$4,'organization.position-move.applied','organization_position_move',$5,$6,$7,$8)",
            )
            .bind(format!("audit-{}", record.id))
            .bind(&record.id)
            .bind(&claimed.created_by)
            .bind(&claimed.authorizing_assignment_id)
            .bind(&record.id)
            .bind(version)
            .bind(&input.reason)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
                 VALUES($1,$2,'organization.position-move.applied','organization_position_move',$3,$4,$5,$6)",
            )
            .bind(format!("event-{}", record.id))
            .bind(&record.id)
            .bind(&record.id)
            .bind(version)
            .bind(
                json!({
                    "companyId": record.company_id,
                    "id": record.id,
                    "positionId": record.position_id,
                    "affectedCount": affected_count,
                    "state": "applied",
                    "effectiveAt": record.effective_at,
                })
                .to_string(),
            )
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn needs_review<'a>(
        &'a mut self,
        id: &'a str,
        code: &'a str,
        now: i64,
    ) -> PositionMoveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            sqlx::query(
                "UPDATE organization_position_moves SET status='needs_review',version=version+1, \
                 last_error=$1,updated_at=$2 WHERE id=$3 AND status='scheduled'",
            )
            .bind(code)
            .bind(now)
            .bind(id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
}

/// Native position-move apply worker.
///
/// Opens one serializable owner unit per claimed move, keeps the advisory claim
/// until the effects commit, and records `needs_review` in a fresh unit after a
/// rollback. At most 50 moves are processed per tick, matching the retained
/// TypeScript drain.
#[derive(Clone)]
pub struct PgPositionMoveDrain {
    pool: DatabasePool,
}

impl PgPositionMoveDrain {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    pub async fn drain(&self, now: i64) -> Result<u32, BusinessError> {
        let mut processed = 0;
        for _ in 0..50 {
            let current = Arc::new(Mutex::new(None::<String>));
            let cell = current.clone();
            let attempt = PgUnitOfWork::new(self.pool.clone())
                .run_once(TransactionOptions::default(), move |connection| {
                    let cell = cell.clone();
                    Box::pin(async move {
                        let mut ports = PgPositionMoves::new(connection);
                        match drain_once(&mut ports, now).await {
                            Ok(DrainOutcome::Empty) => Ok(false),
                            Ok(DrainOutcome::Applied { move_id }) => {
                                *cell.lock().unwrap() = Some(move_id);
                                Ok(true)
                            }
                            Err(failure) => {
                                *cell.lock().unwrap() = failure.move_id().map(str::to_owned);
                                Err(match failure {
                                    crate::application::position_moves::DrainFailure::Business {
                                        error,
                                        ..
                                    } => PgTransactionError::Business(error),
                                    crate::application::position_moves::DrainFailure::Repository {
                                        error,
                                        ..
                                    } => error,
                                })
                            }
                        }
                    })
                })
                .await;
            match attempt {
                Ok(true) => processed += 1,
                Ok(false) => break,
                Err(error) => {
                    let claimed: Option<String> = current.lock().unwrap().clone();
                    let Some(id) = claimed else {
                        return Err(match error {
                            PgTransactionError::Business(error) => error,
                            PgTransactionError::Persistence(_) => BusinessError::new(
                                FailureKind::Unavailable,
                                "POSITION_MOVE_RETRY_REQUIRED",
                                "The position-move apply transaction could not start.",
                            ),
                        });
                    };
                    let code = match &error {
                        PgTransactionError::Business(error) => error.code,
                        PgTransactionError::Persistence(_) => "POSITION_MOVE_RETRY_REQUIRED",
                    };
                    self.needs_review(&id, code, now).await?;
                }
            }
        }
        Ok(processed)
    }

    async fn needs_review(&self, id: &str, code: &str, now: i64) -> Result<(), BusinessError> {
        let id = id.to_owned();
        let code = code.to_owned();
        PgUnitOfWork::new(self.pool.clone())
            .run_once(TransactionOptions::default(), move |connection| {
                let id = id.clone();
                let code = code.clone();
                Box::pin(async move {
                    let mut ports = PgPositionMoves::new(connection);
                    ports.needs_review(&id, &code, now).await
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => BusinessError::new(
                    FailureKind::Unavailable,
                    "POSITION_MOVE_RETRY_REQUIRED",
                    "The position move could not be marked for review.",
                ),
            })
    }
}

/// Convenience entrypoint for the job registry once it is wired by the lead.
pub async fn drain_due_position_moves(pool: DatabasePool, now: i64) -> Result<u32, BusinessError> {
    PgPositionMoveDrain::new(pool).drain(now).await
}
