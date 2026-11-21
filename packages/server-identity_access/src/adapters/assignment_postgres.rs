//! PostgreSQL adapter for the native Identity Access assignment slice.
//!
//! SQL is Identity-owned (`assignments`, `iam_members`, `iam_roles`,
//! `iam_role_revisions`, `iam_identities`, `iam_connections`, `iam_mappings`,
//! `iam_groups`, `iam_audit`, `iam_revoked_sessions` and
//! `active_work_contexts`) plus the shared command receipt, audit and outbox
//! tables. No Organization, People, Finance or other owner row is mutated.
//! The caller's serializable Holding transaction is required and no commit is
//! performed here.
use crate::application::assignment::{AssignmentFuture, AssignmentRepository};
use crate::domain::assignment::{
    AssignmentActor, AssignmentGuardFacts, AssignmentReceipt, AssignmentRoleRow, AssignmentRow,
    REVOKE_OPERATION, ReceiptSnapshot, RevocationPlan, assignment_changed, idempotency_conflict,
    readback_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const SELECT: &str = "SELECT a.id,a.user_id,a.role,a.modules_json::text modules_json,\
 a.company_id,a.branch_id,a.valid_from,a.valid_until,a.revoked_at,a.version,a.scope_kind,\
 a.scope_node_id,a.parent_id,a.source,a.source_id,m.status identity_status,m.login_policy,\
 m.person_id,m.connection_id,m.version::bigint identity_version,mp.group_id,\
 mapping_group.name group_name,mp.connection_id mapping_connection_id,i.enabled directory_enabled,\
 i.observed_at,c.status connection_status,\
 (iam_assignment_active(a.id,$3) AND COALESCE(m.status,'active')='active' AND \
  (m.login_policy IS DISTINCT FROM 'directory' OR (i.enabled=true AND c.status='active' \
   AND COALESCE(i.observed_at,0)>$3-300000))) active \
 FROM assignments a \
 LEFT JOIN iam_members m ON m.user_id=a.user_id AND m.company_id=a.company_id \
 LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id \
 LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id \
 LEFT JOIN iam_mappings mp ON a.source='directory' AND mp.id=a.source_id AND mp.company_id=a.company_id \
 LEFT JOIN iam_groups mapping_group ON mapping_group.connection_id=mp.connection_id \
  AND mapping_group.external_id=mp.group_id";

fn map_row(row: sqlx::postgres::PgRow) -> Result<AssignmentRow, sqlx::Error> {
    Ok(AssignmentRow {
        id: row.try_get("id")?,
        user_id: row.try_get("user_id")?,
        role: row.try_get("role")?,
        modules_json: row.try_get("modules_json")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        valid_from: row.try_get("valid_from")?,
        valid_until: row.try_get("valid_until")?,
        revoked_at: row.try_get("revoked_at")?,
        version: row.try_get("version")?,
        scope_kind: row.try_get("scope_kind")?,
        scope_node_id: row.try_get("scope_node_id")?,
        parent_id: row.try_get("parent_id")?,
        source: row.try_get("source")?,
        source_id: row.try_get("source_id")?,
        identity_status: row.try_get("identity_status")?,
        login_policy: row.try_get("login_policy")?,
        person_id: row.try_get("person_id")?,
        connection_id: row.try_get("connection_id")?,
        identity_version: row.try_get("identity_version")?,
        group_name: row.try_get("group_name")?,
        mapping_connection_id: row.try_get("mapping_connection_id")?,
        directory_enabled: row.try_get("directory_enabled")?,
        observed_at: row.try_get("observed_at")?,
        connection_status: row.try_get("connection_status")?,
        active: row.try_get("active")?,
    })
}

pub struct PgIdentityAssignments<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgIdentityAssignments<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("IAM_ASSIGNMENT_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("IAM_ASSIGNMENT_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("iam-assignment-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn current(
        &mut self,
        actor: &AssignmentActor,
        write: bool,
        now: i64,
    ) -> Result<bool, Error> {
        self.ready().await?;
        let permission = if write { "iam.manage" } else { "iam.read" };
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments a \
             JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id \
             LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id \
             LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id \
             JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active' \
             JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
              AND v.revision=r.published_revision \
             WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
             AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
             AND ($5::text IS NULL OR a.role=$5::text) \
             AND ($6::bigint IS NULL OR a.version=$6::bigint) \
             AND ($7::integer IS NULL OR v.revision=$7::integer) \
             AND iam_role_allows(a.role,a.company_id,$8) AND m.status='active' \
             AND ($9::integer IS NULL OR m.version=$9::integer) \
             AND (m.login_policy IS DISTINCT FROM 'directory' OR (i.enabled=true \
              AND c.status='active' AND COALESCE(i.observed_at,0)>$4-300000)) \
             AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions x WHERE x.session_id=$10 \
              AND x.company_id=$3 AND x.user_id=$2) \
             AND (m.login_policy IS DISTINCT FROM 'emergency' OR ($11::bigint IS NOT NULL \
              AND $11::bigint>$4::bigint)) \
             AND EXISTS(SELECT 1 FROM active_work_contexts w WHERE w.session_id=$10 \
              AND w.company_id=$3 AND w.assignment_id=a.id AND w.assignment_version=a.version \
              AND ($12::bigint IS NULL OR w.revision=$12::bigint)))",
        )
        .bind(actor.assignment_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(actor.company_id.as_str())
        .bind(now)
        .bind(actor.role.as_deref())
        .bind(actor.assignment_version)
        .bind(actor.role_revision)
        .bind(permission)
        .bind(actor.identity_version)
        .bind(actor.session_id.as_str())
        .bind(actor.emergency_until)
        .bind(actor.authority_revision)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn row(
        &mut self,
        company_id: &str,
        id: &str,
        now: i64,
        lock: bool,
    ) -> Result<Option<AssignmentRow>, Error> {
        self.ready().await?;
        let sql = format!(
            "{SELECT} WHERE a.id=$1 AND a.company_id=$2{}",
            if lock { " FOR UPDATE OF a" } else { "" }
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(company_id)
            .bind(now)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_row).transpose().map_err(Into::into)
    }

    async fn receipt_snapshot(&mut self, key: &str) -> Result<Option<ReceiptSnapshot>, Error> {
        self.ready().await?;
        let row = sqlx::query(
            "SELECT request_hash,result_json,operation,actor_id,record_id,authority_json \
             FROM command_receipts WHERE command_id=$1",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok::<_, Error>(ReceiptSnapshot {
                request_hash: row.try_get("request_hash")?,
                result_json: row.try_get("result_json")?,
                operation: row.try_get("operation")?,
                actor_id: row.try_get("actor_id")?,
                record_id: row.try_get("record_id")?,
                authority_json: row
                    .try_get::<Option<String>, _>("authority_json")?
                    .unwrap_or_default(),
            })
        })
        .transpose()
    }

    async fn insert_receipt(
        &mut self,
        actor: &AssignmentActor,
        key: &str,
        plan: &RevocationPlan,
        request_hash: &str,
    ) -> Result<(), Error> {
        let receipt = AssignmentReceipt::revoked(plan.id.clone(), plan.assignment_version);
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(REVOKE_OPERATION)
        .bind(actor.user_id.as_str())
        .bind(request_hash)
        .bind(&plan.id)
        .bind(receipt.to_wire().to_string())
        .bind(plan.revoked_at)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(idempotency_conflict().into());
        }
        Ok(())
    }

    async fn audit(
        &mut self,
        actor: &AssignmentActor,
        key: &str,
        plan: &RevocationPlan,
    ) -> Result<(), Error> {
        let evidence = json!({
            "input": {
                "id": plan.id,
                "expectedVersion": plan.expected_version,
                "reason": plan.reason,
            },
            "result": {
                "id": plan.id,
                "status": "revoked",
                "version": plan.assignment_version,
            },
            "targetUserId": plan.user_id,
            "role": plan.role,
            "scopeKind": plan.scope_kind,
            "scopeNodeId": plan.scope_node_id,
            "authority": serde_json::from_str::<Value>(&actor.authority_json())
                .map_err(|_| readback_invalid())?,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO iam_audit\
             (id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6::jsonb,$7)",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(REVOKE_OPERATION)
        .bind(&plan.id)
        .bind(evidence)
        .bind(plan.revoked_at)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'assignment',$5,$6,$4,$7,$8)",
        )
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(REVOKE_OPERATION)
        .bind(&plan.id)
        .bind(plan.assignment_version)
        .bind(plan.revoked_at)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn outbox(
        &mut self,
        actor: &AssignmentActor,
        key: &str,
        plan: &RevocationPlan,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": plan.id,
            "userId": plan.user_id,
            "role": plan.role,
            "companyId": plan.company_id,
            "branchId": plan.branch_id,
            "departmentId": plan.department_id,
            "scopeKind": plan.scope_kind,
            "scopeNodeId": plan.scope_node_id,
            "status": "revoked",
            "version": plan.assignment_version,
            "revokedAt": plan.revoked_at,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,'assignment',$3,$4,$5,$6)",
        )
        .bind(key)
        .bind(REVOKE_OPERATION)
        .bind(&plan.id)
        .bind(plan.assignment_version)
        .bind(payload)
        .bind(plan.revoked_at)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn readback(&mut self, plan: &RevocationPlan) -> Result<(), Error> {
        let row =
            sqlx::query("SELECT version,revoked_at FROM assignments WHERE id=$1 AND company_id=$2")
                .bind(&plan.id)
                .bind(&plan.company_id)
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(readback_invalid)?;
        if row.try_get::<i64, _>("version")? != plan.assignment_version
            || row.try_get::<Option<i64>, _>("revoked_at")? != Some(plan.revoked_at)
        {
            return Err(readback_invalid().into());
        }
        let remaining = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM active_work_contexts WHERE company_id=$1 AND assignment_id=$2",
        )
        .bind(&plan.company_id)
        .bind(&plan.id)
        .fetch_one(&mut *self.connection)
        .await?;
        if remaining != 0 {
            return Err(readback_invalid().into());
        }
        Ok(())
    }
}

impl AssignmentRepository for PgIdentityAssignments<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        write: bool,
        now: i64,
    ) -> AssignmentFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> AssignmentFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> AssignmentFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move { self.receipt_snapshot(key).await })
    }

    fn workspace_rows<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        now: i64,
    ) -> AssignmentFuture<'a, Vec<AssignmentRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let scope_node = if actor.scope_kind == "holding" {
                None
            } else {
                actor.scope_node_id.as_ref().map(|value| value.as_str())
            };
            let sql = format!(
                "{SELECT} WHERE a.company_id=$1 AND ($2::text IS NULL OR a.scope_node_id=$2::text) \
                 ORDER BY a.user_id,a.role,a.id LIMIT 1001"
            );
            let rows = sqlx::query(AssertSqlSafe(sql))
                .bind(actor.company_id.as_str())
                .bind(scope_node)
                .bind(now)
                .fetch_all(&mut *self.connection)
                .await?;
            rows.into_iter()
                .map(map_row)
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
    }

    fn workspace_roles<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        role_ids: &'a [String],
    ) -> AssignmentFuture<'a, Vec<AssignmentRoleRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT r.id,r.name,r.company_id,r.published_revision::bigint published_revision,r.built_in,r.status,\
                 v.permissions_json::text permissions_json,v.grantable_json::text grantable_json,\
                 v.scopes_json::text scopes_json FROM iam_roles r \
                 JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
                  AND v.revision=r.published_revision \
                 WHERE r.company_id IN ('*',$1) AND r.status='active' AND r.id=ANY($2::text[]) \
                 ORDER BY r.id",
            )
            .bind(actor.company_id.as_str())
            .bind(role_ids)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    let permissions: Vec<String> =
                        serde_json::from_str(&row.try_get::<String, _>("permissions_json")?)
                            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
                    let grantable: Vec<String> =
                        serde_json::from_str(&row.try_get::<String, _>("grantable_json")?)
                            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
                    let scopes: Vec<String> =
                        serde_json::from_str(&row.try_get::<String, _>("scopes_json")?)
                            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
                    Ok(AssignmentRoleRow {
                        id: row.try_get("id")?,
                        name: row.try_get("name")?,
                        company_id: row.try_get("company_id")?,
                        revision: row.try_get("published_revision")?,
                        built_in: row.try_get("built_in")?,
                        permissions,
                        grantable_permissions: grantable,
                        scopes,
                        status: row.try_get("status")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn assignment_for_update<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        id: &'a str,
        now: i64,
    ) -> AssignmentFuture<'a, Option<AssignmentRow>, Self::Error> {
        Box::pin(async move { self.row(actor.company_id.as_str(), id, now, true).await })
    }

    fn guard_facts<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        row: &'a AssignmentRow,
        now: i64,
    ) -> AssignmentFuture<'a, AssignmentGuardFacts, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let facts = sqlx::query(
                "SELECT iam_same_person($1,$2,$3) same_person,\
                 EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=$3 AND m.user_id=$1 \
                  AND m.login_policy='emergency' AND m.status='active') recovery_member,\
                 EXISTS(SELECT 1 FROM iam_members m JOIN assignments a ON a.user_id=m.user_id \
                  AND a.company_id=m.company_id WHERE m.company_id=$3 AND m.user_id<>$1 \
                  AND m.login_policy='emergency' AND m.status='active' AND a.role='holding_owner' \
                  AND a.revoked_at IS NULL AND iam_assignment_active(a.id,$4) \
                  AND iam_principal_active(a.user_id,a.company_id,$4)) other_recovery_administrator,\
                 EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=$3 AND a.user_id<>$1 \
                  AND a.role='holding_owner' AND a.revoked_at IS NULL \
                  AND iam_assignment_active(a.id,$4) \
                  AND iam_principal_active(a.user_id,a.company_id,$4)) other_administrator",
            )
            .bind(&row.user_id)
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(AssignmentGuardFacts {
                same_person: facts.try_get("same_person")?,
                recovery_member: facts.try_get("recovery_member")?,
                other_recovery_administrator: facts.try_get("other_recovery_administrator")?,
                other_administrator: facts.try_get("other_administrator")?,
            })
        })
    }

    fn commit_revoke<'a>(
        &'a mut self,
        actor: &'a AssignmentActor,
        key: &'a str,
        plan: &'a RevocationPlan,
        request_hash: &'a str,
    ) -> AssignmentFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE assignments SET revoked_at=$4,version=version+1 \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND revoked_at IS NULL",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .bind(plan.revoked_at)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(assignment_changed().into());
            }
            sqlx::query(
                "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,\
                 revision=revision+1,updated_at=$3 WHERE company_id=$1 AND assignment_id=$2",
            )
            .bind(&plan.company_id)
            .bind(&plan.id)
            .bind(plan.revoked_at)
            .execute(&mut *self.connection)
            .await?;
            self.insert_receipt(actor, key, plan, request_hash).await?;
            self.audit(actor, key, plan).await?;
            self.outbox(actor, key, plan).await?;
            self.readback(plan).await?;
            Ok(AssignmentReceipt::revoked(plan.id.clone(), plan.assignment_version).to_wire())
        })
    }
}
