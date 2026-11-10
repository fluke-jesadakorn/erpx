//! Owner-side workflow bridge for the native Legal trust command.
//!
//! The existing `execute_legal_trust_command` deliberately requires explicit
//! Identity and Workflow bridges and cannot infer them. This module supplies the
//! **Workflow** half (replay, receipt, audit, outbox) with exact owner SQL and
//! leaves only the Identity facts abstract. Composition still supplies:
//!
//! 1. the normal Account session actor and its current assignment id,
//! 2. `LegalTrustIdentityBridge` over the current IAM/owner authority,
//! 3. the serializable Holding connection.
//!
//! No Identity, Workflow or composition crate is imported, and the offline PDF
//! trust/revocation/timestamp/modification three-state verifier is untouched.
use std::future::Future;

use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, FailureKind, UserId,
};
use serde_json::json;
use sqlx::{Connection, PgConnection, Row};

use super::postgres_trust::PgLegalTrustCommitAuthority;
use super::postgres_trust_commands::PgLegalTrustWorkflow;
use crate::{
    application::trust_management::LegalTrustInspector,
    domain::{
        legal_records::sha256_hex,
        pdf_verification::TrustMaterialKind,
        trust_management::{
            LegalTrustActor, LegalTrustCommand, LegalTrustCommandRequest, LegalTrustReceipt,
        },
    },
};

/// Identity-owned facts the bridge cannot decide. Composition implements this
/// from the normal Account session, IAM and Control authority lease, using the
/// same Holding transaction it passes to the bridge.
pub trait LegalTrustIdentityBridge {
    /// Returns the current authority witness the Workflow receipt will retain.
    fn authorize_manage(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
    ) -> impl Future<Output = Result<String, PgTransactionError<BusinessError>>> + Send;
    /// Re-checks that witness immediately before the owner write.
    fn revalidate_authority(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        witness: &str,
    ) -> impl Future<Output = Result<(), PgTransactionError<BusinessError>>> + Send;
    /// Identity-owned same-person decision (aliases included).
    fn same_person(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        other: &UserId,
    ) -> impl Future<Output = Result<bool, PgTransactionError<BusinessError>>> + Send;
}

/// Default Workflow/CommitAuthority implementation over the exact owner
/// receipt, audit and outbox tables. It performs no Identity decision and opens
/// no transaction of its own.
pub struct PgLegalTrustWorkflowBridge<Identity> {
    identity: Identity,
    assignment_id: AssignmentId,
    now: i64,
}

impl<Identity> PgLegalTrustWorkflowBridge<Identity> {
    pub fn new(identity: Identity, assignment_id: AssignmentId, now: i64) -> Self {
        Self {
            identity,
            assignment_id,
            now,
        }
    }
}

fn conflict(code: &'static str, message: &'static str) -> PgTransactionError<BusinessError> {
    PgTransactionError::Business(BusinessError::new(FailureKind::Conflict, code, message))
}

fn corrupt(code: &'static str) -> PgTransactionError<BusinessError> {
    PgTransactionError::Persistence(PersistenceError::new(code))
}

async fn require_transaction(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
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

fn kind_name(kind: TrustMaterialKind) -> &'static str {
    match kind {
        TrustMaterialKind::Root => "root",
        TrustMaterialKind::Intermediate => "intermediate",
        TrustMaterialKind::Crl => "crl",
        TrustMaterialKind::Ocsp => "ocsp",
    }
}

/// Canonical request hash for the retained receipt. The existing workflow trait
/// carries the parsed command rather than the raw body, so the bridge hashes the
/// exact operation and command fields. A different action under the same key is
/// therefore rejected instead of replayed.
pub fn trust_command_request_hash(command: &LegalTrustCommand) -> String {
    let value = match command {
        LegalTrustCommand::Add {
            kind,
            name,
            source,
            content,
        } => json!({
            "action": command.operation(),
            "kind": kind_name(*kind),
            "name": name,
            "source": source,
            "content": content,
        }),
        LegalTrustCommand::Approve { id, version } | LegalTrustCommand::Retire { id, version } => {
            json!({
                "action": command.operation(),
                "materialId": id.as_str(),
                "materialVersion": version,
            })
        }
    };
    sha256_hex(value.to_string().as_bytes())
}

impl<Identity: LegalTrustIdentityBridge + Send> PgLegalTrustCommitAuthority
    for PgLegalTrustWorkflowBridge<Identity>
{
    async fn revalidate(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        witness: &str,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        self.identity
            .revalidate_authority(connection, actor, witness)
            .await
    }

    async fn independent_person_review(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        created_by: &UserId,
    ) -> Result<bool, PgTransactionError<BusinessError>> {
        Ok(!self
            .identity
            .same_person(connection, actor, created_by)
            .await?)
    }
}

impl<Identity: LegalTrustIdentityBridge + Send> PgLegalTrustWorkflow
    for PgLegalTrustWorkflowBridge<Identity>
{
    async fn authorize_manage(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
    ) -> Result<String, PgTransactionError<BusinessError>> {
        self.identity.authorize_manage(connection, actor).await
    }

    async fn same_person(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        other: &UserId,
    ) -> Result<bool, PgTransactionError<BusinessError>> {
        self.identity.same_person(connection, actor, other).await
    }

    async fn replay(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        key: &CommandKey,
        command: &LegalTrustCommand,
    ) -> Result<Option<LegalTrustReceipt>, PgTransactionError<BusinessError>> {
        require_transaction(connection).await?;
        let row = sqlx::query(
            "SELECT operation,request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
        )
        .bind(key.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let operation: String = row.try_get("operation")?;
        let request_hash: String = row.try_get("request_hash")?;
        if operation != command.operation() || request_hash != trust_command_request_hash(command) {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            ));
        }
        let raw: Option<String> = row.try_get("result_json")?;
        let value: serde_json::Value = raw
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok())
            .ok_or_else(|| corrupt("LEGAL_TRUST_CORRUPT_RECEIPT"))?;
        let id = value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| corrupt("LEGAL_TRUST_CORRUPT_RECEIPT"))?;
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| corrupt("LEGAL_TRUST_CORRUPT_RECEIPT"))?;
        if id != actor.company_id.as_str() {
            return Err(corrupt("LEGAL_TRUST_CORRUPT_RECEIPT"));
        }
        let version = u32::try_from(version)
            .map_err(|_| corrupt("LEGAL_TRUST_CORRUPT_RECEIPT"))?;
        Ok(Some(LegalTrustReceipt {
            id: actor.company_id.clone(),
            version,
        }))
    }

    async fn record_completed(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        key: &CommandKey,
        command: &LegalTrustCommand,
        receipt: &LegalTrustReceipt,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        require_transaction(connection).await?;
        if receipt.id != actor.company_id {
            return Err(conflict(
                "LEGAL_SCOPE_MISMATCH",
                "The trust receipt belongs to another Company.",
            ));
        }
        let operation = command.operation();
        let request_hash = trust_command_request_hash(command);
        let authority = serde_json::to_string(&json!({
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "userId": actor.user_id.as_str(),
            "assignmentId": self.assignment_id.as_str(),
        }))
        .map_err(|_| corrupt("LEGAL_TRUST_CORRUPT_RECEIPT"))?;
        let result = serde_json::to_string(&json!({
            "id": receipt.id.as_str(),
            "version": receipt.version,
        }))
        .map_err(|_| corrupt("LEGAL_TRUST_CORRUPT_RECEIPT"))?;
        let inserted = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (command_id) DO NOTHING",
        )
        .bind(key.as_str())
        .bind(operation)
        .bind(actor.user_id.as_str())
        .bind(&request_hash)
        .bind(receipt.id.as_str())
        .bind(&result)
        .bind(self.now)
        .bind(&authority)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if inserted != 1 {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            ));
        }
        let short = &sha256_hex(key.as_str().as_bytes())[..32];
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,'legal',$6,$7,$5,$8,(SELECT authority_json FROM command_receipts WHERE command_id=$2))",
        )
        .bind(format!("legal-audit-{short}"))
        .bind(key.as_str())
        .bind(actor.user_id.as_str())
        .bind(self.assignment_id.as_str())
        .bind(operation)
        .bind(receipt.id.as_str())
        .bind(i64::from(receipt.version))
        .bind(self.now)
        .execute(&mut *connection)
        .await?;
        let payload = serde_json::to_string(&json!({
            "id": receipt.id.as_str(),
            "version": receipt.version,
            "companyId": actor.company_id.as_str(),
            "operation": operation,
        }))
        .map_err(|_| corrupt("LEGAL_TRUST_CORRUPT_RECEIPT"))?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,'legal',$4,$5,$6,$7)",
        )
        .bind(format!("legal-outbox-{short}"))
        .bind(key.as_str())
        .bind(operation)
        .bind(receipt.id.as_str())
        .bind(i64::from(receipt.version))
        .bind(payload)
        .bind(self.now)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }
}

/// Exact remaining composition bridge for the HTTP trust mount: the session
/// actor + assignment, the IAM-backed `LegalTrustIdentityBridge`, and one
/// serializable Holding connection. Everything else is owner code.
#[allow(clippy::too_many_arguments)]
pub async fn execute_legal_trust_command_with_owner_bridge<Identity>(
    connection: &mut PgConnection,
    actor: &LegalTrustActor,
    assignment_id: &AssignmentId,
    key: &CommandKey,
    command: &LegalTrustCommand,
    inspector: &impl LegalTrustInspector,
    clock: &impl Clock,
    identity: Identity,
) -> Result<LegalTrustReceipt, PgTransactionError<BusinessError>>
where
    Identity: LegalTrustIdentityBridge + Send,
{
    let holding_id = actor.holding_id.clone();
    let company_id = actor.company_id.clone();
    let mut workflow = PgLegalTrustWorkflowBridge::new(
        identity,
        assignment_id.clone(),
        clock.now().get(),
    );
    super::postgres_trust_commands::execute_legal_trust_command(
        connection,
        holding_id,
        company_id,
        &mut workflow,
        inspector,
        clock,
        LegalTrustCommandRequest {
            actor,
            key,
            command,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::RecordId;

    #[test]
    fn request_hash_is_stable_and_action_sensitive() {
        let add = LegalTrustCommand::Add {
            kind: TrustMaterialKind::Root,
            name: "Issuing root".into(),
            source: "https://ca.example/root.cer".into(),
            content: "MIIBszCCAVmgAwIBAgIUexamplecontent".into(),
        };
        assert_eq!(trust_command_request_hash(&add), trust_command_request_hash(&add.clone()));
        assert_eq!(trust_command_request_hash(&add).len(), 64);
        let mut other = add.clone();
        if let LegalTrustCommand::Add { name, .. } = &mut other {
            *name = "Other root".into();
        }
        assert_ne!(trust_command_request_hash(&add), trust_command_request_hash(&other));

        let approve = LegalTrustCommand::Approve {
            id: RecordId::new("material-one").unwrap(),
            version: 3,
        };
        let retire = LegalTrustCommand::Retire {
            id: RecordId::new("material-one").unwrap(),
            version: 3,
        };
        assert_ne!(
            trust_command_request_hash(&approve),
            trust_command_request_hash(&retire)
        );
        assert_eq!(trust_command_request_hash(&approve), trust_command_request_hash(&LegalTrustCommand::Approve {
            id: RecordId::new("material-one").unwrap(),
            version: 3,
        }));
    }
}
