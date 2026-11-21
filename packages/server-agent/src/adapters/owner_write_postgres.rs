use crate::application::owner_write::{OwnerWriteCommand, OwnerWritePort};
use crate::domain::owner_write_registry::{
    AG_INVOCATION_EVIDENCE_INSERT, AG_PRIVATE_SESSION_INSERT, AG_PROVIDER_USAGE_INSERT,
    AG_SESSION_CONTROLS_INSERT, AG_WORKSPACE_TURN_ENGLISH, AG_WORKSPACE_TURN_FAILED,
    AG_WORKSPACE_TURN_INSERT, AG_WORKSPACE_TURN_READY,
    AG_WORKSPACE_TURN_TRANSLATION_FAILED, AG_WORKSPACE_TURN_TRANSLATION_READY,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use sqlx::Connection;
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{Database, PgConnection, Postgres, query::Query};

pub type OwnerWriteError = PgTransactionError<BusinessError>;

type BoundQuery<'q> = Query<'q, Postgres, <Postgres as Database>::Arguments>;

pub struct PgOwnerWrite<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgOwnerWrite<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn sql(kind: &str) -> Result<&'static str, BusinessError> {
        Ok(match kind {
            AG_PRIVATE_SESSION_INSERT =>
                "INSERT INTO ai_private_sessions(id,company_id,user_id,assignment_id,scope_key,conversation_key,agent_assignment_id,created_at,last_used_at,generation) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(company_id,user_id,assignment_id,scope_key,conversation_key,generation) DO NOTHING",
            AG_SESSION_CONTROLS_INSERT =>
                "INSERT INTO ai_private_session_controls(session_id,expires_at,last_used_at,updated_at) VALUES($1,$2,$3,$4) ON CONFLICT(session_id) DO NOTHING",
            AG_INVOCATION_EVIDENCE_INSERT =>
                "INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,input_tokens,output_tokens,error,created_at,governance_snapshot_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT(turn_id) DO NOTHING",
            AG_PROVIDER_USAGE_INSERT =>
                "INSERT INTO ai_provider_usage(id,company_id,connection_id,revision,model,feature,status,input_tokens,output_tokens,created_at,branch_id,funding_source) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(id) DO NOTHING",
            AG_WORKSPACE_TURN_INSERT =>
                "INSERT INTO ai_workspace_turns(id,company_id,user_id,assignment_id,scope_key,conversation_key,question,paths_json,state,created_at,updated_at,private_session_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'running',$9,$10,$11)",
            AG_WORKSPACE_TURN_READY =>
                "UPDATE ai_workspace_turns SET state='ready',answer_json=$1,sources_json=$2,updated_at=$3 WHERE id=$4 AND state='running'",
            AG_WORKSPACE_TURN_FAILED =>
                "UPDATE ai_workspace_turns SET state='failed',error=$1,updated_at=$2 WHERE id=$3 AND state='running'",
            AG_WORKSPACE_TURN_ENGLISH =>
                "UPDATE ai_workspace_turns SET english_answer_json=$1 WHERE id=$2 AND company_id=$3 AND user_id=$4",
            AG_WORKSPACE_TURN_TRANSLATION_READY =>
                "UPDATE ai_workspace_turns SET state='ready',answer_json=$1,updated_at=$2 WHERE id=$3",
            AG_WORKSPACE_TURN_TRANSLATION_FAILED =>
                "UPDATE ai_workspace_turns SET state='failed',updated_at=$1 WHERE id=$2",
            _ => return Err(BusinessError::invalid(
                "AGENT_OWNER_WRITE_INVALID", "Unsupported owner write kind.")),
        })
    }
}

fn bind_one<'q>(query: BoundQuery<'q>, value: &'q Value) -> BoundQuery<'q> {
    match value {
        Value::Null => query.bind(Option::<String>::None),
        Value::Bool(value) => query.bind(*value),
        Value::Number(number) if number.is_i64() => query.bind(number.as_i64().unwrap_or(0)),
        Value::Number(number) if number.is_u64() => {
            query.bind(number.as_u64().unwrap_or(0) as i64)
        }
        Value::Number(number) => query.bind(number.as_f64().unwrap_or(0.0)),
        Value::String(value) => query.bind(value.clone()),
        other => query.bind(other.to_string()),
    }
}

fn bind_all<'q>(mut query: BoundQuery<'q>, values: &'q [Value]) -> BoundQuery<'q> {
    for value in values {
        query = bind_one(query, value);
    }
    query
}

impl OwnerWritePort for PgOwnerWrite<'_> {
    type Error = OwnerWriteError;

    async fn execute(
        &mut self,
        command: &OwnerWriteCommand<'_>,
        key: &str,
    ) -> Result<Value, Self::Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("AGENT_OWNER_WRITE_TRANSACTION_REQUIRED").into());
        }
        let sql = Self::sql(command.kind).map_err(OwnerWriteError::from)?;
        bind_all(sqlx::query(sql), command.values)
            .execute(&mut *self.connection)
            .await?;

        let receipt_id = format!("agent-owner-write:{}:{}", command.kind, key);
        let audit_id = format!("agent-owner-write-audit:{}:{}", command.kind, key);
        let outbox_id = format!("agent-owner-write-outbox:{}:{}", command.kind, key);
        let result = json!({"kind": command.kind, "companyId": command.company_id});
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(&receipt_id)
        .bind(command.kind)
        .bind(command.actor_id)
        .bind(key)
        .bind(command.company_id)
        .bind(&result)
        .bind(command.now)
        .bind(json!({}))
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&audit_id)
        .bind(&receipt_id)
        .bind(command.actor_id)
        .bind("")
        .bind(command.kind)
        .bind("owner_write")
        .bind(command.company_id)
        .bind(1_i64)
        .bind("Agent native owner write")
        .bind(command.now)
        .bind(json!({}))
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&outbox_id)
        .bind(&receipt_id)
        .bind(format!("{}.written", command.kind))
        .bind("owner_write")
        .bind(command.company_id)
        .bind(1_i64)
        .bind(&result)
        .bind(command.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(result)
    }
}
