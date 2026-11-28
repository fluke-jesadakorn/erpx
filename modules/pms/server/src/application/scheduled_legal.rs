use erpx_platform_shared_server::public::{CompanyId, TimestampMillis};
use std::future::Future;
pub trait ScheduledLegalReadPort {
    type Error;
    type Inspection;
    fn inspect(
        &mut self,
        company: &CompanyId,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<Self::Inspection, Self::Error>> + Send;
}
