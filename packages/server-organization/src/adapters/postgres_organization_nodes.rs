//! PostgreSQL adapter for the native organization node workspace.
//!
//! The caller owns the Holding transaction, current session proof and Company
//! lease. This adapter re-checks the retained IAM source, all optimistic
//! versions and the immutable change history in that same unit.
use crate::{
    application::organization_nodes::{
        CancelChangeCommand, ChangeQueue, ChangeRow, NodeTarget, OrganizationCommandActor,
        OrganizationFuture, OrganizationNodeRepository, OrganizationWorkspace, ReviewChangeCommand,
        SaveChangeCommand, SaveTargetFacts, SharedChannel, WorkspaceChange, WorkspaceEvent,
        WorkspaceLevel, WorkspaceNode, WorkspacePosition, WorkspaceTemplate,
    },
    domain::organization_nodes::{
        ChangeFilters, NodeChangeAction, NodeChangeState, NodeImpact, NodeLifecycleKind,
        NodeStatus, OrganizationActor, OrganizationScopeKind, RelocationSnapshot,
        relocation_snapshot_json,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandEventScope, CommandField, CommandJson, WorkAuthority, command_fingerprint,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

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
        assignment_scope_kind: command.assignment_scope_kind.clone(),
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

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

fn conflict_code(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_READ_UNAVAILABLE",
        "Organization facts are unavailable.",
    )
}

fn authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORGANIZATION_AUTHORITY_CHANGED",
        "Your current organization authority is unavailable.",
    )
}

/// Map a duplicate-key failure to the retained command conflict. Every other
/// persistence failure keeps its real source.
fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "23505")
    {
        return conflict().into();
    }
    error.into()
}

fn node_status(value: &str) -> Result<NodeStatus, Error> {
    NodeStatus::parse(value).ok_or_else(|| unavailable().into())
}

fn node_kind(value: &str) -> Result<NodeLifecycleKind, Error> {
    NodeLifecycleKind::parse(value).ok_or_else(|| unavailable().into())
}

fn action(value: &str) -> Result<NodeChangeAction, Error> {
    NodeChangeAction::parse(value).ok_or_else(|| unavailable().into())
}

fn state(value: &str) -> Result<NodeChangeState, Error> {
    NodeChangeState::parse(value).ok_or_else(|| unavailable().into())
}

fn user(value: String) -> Result<UserId, Error> {
    UserId::new(value).map_err(|_| unavailable().into())
}

fn change(row: &PgRow) -> Result<ChangeRow, Error> {
    Ok(ChangeRow {
        id: row.try_get("id")?,
        target_node_id: row.try_get("target_node_id")?,
        action: action(row.try_get("action")?)?,
        node_kind: node_kind(row.try_get("node_kind")?)?,
        slug: row.try_get("slug")?,
        display_name: row.try_get("display_name")?,
        branch_id: row.try_get("branch_id")?,
        source_branch_id: row.try_get("source_branch_id")?,
        expected_node_version: row.try_get("expected_node_version")?,
        state: state(row.try_get("state")?)?,
        version: row.try_get("version")?,
        requested_by: user(row.try_get("requested_by")?)?,
        reviewed_by: row
            .try_get::<Option<String>, _>("reviewed_by")?
            .map(user)
            .transpose()?,
        reason: row.try_get("reason")?,
        review_reason: row.try_get("review_reason")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        reviewed_at: row.try_get("reviewed_at")?,
        applied_at: row.try_get("applied_at")?,
    })
}

fn workspace_node(row: &PgRow) -> Result<WorkspaceNode, Error> {
    Ok(WorkspaceNode {
        id: row.try_get("id")?,
        kind: row.try_get("kind")?,
        slug: row.try_get("slug")?,
        display_name: row.try_get("display_name")?,
        parent_id: row.try_get("parent_id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        status: row.try_get("status")?,
        hierarchy_revision: row.try_get("hierarchy_revision")?,
    })
}

fn workspace_position(row: &PgRow) -> Result<WorkspacePosition, Error> {
    Ok(WorkspacePosition {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        title: row.try_get("title")?,
        level_id: row.try_get("level_id")?,
        level_number: row.try_get("level_number")?,
        reports_to_position_id: row.try_get("reports_to_position_id")?,
        requirements: row.try_get("requirements")?,
        is_ceo: row.try_get("is_ceo")?,
        roster_required: row.try_get("roster_required")?,
        active: row.try_get("active")?,
        version: row.try_get("version")?,
    })
}

fn workspace_level(row: &PgRow) -> Result<WorkspaceLevel, Error> {
    Ok(WorkspaceLevel {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        level_number: row.try_get("level_number")?,
    })
}

fn workspace_template(row: &PgRow) -> Result<WorkspaceTemplate, Error> {
    let raw: String = row.try_get("positions_json")?;
    let positions = serde_json::from_str::<Vec<Value>>(&raw).map_err(|_| unavailable())?;
    Ok(WorkspaceTemplate {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        version: row.try_get("version")?,
        positions,
    })
}

/// Exact current audience predicate from `organizationChannelEngagementSql`.
/// The node alias and user parameter are supplied by the closed query here.
fn member_sql(node: &str, user_parameter: &str) -> String {
    format!(
        "EXISTS(SELECT 1 FROM hr_people op JOIN hr_engagements oe ON oe.person_id=op.id AND oe.company_id=op.company_id \
         WHERE op.company_id={node}.company_id AND op.user_id={user_parameter} AND op.status='active' AND op.merged_into_id IS NULL \
         AND oe.status='active' AND oe.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
         AND (oe.end_date IS NULL OR oe.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
         AND (SELECT count(*) FROM hr_engagements other WHERE other.person_id=op.id AND other.company_id=op.company_id AND other.status='active' \
              AND other.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
              AND (other.end_date IS NULL OR other.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))=1 \
         AND ({node}.kind='company' OR ({node}.kind='branch' AND oe.branch_id={node}.id) OR ({node}.kind='department' AND oe.department_id={node}.id)) \
         AND EXISTS(SELECT 1 FROM assignments oa WHERE oa.company_id=op.company_id AND oa.user_id=op.user_id AND oa.revoked_at IS NULL \
              AND oa.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
              AND oa.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))"
    )
}

pub struct PgOrganizationNodes<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgOrganizationNodes<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn transaction(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn authorize(&mut self, actor: &OrganizationActor, now: i64) -> Result<(), Error> {
        self.transaction()?;
        let allowed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments g \
             JOIN organization_nodes c ON c.id=g.company_id AND c.kind='company' AND c.status='active' \
             WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
             AND g.scope_kind='holding' AND g.revoked_at IS NULL \
             AND iam_assignment_active(g.id,$5) AND iam_principal_active(g.user_id,g.company_id,$5) \
             AND iam_role_allows(g.role,g.company_id,'organization.manage'))",
        )
        .bind(actor.assignment_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(&actor.role)
        .bind(actor.company_id.as_str())
        .bind(now)
        .fetch_one(&mut *self.connection)
        .await?;
        if !allowed {
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
            return Err(conflict_code(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let raw: Option<String> = row.try_get("result_json")?;
        raw.and_then(|value| serde_json::from_str(&value).ok())
            .map(Some)
            .ok_or_else(|| {
                conflict_code(
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
            return Err(conflict_code(
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
        let state = result.get("state").and_then(Value::as_str);
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

    async fn notice(
        &mut self,
        company: &CompanyId,
        id: &str,
        key: &str,
        event: &str,
    ) -> Result<(), Error> {
        let recipient = match event {
            "submitted" => "NOT iam_same_person(a.user_id,c.requested_by,c.company_id)",
            "cancelled" => {
                "EXISTS(SELECT 1 FROM staff_notifications n WHERE n.company_id=c.company_id \
                 AND n.record_type='organization_change' AND n.record_id=c.id AND n.user_id=a.user_id) \
                 AND a.user_id<>c.requested_by"
            }
            _ => "a.user_id=c.requested_by",
        };
        let message = match event {
            "submitted" => "Review organization change: ",
            "applied" => "Organization change applied: ",
            "rejected" => "Organization change returned for correction: ",
            _ => "Organization change cancelled: ",
        };
        let sql = format!(
            "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) \
             SELECT DISTINCT $1||a.user_id,c.company_id,a.user_id,'organization_change',c.id,c.version,$2||c.display_name,c.updated_at \
             FROM organization_node_changes c JOIN assignments a ON a.company_id=c.company_id \
             WHERE c.id=$3 AND c.company_id=$4 \
             AND a.scope_kind='holding' AND a.revoked_at IS NULL \
             AND iam_role_allows(a.role,a.company_id,'organization.manage') \
             AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND ({recipient}) ON CONFLICT DO NOTHING"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(format!("org-change-notice-{key}-"))
            .bind(message)
            .bind(id)
            .bind(company.as_str())
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
        Ok(())
    }

    async fn node_row(&mut self, company: &CompanyId, node: &str) -> Result<Option<PgRow>, Error> {
        Ok(sqlx::query(
            "SELECT id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision \
             FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind IN ('branch','department') FOR UPDATE",
        )
        .bind(node)
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?)
    }

    async fn branch_active(
        &mut self,
        company: &CompanyId,
        branch: &RecordId,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='branch' AND status='active')",
        )
        .bind(branch.as_str())
        .bind(company.as_str())
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn reserved_department(
        &mut self,
        company: &CompanyId,
        node: &str,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM organization_draft_reservations r WHERE r.company_id=$1 \
             AND ((r.kind='position' AND EXISTS(SELECT 1 FROM organization_positions p WHERE p.id=r.entity_id AND p.company_id=r.company_id AND p.department_id=$2)) \
             OR (r.kind='person' AND EXISTS(SELECT 1 FROM hr_engagements e WHERE e.id=r.entity_id AND e.company_id=r.company_id AND e.department_id=$2))))",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn has_children(&mut self, company: &CompanyId, node: &str) -> Result<bool, Error> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE company_id=$1 AND parent_id=$2 AND status<>'archived')",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn slug_conflict(
        &mut self,
        company: &CompanyId,
        parent: &str,
        slug: &str,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE company_id=$1 AND parent_id=$2 AND slug=$3)",
        )
        .bind(company.as_str())
        .bind(parent)
        .bind(slug)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn impact(&mut self, company: &CompanyId, node: &str) -> Result<NodeImpact, Error> {
        let row = sqlx::query(
            "SELECT \
             (SELECT count(*) FROM organization_nodes WHERE company_id=$1 AND parent_id=$2 AND status='active') child_nodes, \
             (SELECT count(*) FROM hr_engagements WHERE company_id=$1 AND status='active' AND (branch_id=$2 OR department_id=$2)) staff, \
             (SELECT count(*) FROM organization_positions WHERE company_id=$1 AND active=1 AND (branch_id=$2 OR department_id=$2)) positions, \
             (SELECT count(*) FROM assignments WHERE company_id=$1 AND revoked_at IS NULL AND scope_node_id=$2) assignments, \
             (SELECT count(*) FROM commerce_warehouses WHERE company_id=$1 AND branch_id=$2) warehouses",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(NodeImpact {
            child_nodes: row.try_get("child_nodes")?,
            staff: row.try_get("staff")?,
            positions: row.try_get("positions")?,
            assignments: row.try_get("assignments")?,
            warehouses: row.try_get("warehouses")?,
        })
    }

    async fn relocation_snapshot(
        &mut self,
        company: &CompanyId,
        node: &str,
    ) -> Result<String, Error> {
        let engagements = sqlx::query(
            "SELECT id,version::bigint version,status FROM hr_engagements WHERE company_id=$1 AND department_id=$2 \
             AND status IN ('draft','onboarding','active','suspended') ORDER BY id",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_all(&mut *self.connection)
        .await?;
        let positions = sqlx::query(
            "SELECT id,version::bigint version FROM organization_positions WHERE company_id=$1 AND department_id=$2 AND active=1 ORDER BY id",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_all(&mut *self.connection)
        .await?;
        let assignments = sqlx::query(
            "SELECT id,version::bigint version FROM assignments WHERE company_id=$1 AND scope_kind='department' AND scope_node_id=$2 AND revoked_at IS NULL ORDER BY id",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_all(&mut *self.connection)
        .await?;
        let rules = sqlx::query(
            "SELECT id,version::bigint version FROM organization_gpo_rules WHERE company_id=$1 AND target_kind='department' AND target_id=$2 ORDER BY id",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_all(&mut *self.connection)
        .await?;
        let head = sqlx::query(
            "SELECT version::bigint version,position_id,person_id,effective_at FROM authority_seats WHERE company_id=$1 AND kind='head' AND scope_id=$2",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_optional(&mut *self.connection)
        .await?;
        let legacy = sqlx::query(
            "SELECT version::bigint version,person_id FROM department_heads WHERE company_id=$1 AND department_id=$2",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_optional(&mut *self.connection)
        .await?;
        let flow = sqlx::query(
            "SELECT id,version::bigint version FROM department_flow_versions WHERE company_id=$1 AND department_id=$2 ORDER BY version DESC LIMIT 1",
        )
        .bind(company.as_str())
        .bind(node)
        .fetch_optional(&mut *self.connection)
        .await?;
        let snapshot = RelocationSnapshot {
            engagements: engagements
                .into_iter()
                .map(|row| {
                    Ok((
                        row.try_get("id")?,
                        row.try_get("version")?,
                        row.try_get("status")?,
                    ))
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?,
            positions: positions
                .into_iter()
                .map(|row| {
                    Ok::<(String, i64), sqlx::Error>((row.try_get("id")?, row.try_get("version")?))
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?,
            assignments: assignments
                .into_iter()
                .map(|row| {
                    Ok::<(String, i64), sqlx::Error>((row.try_get("id")?, row.try_get("version")?))
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?,
            rules: rules
                .into_iter()
                .map(|row| {
                    Ok::<(String, i64), sqlx::Error>((row.try_get("id")?, row.try_get("version")?))
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?,
            head: head
                .map(|row| {
                    Ok::<(i64, Option<String>, Option<String>, Option<i64>), sqlx::Error>((
                        row.try_get("version")?,
                        row.try_get("position_id")?,
                        row.try_get("person_id")?,
                        row.try_get("effective_at")?,
                    ))
                })
                .transpose()?,
            legacy_head: legacy
                .map(|row| {
                    Ok::<(i64, String), sqlx::Error>((
                        row.try_get("version")?,
                        row.try_get("person_id")?,
                    ))
                })
                .transpose()?,
            flow: flow
                .map(|row| {
                    Ok::<(String, i64), sqlx::Error>((row.try_get("id")?, row.try_get("version")?))
                })
                .transpose()?,
        };
        Ok(format!(
            "{:x}",
            Sha256::digest(relocation_snapshot_json(&snapshot).as_bytes())
        ))
    }

    async fn apply_create(
        &mut self,
        command: &ReviewChangeCommand,
        node_id: &str,
    ) -> Result<(), Error> {
        let change = &command.change;
        let parent = if change.node_kind == NodeLifecycleKind::Branch {
            command.actor.command.company_id.as_str().to_owned()
        } else {
            change.branch_id.clone().unwrap_or_default()
        };
        let kind = change.node_kind.as_str();
        let slug = change.slug.clone().unwrap_or_default();
        sqlx::query(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
             VALUES($1,$2,$3,$4,$5,$6,$7,'active',1)",
        )
        .bind(node_id)
        .bind(kind)
        .bind(slug)
        .bind(&change.display_name)
        .bind(parent)
        .bind(command.actor.command.company_id.as_str())
        .bind(&change.branch_id)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        Ok(())
    }

    async fn assert_node_version(&mut self, command: &ReviewChangeCommand) -> Result<(), Error> {
        let change = &command.change;
        let target = change.target_node_id.clone().unwrap_or_default();
        let expected_branch = if change.action == NodeChangeAction::Relocate {
            change.source_branch_id.clone()
        } else {
            change.branch_id.clone()
        };
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND hierarchy_revision=$3 \
             AND kind=$4 AND branch_id IS NOT DISTINCT FROM $5)",
        )
        .bind(target)
        .bind(command.actor.command.company_id.as_str())
        .bind(change.expected_node_version.unwrap_or_default())
        .bind(change.node_kind.as_str())
        .bind(expected_branch)
        .fetch_one(&mut *self.connection)
        .await?;
        if !exists {
            return Err(conflict_code(
                "NODE_VERSION_CHANGED",
                "The organization unit changed. Reload and review it again.",
            )
            .into());
        }
        Ok(())
    }

    async fn apply_relocation(
        &mut self,
        command: &ReviewChangeCommand,
        node_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        let change = &command.change;
        let company = command.actor.command.company_id.as_str();
        let source = change.target_node_id.clone().unwrap_or_default();
        let destination = change.branch_id.clone().unwrap_or_default();
        let display_name = &change.display_name;
        let actor = command.actor.command.user_id.as_str();
        let command_id = &command.change.id;
        let destination_active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='branch' AND status='active')",
        )
        .bind(&destination)
        .bind(company)
        .fetch_one(&mut *self.connection)
        .await?;
        if !destination_active {
            return Err(conflict_code(
                "BRANCH_INVALID",
                "Choose an active branch in this company.",
            )
            .into());
        }
        let children: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE company_id=$1 AND parent_id=$2 AND status<>'archived')",
        )
        .bind(company)
        .bind(&source)
        .fetch_one(&mut *self.connection)
        .await?;
        if children {
            return Err(conflict_code(
                "RELOCATION_HAS_CHILDREN",
                "Move or archive the department's child units before relocating it.",
            )
            .into());
        }
        let slug_conflict: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM organization_nodes destination JOIN organization_nodes source \
             ON source.id=$1 AND source.company_id=destination.company_id \
             WHERE destination.company_id=$2 AND destination.parent_id=$3 AND destination.slug=source.slug)",
        )
        .bind(&source)
        .bind(company)
        .bind(&destination)
        .fetch_one(&mut *self.connection)
        .await?;
        if slug_conflict {
            return Err(conflict_code(
                "RELOCATION_SLUG_CONFLICT",
                "A department with this URL code already exists in the destination branch.",
            )
            .into());
        }
        sqlx::query(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
             SELECT $1,kind,slug,$2,$3,company_id,$3,'active',1 FROM organization_nodes WHERE id=$4 AND company_id=$5",
        )
        .bind(node_id)
        .bind(display_name)
        .bind(&destination)
        .bind(&source)
        .bind(company)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "UPDATE organization_nodes SET status='archived',hierarchy_revision=hierarchy_revision+1 WHERE id=$1 AND company_id=$2",
        )
        .bind(&source)
        .bind(company)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "UPDATE hr_engagements SET branch_id=$1,department_id=$2,updated_at=$3 \
             WHERE company_id=$4 AND department_id=$5 AND status IN ('draft','onboarding','active','suspended')",
        )
        .bind(&destination)
        .bind(node_id)
        .bind(now)
        .bind(company)
        .bind(&source)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "UPDATE employees SET branch_id=$1,department_id=$2 WHERE company_id=$3 AND department_id=$4 AND status='active'",
        )
        .bind(&destination)
        .bind(node_id)
        .bind(company)
        .bind(&source)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "UPDATE organization_positions SET branch_id=$1,department_id=$2,updated_at=$3 \
             WHERE company_id=$4 AND department_id=$5 AND active=1",
        )
        .bind(&destination)
        .bind(node_id)
        .bind(now)
        .bind(company)
        .bind(&source)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "UPDATE assignments SET branch_id=$1,scope_node_id=$2,version=version+1 \
             WHERE company_id=$3 AND scope_kind='department' AND scope_node_id=$4 AND revoked_at IS NULL",
        )
        .bind(&destination)
        .bind(node_id)
        .bind(company)
        .bind(&source)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) \
             SELECT company_id,kind,$1,1,position_id,person_id,effective_at FROM authority_seats \
             WHERE company_id=$2 AND kind='head' AND scope_id=$3",
        )
        .bind(node_id)
        .bind(company)
        .bind(&source)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,source_reference,command_id,created_at) \
             SELECT $1,company_id,'head',$2,1,position_id,person_id,COALESCE(effective_at,$3::bigint),$4,'Carried forward during department relocation','explicit',$5,$6,$3 \
             FROM authority_seats WHERE company_id=$7 AND kind='head' AND scope_id=$5",
        )
        .bind(format!("appointment-relocated-{command_id}"))
        .bind(node_id)
        .bind(now)
        .bind(actor)
        .bind(&source)
        .bind(format!("appointment-relocated-{command_id}"))
        .bind(company)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) \
             SELECT $1,company_id,person_id,1,$2,$3::bigint FROM authority_seats \
             WHERE company_id=$4 AND kind='head' AND scope_id=$5 AND person_id IS NOT NULL \
             UNION ALL \
             SELECT $1,legacy.company_id,legacy.person_id,1,$2,$3::bigint FROM department_heads legacy \
             WHERE legacy.company_id=$4 AND legacy.department_id=$5 \
             AND NOT EXISTS(SELECT 1 FROM authority_seats s WHERE s.company_id=legacy.company_id AND s.kind='head' AND s.scope_id=legacy.department_id)",
        )
        .bind(node_id)
        .bind(actor)
        .bind(now)
        .bind(company)
        .bind(&source)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "INSERT INTO department_flow_versions(id,company_id,department_id,version,steps_json,published_by,reason,command_id,created_at) \
             SELECT $1,company_id,$2,1,steps_json,$3,'Carried forward during department relocation',$4,$5 \
             FROM department_flow_versions WHERE company_id=$6 AND department_id=$7 ORDER BY version DESC LIMIT 1",
        )
        .bind(format!("department-flow-relocated-{command_id}"))
        .bind(node_id)
        .bind(actor)
        .bind(format!("department-flow-relocated-{command_id}"))
        .bind(now)
        .bind(company)
        .bind(&source)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        Ok(())
    }

    async fn apply_channels(
        &mut self,
        company: &str,
        display_name: &str,
        node: &str,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO messaging_teams(id,company_id,name,owner_id,organization_managed,created_at) \
             VALUES($1,$2,'Organization','organization',true,$3) ON CONFLICT DO NOTHING",
        )
        .bind(format!("org-team-{company}"))
        .bind(company)
        .bind(now)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,organization_node_id,created_at) \
             SELECT $1,$2,'channel',$3,'organization',$4,$5,$6 \
             WHERE EXISTS(SELECT 1 FROM organization_nodes WHERE id=$5 AND company_id=$2 AND status='active') \
             ON CONFLICT DO NOTHING",
        )
        .bind(format!("org-channel-{node}"))
        .bind(company)
        .bind(display_name)
        .bind(format!("org-team-{company}"))
        .bind(node)
        .bind(now)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        sqlx::query(
            "UPDATE messaging_rooms SET name=$1 WHERE company_id=$2 AND organization_node_id=$3",
        )
        .bind(display_name)
        .bind(company)
        .bind(node)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        Ok(())
    }

    async fn insert_event(
        &mut self,
        company: &str,
        change_id: &str,
        key: &str,
        event: &str,
        actor: &str,
        version: i64,
        snapshot: &Value,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO organization_node_change_events(id,change_id,company_id,action,actor_id,version,snapshot_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(format!("org-change-event-{key}"))
        .bind(change_id)
        .bind(company)
        .bind(event)
        .bind(actor)
        .bind(version)
        .bind(snapshot.to_string())
        .bind(now)
        .execute(&mut *self.connection)
        .await
        .map_err(write_error)?;
        Ok(())
    }

    async fn require_change_version(
        &mut self,
        company: &CompanyId,
        change_id: &str,
        version: i64,
        allowed: &[&str],
    ) -> Result<ChangeRow, Error> {
        let row = sqlx::query(
            "SELECT * FROM organization_node_changes WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(change_id)
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "ORGANIZATION_CHANGE_NOT_FOUND",
                "Organization change not found.",
            )
        })?;
        let current = change(&row)?;
        if current.version != version || !allowed.contains(&current.state.as_str()) {
            return Err(conflict().into());
        }
        Ok(current)
    }
}

fn aggregate(operation: &str) -> String {
    operation.split('.').take(2).collect::<Vec<_>>().join("_")
}

impl OrganizationNodeRepository for PgOrganizationNodes<'_> {
    type Error = Error;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        filters: &'a ChangeFilters,
    ) -> OrganizationFuture<'a, OrganizationWorkspace, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let branch = actor.branch_id.as_ref().map(RecordId::as_str);
            let department = actor.department_id.as_ref().map(RecordId::as_str);
            let rows = sqlx::query(
                "SELECT id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision \
                 FROM organization_nodes WHERE company_id=$1 AND kind IN ('company','branch','department') \
                 AND ($2::text IS NULL OR id=$2 OR branch_id=$2) \
                 AND ($3::text IS NULL OR kind<>'department' OR id=$3) \
                 ORDER BY status,display_name,id LIMIT 4001",
            )
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(department)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > 4000 {
                return Err(unavailable().into());
            }
            let nodes = rows
                .iter()
                .map(workspace_node)
                .collect::<Result<Vec<_>, _>>()?;
            let position_rows = sqlx::query(
                "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id, \
                 l.level_number::bigint level_number,p.reports_to_position_id,p.requirements, \
                 p.is_ceo::bigint is_ceo,p.roster_required::bigint roster_required, \
                 p.active::bigint active,p.version::bigint version \
                 FROM organization_positions p \
                 JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id \
                 WHERE p.company_id=$1 ORDER BY l.level_number,p.title,p.id LIMIT 4001",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            if position_rows.len() > 4000 {
                return Err(unavailable().into());
            }
            let mut positions = Vec::new();
            for row in &position_rows {
                let node_branch: Option<String> = row.try_get("branch_id")?;
                let node_department: Option<String> = row.try_get("department_id")?;
                let branch_matches =
                    branch.is_none() || node_branch.is_none() || node_branch.as_deref() == branch;
                let department_matches = department.is_none()
                    || node_department.is_none()
                    || node_department.as_deref() == department;
                if branch_matches && department_matches {
                    positions.push(workspace_position(row)?);
                }
            }
            let level_rows = sqlx::query(
                "SELECT id,name,level_number::bigint level_number FROM staff_levels \
                 WHERE company_id=$1 AND active=1 AND level_number IS NOT NULL \
                 ORDER BY level_number LIMIT 101",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            if level_rows.len() > 100 {
                return Err(unavailable().into());
            }
            let levels = level_rows
                .iter()
                .map(workspace_level)
                .collect::<Result<Vec<_>, _>>()?;
            let templates = if actor.can_manage() {
                let template_rows = sqlx::query(
                    "SELECT id,name,version::bigint version,positions_json FROM organization_templates \
                     WHERE company_id=$1 ORDER BY name LIMIT 501",
                )
                .bind(actor.company_id.as_str())
                .fetch_all(&mut *self.connection)
                .await?;
                if template_rows.len() > 500 {
                    return Err(unavailable().into());
                }
                template_rows
                    .iter()
                    .map(workspace_template)
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                Vec::new()
            };
            let can_manage_spending =
                actor.can_manage() && actor.assignment_scope_kind == OrganizationScopeKind::Holding;
            let member = member_sql("n", "$2");
            let channel_rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT n.id node_id,r.id room_id,r.name,n.kind,n.status FROM organization_nodes n \
                 JOIN messaging_rooms r ON r.company_id=n.company_id AND r.organization_node_id=n.id \
                 WHERE n.company_id=$1 AND n.status='active' AND n.kind IN ('company','branch','department') AND {member} \
                 ORDER BY n.kind,n.display_name,n.id LIMIT 4001"
            )))
            .bind(actor.company_id.as_str())
            .bind(actor.user_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            if channel_rows.len() > 4000 {
                return Err(unavailable().into());
            }
            let channels = channel_rows
                .into_iter()
                .map(|row| {
                    Ok(SharedChannel {
                        node_id: row.try_get("node_id")?,
                        room_id: row.try_get("room_id")?,
                        name: row.try_get("name")?,
                        kind: row.try_get("kind")?,
                        status: row.try_get("status")?,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let channel_node_rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT n.id FROM organization_nodes n WHERE n.company_id=$1 AND n.status='active' \
                 AND n.kind IN ('company','branch','department') AND {member} \
                 ORDER BY n.kind,n.display_name,n.id LIMIT 4001"
            )))
            .bind(actor.company_id.as_str())
            .bind(actor.user_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            let authorized_channel_nodes = channel_node_rows
                .into_iter()
                .map(|row| row.try_get::<String, _>("id"))
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let can_manage_units = actor.can_manage_units();
            let total = if can_manage_units {
                let mut sql = String::from(
                    "SELECT count(*) total FROM organization_node_changes WHERE company_id=$2",
                );
                let mut index = 3;
                if filters.change_id.is_some() {
                    sql.push_str(&format!(" AND id=${index}"));
                    index += 1;
                }
                if let crate::domain::organization_nodes::ChangeStateFilter::One(_) = filters.state
                {
                    sql.push_str(&format!(" AND state=${index}"));
                    index += 1;
                }
                sql.push_str(&format!(
                    " AND (display_name ILIKE ${index} OR reason ILIKE ${index})"
                ));
                let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.clone()))
                    .bind(actor.user_id.as_str())
                    .bind(actor.company_id.as_str());
                if let Some(change_id) = &filters.change_id {
                    query = query.bind(change_id);
                }
                if let crate::domain::organization_nodes::ChangeStateFilter::One(state) =
                    filters.state
                {
                    query = query.bind(state.as_str());
                }
                query
                    .bind(filters.like_pattern())
                    .fetch_one(&mut *self.connection)
                    .await?
            } else {
                0
            };
            let pages = ((total + 19) / 20).max(1);
            let page = filters.page.min(pages);
            let offset = (page - 1) * 20;
            let changes = if can_manage_units {
                let state_predicate = match filters.change_id {
                    Some(_) => "TRUE".to_owned(),
                    None => filters.sql_state().to_owned(),
                };
                let mut sql = format!(
                    "SELECT *,NOT iam_same_person(requested_by,$1,company_id) independent_reviewer \
                     FROM organization_node_changes WHERE company_id=$2"
                );
                let mut index = 3;
                if filters.change_id.is_some() {
                    sql.push_str(&format!(" AND id=${index}"));
                    index += 1;
                }
                sql.push_str(&format!(" AND {state_predicate}"));
                sql.push_str(&format!(
                    " AND (display_name ILIKE ${index} OR reason ILIKE ${index}) \
                     ORDER BY updated_at DESC,id LIMIT ${} OFFSET ${}",
                    index + 1,
                    index + 2
                ));
                let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                    .bind(actor.user_id.as_str())
                    .bind(actor.company_id.as_str());
                if let Some(change_id) = &filters.change_id {
                    query = query.bind(change_id);
                }
                query
                    .bind(filters.like_pattern())
                    .bind(20_i64)
                    .bind(offset)
                    .fetch_all(&mut *self.connection)
                    .await?
            } else {
                Vec::new()
            };
            let ids = changes
                .iter()
                .map(|row| row.try_get::<String, _>("id"))
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            let events = if ids.is_empty() {
                Vec::new()
            } else {
                sqlx::query(
                    "SELECT e.change_id,e.action,e.version,e.created_at,e.snapshot_json,COALESCE(p.legal_name,e.actor_id) actor_name \
                     FROM organization_node_change_events e \
                     LEFT JOIN hr_people p ON p.user_id=e.actor_id AND p.company_id=e.company_id AND p.merged_into_id IS NULL \
                     WHERE e.company_id=$1 AND e.change_id=ANY($2::text[]) ORDER BY e.version,e.created_at,e.id",
                )
                .bind(actor.company_id.as_str())
                .bind(&ids)
                .fetch_all(&mut *self.connection)
                .await?
            };
            let mut node_changes = Vec::with_capacity(changes.len());
            for row in &changes {
                let current = change(row)?;
                let independent: bool = row.try_get("independent_reviewer")?;
                let can_review = current.state == NodeChangeState::Submitted && independent;
                let change_events = events
                    .iter()
                    .filter(|event| {
                        event
                            .try_get::<String, _>("change_id")
                            .is_ok_and(|id| id == current.id)
                    })
                    .map(|event| {
                        let snapshot: String = event.try_get("snapshot_json")?;
                        let reason =
                            serde_json::from_str::<Value>(&snapshot)
                                .ok()
                                .and_then(|value| {
                                    value
                                        .get("reason")
                                        .and_then(Value::as_str)
                                        .map(str::to_owned)
                                });
                        Ok(WorkspaceEvent {
                            action: event.try_get("action")?,
                            actor_name: event.try_get("actor_name")?,
                            version: event.try_get("version")?,
                            reason,
                            created_at: event.try_get("created_at")?,
                        })
                    })
                    .collect::<Result<Vec<_>, sqlx::Error>>()?;
                node_changes.push(WorkspaceChange {
                    id: current.id,
                    target_node_id: current.target_node_id,
                    action: current.action.as_str().into(),
                    node_kind: current.node_kind.as_str().into(),
                    slug: current.slug,
                    display_name: current.display_name,
                    branch_id: current.branch_id,
                    source_branch_id: current.source_branch_id,
                    expected_node_version: current.expected_node_version,
                    state: current.state.as_str().into(),
                    version: current.version,
                    requested_by: current.requested_by.as_str().into(),
                    reviewed_by: current.reviewed_by.map(|id| id.as_str().to_owned()),
                    reason: current.reason,
                    review_reason: current.review_reason,
                    created_at: current.created_at,
                    updated_at: current.updated_at,
                    reviewed_at: current.reviewed_at,
                    applied_at: current.applied_at,
                    can_review,
                    events: change_events,
                });
            }
            Ok(OrganizationWorkspace {
                nodes,
                positions,
                levels,
                templates,
                node_changes,
                change_queue: ChangeQueue {
                    record_id: filters.change_id.clone(),
                    state: filters.rendered_state().into(),
                    query: filters.query.clone(),
                    page,
                    page_size: 20,
                    total,
                    pages,
                },
                authorized_channel_nodes,
                channels,
                actor_id: actor.user_id.as_str().into(),
                can_manage: actor.can_manage(),
                can_manage_units,
                can_manage_spending,
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
    ) -> OrganizationFuture<'a, Option<Value>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let hash = fingerprint(actor, operation, record_id, input)?;
            self.receipt(key, &hash).await
        })
    }

    fn change<'a>(
        &'a mut self,
        company: &'a CompanyId,
        change_id: &'a str,
    ) -> OrganizationFuture<'a, Option<ChangeRow>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let row = sqlx::query(
                "SELECT * FROM organization_node_changes WHERE id=$1 AND company_id=$2",
            )
            .bind(change_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.as_ref().map(change).transpose()
        })
    }

    fn target<'a>(
        &'a mut self,
        company: &'a CompanyId,
        node: &'a str,
    ) -> OrganizationFuture<'a, Option<(Option<String>, String)>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let row = sqlx::query(
                "SELECT branch_id,slug FROM organization_nodes WHERE id=$1 AND company_id=$2",
            )
            .bind(node)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok((
                    row.try_get::<Option<String>, _>("branch_id")?,
                    row.try_get::<String, _>("slug")?,
                ))
            })
            .transpose()
        })
    }

    fn save_target_facts<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        input: &'a crate::domain::organization_nodes::NodeChangeInput,
        _current: Option<&'a ChangeRow>,
    ) -> OrganizationFuture<'a, SaveTargetFacts, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let mut facts = SaveTargetFacts::default();
            if let Some(branch) = &input.branch_id {
                facts.branch_active = self.branch_active(&actor.company_id, branch).await?;
            }
            if let Some(target) = &input.target_node_id {
                let row = self.node_row(&actor.company_id, target.as_str()).await?;
                facts.node = row
                    .as_ref()
                    .map(|row| {
                        Ok::<_, Error>(NodeTarget {
                            kind: node_kind(row.try_get("kind")?)?,
                            branch_id: row.try_get("branch_id")?,
                            slug: row.try_get("slug")?,
                            status: node_status(row.try_get("status")?)?,
                            hierarchy_revision: row.try_get("hierarchy_revision")?,
                        })
                    })
                    .transpose()?;
                facts.reserved_department = self
                    .reserved_department(&actor.company_id, target.as_str())
                    .await?;
                facts.has_children = self
                    .has_children(&actor.company_id, target.as_str())
                    .await?;
                if input.is_relocation() {
                    let slug = facts
                        .node
                        .as_ref()
                        .map(|node| node.slug.clone())
                        .unwrap_or_default();
                    let destination = input
                        .branch_id
                        .as_ref()
                        .map(RecordId::as_str)
                        .unwrap_or_default();
                    facts.slug_conflict = self
                        .slug_conflict(&actor.company_id, destination, &slug)
                        .await?;
                    facts.relocation_snapshot = Some(
                        self.relocation_snapshot(&actor.company_id, target.as_str())
                            .await?,
                    );
                }
            }
            Ok(facts)
        })
    }

    fn preview<'a>(
        &'a mut self,
        company: &'a CompanyId,
        node: &'a str,
    ) -> OrganizationFuture<'a, (NodeImpact, String), Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let impact = self.impact(company, node).await?;
            let snapshot = self.relocation_snapshot(company, node).await?;
            Ok((impact, snapshot))
        })
    }

    fn independent_reviewer<'a>(
        &'a mut self,
        actor: &'a OrganizationActor,
        requester: &'a UserId,
    ) -> OrganizationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let same: bool = sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)")
                .bind(actor.user_id.as_str())
                .bind(requester.as_str())
                .bind(actor.company_id.as_str())
                .fetch_one(&mut *self.connection)
                .await?;
            Ok(!same)
        })
    }

    fn submitted_snapshot<'a>(
        &'a mut self,
        change_id: &'a str,
    ) -> OrganizationFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let row = sqlx::query(
                "SELECT snapshot_json FROM organization_node_change_events WHERE change_id=$1 AND action='submitted' \
                 ORDER BY created_at DESC,id DESC LIMIT 1",
            )
            .bind(change_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else { return Ok(None) };
            let raw: String = row.try_get("snapshot_json")?;
            Ok(serde_json::from_str::<Value>(&raw).ok().and_then(|value| {
                value
                    .get("previewSnapshot")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }))
        })
    }

    fn relocation_snapshot<'a>(
        &'a mut self,
        company: &'a CompanyId,
        node: &'a str,
    ) -> OrganizationFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            PgOrganizationNodes::relocation_snapshot(self, company, node).await
        })
    }

    fn persist_save<'a>(
        &'a mut self,
        command: &'a SaveChangeCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let projection = command.actor.projection();
            self.authorize(&projection, now).await?;
            let version = command.input.expected_version + 1;
            let state = if command.submit { "submitted" } else { "draft" };
            if command.input.is_relocation() {
                let target = command
                    .input
                    .target_node_id
                    .as_ref()
                    .map(RecordId::as_str)
                    .unwrap_or_default();
                let current = self
                    .relocation_snapshot(&projection.company_id, target)
                    .await?;
                let saved = command.input.preview_snapshot.as_ref();
                if let Some(saved) = saved
                    && saved != &current
                {
                    return Err(conflict_code(
                        "ORGANIZATION_RELOCATION_CHANGED",
                        "The department's people, positions, or access assignments changed. Preview the move again.",
                    )
                    .into());
                }
            }
            if command.existing {
                self.require_change_version(
                    &projection.company_id,
                    &command.record_id,
                    command.input.expected_version,
                    &["draft", "rejected"],
                )
                .await?;
            }
            let snapshot = {
                let mut value = json!({
                    "action": command.input.action.as_str(),
                    "targetNodeId": command.input.target_node_id.as_ref().map(RecordId::as_str),
                    "kind": command.input.kind.as_str(),
                    "slug": command.slug,
                    "displayName": command.input.display_name,
                    "branchId": command.input.branch_id.as_ref().map(RecordId::as_str),
                    "expectedNodeVersion": command.input.expected_node_version,
                    "expectedVersion": command.input.expected_version,
                    "reason": command.input.reason,
                    "state": state,
                });
                if let Some(preview) = &command.input.preview_snapshot {
                    value["previewSnapshot"] = Value::String(preview.clone());
                }
                value
            };
            sqlx::query(
                "INSERT INTO organization_node_changes(id,company_id,target_node_id,action,node_kind,slug,display_name,branch_id,source_branch_id,expected_node_version,state,version,requested_by,reason,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) \
                 ON CONFLICT(id) DO UPDATE SET target_node_id=excluded.target_node_id,action=excluded.action,node_kind=excluded.node_kind,slug=excluded.slug,display_name=excluded.display_name,branch_id=excluded.branch_id,source_branch_id=excluded.source_branch_id,expected_node_version=excluded.expected_node_version,state=excluded.state,version=excluded.version,reason=excluded.reason,reviewed_by=NULL,review_reason=NULL,reviewed_at=NULL,updated_at=excluded.updated_at",
            )
            .bind(&command.record_id)
            .bind(projection.company_id.as_str())
            .bind(command.input.target_node_id.as_ref().map(RecordId::as_str))
            .bind(command.input.action.as_str())
            .bind(command.input.kind.as_str())
            .bind(&command.slug)
            .bind(&command.input.display_name)
            .bind(command.input.branch_id.as_ref().map(RecordId::as_str))
            .bind(&command.source_branch_id)
            .bind(command.input.expected_node_version)
            .bind(state)
            .bind(version)
            .bind(projection.user_id.as_str())
            .bind(&command.input.reason)
            .bind(now)
            .bind(now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            self.insert_event(
                projection.company_id.as_str(),
                &command.record_id,
                &command.key,
                if command.submit {
                    "submitted"
                } else {
                    "drafted"
                },
                projection.user_id.as_str(),
                version,
                &snapshot,
                now,
            )
            .await?;
            if command.submit {
                self.notice(
                    &projection.company_id,
                    &command.record_id,
                    &command.key,
                    "submitted",
                )
                .await?;
            }
            let result = json!({"id": command.record_id, "version": version, "state": state});
            let hash = fingerprint(
                &command.actor,
                if command.submit {
                    "organization.node-change.submit"
                } else {
                    "organization.node-change.save"
                },
                &command.record_id,
                &snapshot_input(&command.input),
            )?;
            self.save_receipt(
                &command.actor,
                &command.key,
                if command.submit {
                    "organization.node-change.submit"
                } else {
                    "organization.node-change.save"
                },
                &command.record_id,
                &hash,
                &result,
                version,
                now,
            )
            .await?;
            Ok(result)
        })
    }

    fn persist_review<'a>(
        &'a mut self,
        command: &'a ReviewChangeCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let projection = command.actor.projection();
            self.authorize(&projection, now).await?;
            let version = command.change.version + 1;
            if command.state == NodeChangeState::Applied {
                let node_id = command.node_id.clone().unwrap_or_default();
                if matches!(
                    command.change.action,
                    NodeChangeAction::Suspend | NodeChangeAction::Archive
                ) {
                    let target = command.change.target_node_id.clone().unwrap_or_default();
                    let impact = self.impact(&projection.company_id, &target).await?;
                    crate::domain::organization_nodes::validate_lifecycle_impact(
                        command.change.action,
                        impact,
                    )?;
                }
                match command.change.action {
                    NodeChangeAction::Create => self.apply_create(command, &node_id).await?,
                    NodeChangeAction::Relocate => {
                        self.assert_node_version(command).await?;
                        self.apply_relocation(command, &node_id, now).await?;
                    }
                    _ => {
                        self.assert_node_version(command).await?;
                        let status = match command.change.action {
                            NodeChangeAction::Suspend => Some("suspended"),
                            NodeChangeAction::Archive => Some("archived"),
                            NodeChangeAction::Reactivate => Some("active"),
                            _ => None,
                        };
                        sqlx::query(
                            "UPDATE organization_nodes SET display_name=$1,status=COALESCE($2,status),hierarchy_revision=hierarchy_revision+1 \
                             WHERE id=$3 AND company_id=$4",
                        )
                        .bind(&command.change.display_name)
                        .bind(status)
                        .bind(command.change.target_node_id.clone().unwrap_or_default())
                        .bind(projection.company_id.as_str())
                        .execute(&mut *self.connection)
                        .await
                        .map_err(write_error)?;
                    }
                }
                self.apply_channels(
                    projection.company_id.as_str(),
                    &command.change.display_name,
                    &node_id,
                    now,
                )
                .await?;
            }
            self.require_change_version(
                &projection.company_id,
                &command.change.id,
                command.input.expected_version,
                &["submitted"],
            )
            .await?;
            let applied_at = (command.state == NodeChangeState::Applied).then_some(now);
            sqlx::query(
                "UPDATE organization_node_changes SET state=$1,version=$2,reviewed_by=$3,review_reason=$4,reviewed_at=$5,applied_at=$6,updated_at=$7 \
                 WHERE id=$8 AND company_id=$9",
            )
            .bind(command.state.as_str())
            .bind(version)
            .bind(projection.user_id.as_str())
            .bind(&command.input.reason)
            .bind(now)
            .bind(applied_at)
            .bind(now)
            .bind(&command.change.id)
            .bind(projection.company_id.as_str())
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            let event = if command.state == NodeChangeState::Applied {
                "applied"
            } else {
                "rejected"
            };
            let snapshot = json!({
                "decision": match command.input.decision {
                    crate::domain::organization_nodes::NodeReviewDecision::Approve => "approve",
                    crate::domain::organization_nodes::NodeReviewDecision::Reject => "reject",
                },
                "reason": command.input.reason,
                "appliedNodeId": command.node_id,
            });
            self.insert_event(
                projection.company_id.as_str(),
                &command.change.id,
                &command.key,
                event,
                projection.user_id.as_str(),
                version,
                &snapshot,
                now,
            )
            .await?;
            self.notice(
                &projection.company_id,
                &command.change.id,
                &command.key,
                event,
            )
            .await?;
            let result = json!({
                "id": command.change.id,
                "version": version,
                "state": command.state.as_str(),
                "nodeId": command.node_id,
            });
            let decision = json!({
                "expectedVersion": command.input.expected_version,
                "decision": match command.input.decision {
                    crate::domain::organization_nodes::NodeReviewDecision::Approve => "approve",
                    crate::domain::organization_nodes::NodeReviewDecision::Reject => "reject",
                },
                "reason": command.input.reason,
            });
            let hash = fingerprint(
                &command.actor,
                "organization.node-change.review",
                &command.change.id,
                &decision,
            )?;
            self.save_receipt(
                &command.actor,
                &command.key,
                "organization.node-change.review",
                &command.change.id,
                &hash,
                &result,
                version,
                now,
            )
            .await?;
            Ok(result)
        })
    }

    fn persist_cancel<'a>(
        &'a mut self,
        command: &'a CancelChangeCommand,
        now: i64,
    ) -> OrganizationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let projection = command.actor.projection();
            self.authorize(&projection, now).await?;
            self.require_change_version(
                &projection.company_id,
                &command.change.id,
                command.change.version,
                &["draft", "submitted", "rejected"],
            )
            .await?;
            sqlx::query(
                "UPDATE organization_node_changes SET state='cancelled',version=$1,updated_at=$2 WHERE id=$3 AND company_id=$4",
            )
            .bind(command.version)
            .bind(now)
            .bind(&command.change.id)
            .bind(projection.company_id.as_str())
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            let snapshot = json!({"expectedVersion": command.change.version});
            self.insert_event(
                projection.company_id.as_str(),
                &command.change.id,
                &command.key,
                "cancelled",
                projection.user_id.as_str(),
                command.version,
                &snapshot,
                now,
            )
            .await?;
            self.notice(
                &projection.company_id,
                &command.change.id,
                &command.key,
                "cancelled",
            )
            .await?;
            let result =
                json!({"id": command.change.id, "version": command.version, "state": "cancelled"});
            let hash = fingerprint(
                &command.actor,
                "organization.node-change.cancel",
                &command.change.id,
                &snapshot,
            )?;
            self.save_receipt(
                &command.actor,
                &command.key,
                "organization.node-change.cancel",
                &command.change.id,
                &hash,
                &result,
                command.version,
                now,
            )
            .await?;
            Ok(result)
        })
    }
}

fn snapshot_input(input: &crate::domain::organization_nodes::NodeChangeInput) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("action".into(), Value::String(input.action.as_str().into()));
    object.insert(
        "targetNodeId".into(),
        input
            .target_node_id
            .as_ref()
            .map(|id| Value::String(id.as_str().into()))
            .unwrap_or(Value::Null),
    );
    object.insert("kind".into(), Value::String(input.kind.as_str().into()));
    object.insert(
        "slug".into(),
        input
            .slug
            .as_ref()
            .map(|slug| Value::String(slug.clone()))
            .unwrap_or(Value::Null),
    );
    object.insert(
        "displayName".into(),
        Value::String(input.display_name.clone()),
    );
    object.insert(
        "branchId".into(),
        input
            .branch_id
            .as_ref()
            .map(|id| Value::String(id.as_str().into()))
            .unwrap_or(Value::Null),
    );
    object.insert(
        "expectedNodeVersion".into(),
        input
            .expected_node_version
            .map(|value| json!(value))
            .unwrap_or(Value::Null),
    );
    object.insert("expectedVersion".into(), json!(input.expected_version));
    object.insert("reason".into(), Value::String(input.reason.clone()));
    if let Some(snapshot) = &input.preview_snapshot {
        object.insert("previewSnapshot".into(), Value::String(snapshot.clone()));
    }
    Value::Object(object)
}
