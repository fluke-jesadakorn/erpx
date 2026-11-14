use std::future::Future;

use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use sqlx::{Connection, PgConnection};

use super::postgres_trust::{PgLegalTrustCommitAuthority, PgLegalTrustStore};
use crate::{
    application::trust_management::{
        LegalTrustCommandPorts, LegalTrustInspector, PreparedLegalTrustCommand,
        prepare_legal_trust_command,
    },
    domain::{
        pdf_verification::{TrustMaterialKind, TrustMaterialState},
        trust_management::{
            LegalTrustActor, LegalTrustCommand, LegalTrustCommandRequest, LegalTrustReceipt,
            LegalTrustRecord,
        },
    },
};

/// Cross-owner composition seam. Identity supplies current effective permission
/// and person evidence; Workflow owns replay/collision checking, receipt and audit.
/// Every callback borrows this same Holding transaction and retains its Control
/// authority lease until outer commit. No callback may independently commit or
/// perform provider effects. No production implementation is inferred here.
pub trait PgLegalTrustWorkflow: PgLegalTrustCommitAuthority {
    fn authorize_manage(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
    ) -> impl Future<Output = Result<String, PgTransactionError<BusinessError>>> + Send;
    fn same_person(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        other: &UserId,
    ) -> impl Future<Output = Result<bool, PgTransactionError<BusinessError>>> + Send;
    fn replay(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        key: &CommandKey,
        command: &LegalTrustCommand,
    ) -> impl Future<Output = Result<Option<LegalTrustReceipt>, PgTransactionError<BusinessError>>> + Send;
    fn record_completed(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        key: &CommandKey,
        command: &LegalTrustCommand,
        receipt: &LegalTrustReceipt,
    ) -> impl Future<Output = Result<(), PgTransactionError<BusinessError>>> + Send;
}

struct PgTrustCommandRepository<'c, Workflow> {
    holding_id: HoldingId,
    company_id: CompanyId,
    connection: &'c mut PgConnection,
    workflow: &'c mut Workflow,
}

impl<Workflow: PgLegalTrustWorkflow + Send> LegalTrustCommandPorts
    for PgTrustCommandRepository<'_, Workflow>
{
    type Error = PgTransactionError<BusinessError>;
    async fn authorize_manage(&mut self, actor: &LegalTrustActor) -> Result<String, Self::Error> {
        require_scope(self.connection, &self.holding_id, &self.company_id, actor).await?;
        self.workflow.authorize_manage(self.connection, actor).await
    }
    async fn replay(
        &mut self,
        actor: &LegalTrustActor,
        key: &CommandKey,
        command: &LegalTrustCommand,
    ) -> Result<Option<LegalTrustReceipt>, Self::Error> {
        self.workflow
            .replay(self.connection, actor, key, command)
            .await
    }
    async fn lock_company(&mut self, actor: &LegalTrustActor) -> Result<(), Self::Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("legal:{}", actor.company_id.as_str()))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
    async fn material(
        &mut self,
        actor: &LegalTrustActor,
        id: &RecordId,
    ) -> Result<Option<LegalTrustRecord>, Self::Error> {
        let row=sqlx::query_as::<_,(i64,String,String,String,String)>("SELECT version::bigint,kind,state,created_by,checksum FROM legal_trust_material WHERE id=$1 AND company_id=$2 FOR UPDATE")
            .bind(id.as_str()).bind(actor.company_id.as_str()).fetch_optional(&mut *self.connection).await?;
        let Some((version, kind, state, created_by, checksum)) = row else {
            return Ok(None);
        };
        let corrupt = || PersistenceError::new("LEGAL_TRUST_CORRUPT_RECORD");
        let version = u64::try_from(version).map_err(|_| corrupt())?;
        let kind = match kind.as_str() {
            "root" => TrustMaterialKind::Root,
            "intermediate" => TrustMaterialKind::Intermediate,
            "crl" => TrustMaterialKind::Crl,
            "ocsp" => TrustMaterialKind::Ocsp,
            _ => return Err(corrupt().into()),
        };
        let state = match state.as_str() {
            "proposed" => TrustMaterialState::Proposed,
            "approved" => TrustMaterialState::Approved,
            "retired" => TrustMaterialState::Retired,
            _ => return Err(corrupt().into()),
        };
        Ok(Some(LegalTrustRecord {
            id: id.clone(),
            company_id: actor.company_id.clone(),
            version,
            kind,
            state,
            created_by: UserId::new(created_by).map_err(|_| corrupt())?,
            checksum_sha256: checksum,
        }))
    }
    async fn same_person(
        &mut self,
        actor: &LegalTrustActor,
        other: &UserId,
    ) -> Result<bool, Self::Error> {
        self.workflow
            .same_person(self.connection, actor, other)
            .await
    }
    async fn active_count(&mut self, actor: &LegalTrustActor) -> Result<u64, Self::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM legal_trust_material WHERE company_id=$1 AND state<>'retired'",
        )
        .bind(actor.company_id.as_str())
        .fetch_one(&mut *self.connection)
        .await?;
        u64::try_from(count).map_err(|_| PersistenceError::new("LEGAL_TRUST_CORRUPT_COUNT").into())
    }
}

/// A real command workflow on the borrowed transaction. This function deliberately
/// requires explicit current-authority and Workflow owner bridges; it cannot
/// activate a route with a scope-only actor or its own independent transaction.
pub async fn execute_legal_trust_command(
    connection: &mut PgConnection,
    holding_id: HoldingId,
    company_id: CompanyId,
    workflow: &mut (impl PgLegalTrustWorkflow + Send),
    inspector: &impl LegalTrustInspector,
    clock: &impl Clock,
    request: LegalTrustCommandRequest<'_>,
) -> Result<LegalTrustReceipt, PgTransactionError<BusinessError>> {
    let LegalTrustCommandRequest {
        actor,
        key,
        command,
    } = request;
    let prepared = {
        let mut repository = PgTrustCommandRepository {
            holding_id: holding_id.clone(),
            company_id: company_id.clone(),
            connection,
            workflow,
        };
        prepare_legal_trust_command(&mut repository, inspector, clock, actor, key, command).await?
    };
    match prepared {
        PreparedLegalTrustCommand::Replay(receipt) => Ok(receipt),
        PreparedLegalTrustCommand::Change(plan) => {
            let receipt = PgLegalTrustStore::new(holding_id, company_id, connection)
                .persist(&plan, workflow)
                .await?;
            workflow
                .record_completed(connection, actor, key, command, &receipt)
                .await?;
            Ok(receipt)
        }
    }
}

async fn require_scope(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    actor: &LegalTrustActor,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    if holding != &actor.holding_id || company != &actor.company_id {
        return Err(PgTransactionError::Business(BusinessError::new(
            FailureKind::Forbidden,
            "LEGAL_SCOPE_MISMATCH",
            "The Legal command belongs to another Holding or Company.",
        )));
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
