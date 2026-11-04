//! PostgreSQL adapter for Workflow approval oversight.
//!
//! Reads only Workflow-owned approval evidence (`approval_instances`,
//! `approval_steps`, `approval_decisions`) and the shared, immutable
//! `command_receipts`, `audit_events` and `outbox_events` rows linked from the
//! approval decisions. No business-record, Identity, People or Organization row
//! is read or written. The caller owns the serializable Holding transaction.
use crate::application::approval_oversight::{
    ApprovalOversightFuture, ApprovalOversightRepository,
};
use crate::domain::approval_oversight::{
    ApprovalAuditEvent, ApprovalCommandReceipt, ApprovalEvidence, ApprovalEvidenceDecision,
    ApprovalEvidenceInstance, ApprovalEvidenceQuery, ApprovalEvidenceStep, ApprovalOutboxEvent,
    ApprovalOversightActor, ApprovalQueueRow, MAX_AUDIT, MAX_HISTORY, MAX_OUTBOX, MAX_QUEUE,
    MAX_RECEIPTS, MAX_STEPS, MAX_TEXT, source_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};
use std::collections::BTreeSet;

type Error = PgTransactionError<BusinessError>;

const QUEUE: &str = "SELECT id,company_id,branch_id,department_id,record_type,record_id,state,\
 CASE WHEN blocked_reason IS NULL THEN NULL WHEN octet_length(blocked_reason)<=$2 THEN blocked_reason END blocked_reason,\
 COALESCE(octet_length(blocked_reason),0)>$2 blocked_oversized,version,created_at \
 FROM approval_instances \
 WHERE company_id=$1 AND state IN ('pending','blocked') \
 AND record_type IN ('expense','leave','leave_cancellation','task','overtime') \
 ORDER BY created_at DESC,insertion_order DESC LIMIT $3";

const INSTANCE: &str = "SELECT id,company_id,branch_id,department_id,record_type,record_id,phase,cycle,\
 requester_user_id,assignee_user_id,state,current_step,\
 CASE WHEN blocked_reason IS NULL THEN NULL WHEN octet_length(blocked_reason)<=$4 THEN blocked_reason END blocked_reason,\
 COALESCE(octet_length(blocked_reason),0)>$4 blocked_oversized,version,created_at,updated_at \
 FROM approval_instances \
 WHERE company_id=$1 AND record_type=$2 AND record_id=$3 \
 ORDER BY insertion_order DESC LIMIT 1";

const STEPS: &str = "SELECT id,instance_id,ordinal,approver_user_id,authority,state,\
 CASE WHEN approver_pool_json IS NULL THEN NULL WHEN octet_length(approver_pool_json)<=$2 THEN approver_pool_json END approver_pool_json,\
 CASE WHEN route_snapshot_json IS NULL THEN NULL WHEN octet_length(route_snapshot_json)<=$2 THEN route_snapshot_json END route_snapshot_json,\
 (COALESCE(octet_length(approver_pool_json),0)>$2 OR COALESCE(octet_length(route_snapshot_json),0)>$2) oversized,\
 required_role,stage \
 FROM approval_steps WHERE instance_id=$1 ORDER BY ordinal,id LIMIT $3";

const HISTORY: &str = "SELECT id,instance_id,step_id,action,actor_user_id,previous_approver_user_id,\
 approver_user_id,\
 CASE WHEN octet_length(reason)<=$2 THEN reason END reason,\
 octet_length(reason)>$2 oversized,reviewed_version,command_id,created_at \
 FROM approval_decisions WHERE instance_id=$1 \
 ORDER BY created_at,insertion_order,id LIMIT $3";

const RECEIPTS: &str = "SELECT command_id,operation,actor_id,record_id,request_hash,\
 CASE WHEN result_json IS NULL THEN NULL WHEN octet_length(result_json)<=$2 THEN result_json END result_json,\
 COALESCE(octet_length(result_json),0)>$2 oversized,created_at \
 FROM command_receipts WHERE command_id=ANY($1) ORDER BY created_at,command_id LIMIT $3";

const AUDIT: &str = "SELECT id,command_id,actor_id,assignment_id,action,record_type,record_id,\
 record_version,purpose,created_at FROM audit_events \
 WHERE command_id=ANY($1) ORDER BY created_at,id LIMIT $2";

const OUTBOX: &str = "SELECT id,command_id,event_type,aggregate_type,aggregate_id,\
 aggregate_version,created_at FROM outbox_events \
 WHERE command_id=ANY($1) ORDER BY created_at,id LIMIT $2";

pub struct PgApprovalOversight<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgApprovalOversight<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("APPROVAL_EVIDENCE_TRANSACTION_REQUIRED").into());
        }
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new("APPROVAL_EVIDENCE_SERIALIZABLE_REQUIRED").into());
        }
        sqlx::query(
            "LOCK TABLE approval_instances,approval_steps,approval_decisions,\
             command_receipts,audit_events,outbox_events IN SHARE MODE",
        )
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn queue_rows(&mut self, company: &str) -> Result<Vec<ApprovalQueueRow>, Error> {
        self.ready().await?;
        let rows = sqlx::query(QUEUE)
            .bind(company)
            .bind(MAX_TEXT as i32)
            .bind(MAX_QUEUE as i64 + 1)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| {
                if row.try_get::<bool, _>("blocked_oversized")? {
                    return Err(source_invalid().into());
                }
                Ok(ApprovalQueueRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    record_type: row.try_get("record_type")?,
                    record_id: row.try_get("record_id")?,
                    state: row.try_get("state")?,
                    blocked_reason: row.try_get("blocked_reason")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn instance(
        &mut self,
        company: &str,
        kind: &str,
        record_id: &str,
    ) -> Result<Option<ApprovalEvidenceInstance>, Error> {
        let row = sqlx::query(INSTANCE)
            .bind(company)
            .bind(kind)
            .bind(record_id)
            .bind(MAX_TEXT as i32)
            .fetch_optional(&mut *self.connection)
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.try_get::<bool, _>("blocked_oversized")? {
            return Err(source_invalid().into());
        }
        Ok(Some(ApprovalEvidenceInstance {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            department_id: row.try_get("department_id")?,
            record_type: row.try_get("record_type")?,
            record_id: row.try_get("record_id")?,
            phase: row.try_get("phase")?,
            cycle: row.try_get("cycle")?,
            requester_user_id: row.try_get("requester_user_id")?,
            assignee_user_id: row.try_get("assignee_user_id")?,
            state: row.try_get("state")?,
            current_step: row.try_get("current_step")?,
            blocked_reason: row.try_get("blocked_reason")?,
            version: row.try_get("version")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        }))
    }

    async fn steps(&mut self, instance_id: &str) -> Result<Vec<ApprovalEvidenceStep>, Error> {
        let rows = sqlx::query(STEPS)
            .bind(instance_id)
            .bind(MAX_TEXT as i32)
            .bind(MAX_STEPS as i64 + 1)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| {
                if row.try_get::<bool, _>("oversized")? {
                    return Err(source_invalid().into());
                }
                Ok(ApprovalEvidenceStep {
                    id: row.try_get("id")?,
                    instance_id: row.try_get("instance_id")?,
                    ordinal: row.try_get("ordinal")?,
                    approver_user_id: row.try_get("approver_user_id")?,
                    authority: row.try_get("authority")?,
                    state: row.try_get("state")?,
                    approver_pool_json: row.try_get("approver_pool_json")?,
                    required_role: row.try_get("required_role")?,
                    stage: row.try_get("stage")?,
                    route_snapshot_json: row.try_get("route_snapshot_json")?,
                })
            })
            .collect()
    }

    async fn history(&mut self, instance_id: &str) -> Result<Vec<ApprovalEvidenceDecision>, Error> {
        let rows = sqlx::query(HISTORY)
            .bind(instance_id)
            .bind(MAX_TEXT as i32)
            .bind(MAX_HISTORY as i64 + 1)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| {
                if row.try_get::<bool, _>("oversized")? {
                    return Err(source_invalid().into());
                }
                Ok(ApprovalEvidenceDecision {
                    id: row.try_get("id")?,
                    instance_id: row.try_get("instance_id")?,
                    step_id: row.try_get("step_id")?,
                    action: row.try_get("action")?,
                    actor_user_id: row.try_get("actor_user_id")?,
                    previous_approver_user_id: row.try_get("previous_approver_user_id")?,
                    approver_user_id: row.try_get("approver_user_id")?,
                    reason: row.try_get("reason")?,
                    reviewed_version: row.try_get("reviewed_version")?,
                    command_id: row.try_get("command_id")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn receipts(&mut self, ids: &[String]) -> Result<Vec<ApprovalCommandReceipt>, Error> {
        let rows = sqlx::query(RECEIPTS)
            .bind(ids)
            .bind(MAX_TEXT as i32)
            .bind(MAX_RECEIPTS as i64 + 1)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| {
                if row.try_get::<bool, _>("oversized")? {
                    return Err(source_invalid().into());
                }
                Ok(ApprovalCommandReceipt {
                    command_id: row.try_get("command_id")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn audit(&mut self, ids: &[String]) -> Result<Vec<ApprovalAuditEvent>, Error> {
        let rows = sqlx::query(AUDIT)
            .bind(ids)
            .bind(MAX_AUDIT as i64 + 1)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ApprovalAuditEvent {
                    id: row.try_get("id")?,
                    command_id: row.try_get("command_id")?,
                    actor_id: row.try_get("actor_id")?,
                    assignment_id: row.try_get("assignment_id")?,
                    action: row.try_get("action")?,
                    record_type: row.try_get("record_type")?,
                    record_id: row.try_get("record_id")?,
                    record_version: row.try_get("record_version")?,
                    purpose: row.try_get("purpose")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn outbox(&mut self, ids: &[String]) -> Result<Vec<ApprovalOutboxEvent>, Error> {
        let rows = sqlx::query(OUTBOX)
            .bind(ids)
            .bind(MAX_OUTBOX as i64 + 1)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ApprovalOutboxEvent {
                    id: row.try_get("id")?,
                    command_id: row.try_get("command_id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_type: row.try_get("aggregate_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_version: row.try_get("aggregate_version")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }
}

impl ApprovalOversightRepository for PgApprovalOversight<'_> {
    type Error = Error;

    fn open_queue<'a>(
        &'a mut self,
        actor: &'a ApprovalOversightActor,
        _: i64,
    ) -> ApprovalOversightFuture<'a, Vec<ApprovalQueueRow>, Self::Error> {
        Box::pin(async move { self.queue_rows(actor.company_id.as_str()).await })
    }

    fn evidence<'a>(
        &'a mut self,
        actor: &'a ApprovalOversightActor,
        query: &'a ApprovalEvidenceQuery,
        _: i64,
    ) -> ApprovalOversightFuture<'a, Option<ApprovalEvidence>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let Some(instance) = self
                .instance(
                    actor.company_id.as_str(),
                    query.kind.as_str(),
                    query.record_id.as_str(),
                )
                .await?
            else {
                return Ok(None);
            };
            let steps = self.steps(instance.id.as_str()).await?;
            if steps.len() > MAX_STEPS {
                return Err(source_invalid().into());
            }
            let history = self.history(instance.id.as_str()).await?;
            if history.len() > MAX_HISTORY {
                return Err(source_invalid().into());
            }
            let command_ids = history
                .iter()
                .map(|decision| decision.command_id.clone())
                .collect::<BTreeSet<_>>();
            if command_ids.len() > MAX_RECEIPTS {
                return Err(source_invalid().into());
            }
            let ids = command_ids.into_iter().collect::<Vec<_>>();
            let (receipts, audit, outbox) = if ids.is_empty() {
                (Vec::new(), Vec::new(), Vec::new())
            } else {
                (
                    self.receipts(&ids).await?,
                    self.audit(&ids).await?,
                    self.outbox(&ids).await?,
                )
            };
            if receipts.len() > MAX_RECEIPTS || audit.len() > MAX_AUDIT || outbox.len() > MAX_OUTBOX
            {
                return Err(source_invalid().into());
            }
            Ok(Some(ApprovalEvidence {
                instance,
                steps,
                history,
                receipts,
                audit,
                outbox,
            }))
        })
    }
}
