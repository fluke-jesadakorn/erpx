use crate::domain::processing::{
    LegalProcessingActor, LegalProcessingKind, LegalProcessingReceipt, PROCESSING_ACTOR_LIMIT,
    PreparedLegalProcessingQueue, scope_error, validate_authority, validate_checksum,
};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, RecordId};
use std::future::Future;

pub trait LegalProcessingCommandPorts {
    type Error: From<BusinessError> + Send;
    fn prepare(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document_id: &RecordId,
        kind: LegalProcessingKind,
    ) -> impl Future<Output = Result<PreparedLegalProcessingQueue, Self::Error>> + Send;
    fn replay(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        document_id: &RecordId,
        kind: LegalProcessingKind,
    ) -> impl Future<Output = Result<Option<LegalProcessingReceipt>, Self::Error>> + Send;
    fn recent_count(&mut self, since: i64)
    -> impl Future<Output = Result<u64, Self::Error>> + Send;
    fn revalidate(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        kind: LegalProcessingKind,
        prepared: &PreparedLegalProcessingQueue,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn persist(
        &mut self,
        actor: &LegalProcessingActor,
        key: &CommandKey,
        kind: LegalProcessingKind,
        prepared: &PreparedLegalProcessingQueue,
        receipt: &LegalProcessingReceipt,
        now: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// The current owner parent is checked before Workflow replay. The repository
/// retains the same transaction through current revalidation, job and receipt.
pub async fn queue_legal_processing<Ports: LegalProcessingCommandPorts>(
    ports: &mut Ports,
    clock: &impl Clock,
    actor: &LegalProcessingActor,
    key: &CommandKey,
    document_id: &RecordId,
    kind: LegalProcessingKind,
) -> Result<LegalProcessingReceipt, Ports::Error> {
    validate_authority(&actor.selection_revision)?;
    let prepared = ports.prepare(actor, key, document_id, kind).await?;
    validate_checksum(&prepared.document.checksum_sha256)?;
    validate_authority(&prepared.authority.revision)?;
    if prepared.document.document_id != *document_id || prepared.document.item_version == 0 {
        return Err(scope_error().into());
    }
    let snapshot = &prepared.authority.actor_snapshot_json;
    if snapshot.len() > PROCESSING_ACTOR_LIMIT
        || !serde_json::from_str::<serde_json::Value>(snapshot).is_ok_and(|value| value.is_object())
    {
        return Err(BusinessError::invalid(
            "LEGAL_PROCESSING_ACTOR_INVALID",
            "Invalid processing selection snapshot.",
        )
        .into());
    }
    if let Some(receipt) = ports.replay(actor, key, document_id, kind).await? {
        if receipt.id != prepared.document.item_id
            || receipt.job_id.as_str() != key.as_str()
            || receipt.state != "queued"
        {
            return Err(scope_error().into());
        }
        return Ok(receipt);
    }
    let now = clock.now().get();
    if ports.recent_count(now.saturating_sub(60_000)).await? >= 20 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "LEGAL_PROCESSING_RATE_LIMIT",
            "Wait before queuing more document processing.",
        )
        .into());
    }
    ports.revalidate(actor, key, kind, &prepared).await?;
    let receipt = LegalProcessingReceipt {
        id: prepared.document.item_id.clone(),
        job_id: RecordId::new(key.as_str())?,
        state: "queued".to_owned(),
    };
    ports
        .persist(actor, key, kind, &prepared, &receipt, now)
        .await?;
    Ok(receipt)
}
