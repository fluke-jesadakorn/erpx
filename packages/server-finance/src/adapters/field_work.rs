//! Finance owns candidate enumeration; outer composition retains each source.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use erpx_server_runtime::public::postgres::PgTransactionError;
use sqlx::PgConnection;

pub async fn field_financial_work_candidates(c:&mut PgConnection,company:&CompanyId)->Result<Vec<(String,String)>,PgTransactionError<BusinessError>>{
    let rows:Vec<(String,String)>=sqlx::query_as("SELECT kind,id FROM (SELECT 'invoice'::text kind,i.id FROM finance_invoices i WHERE i.company_id=$1 AND erpx_finance_field_invoice_owned(i.id,i.company_id) UNION ALL SELECT 'journal'::text kind,j.id FROM finance_journals j WHERE j.company_id=$1 AND erpx_finance_field_journal_owned(j.id,j.company_id)) candidates ORDER BY kind,id LIMIT 10001")
        .bind(company.as_str()).fetch_all(c).await?;
    if rows.len()>10000{return Err(BusinessError::new(FailureKind::Unavailable,"FINANCE_WORK_LIMIT","The financial Work contribution exceeds its bounded read.").into());}
    Ok(rows)
}
