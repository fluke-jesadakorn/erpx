//! PostgreSQL read adapter for the native CRM lead/opportunity pipeline.
//!
//! Only the CRM owner's `crm_leads` and `crm_opportunities` tables are read
//! here. Staff, customer and package facts arrive through public ports supplied
//! by composition. The caller owns the retained serializable Holding
//! transaction; this adapter never creates, qualifies, reassigns or closes a
//! lead, opportunity or any cross-owner row.
use crate::application::pipeline::{CrmPipelineFuture, CrmPipelineRepository};
use crate::domain::pipeline::{CrmForecastSourceRow, CrmPipelineCursor, CrmPipelineRow};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_PIPELINE_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_PIPELINE_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgCrmPipelineRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCrmPipelineRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn page(
        &mut self,
        company: &CompanyId,
        branch_id: Option<&str>,
        department_id: Option<&str>,
        search: &str,
        stage: Option<&str>,
        cursor: Option<&CrmPipelineCursor>,
        limit: usize,
    ) -> Result<Vec<CrmPipelineRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT l.id,l.company_id,l.branch_id,l.department_id,l.title,l.contact_name,\
             l.status,l.due_date,l.owner_id,l.version,l.updated_at,\
             v.id AS opportunity_id,v.stage,v.expected_minor,v.currency,v.close_date,\
             v.customer_id,v.version AS opportunity_version \
             FROM crm_leads l \
             LEFT JOIN crm_opportunities v ON v.lead_id=l.id \
             WHERE l.company_id=$1 \
             AND ($2::text IS NULL OR l.branch_id=$2) \
             AND ($3::text IS NULL OR l.department_id=$3) \
             AND ($4='' OR position(lower($4) in lower(l.title||' '||l.contact_name))>0) \
             AND ($5::text IS NULL OR COALESCE(v.stage,l.status)=$5) \
             AND ($6::bigint IS NULL OR (l.updated_at,l.id) < ($6,$7)) \
             ORDER BY l.updated_at DESC,l.id DESC LIMIT $8",
        )
        .bind(company.as_str())
        .bind(branch_id)
        .bind(department_id)
        .bind(search)
        .bind(stage)
        .bind(cursor.map(|value| value.updated_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(CrmPipelineRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    title: row.try_get("title")?,
                    contact_name: row.try_get("contact_name")?,
                    status: row.try_get("status")?,
                    stage: row.try_get("stage")?,
                    opportunity_id: row.try_get("opportunity_id")?,
                    expected_minor: row.try_get("expected_minor")?,
                    currency: row.try_get("currency")?,
                    close_date: row.try_get("close_date")?,
                    due_date: row.try_get("due_date")?,
                    owner_id: row.try_get("owner_id")?,
                    owner_name: None,
                    customer_id: row.try_get("customer_id")?,
                    customer_name: None,
                    version: row.try_get("version")?,
                    opportunity_version: row.try_get("opportunity_version")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }

    pub async fn forecast(
        &mut self,
        company: &CompanyId,
        branch_id: Option<&str>,
        department_id: Option<&str>,
    ) -> Result<Vec<CrmForecastSourceRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT v.currency,v.stage,\
             COALESCE(SUM(CASE WHEN v.stage IN ('discovery','proposal','negotiation') \
                 THEN v.expected_minor ELSE 0 END),0)::bigint AS unweighted_minor,\
             COALESCE(SUM(CASE v.stage \
                 WHEN 'discovery' THEN floor((v.expected_minor::numeric*25+50)/100) \
                 WHEN 'proposal' THEN floor((v.expected_minor::numeric*50+50)/100) \
                 WHEN 'negotiation' THEN floor((v.expected_minor::numeric*75+50)/100) \
                 ELSE 0 END),0)::bigint AS weighted_minor,\
             COALESCE(SUM(CASE WHEN v.stage='won' THEN v.expected_minor ELSE 0 END),0)::bigint AS won_minor \
             FROM crm_opportunities v \
             JOIN crm_leads l ON l.id=v.lead_id \
             WHERE l.company_id=$1 \
             AND ($2::text IS NULL OR l.branch_id=$2) \
             AND ($3::text IS NULL OR l.department_id=$3) \
             GROUP BY v.currency,v.stage ORDER BY v.currency,v.stage",
        )
        .bind(company.as_str())
        .bind(branch_id)
        .bind(department_id)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(CrmForecastSourceRow {
                    currency: row.try_get("currency")?,
                    stage: row.try_get("stage")?,
                    unweighted_minor: row.try_get("unweighted_minor")?,
                    weighted_minor: row.try_get("weighted_minor")?,
                    won_minor: row.try_get("won_minor")?,
                })
            })
            .collect()
    }
}

impl CrmPipelineRepository for PgCrmPipelineRead<'_> {
    type Error = Error;

    fn page<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        search: &'a str,
        stage: Option<&'a str>,
        cursor: Option<&'a CrmPipelineCursor>,
        limit: usize,
    ) -> CrmPipelineFuture<'a, Vec<CrmPipelineRow>, Self::Error> {
        Box::pin(async move {
            PgCrmPipelineRead::page(
                self,
                company,
                branch_id,
                department_id,
                search,
                stage,
                cursor,
                limit,
            )
            .await
        })
    }

    fn forecast<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> CrmPipelineFuture<'a, Vec<CrmForecastSourceRow>, Self::Error> {
        Box::pin(async move {
            PgCrmPipelineRead::forecast(self, company, branch_id, department_id).await
        })
    }
}
