use erpx_server_identity_access::public::effective_authority::{
    PgQueuedWorkSourcePort, QueuedWorkBinding,
};
use erpx_server_legal::public::{
    CurrentLegalProcessingSource, LegalProcessingSourceRequest,
    load_current_legal_processing_source,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId, RecordId};
use sqlx::PgConnection;

/// The trusted worker selects a configured Holding pool and exact owner job key.
/// Loading the actual opaque Legal source happens inside Identity's fresh UoW.
/// There is no Actor input and no Identity↔Legal implementation dependency.
#[derive(Clone)]
pub struct LegalQueuedSource {
    holding_id: HoldingId,
    company_id: CompanyId,
    job_id: RecordId,
    lease_token: RecordId,
}
impl LegalQueuedSource {
    pub fn new(
        holding_id: HoldingId,
        company_id: CompanyId,
        job_id: RecordId,
        lease_token: RecordId,
    ) -> Self {
        Self {
            holding_id,
            company_id,
            job_id,
            lease_token,
        }
    }
}
impl PgQueuedWorkSourcePort for LegalQueuedSource {
    type Source = CurrentLegalProcessingSource;
    fn load_source<'c>(
        &'c self,
        connection: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, Self::Source, BusinessError> {
        Box::pin(async move {
            load_current_legal_processing_source(
                connection,
                LegalProcessingSourceRequest {
                    holding_id: self.holding_id.clone(),
                    company_id: self.company_id.clone(),
                    job_id: self.job_id.clone(),
                    lease_token: self.lease_token.clone(),
                },
                now,
            )
            .await
        })
    }
    fn binding(&self, source: &Self::Source) -> Result<QueuedWorkBinding, BusinessError> {
        let binding = QueuedWorkBinding::parse(source.actor_snapshot_json())?;
        if source.holding_id() != &self.holding_id
            || source.company_id() != &self.company_id
            || source.job().id != self.job_id
            || source.lease().token != self.lease_token
            || &binding.holding_id != source.holding_id()
            || &binding.company_id != source.company_id()
            || &binding.user_id != source.user_id()
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "QUEUED_OWNER_SOURCE_CHANGED",
                "The original owner source changed.",
            ));
        }
        Ok(binding)
    }
    fn require_current<'c>(
        &'c self,
        connection: &'c mut PgConnection,
        source: &'c Self::Source,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move { source.require_current_or_settled(connection, now).await })
    }
}
