//! The caller binds owner reads and witness checks to its retained unit.
use crate::domain::source_record::{
    SourceRecord, SourceReference, require_matching_source, validate_reference,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub trait SourceRecordPort: Send {
    type Error: From<BusinessError> + Send;
    type Witness: Send;
    /// Authorize and capture the real owning source in the caller's unit.
    fn prepare(
        &mut self,
        source: &SourceReference,
    ) -> impl Future<Output = Result<(SourceRecord, Self::Witness), Self::Error>> + Send;
    /// Revalidate the same owner witness, after deferred effects and before commit.
    fn require_current(
        &mut self,
        witness: &Self::Witness,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
pub struct PreparedSource<W> {
    pub record: SourceRecord,
    witness: W,
}
impl<W> PreparedSource<W> {
    /// Transfer the real owner snapshot and opaque witness to a consuming
    /// owner's port. The consumer must revalidate it after deferred effects.
    pub fn into_parts(self) -> (SourceRecord, W) {
        (self.record, self.witness)
    }
    pub async fn require_current<P: SourceRecordPort<Witness = W>>(
        &self,
        port: &mut P,
    ) -> Result<(), P::Error> {
        port.require_current(&self.witness).await
    }
}
pub async fn prepare_source<P: SourceRecordPort>(
    port: &mut P,
    company_id: &str,
    source: &SourceReference,
) -> Result<PreparedSource<P::Witness>, P::Error> {
    validate_reference(source)?;
    let (record, witness) = port.prepare(source).await?;
    require_matching_source(source, company_id, &record)?;
    Ok(PreparedSource { record, witness })
}
