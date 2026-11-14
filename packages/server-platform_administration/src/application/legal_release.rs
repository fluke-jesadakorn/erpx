use erpx_server_shared::public::{CompanyId, TimestampMillis};
use std::future::Future;
/// Read evidence only; personal/record/purchase authorization remains separate.
pub trait LegalReleaseReadPort {
    type Error;
    type Inspection;
    fn inspect(
        &mut self,
        company: &CompanyId,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<Self::Inspection, Self::Error>> + Send;
}
