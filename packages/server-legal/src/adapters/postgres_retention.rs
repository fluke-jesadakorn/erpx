//! PostgreSQL adapter for the native Legal retention commands.
//!
//! One caller-supplied serializable Holding transaction. The adapter locks the
//! Company for policy changes, locks the current destruction row for decisions,
//! re-checks the item/policy/destruction version, and writes receipt, audit,
//! outbox, item version and event in that same unit.
use crate::{
    application::retention::{
        LegalRetentionPorts, RetentionAssignmentFacts, RetentionDestructionRow,
        RetentionError, RetentionFuture, RetentionPolicyRow,
    },
    domain::{auxiliary::LegalAuxiliaryActor, legal_records::sha256_hex},
};
use crate::application::retention::execute_retention_command;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PersistenceError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::IdentifierSource;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

pub struct PgLegalRetention<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgLegalRetention<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    pub fn connection(&mut self) -> &mut PgConnection {
        self.connection
    }

    async fn ready(&mut self) -> Result<(), RetentionError> {
        require_transaction(self.connection).await
    }
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn short_hash(value: &str) -> String {
    sha256_hex(value.as_bytes())[..32].to_owned()
}

async fn require_transaction(connection: &mut PgConnection) -> Result<(), RetentionError> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}

impl LegalRetentionPorts for PgLegalRetention<'_> {
    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        _operation: &'a str,
        _aggregate: &'a str,
        _raw: &'a Value,
    ) -> RetentionFuture<'a, Option<Value>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
            )
            .bind(key.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else { return Ok(None) };
            let saved: String = row.try_get("request_hash")?;
            if saved != request_hash {
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
        })
    }

    fn item<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: &'a str,
    ) -> RetentionFuture<'a, Option<(String, String, i64)>> {
        Box::pin(async move {
            self.ready().await?;
            let sql = format!(
                "SELECT i.state,i.kind,i.version::bigint version FROM legal_items i \
                 WHERE i.id=$5 AND {}",
                crate::domain::auxiliary::visibility_for(actor, "i")
            );
            let row = sqlx::query(AssertSqlSafe(sql.as_str()))
                .bind(actor.company_id.as_str())
                .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
                .bind(actor.department_id.as_ref().map(|value| value.as_str()))
                .bind(actor.user_id.as_str())
                .bind(id)
                .fetch_optional(&mut *self.connection)
                .await?;
            let Some(row) = row else { return Ok(None) };
            Ok(Some((
                row.try_get("state")?,
                row.try_get("kind")?,
                row.try_get("version")?,
            )))
        })
    }

    fn company_lock<'a>(&'a mut self, company: &'a str) -> RetentionFuture<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("legal:{company}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn create_policy<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: &'a str,
        name: &'a str,
        days: i64,
        basis: &'a str,
        now: i64,
    ) -> RetentionFuture<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            let days = i32::try_from(days)
                .map_err(|_| conflict("LEGAL_REQUEST_INVALID", "Choose a current retention period."))?;
            sqlx::query(
                "INSERT INTO legal_retention_policies(id,company_id,name,days,basis,state,created_by,version,created_at) \
                 VALUES($1,$2,$3,$4,$5,'draft',$6,1,$7)",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(name)
            .bind(days)
            .bind(basis)
            .bind(actor.user_id.as_str())
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn policy<'a>(
        &'a mut self,
        company: &'a str,
        id: Option<&'a str>,
    ) -> RetentionFuture<'a, Option<RetentionPolicyRow>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,state,version::bigint version,created_by,days FROM legal_retention_policies \
                 WHERE company_id=$1 AND ($2::text IS NULL OR id=$2) \
                 ORDER BY created_at DESC,id DESC LIMIT 1",
            )
            .bind(company)
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else { return Ok(None) };
            Ok(Some(RetentionPolicyRow {
                id: row.try_get("id")?,
                state: row.try_get("state")?,
                version: row.try_get("version")?,
                created_by: row.try_get("created_by")?,
                days: i64::from(row.try_get::<i32, _>("days")?),
            }))
        })
    }

    fn decide_policy<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: &'a str,
        version: i64,
        state: &'a str,
    ) -> RetentionFuture<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            let updated = sqlx::query(
                "UPDATE legal_retention_policies SET state=$3,approved_by=$4,version=version+1 \
                 WHERE id=$1 AND company_id=$2 AND version=$5",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(state)
            .bind(actor.user_id.as_str())
            .bind(i32::try_from(version).unwrap_or(i32::MAX))
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(conflict(
                    "LEGAL_VERSION_CONFLICT",
                    "Policy changed. Refresh.",
                )
                .into());
            }
            Ok(())
        })
    }

    fn approved_policy<'a>(
        &'a mut self,
        company: &'a str,
        id: Option<&'a str>,
    ) -> RetentionFuture<'a, Option<(String, i64, i64)>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,version::bigint version,days FROM legal_retention_policies \
                 WHERE company_id=$1 AND ($2::text IS NULL OR id=$2) AND state='approved' \
                 ORDER BY created_at DESC,id DESC LIMIT 1",
            )
            .bind(company)
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else { return Ok(None) };
            Ok(Some((
                row.try_get("id")?,
                row.try_get("version")?,
                i64::from(row.try_get::<i32, _>("days")?),
            )))
        })
    }

    fn assign_retention<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        item: &'a str,
        policy_id: &'a str,
        policy_version: i64,
        days: i64,
        now: i64,
    ) -> RetentionFuture<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            let eligible_at = days
                .checked_mul(86_400_000)
                .and_then(|value| value.checked_add(now))
                .ok_or_else(|| conflict("LEGAL_REQUEST_INVALID", "Choose a current retention period."))?;
            sqlx::query(
                "INSERT INTO legal_retention_assignments(item_id,policy_id,policy_version,eligible_at,assigned_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6) \
                 ON CONFLICT(item_id) DO UPDATE SET policy_id=excluded.policy_id,\
                 policy_version=excluded.policy_version,eligible_at=excluded.eligible_at,\
                 assigned_by=excluded.assigned_by,created_at=excluded.created_at",
            )
            .bind(item)
            .bind(policy_id)
            .bind(i32::try_from(policy_version).unwrap_or(i32::MAX))
            .bind(eligible_at)
            .bind(actor.user_id.as_str())
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn eligible_assignment<'a>(
        &'a mut self,
        item: &'a str,
        now: i64,
    ) -> RetentionFuture<'a, Option<RetentionAssignmentFacts>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT policy_id,policy_version::bigint policy_version \
                 FROM legal_retention_assignments WHERE item_id=$1 AND eligible_at<=$2",
            )
            .bind(item)
            .bind(now)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else { return Ok(None) };
            Ok(Some(RetentionAssignmentFacts {
                policy_id: row.try_get("policy_id")?,
                policy_version: row.try_get("policy_version")?,
            }))
        })
    }

    fn request_destruction<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        item: &'a str,
        policy_id: &'a str,
        policy_version: i64,
        manifest: &'a Value,
        reason: &'a str,
        now: i64,
    ) -> RetentionFuture<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            let id = format!("destruction-{}", short_hash(&format!("{item}:{now}")));
            let manifest = serde_json::to_string(manifest).map_err(|_| {
                conflict("LEGAL_RETENTION_MANIFEST", "The destruction manifest is unavailable.")
            })?;
            sqlx::query(
                "INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at,version) \
                 VALUES($1,$2,$3,$4,$5,'requested',$6,$7::jsonb,$8,1)",
            )
            .bind(id)
            .bind(item)
            .bind(policy_id)
            .bind(i32::try_from(policy_version).unwrap_or(i32::MAX))
            .bind(actor.user_id.as_str())
            .bind(reason)
            .bind(manifest)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn destruction<'a>(
        &'a mut self,
        item: &'a str,
        id: Option<&'a str>,
    ) -> RetentionFuture<'a, Option<RetentionDestructionRow>> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,state,requested_by,approved_by FROM legal_destructions \
                 WHERE item_id=$1 AND (($2::text IS NOT NULL AND id=$2) OR \
                 ($2::text IS NULL AND state NOT IN ('completed','cancelled'))) \
                 ORDER BY created_at DESC,id DESC LIMIT 1 FOR UPDATE",
            )
            .bind(item)
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else { return Ok(None) };
            Ok(Some(RetentionDestructionRow {
                id: row.try_get("id")?,
                state: row.try_get("state")?,
                requested_by: row.try_get("requested_by")?,
                approved_by: row.try_get("approved_by")?,
            }))
        })
    }

    fn change_destruction<'a>(
        &'a mut self,
        id: &'a str,
        state: &'a str,
        actor: &'a LegalAuxiliaryActor,
        now: i64,
    ) -> RetentionFuture<'a, ()> {
        Box::pin(async move {
            self.ready().await?;
            let approved = state == "approved";
            sqlx::query(
                "UPDATE legal_destructions SET state=$2,\
                 approved_by=CASE WHEN $3 THEN $4 ELSE approved_by END,\
                 approved_at=CASE WHEN $3 THEN $5 ELSE approved_at END,\
                 completed_at=CASE WHEN $2='completed' THEN $5 ELSE completed_at END,\
                 error=CASE WHEN $2 IN ('requested','approved') THEN NULL ELSE error END,\
                 version=version+1 WHERE id=$1",
            )
            .bind(id)
            .bind(state)
            .bind(approved)
            .bind(actor.user_id.as_str())
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn active_hold<'a>(&'a mut self, item: &'a str) -> RetentionFuture<'a, bool> {
        Box::pin(async move {
            self.ready().await?;
            let held = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM legal_holds h WHERE h.state<>'released' AND \
                 (h.item_id=$1 OR EXISTS(SELECT 1 FROM legal_hold_targets t \
                 WHERE t.hold_id=h.id AND t.item_id=$1)))",
            )
            .bind(item)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(held)
        })
    }

    fn manifest<'a>(
        &'a mut self,
        item: &'a str,
        company: &'a str,
        now: i64,
    ) -> RetentionFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let text = sqlx::query_scalar::<_, String>(
                "SELECT jsonb_build_object(\
                 'itemId',$1,'companyId',$2,'generatedAt',$3,\
                 'documents',COALESCE((SELECT jsonb_agg(jsonb_build_object(\
                   'id',d.id,'version',d.version,'checksum',d.checksum,'mediaType',d.media_type,\
                   'bytes',d.bytes,'objectKey',d.object_key,\
                   'attachmentLinkId',d.attachment_link_id,\
                   'attachmentSourceId',d.attachment_source_id) ORDER BY d.version) \
                   FROM legal_documents d WHERE d.item_id=$1),'[]'::jsonb),\
                 'preservations',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.object_key) \
                   FROM legal_file_preservations p WHERE p.item_id=$1),'[]'::jsonb))::text",
            )
            .bind(item)
            .bind(company)
            .bind(now)
            .fetch_one(&mut *self.connection)
            .await?;
            serde_json::from_str(&text).map_err(|_| {
                conflict(
                    "LEGAL_RETENTION_MANIFEST",
                    "The destruction manifest is unavailable.",
                )
                .into()
            })
        })
    }

    fn commit<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        operation: &'a str,
        aggregate: &'a str,
        version: i64,
        expected_version: Option<i64>,
        item: Option<&'a str>,
        event_id: &'a str,
        note: &'a str,
        result: Value,
        now: i64,
    ) -> RetentionFuture<'a, Value> {
        Box::pin(async move {
            self.ready().await?;
            let authority = serde_json::to_string(&json!({
                "companyId": actor.company_id.as_str(),
                "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
                "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
                "role": actor.role,
                "userId": actor.user_id.as_str(),
                "assignmentId": actor.assignment_id.as_str(),
            }))
            .map_err(|_| {
                conflict(
                    "LEGAL_RECEIPT_UNAVAILABLE",
                    "The Legal command receipt is unavailable.",
                )
            })?;
            let inserted = sqlx::query(
                "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (command_id) DO NOTHING",
            )
            .bind(key.as_str())
            .bind(operation)
            .bind(actor.user_id.as_str())
            .bind(request_hash)
            .bind(aggregate)
            .bind(result.to_string())
            .bind(now)
            .bind(&authority)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if inserted != 1 {
                return Err(conflict(
                    "IDEMPOTENCY_CONFLICT",
                    "This command key belongs to a different request.",
                )
                .into());
            }
            sqlx::query(
                "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
                 VALUES($1,$2,$3,$4,$5,'legal',$6,$7,$5,$8,$9)",
            )
            .bind(format!("legal-audit-{}", short_hash(key.as_str())))
            .bind(key.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.assignment_id.as_str())
            .bind(operation)
            .bind(aggregate)
            .bind(version)
            .bind(now)
            .bind(&authority)
            .execute(&mut *self.connection)
            .await?;
            let payload = serde_json::to_string(&json!({
                "id": aggregate,
                "version": version,
                "companyId": actor.company_id.as_str(),
                "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
                "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
                "result": result,
            }))
            .map_err(|_| {
                conflict(
                    "LEGAL_RECEIPT_UNAVAILABLE",
                    "The Legal command outbox is unavailable.",
                )
            })?;
            sqlx::query(
                "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
                 VALUES($1,$2,$3,'legal',$4,$5,$6,$7)",
            )
            .bind(format!("legal-outbox-{}", short_hash(key.as_str())))
            .bind(key.as_str())
            .bind(operation)
            .bind(aggregate)
            .bind(version)
            .bind(payload)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            if let Some(item) = item {
                let expected = expected_version.ok_or_else(|| {
                    conflict(
                        "LEGAL_VERSION_CONFLICT",
                        "The Legal record changed. Refresh before saving.",
                    )
                })?;
                let updated = sqlx::query(
                    "UPDATE legal_items SET version=version+1,updated_at=$3 \
                     WHERE id=$1 AND version=$2",
                )
                .bind(item)
                .bind(i32::try_from(expected).unwrap_or(i32::MAX))
                .bind(now)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if updated != 1 {
                    return Err(conflict(
                        "LEGAL_VERSION_CONFLICT",
                        "The Legal record changed. Refresh before saving.",
                    )
                    .into());
                }
                sqlx::query(
                    "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7)",
                )
                .bind(event_id)
                .bind(item)
                .bind(operation)
                .bind(actor.user_id.as_str())
                .bind(note)
                .bind(i32::try_from(version).unwrap_or(i32::MAX))
                .bind(now)
                .execute(&mut *self.connection)
                .await?;
            }
            Ok(result)
        })
    }
}

/// Composition entry point: one serializable Holding transaction, replay first,
/// then the owner rules, receipt/audit/outbox and persisted effects.
pub async fn execute_retention(
    pool: &DatabasePool,
    actor: &LegalAuxiliaryActor,
    key: &CommandKey,
    request_hash: &str,
    raw: &Value,
    now: i64,
) -> Result<Value, RetentionError> {
    let pool = pool.clone();
    let actor = actor.clone();
    let key = key.clone();
    let request_hash = request_hash.to_owned();
    let raw = raw.clone();
    PgUnitOfWork::new(pool)
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let key = key.clone();
            let request_hash = request_hash.clone();
            let raw = raw.clone();
            Box::pin(async move {
                let mut ports = PgLegalRetention::new(connection);
                let mut identifiers = CommandIdentifiers {
                    seed: format!("{}:{now}", key.as_str()),
                    counter: 0,
                };
                execute_retention_command(
                    &mut ports,
                    &actor,
                    &key,
                    &request_hash,
                    &raw,
                    &mut identifiers,
                    now,
                )
                .await
            })
        })
        .await
}

struct CommandIdentifiers {
    seed: String,
    counter: u64,
}

impl IdentifierSource for CommandIdentifiers {
    fn next(&mut self) -> RecordId {
        self.counter += 1;
        let digest = sha256_hex(format!("{}:{}", self.seed, self.counter).as_bytes());
        RecordId::new(format!("legal-event-{}", &digest[..40]))
            .unwrap_or_else(|_| unreachable!("derived identifier is valid"))
    }
}

/// Capture-free construction helper for composition. The connection must
/// already be inside the caller's serializable Legal/Holding transaction.
pub fn legal_retention(connection: &mut PgConnection) -> PgLegalRetention<'_> {
    PgLegalRetention::new(connection)
}

/// Map an owner error for the transport.
pub fn legal_retention_business_error(error: RetentionError) -> BusinessError {
    match error {
        erpx_server_runtime::public::postgres::PgTransactionError::Business(error) => error,
        erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_) => {
            BusinessError::new(
                FailureKind::Unavailable,
                "LEGAL_UNAVAILABLE",
                "The Legal service is unavailable.",
            )
        }
    }
}
