//! People-owned display names for active Identity members. These are not role
//! or identity grants; the caller separately witnesses Identity and Organization.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
pub async fn read_service_count_member_names(
    c: &mut PgConnection,
    company: &CompanyId,
    ids: &[String],
) -> Result<Vec<(String, String)>, Error> {
    if !c.is_in_transaction() || ids.len() > 500 || ids.iter().any(|v| v.len() > 200) {
        return Err(PersistenceError::new("STOCK_COUNT_FLOW_PEOPLE_INVALID").into());
    }
    let rows: Vec<(String,Option<String>)>=sqlx::query_as("SELECT user_id,CASE WHEN octet_length(legal_name)<=3000 THEN legal_name END FROM hr_people WHERE company_id=$1 AND user_id=ANY($2::text[]) AND merged_into_id IS NULL ORDER BY user_id,id LIMIT 501").bind(company.as_str()).bind(ids).fetch_all(c).await?;
    if rows.len() > 500
        || rows
            .iter()
            .map(|(id, name)| id.len() + name.as_ref().map_or(0, String::len))
            .sum::<usize>()
            > 2 * 1024 * 1024
    {
        return Err(PersistenceError::new("STOCK_COUNT_FLOW_PEOPLE_INVALID").into());
    }
    rows.into_iter()
        .map(|(id, name)| {
            name.map(|name| (id, name))
                .ok_or_else(|| PersistenceError::new("STOCK_COUNT_FLOW_PEOPLE_INVALID").into())
        })
        .collect()
}
