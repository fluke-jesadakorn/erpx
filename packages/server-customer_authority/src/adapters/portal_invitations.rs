//! PostgreSQL adapter for the native Customer Portal invitation commands.
//!
//! SQL is Customer Authority-owned (`customer_portal_invitations`,
//! `customer_portal_invitation_events`) over the approved
//! `customer_access_bindings` and the original Commerce order. The caller owns
//! the serializable Holding transaction; this adapter never opens or commits it.
use crate::application::portal_invitations::{
    InvitationFuture, InvitationReceiptSnapshot, InvitationRepository,
};
use crate::domain::access::AccessActor;
use crate::domain::portal_invitations::{
    ActiveInvitation, InvitationBinding, InvitationHistory, InvitationPlan, InvitationReceipt,
    issued_event_id, readback_invalid, receipt, supersede_event_id,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgPortalInvitation<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPortalInvitation<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    fn ready(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(
                PersistenceError::new("CUSTOMER_PORTAL_INVITATION_TRANSACTION_REQUIRED").into(),
            );
        }
        Ok(())
    }

    async fn event(
        &mut self,
        id: String,
        invitation_id: &str,
        action: &str,
        actor_id: &str,
        reason: &str,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO customer_portal_invitation_events\
             (id,invitation_id,action,actor_id,reason,created_at) VALUES($1,$2,$3,$4,$5,$6)",
        )
        .bind(id)
        .bind(invitation_id)
        .bind(action)
        .bind(actor_id)
        .bind(reason)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn audit(
        &mut self,
        actor: &AccessActor,
        key: &str,
        operation: &str,
        binding_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'customer_access_binding',$5,$6,$4,$7,$8)",
        )
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(operation)
        .bind(binding_id)
        .bind(version)
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn outbox(
        &mut self,
        actor: &AccessActor,
        key: &str,
        operation: &str,
        binding_id: &str,
        order_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": binding_id,
            "state": "queued",
            "orderId": order_id,
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,'customer_access_binding',$3,$4,$5,$6)",
        )
        .bind(key)
        .bind(operation)
        .bind(binding_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl InvitationRepository for PgPortalInvitation<'_> {
    type Error = Error;

    fn binding<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        binding_id: &'a str,
    ) -> InvitationFuture<'a, Option<InvitationBinding>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let branch = actor.branch_id.as_ref().map(|value| value.as_str());
            let department = actor.department_id.as_ref().map(|value| value.as_str());
            let row = sqlx::query(
                "SELECT b.id,b.company_id,o.branch_id,o.department_id,b.email,b.status,b.order_id,\
                 o.state order_state,b.expires_at,b.version,b.created_by,\
                 EXISTS(SELECT 1 FROM customer_access_identities identity \
                   WHERE identity.binding_id=b.id) identity_exists \
                 FROM customer_access_bindings b \
                 JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id \
                   AND o.party_id=b.customer_id \
                 WHERE b.id=$1 AND b.company_id=$2 \
                   AND ($3::text IS NULL OR o.branch_id=$3) \
                   AND ($4::text IS NULL OR o.department_id=$4) \
                   AND o.kind IN ('quote','sales') \
                 FOR UPDATE OF b",
            )
            .bind(binding_id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(department)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(InvitationBinding {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    email: row.try_get("email")?,
                    status: row.try_get("status")?,
                    order_id: row.try_get("order_id")?,
                    order_state: row.try_get("order_state")?,
                    expires_at: row.try_get("expires_at")?,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                    identity_exists: row.try_get("identity_exists")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn active<'a>(
        &'a mut self,
        binding_id: &'a str,
    ) -> InvitationFuture<'a, Option<ActiveInvitation>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let row = sqlx::query(
                "SELECT id,generation,state,token_expires_at FROM customer_portal_invitations \
                 WHERE binding_id=$1 AND state IN ('queued','running','sent','accepted') \
                 ORDER BY generation DESC LIMIT 1 FOR UPDATE",
            )
            .bind(binding_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(ActiveInvitation {
                    id: row.try_get("id")?,
                    generation: i64::from(row.try_get::<i32, _>("generation")?),
                    state: row.try_get("state")?,
                    token_expires_at: row.try_get("token_expires_at")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> InvitationFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready()?;
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
    ) -> InvitationFuture<'a, Option<InvitationReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(InvitationReceiptSnapshot {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                })
            })
            .transpose()
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn create<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        operation: &'a str,
        request_hash: &'a str,
        plan: &'a InvitationPlan,
        binding_id: &'a str,
        expected_version: i64,
    ) -> InvitationFuture<'a, InvitationReceipt, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let actor_id = actor.user_id.as_str().to_owned();
            if let Some(previous) = plan.previous.as_ref() {
                let terminal = plan.terminal.unwrap_or("superseded");
                sqlx::query(
                    "UPDATE customer_portal_invitations SET state=$1,lease_token=NULL,\
                     lease_until=NULL,last_error=$2 \
                     WHERE id=$3 AND state IN ('queued','running','sent')",
                )
                .bind(terminal)
                .bind(terminal.to_uppercase())
                .bind(&previous.id)
                .execute(&mut *self.connection)
                .await?;
                self.event(
                    supersede_event_id(),
                    &previous.id,
                    terminal,
                    &actor_id,
                    &plan.reason,
                    plan.now,
                )
                .await?;
            }
            sqlx::query(
                "INSERT INTO customer_portal_invitations\
                 (id,binding_id,holding_slug,generation,email,token_hash,token_key_version,\
                 token_expires_at,state,attempts,next_attempt_at,created_by,created_at,command_id) \
                 VALUES($1,$2,$3,$4,$5,$6,1,$7,'queued',0,$8,$9,$8,$10)",
            )
            .bind(&plan.invitation_id)
            .bind(binding_id)
            .bind(&plan.holding_slug)
            .bind(plan.generation)
            .bind(&plan.email)
            .bind(&plan.token_hash)
            .bind(plan.token_expires_at)
            .bind(plan.now)
            .bind(&actor_id)
            .bind(key)
            .execute(&mut *self.connection)
            .await?;
            self.event(
                issued_event_id(key),
                &plan.invitation_id,
                "issued",
                &actor_id,
                &plan.reason,
                plan.now,
            )
            .await?;
            let receipt = receipt(plan, binding_id, expected_version);
            let changed = sqlx::query(
                "INSERT INTO command_receipts\
                 (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,\
                 authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT(command_id) DO NOTHING",
            )
            .bind(key)
            .bind(operation)
            .bind(&actor_id)
            .bind(request_hash)
            .bind(binding_id)
            .bind(serde_json::to_string(&receipt).unwrap_or_else(|_| "{}".into()))
            .bind(plan.now)
            .bind(actor.authority_json())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(crate::domain::portal_invitations::idempotency_conflict().into());
            }
            self.audit(
                actor,
                key,
                operation,
                binding_id,
                expected_version,
                plan.now,
            )
            .await?;
            let order_id: String =
                sqlx::query_scalar("SELECT order_id FROM customer_access_bindings WHERE id=$1")
                    .bind(binding_id)
                    .fetch_one(&mut *self.connection)
                    .await?;
            self.outbox(
                actor,
                key,
                operation,
                binding_id,
                &order_id,
                expected_version,
                plan.now,
            )
            .await?;
            let readback = sqlx::query(
                "SELECT binding_id,holding_slug,generation,email,token_hash,token_expires_at,state \
                 FROM customer_portal_invitations WHERE id=$1",
            )
            .bind(&plan.invitation_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(readback) = readback else {
                return Err(readback_invalid().into());
            };
            if readback.try_get::<String, _>("binding_id")? != binding_id
                || readback.try_get::<String, _>("holding_slug")? != plan.holding_slug
                || readback.try_get::<i32, _>("generation")? as i64 != plan.generation
                || readback.try_get::<String, _>("email")? != plan.email
                || readback.try_get::<String, _>("token_hash")? != plan.token_hash
                || readback.try_get::<i64, _>("token_expires_at")? != plan.token_expires_at
                || readback.try_get::<String, _>("state")? != "queued"
            {
                return Err(readback_invalid().into());
            }
            Ok(receipt)
        })
    }

    fn history<'a>(
        &'a mut self,
        binding_id: &'a str,
    ) -> InvitationFuture<'a, InvitationHistory, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let invitations = sqlx::query(
                "SELECT id,binding_id,generation,holding_slug,email,state,token_expires_at,\
                 attempts,next_attempt_at,last_error,created_by,created_at,sent_at,accepted_at,\
                 accepted_user_id \
                 FROM customer_portal_invitations WHERE binding_id=$1 ORDER BY generation DESC",
            )
            .bind(binding_id)
            .fetch_all(&mut *self.connection)
            .await?;
            let events = sqlx::query(
                "SELECT event.id,event.invitation_id,event.action,event.actor_id,event.reason,\
                 event.created_at \
                 FROM customer_portal_invitation_events event \
                 JOIN customer_portal_invitations invitation ON invitation.id=event.invitation_id \
                 WHERE invitation.binding_id=$1 ORDER BY event.created_at,event.id",
            )
            .bind(binding_id)
            .fetch_all(&mut *self.connection)
            .await?;
            Ok(InvitationHistory {
                invitations: invitations.iter().map(invitation_wire).collect(),
                invitation_events: events.iter().map(event_wire).collect(),
            })
        })
    }
}

fn invitation_wire(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<String, _>("id"),
        "binding_id": row.get::<String, _>("binding_id"),
        "generation": row.get::<i32, _>("generation"),
        "holding_slug": row.get::<String, _>("holding_slug"),
        "email": row.get::<String, _>("email"),
        "state": row.get::<String, _>("state"),
        "token_expires_at": row.get::<i64, _>("token_expires_at"),
        "attempts": row.get::<i32, _>("attempts"),
        "next_attempt_at": row.get::<i64, _>("next_attempt_at"),
        "last_error": row.get::<Option<String>, _>("last_error"),
        "created_by": row.get::<String, _>("created_by"),
        "created_at": row.get::<i64, _>("created_at"),
        "sent_at": row.get::<Option<i64>, _>("sent_at"),
        "accepted_at": row.get::<Option<i64>, _>("accepted_at"),
        "accepted_user_id": row.get::<Option<String>, _>("accepted_user_id"),
    })
}

fn event_wire(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": row.get::<String, _>("id"),
        "invitation_id": row.get::<String, _>("invitation_id"),
        "action": row.get::<String, _>("action"),
        "actor_id": row.get::<String, _>("actor_id"),
        "reason": row.get::<String, _>("reason"),
        "created_at": row.get::<i64, _>("created_at"),
    })
}
