//! Unmounted Postgres driver materialization probes, after replay. These do not
//! authorize commands. Parallel detail error precedence remains pending under
//! retire.procurement.rfq-writers; do not assemble these into a route result.
use sqlx::{Column, PgConnection, Row, TypeInfo, ValueRef, postgres::PgRow};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RfqMaterialization {
    Safe,
    NumberOutOfRange,
    UnsupportedNumeric,
}

fn inspect_rows(rows: &[PgRow]) -> Result<RfqMaterialization, sqlx::Error> {
    const MAX: i64 = 9_007_199_254_740_991;
    for row in rows {
        for (index, column) in row.columns().iter().enumerate() {
            if row.try_get_raw(index)?.is_null() {
                continue;
            }
            match column.type_info().name() {
                "INT8" => {
                    let value: i64 = row.try_get(index)?;
                    if !(-MAX..=MAX).contains(&value) {
                        return Ok(RfqMaterialization::NumberOutOfRange);
                    }
                }
                // The TS driver applies safeNumber to NUMERIC too. This stage
                // does not claim parity for an unexpected NUMERIC projection.
                "NUMERIC" => return Ok(RfqMaterialization::UnsupportedNumeric),
                _ => {}
            }
        }
    }
    Ok(RfqMaterialization::Safe)
}

/// Exact RFQ get projection and scope. None represents the original 404 path;
/// call only after the original purchase admission and replay ordering.
pub(crate) async fn rfq_row_materialization(
    c: &mut PgConnection,
    id: &str,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<Option<RfqMaterialization>, sqlx::Error> {
    let rows=sqlx::query("SELECT r.* FROM procurement_rfqs r WHERE r.id=$1 AND r.company_id=$2 AND ($3::text IS NULL OR r.branch_id=$3) AND ($4::text IS NULL OR r.department_id=$4)")
        .bind(id).bind(company).bind(branch).bind(department).fetch_all(c).await?;
    if rows.is_empty() {
        Ok(None)
    } else {
        Ok(Some(inspect_rows(&rows)?))
    }
}

/// Records detail's SELECT * runs before requestAccess; sourceState runs after
/// RFQ detail and before sourceBlocked. The caller must preserve that order.
pub(crate) async fn rfq_records_materialization(
    c: &mut PgConnection,
    id: &str,
    company: &str,
    detail: bool,
) -> Result<Option<RfqMaterialization>, sqlx::Error> {
    use erpx_server_records::public::rfq_request_source::{
        rfq_request_detail_row_sql, rfq_request_source_state_sql,
    };
    let sql = if detail {
        rfq_request_detail_row_sql()
    } else {
        rfq_request_source_state_sql()
    };
    let rows = sqlx::query(sql).bind(id).bind(company).fetch_all(c).await?;
    if rows.is_empty() {
        Ok(None)
    } else {
        Ok(Some(inspect_rows(&rows)?))
    }
}

/// Inspect one of the four original detail projections. These individual
/// results must not be combined into an authoritative detail response until
/// Promise.all error ordering and all payload conversion are implemented.
pub(crate) enum RfqDetailProjection {
    Lines,
    Offers,
    OfferLines,
    Events,
}

/// Source observation only; a successful result is not the full TS detail
/// payload and a raw SQL error must reach the eventual command error boundary.
#[derive(Debug)]
pub(crate) enum RfqDetailAndSourceProbe {
    DetailNumberOutOfRange,
    NumericMaterializationPending,
    Sql(sqlx::Error),
    Source(Option<RfqMaterialization>),
}

/// Enabled SERIALIZABLE client only, at the position after the RFQ get. pg's
/// single client executes the four Promise.all submissions in order. Finish
/// every submission even when JS number decoding fails: a later SQL failure
/// still aborts the transaction. Preserve the first settled error, and query
/// sourceState only when all four complete successfully. Disabled pool mode
/// has different error/transaction semantics and cannot use this function.
pub(crate) async fn rfq_enabled_detail_then_source_state(
    c: &mut PgConnection,
    id: &str,
    request: Option<&str>,
    company: &str,
) -> RfqDetailAndSourceProbe {
    use RfqDetailAndSourceProbe as Probe;
    let mut first = None;
    for projection in [
        RfqDetailProjection::Lines,
        RfqDetailProjection::Offers,
        RfqDetailProjection::OfferLines,
        RfqDetailProjection::Events,
    ] {
        let result = rfq_detail_materialization(&mut *c, id, projection).await;
        let failure = match result {
            Ok(RfqMaterialization::Safe) => None,
            Ok(RfqMaterialization::NumberOutOfRange) => Some(Probe::DetailNumberOutOfRange),
            Ok(RfqMaterialization::UnsupportedNumeric) => {
                Some(Probe::NumericMaterializationPending)
            }
            Err(error) => Some(Probe::Sql(error)),
        };
        if first.is_none() {
            first = failure;
        }
    }
    if let Some(failure) = first {
        return failure;
    }
    match request.filter(|id| !id.is_empty()) {
        None => Probe::Source(None),
        Some(request) => match rfq_records_materialization(c, request, company, false).await {
            Ok(value) => Probe::Source(value),
            Err(error) => Probe::Sql(error),
        },
    }
}
pub(crate) async fn rfq_detail_materialization(
    c: &mut PgConnection,
    id: &str,
    projection: RfqDetailProjection,
) -> Result<RfqMaterialization, sqlx::Error> {
    let sql = match projection {
        RfqDetailProjection::Lines => {
            "SELECT * FROM procurement_rfq_lines WHERE rfq_id=$1 ORDER BY ordinal"
        }
        RfqDetailProjection::Offers => {
            "SELECT o.*,p.name supplier_name FROM procurement_rfq_offers o JOIN (SELECT \"id\",\"company_id\",\"kind\",\"code\",\"name\",\"currency\",\"tax_registration\",\"payment_terms_days\",\"credit_limit_minor\",\"status\",\"version\",\"created_at\",\"updated_at\",\"person_id\",\"notification_email\",\"notification_consent\",\"merged_into_id\",\"merged_at\",\"merged_by\",\"phone\",\"address_json\" FROM finance_parties) p ON p.id=o.supplier_id WHERE o.rfq_id=$1 ORDER BY o.rfq_version DESC"
        }
        RfqDetailProjection::OfferLines => {
            "SELECT l.* FROM procurement_rfq_offer_lines l JOIN procurement_rfq_offers o ON o.id=l.offer_id WHERE o.rfq_id=$1"
        }
        RfqDetailProjection::Events => {
            "SELECT * FROM procurement_rfq_events WHERE rfq_id=$1 ORDER BY version DESC"
        }
    };
    let rows = sqlx::query(sql).bind(id).fetch_all(c).await?;
    inspect_rows(&rows)
}
