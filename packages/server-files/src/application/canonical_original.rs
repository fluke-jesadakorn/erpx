use crate::domain::canonical_original::{CanonicalOriginalFacts, LegalOriginalQuery, changed};
use erpx_server_shared::public::BusinessError;
use sha2::{Digest, Sha256};
use std::future::Future;

/// Storage capability is injected only at composition. A database object key
/// never authorizes a read; the Files application obtains current owner policy
/// before invoking this port.
pub trait OriginalByteStore: Send + Sync {
    fn read_verified(
        &self,
        key: &str,
        size: usize,
        checksum: &str,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<u8>, BusinessError>> + Send;
}

/// Called by the transaction adapter only after the current owning record
/// policy succeeds. Keep byte integrity independent of a particular disk or
/// object-store adapter, including injected adapters used by other runtimes.
pub(crate) async fn read_original_bytes(
    store: &impl OriginalByteStore,
    facts: &CanonicalOriginalFacts,
    query: &LegalOriginalQuery,
) -> Result<Vec<u8>, BusinessError> {
    facts.validate(query)?;
    let bytes = store
        .read_verified(
            &facts.object_key,
            facts.bytes as usize,
            &facts.checksum_sha256,
            query.max_bytes,
        )
        .await?;
    if bytes.len() != facts.bytes as usize
        || bytes.len() > query.max_bytes
        || hex::encode(Sha256::digest(&bytes)) != query.expected_checksum_sha256
    {
        return Err(changed());
    }
    Ok(bytes)
}
