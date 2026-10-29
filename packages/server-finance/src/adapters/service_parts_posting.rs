//! Finance-owned reference and history witness for Service parts posting setup.
//! This source conveys facts, not permission or purchase authority.
use crate::adapters::postgres_journal_facts::embedded_currency_rules;
use crate::domain::service_parts_posting::{
    SalesDefaultsVersion, ServicePartsPostingFinanceHistory, ServicePartsPostingMapping,
    ServicePartsPostingReferences, select_defaults, validate_defaults, validate_mapping, values,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, PartialEq)]
struct Image {
    rows: Vec<(String, Value)>,
    data: ServicePartsPostingReferences,
}
pub struct CurrentServicePartsPostingReferences {
    company: CompanyId,
    existing: Option<ServicePartsPostingMapping>,
    include_defaults: bool,
    unit: Unit,
    image: Image,
}
impl CurrentServicePartsPostingReferences {
    pub fn company(&self) -> &CompanyId {
        &self.company
    }
    pub fn data(&self) -> &ServicePartsPostingReferences {
        &self.image.data
    }
    pub fn validate_mapping(
        &self,
        mapping: &ServicePartsPostingMapping,
    ) -> Result<(), BusinessError> {
        let rules = embedded_currency_rules().map_err(|_| {
            BusinessError::invalid("CURRENCY_SCALE_REQUIRED", "Configure the currency scale.")
        })?;
        validate_mapping(&self.image.rows, mapping, &rules)
    }
    pub fn validate_defaults(
        &self,
        mapping: &ServicePartsPostingMapping,
        expected: Option<&SalesDefaultsVersion>,
    ) -> Result<(), BusinessError> {
        validate_defaults(&self.image.data, mapping, expected)
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(
                c,
                &self.company,
                self.existing.as_ref(),
                self.include_defaults,
            )
            .await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
    /// Revalidate every original Finance row after the one Finance-owned
    /// chart/account insert. The inserted row is verified separately by its
    /// owner writer; this method never authorizes a command.
    pub async fn require_after_prerequisite_insert(
        &self,
        c: &mut PgConnection,
        kind: &str,
        id: &str,
    ) -> Result<(), Error> {
        if !matches!(kind, "chart" | "account") || unit(c).await? != self.unit {
            return Err(changed());
        }
        if self
            .image
            .rows
            .iter()
            .any(|(k, v)| k == kind && v.get("id").and_then(Value::as_str) == Some(id))
        {
            return Err(changed());
        }
        let current = capture(
            c,
            &self.company,
            self.existing.as_ref(),
            self.include_defaults,
        )
        .await?;
        let mut matches = 0;
        let retained: Vec<_> = current
            .rows
            .into_iter()
            .filter(|(k, v)| {
                let inserted = k == kind && v.get("id").and_then(Value::as_str) == Some(id);
                if inserted {
                    matches += 1;
                }
                !inserted
            })
            .collect();
        if matches != 1 || retained != self.image.rows {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_parts_posting_references(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_charts,finance_accounts,finance_sales_defaults,company_money_settings,company_currency_scales,finance_invoices,finance_payments,finance_journals IN SHARE MODE").execute(c).await?;
    Ok(())
}
pub async fn read_service_parts_posting_references(
    c: &mut PgConnection,
    company: &CompanyId,
    existing: Option<&ServicePartsPostingMapping>,
) -> Result<CurrentServicePartsPostingReferences, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company, existing, true).await?;
    Ok(CurrentServicePartsPostingReferences {
        company: company.clone(),
        existing: existing.cloned(),
        include_defaults: true,
        unit,
        image,
    })
}
/// Exact prerequisite sources exclude unrelated Sales-default rows.
pub async fn read_service_parts_prerequisite_finance(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<CurrentServicePartsPostingReferences, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company, None, false).await?;
    Ok(CurrentServicePartsPostingReferences {
        company: company.clone(),
        existing: None,
        include_defaults: false,
        unit,
        image,
    })
}
/// Lock only Finance tables in the retained prerequisite source set.
pub async fn lock_service_parts_prerequisite_finance(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_charts,finance_accounts,company_money_settings,company_currency_scales,finance_invoices,finance_payments,finance_journals IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    existing: Option<&ServicePartsPostingMapping>,
    include_defaults: bool,
) -> Result<Image, Error> {
    // The budget is checked in SQL before transferring owner rows.
    let source_sql = if include_defaults {
        "WITH source AS MATERIALIZED (SELECT 'chart' kind,c.id key,to_jsonb(c) body FROM finance_charts c WHERE c.company_id=$1 UNION ALL SELECT 'account',a.id,to_jsonb(a) FROM finance_accounts a WHERE a.company_id=$1 UNION ALL SELECT 'default',d.id,to_jsonb(d) FROM finance_sales_defaults d WHERE d.company_id=$1 UNION ALL SELECT 'settings',s.company_id,to_jsonb(s) FROM company_money_settings s WHERE s.company_id=$1 UNION ALL SELECT 'scale',s.currency,to_jsonb(s) FROM company_currency_scales s WHERE s.company_id=$1 UNION ALL SELECT 'invoice',i.id,to_jsonb(i) FROM finance_invoices i WHERE i.company_id=$1 UNION ALL SELECT 'payment',p.id,to_jsonb(p) FROM finance_payments p WHERE p.company_id=$1 UNION ALL SELECT 'journal',j.id,to_jsonb(j) FROM finance_journals j WHERE j.company_id=$1), measured AS (SELECT *,sum(octet_length(body::text)+octet_length(key)) OVER () bytes,count(*) OVER () count,row_number() OVER (ORDER BY kind,CASE WHEN kind='default' THEN (body->>'created_at')::bigint END DESC NULLS LAST,key) ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN kind END kind,CASE WHEN bytes<=2097152 AND count<=10000 THEN body::text END body FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY kind,CASE WHEN kind='default' THEN (body->>'created_at')::bigint END DESC NULLS LAST,key"
    } else {
        "WITH source AS MATERIALIZED (SELECT 'chart' kind,c.id key,to_jsonb(c) body FROM finance_charts c WHERE c.company_id=$1 UNION ALL SELECT 'account',a.id,to_jsonb(a) FROM finance_accounts a WHERE a.company_id=$1 UNION ALL SELECT 'settings',s.company_id,to_jsonb(s) FROM company_money_settings s WHERE s.company_id=$1 UNION ALL SELECT 'scale',s.currency,to_jsonb(s) FROM company_currency_scales s WHERE s.company_id=$1 UNION ALL SELECT 'invoice',i.id,to_jsonb(i) FROM finance_invoices i WHERE i.company_id=$1 UNION ALL SELECT 'payment',p.id,to_jsonb(p) FROM finance_payments p WHERE p.company_id=$1 UNION ALL SELECT 'journal',j.id,to_jsonb(j) FROM finance_journals j WHERE j.company_id=$1), measured AS (SELECT *,sum(octet_length(body::text)+octet_length(key)) OVER () bytes,count(*) OVER () count,row_number() OVER (ORDER BY kind,CASE WHEN kind='default' THEN (body->>'created_at')::bigint END DESC NULLS LAST,key) ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN kind END kind,CASE WHEN bytes<=2097152 AND count<=10000 THEN body::text END body FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY kind,CASE WHEN kind='default' THEN (body->>'created_at')::bigint END DESC NULLS LAST,key"
    };
    let records = sqlx::query(source_sql)
        .bind(company.as_str())
        .fetch_all(&mut *c)
        .await?;
    let mut rows = Vec::with_capacity(records.len());
    for r in records {
        let kind = r
            .try_get::<Option<String>, _>("kind")?
            .ok_or_else(limited)?;
        let body = r
            .try_get::<Option<String>, _>("body")?
            .ok_or_else(limited)?;
        let value = serde_json::from_str(&body).map_err(|_| limited())?;
        rows.push((kind, value));
    }
    let charts = values(&rows, "chart");
    let accounts = values(&rows, "account");
    let defaults = values(&rows, "default");
    let (approved_sales_defaults, expected_sales_defaults, defaults_blocked) =
        select_defaults(&defaults, &charts, &accounts, existing)?;
    let data = ServicePartsPostingReferences {
        charts,
        accounts,
        defaults_blocked,
        approved_sales_defaults,
        expected_sales_defaults,
        reference_image: if include_defaults {
            reference_image(c, company).await?
        } else {
            prerequisite_reference_image(c, company).await?
        },
        history_image: history_image(c, company).await?,
        finance_history: rows
            .iter()
            .any(|(k, _)| matches!(k.as_str(), "invoice" | "payment" | "journal")),
    };
    Ok(Image { rows, data })
}
// Preserve PostgreSQL's aggregate ordering, jsonb text and NULL-on-empty semantics.
async fn history_image(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<ServicePartsPostingFinanceHistory, Error> {
    let row = sqlx::query("WITH images AS MATERIALIZED (SELECT (SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id)::text FROM finance_invoices i WHERE i.company_id=$1) invoices,(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id)::text FROM finance_payments p WHERE p.company_id=$1) payments,(SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id)::text FROM finance_journals j WHERE j.company_id=$1) journals), bounded AS (SELECT *,coalesce(octet_length(invoices),0)+coalesce(octet_length(payments),0)+coalesce(octet_length(journals),0)<=4194304 ok FROM images) SELECT ok,CASE WHEN ok THEN invoices END invoices,CASE WHEN ok THEN payments END payments,CASE WHEN ok THEN journals END journals FROM bounded").bind(company.as_str()).fetch_one(c).await?;
    if !row.try_get::<bool, _>("ok")? {
        return Err(limited());
    }
    Ok(ServicePartsPostingFinanceHistory {
        invoices: row.try_get("invoices")?,
        payments: row.try_get("payments")?,
        journals: row.try_get("journals")?,
    })
}
// Recreate the retained reference context image from PostgreSQL source strings.
// SQL ordering and jsonb text are significant to the existing context key.
async fn reference_image(c: &mut PgConnection, company: &CompanyId) -> Result<String, Error> {
    let row = sqlx::query("WITH images AS MATERIALIZED (SELECT COALESCE((SELECT jsonb_agg(to_jsonb(c)::text ORDER BY c.id) FROM finance_charts c WHERE c.company_id=$1),'[]'::jsonb)::text charts,COALESCE((SELECT jsonb_agg(to_jsonb(a)::text ORDER BY a.id) FROM finance_accounts a WHERE a.company_id=$1),'[]'::jsonb)::text accounts,COALESCE((SELECT jsonb_agg(to_jsonb(d)::text ORDER BY d.id) FROM finance_sales_defaults d WHERE d.company_id=$1),'[]'::jsonb)::text defaults,jsonb_build_object('settings',(SELECT to_jsonb(s) FROM company_money_settings s WHERE s.company_id=$1),'scales',COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY s.currency) FROM company_currency_scales s WHERE s.company_id=$1),'[]'::jsonb))::text money) SELECT CASE WHEN octet_length(charts)+octet_length(accounts)+octet_length(defaults)+octet_length(money)<=4194304 THEN charts END charts,CASE WHEN octet_length(charts)+octet_length(accounts)+octet_length(defaults)+octet_length(money)<=4194304 THEN accounts END accounts,CASE WHEN octet_length(charts)+octet_length(accounts)+octet_length(defaults)+octet_length(money)<=4194304 THEN defaults END defaults,CASE WHEN octet_length(charts)+octet_length(accounts)+octet_length(defaults)+octet_length(money)<=4194304 THEN money END money FROM images").bind(company.as_str()).fetch_one(c).await?;
    let get = |name| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(name)?.ok_or_else(limited)
    };
    let charts: Vec<String> = serde_json::from_str(&get("charts")?).map_err(|_| limited())?;
    let accounts: Vec<String> = serde_json::from_str(&get("accounts")?).map_err(|_| limited())?;
    let defaults: Vec<String> = serde_json::from_str(&get("defaults")?).map_err(|_| limited())?;
    let inner = serde_json::to_string(&(charts, accounts)).map_err(|_| limited())?;
    let sales = serde_json::to_string(&defaults).map_err(|_| limited())?;
    serde_json::to_string(&(inner, get("money")?, sales)).map_err(|_| limited())
}
async fn prerequisite_reference_image(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<String, Error> {
    let row=sqlx::query("WITH images AS MATERIALIZED (SELECT COALESCE((SELECT jsonb_agg(to_jsonb(c)::text ORDER BY c.id) FROM finance_charts c WHERE c.company_id=$1),'[]'::jsonb)::text charts,COALESCE((SELECT jsonb_agg(to_jsonb(a)::text ORDER BY a.id) FROM finance_accounts a WHERE a.company_id=$1),'[]'::jsonb)::text accounts) SELECT CASE WHEN octet_length(charts)+octet_length(accounts)<=4194304 THEN charts END charts,CASE WHEN octet_length(charts)+octet_length(accounts)<=4194304 THEN accounts END accounts FROM images").bind(company.as_str()).fetch_one(c).await?;
    let get = |name| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(name)?.ok_or_else(limited)
    };
    let charts: Vec<String> = serde_json::from_str(&get("charts")?).map_err(|_| limited())?;
    let accounts: Vec<String> = serde_json::from_str(&get("accounts")?).map_err(|_| limited())?;
    let inner = serde_json::to_string(&(charts, accounts)).map_err(|_| limited())?;
    serde_json::to_string(&(inner, String::new(), String::from("[]"))).map_err(|_| limited())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PARTS_POSTING_SOURCE_CHANGED",
        "The Finance posting source changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SERVICE_PARTS_POSTING_SOURCE_LIMIT",
        "The Finance posting source exceeds its bounded read.",
    )
    .into()
}
