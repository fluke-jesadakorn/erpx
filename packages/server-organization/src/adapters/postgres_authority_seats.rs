use crate::domain::authority_seats::HrAuthoritySeat;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};
use sqlx::{PgConnection, Row};

/// Locks owning Organization sources before IAM/People eligibility snapshots.
/// It borrows the caller's transaction and never starts, retries or commits one.
pub async fn lock_organization_authority_sources(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    sqlx::query(
        "LOCK TABLE organization_nodes,authority_seats,organization_positions IN SHARE MODE",
    )
    .execute(connection)
    .await?;
    Ok(())
}
pub struct PgOrganizationAuthorityReader<'c> {
    connection: &'c mut PgConnection,
}
impl<'c> PgOrganizationAuthorityReader<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    pub async fn current_hr_seat(
        &mut self,
        company: &CompanyId,
        now: TimestampMillis,
    ) -> Result<Option<HrAuthoritySeat>, PgTransactionError<BusinessError>> {
        let row=sqlx::query("SELECT company_id,person_id,position_id,version::bigint version,effective_at FROM authority_seats WHERE company_id=$1 AND kind='hr' AND scope_id=$1 AND person_id IS NOT NULL AND effective_at<=$2")
            .bind(company.as_str()).bind(now.get()).fetch_optional(&mut *self.connection).await?;
        row.map(|row| {
            Ok(HrAuthoritySeat {
                company_id: CompanyId::new(row.try_get::<String, _>("company_id")?)?,
                person_id: RecordId::new(row.try_get::<String, _>("person_id")?)?,
                position_id: row
                    .try_get::<Option<String>, _>("position_id")?
                    .map(RecordId::new)
                    .transpose()?,
                version: row.try_get("version")?,
                effective_at: TimestampMillis::new(row.try_get("effective_at")?),
            })
        })
        .transpose()
    }
}
