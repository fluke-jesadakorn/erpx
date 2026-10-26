use erpx_server_shared::public::{CompanyId, RecordId, TimestampMillis};
/// Current Organization source witness. People and IAM still validate its occupant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HrAuthoritySeat {
    pub company_id: CompanyId,
    pub person_id: RecordId,
    pub position_id: Option<RecordId>,
    pub version: i64,
    pub effective_at: TimestampMillis,
}
