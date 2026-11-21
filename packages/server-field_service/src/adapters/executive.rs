//! Field owns its report candidates. Organization and People labels, current
//! selected IAM and per-record Records visibility are composed outside it.
use crate::domain::executive::*;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FIELD_REPORT_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn rows(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    q: &FieldExecutiveQuery,
    record_id: Option<&str>,
) -> Result<Vec<FieldExecutiveCandidate>, Error> {
    q.validate()?;
    let result=sqlx::query(r#"
SELECT row_number() OVER(ORDER BY o.id) source_order,o.id,o.version::bigint version,o.status,o.branch_id,o.department_id,o.technician_id,
 o.scheduled_at,o.visit_completed_at,o.created_at,o.updated_at,
 to_char(to_timestamp((CASE WHEN $4='service-billing-releases' THEN b.created_at ELSE o.scheduled_at END)/1000.0) AT TIME ZONE 'UTC','YYYY-MM') report_month
FROM field_service_orders o LEFT JOIN field_service_billing_releases b ON b.order_id=o.id
WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) AND ($3::text IS NULL OR o.department_id=$3)
 AND (($4='open-service-orders' AND o.status NOT IN('completed','cancelled','billed'))
 OR ($4='service-billing-releases' AND b.created_at>=$5 AND b.created_at<$6)
 OR ($4='field-visit-completion' AND o.scheduled_at IS NOT NULL AND o.status IN('scheduled','dispatched','in_visit','exception','completed','billed') AND o.scheduled_at>=$5 AND o.scheduled_at<$6))
 AND ($7='' OR position(lower($7) in lower(o.id))>0 OR position(lower($7) in lower(o.status))>0)
 AND ($8::bigint IS NULL OR (o.scheduled_at,o.id)<($8,$9))
 AND ($10::text IS NULL OR o.id=$10)
ORDER BY o.scheduled_at DESC NULLS LAST,o.id DESC LIMIT 1001
"#).bind(company.as_str()).bind(branch).bind(department).bind(q.metric.as_str()).bind(q.from_at).bind(q.to_at+86_400_000).bind(&q.query).bind(q.before_at).bind(&q.before_id).bind(record_id).fetch_all(c).await?;
    if result.len() > 1000 {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "FIELD_REPORT_LIMIT",
            "This report exceeds the bounded native source read.",
        )
        .into());
    }
    result
        .into_iter()
        .map(|r| {
            Ok(FieldExecutiveCandidate {
                source_order: r.try_get("source_order")?,
                id: r.try_get("id")?,
                version: r.try_get("version")?,
                status: r.try_get("status")?,
                branch_id: r.try_get("branch_id")?,
                department_id: r.try_get("department_id")?,
                technician_id: r.try_get("technician_id")?,
                scheduled_at: r.try_get("scheduled_at")?,
                visit_completed_at: r.try_get("visit_completed_at")?,
                month: r.try_get("report_month")?,
                created_at: r.try_get("created_at")?,
                updated_at: r.try_get("updated_at")?,
            })
        })
        .collect()
}
pub struct CurrentFieldExecutiveCandidates {
    unit: Unit,
    company: CompanyId,
    branch: Option<String>,
    department: Option<String>,
    query: FieldExecutiveQuery,
    record_id: Option<String>,
    rows: Vec<FieldExecutiveCandidate>,
}
impl CurrentFieldExecutiveCandidates {
    pub fn data(&self) -> &[FieldExecutiveCandidate] {
        &self.rows
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || rows(
                c,
                &self.company,
                self.branch.as_deref(),
                self.department.as_deref(),
                &self.query,
                self.record_id.as_deref(),
            )
            .await?
                != self.rows
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FIELD_REPORT_SOURCE_CHANGED",
                "Reload the current Field report.",
            )
            .into());
        }
        Ok(())
    }
}
pub async fn read_field_executive_candidates(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
    query: FieldExecutiveQuery,
) -> Result<CurrentFieldExecutiveCandidates, Error> {
    read_field_evidence_candidates(c,company,branch,department,query,None).await
}
/// Exact record selection is owned SQL, not a client-side substring filter.
pub async fn read_field_evidence_candidates(
    c:&mut PgConnection,company:&CompanyId,branch:Option<&str>,department:Option<&str>,query:FieldExecutiveQuery,record_id:Option<&str>,
)->Result<CurrentFieldExecutiveCandidates,Error>{
    let unit = unit(c).await?;
    let rows = rows(c, company, branch, department, &query, record_id).await?;
    Ok(CurrentFieldExecutiveCandidates {
        unit,
        company: company.clone(),
        branch: branch.map(str::to_owned),
        department: department.map(str::to_owned),
        query,
        record_id:record_id.map(str::to_owned),
        rows,
    })
}

/// Preserve the database collation used by the original grouped report. Source
/// paging and the Other remainder must use that same exposed group ordering.
pub async fn order_field_metric_rows(
    c: &mut PgConnection,
    rows: Vec<FieldMetricRow>,
) -> Result<Vec<FieldMetricRow>, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_REPORT_SERIALIZABLE_REQUIRED").into());
    }
    let labels = rows.iter().map(|r| r.label.clone()).collect::<Vec<_>>();
    let order: Vec<String> =
        sqlx::query_scalar("SELECT label FROM unnest($1::text[]) AS labels(label) ORDER BY label")
            .bind(labels)
            .fetch_all(c)
            .await?;
    let mut by_label = rows
        .into_iter()
        .map(|r| (r.label.clone(), r))
        .collect::<std::collections::BTreeMap<_, _>>();
    order
        .into_iter()
        .map(|label| {
            by_label.remove(&label).ok_or_else(|| {
                BusinessError::invalid("FIELD_REPORT_SOURCE_INVALID", "The report group changed.")
                    .into()
            })
        })
        .collect()
}
