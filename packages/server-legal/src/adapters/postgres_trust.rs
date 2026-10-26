use std::future::Future;

use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId, UserId};
use sqlx::{Connection, PgConnection};

use crate::{
    application::trust_management::{
        LegalTrustChange, LegalTrustChangePlan, changed, independent_review,
    },
    domain::{
        pdf_verification::{TrustMaterialKind, TrustMaterialState},
        trust_management::{LegalTrustActor, LegalTrustReceipt},
    },
};

/// Composition must supply current effective Identity/owner policy, using this
/// same Holding transaction and its retained Control request lease. There is no
/// permissive/default implementation. Scope facts alone are not authorization.
pub trait PgLegalTrustCommitAuthority {
    fn revalidate(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        witness: &str,
    ) -> impl Future<Output = Result<(), PgTransactionError<BusinessError>>> + Send;
    fn independent_person_review(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        created_by: &UserId,
    ) -> impl Future<Output = Result<bool, PgTransactionError<BusinessError>>> + Send;
}

/// A writer inside the caller's existing serializable Holding transaction. It
/// never begins, commits, retries or closes that transaction or its Control lease.
pub struct PgLegalTrustStore<'c> {
    holding_id: HoldingId,
    company_id: CompanyId,
    connection: &'c mut PgConnection,
}
impl<'c> PgLegalTrustStore<'c> {
    pub fn new(
        holding_id: HoldingId,
        company_id: CompanyId,
        connection: &'c mut PgConnection,
    ) -> Self {
        Self {
            holding_id,
            company_id,
            connection,
        }
    }

    /// The outer owner must commit its idempotency receipt and audit atomically
    /// with this change. SQL errors retain SQLSTATE for the outer UoW's retries.
    pub async fn persist(
        &mut self,
        plan: &LegalTrustChangePlan,
        authority: &mut impl PgLegalTrustCommitAuthority,
    ) -> Result<LegalTrustReceipt, PgTransactionError<BusinessError>> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
        }
        let isolation = sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
        }
        if plan.actor.holding_id != self.holding_id
            || plan.actor.company_id != self.company_id
            || plan.actor.branch_id.is_some()
            || plan.actor.department_id.is_some()
        {
            return Err(PgTransactionError::Business(BusinessError::new(
                FailureKind::Forbidden,
                "LEGAL_SCOPE_MISMATCH",
                "The trust change belongs to another Holding or Company scope.",
            )));
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("legal:{}", self.company_id.as_str()))
            .execute(&mut *self.connection)
            .await?;
        authority
            .revalidate(&mut *self.connection, &plan.actor, &plan.authority_witness)
            .await?;
        match &plan.change {
            LegalTrustChange::Add {
                id,
                kind,
                name,
                source,
                material,
            } => {
                let count=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM legal_trust_material WHERE company_id=$1 AND state<>'retired'")
                    .bind(self.company_id.as_str()).fetch_one(&mut *self.connection).await?;
                if count >= 100 {
                    return Err(PgTransactionError::Business(BusinessError::invalid(
                        "LEGAL_TRUST_LIMIT",
                        "Retire obsolete trust material before adding more.",
                    )));
                }
                sqlx::query("INSERT INTO legal_trust_material(id,company_id,kind,name,source,content,checksum,state,created_by,created_at) VALUES($1,$2,$3,$4,$5,encode($6,'base64'),$7,'proposed',$8,$9)")
                    .bind(id.as_str()).bind(self.company_id.as_str()).bind(kind_name(*kind)).bind(name).bind(source)
                    .bind(&material.content).bind(&material.checksum_sha256).bind(plan.actor.user_id.as_str()).bind(plan.now.get())
                    .execute(&mut *self.connection).await?;
            }
            LegalTrustChange::Decision { source, state } => {
                let row=sqlx::query_as::<_,(i64,String,String,String,String)>("SELECT version::bigint,state,created_by,checksum,kind FROM legal_trust_material WHERE id=$1 AND company_id=$2 FOR UPDATE")
                    .bind(source.id.as_str()).bind(self.company_id.as_str()).fetch_optional(&mut *self.connection).await?;
                let Some((version, current, created_by, checksum, kind)) = row else {
                    return Err(PgTransactionError::Business(changed()));
                };
                if u64::try_from(version).ok() != Some(source.version)
                    || current != state_name(source.state)
                    || created_by != source.created_by.as_str()
                    || checksum != source.checksum_sha256
                    || kind != kind_name(source.kind)
                {
                    return Err(PgTransactionError::Business(changed()));
                }
                if *state == TrustMaterialState::Approved
                    && (source.state != TrustMaterialState::Proposed
                        || source.created_by == plan.actor.user_id
                        || !authority
                            .independent_person_review(
                                &mut *self.connection,
                                &plan.actor,
                                &source.created_by,
                            )
                            .await?)
                {
                    return Err(PgTransactionError::Business(independent_review()));
                }
                let result=sqlx::query("UPDATE legal_trust_material SET state=$1,approved_by=$2,version=version+1 WHERE id=$3 AND company_id=$4 AND version=$5")
                    .bind(state_name(*state)).bind(plan.actor.user_id.as_str()).bind(source.id.as_str()).bind(self.company_id.as_str())
                    .bind(version).execute(&mut *self.connection).await?;
                if result.rows_affected() != 1 {
                    return Err(PgTransactionError::Business(changed()));
                }
            }
        }
        Ok(plan.receipt.clone())
    }
}

fn kind_name(kind: TrustMaterialKind) -> &'static str {
    match kind {
        TrustMaterialKind::Root => "root",
        TrustMaterialKind::Intermediate => "intermediate",
        TrustMaterialKind::Crl => "crl",
        TrustMaterialKind::Ocsp => "ocsp",
    }
}
fn state_name(state: TrustMaterialState) -> &'static str {
    match state {
        TrustMaterialState::Proposed => "proposed",
        TrustMaterialState::Approved => "approved",
        TrustMaterialState::Retired => "retired",
    }
}
