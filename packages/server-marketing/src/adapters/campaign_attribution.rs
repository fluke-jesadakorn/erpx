//! PostgreSQL read adapter for the native Marketing campaign/attribution
//! report.
//!
//! Only the Marketing owner's `marketing_campaigns` and
//! `marketing_lead_candidates` tables are read here. Package facts arrive
//! through the public Commercial port supplied by composition. The caller owns
//! the retained serializable Holding transaction; this adapter never creates,
//! links, activates, retires or hands off any Marketing or cross-owner row.
use crate::application::campaign_attribution::{
    MarketingCampaignFuture, MarketingCampaignRepository,
};
use crate::domain::campaign_attribution::{
    MarketingAttributionRow, MarketingCampaignRow, source_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgMarketingCampaignRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgMarketingCampaignRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(
                PersistenceError::new("MARKETING_CAMPAIGN_READ_TRANSACTION_REQUIRED").into(),
            );
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(
                PersistenceError::new("MARKETING_CAMPAIGN_READ_SERIALIZABLE_REQUIRED").into(),
            );
        }
        Ok(())
    }

    pub async fn campaigns(
        &mut self,
        company: &CompanyId,
        limit: usize,
    ) -> Result<Vec<MarketingCampaignRow>, Error> {
        self.ready().await?;
        let rows = sqlx::query(
            "SELECT id,company_id,code,name,version,created_at FROM marketing_campaigns \
             WHERE company_id=$1 ORDER BY code COLLATE \"C\",id COLLATE \"C\" LIMIT $2",
        )
        .bind(company.as_str())
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(MarketingCampaignRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    code: row.try_get("code")?,
                    name: row.try_get("name")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    pub async fn attribution(
        &mut self,
        company: &CompanyId,
        branch_id: Option<&str>,
        department_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MarketingAttributionRow>, Error> {
        self.ready().await?;
        let invalid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM marketing_lead_candidates \
             WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$2) \
             AND ($3::text IS NULL OR department_id=$3) \
             AND NOT erpx_json_valid(attribution_json))",
        )
        .bind(company.as_str())
        .bind(branch_id)
        .bind(department_id)
        .fetch_one(&mut *self.connection)
        .await?;
        if invalid {
            return Err(source_invalid().into());
        }
        let rows = sqlx::query(
            "SELECT campaign,source,captured,pending,handed_off,rejected FROM (\
             SELECT COALESCE(NULLIF(BTRIM(attribution_json::jsonb->>'campaign'),''),'Unspecified') AS campaign,\
             COALESCE(NULLIF(BTRIM(attribution_json::jsonb->>'source'),''),NULLIF(BTRIM(attribution_json::jsonb->>'utmSource'),''),'Unspecified') AS source,\
             COUNT(*)::bigint AS captured,\
             COUNT(*) FILTER (WHERE status='pending')::bigint AS pending,\
             COUNT(*) FILTER (WHERE status='handed_off')::bigint AS handed_off,\
             COUNT(*) FILTER (WHERE status='rejected')::bigint AS rejected \
             FROM marketing_lead_candidates WHERE company_id=$1 \
             AND ($2::text IS NULL OR branch_id=$2) \
             AND ($3::text IS NULL OR department_id=$3) \
             GROUP BY 1,2) grouped \
             ORDER BY captured DESC,campaign COLLATE \"C\",source COLLATE \"C\" LIMIT $4",
        )
        .bind(company.as_str())
        .bind(branch_id)
        .bind(department_id)
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(MarketingAttributionRow {
                    campaign: row.try_get("campaign")?,
                    source: row.try_get("source")?,
                    captured: row.try_get("captured")?,
                    pending: row.try_get("pending")?,
                    handed_off: row.try_get("handed_off")?,
                    rejected: row.try_get("rejected")?,
                })
            })
            .collect()
    }
}

impl MarketingCampaignRepository for PgMarketingCampaignRead<'_> {
    type Error = Error;

    fn campaigns<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> MarketingCampaignFuture<'a, Vec<MarketingCampaignRow>, Self::Error> {
        Box::pin(async move { PgMarketingCampaignRead::campaigns(self, company, limit).await })
    }

    fn attribution<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        limit: usize,
    ) -> MarketingCampaignFuture<'a, Vec<MarketingAttributionRow>, Self::Error> {
        Box::pin(async move {
            PgMarketingCampaignRead::attribution(self, company, branch_id, department_id, limit)
                .await
        })
    }
}
