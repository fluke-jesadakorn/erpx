//! sqlx execution for the lane-40 Marketing read family. The statements start
//! from the byte-equivalent TypeScript projections, then keep the Company,
//! branch, department and form-provenance predicates on the owner side. Only
//! Marketing-owned tables are read. No lock is taken and no statement is
//! accepted from the wire.
use crate::adapters::native_lane_40::application::{Lane40ReadFuture, Lane40Reader};
use crate::adapters::native_lane_40::domain::{
    Lane40ReadResult, Lane40Request, RD_ACTIVE_FORM_SOURCE, RD_CAMPAIGN_SUMMARY,
    RD_CANDIDATE_DETAIL, RD_CANDIDATE_HISTORY, RD_CANDIDATE_LIST, RD_FORM_CONTENT_ROW,
    RD_PUBLIC_RECEIPT, unavailable,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

/// The retained form-provenance guard, rendered from owner-local tables only.
/// It is the SQL equivalent of the TypeScript `marketingCandidateReadFilter`
/// when package enforcement is active. No cross-owner table is involved.
const PROVENANCE_GUARD: &str = "EXISTS(SELECT 1 FROM (\
 SELECT candidate.* FROM marketing_lead_candidates candidate\
 JOIN marketing_assets form ON form.id=candidate.form_id AND form.company_id=candidate.company_id AND form.kind='form' AND form.version>=candidate.form_version\
 JOIN marketing_asset_events captured ON captured.asset_id=form.id AND captured.version=candidate.form_version AND captured.action='activate' AND captured.status='active'\
 JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'\
 WHERE length(trim(candidate.consent_evidence))>=3) native_candidate\
 WHERE native_candidate.id=c.id AND native_candidate.company_id=c.company_id)";

const CANDIDATE_PROJECTION: &str =
    "(to_jsonb(c) || jsonb_build_object('form_name',a.name))::text AS value";
const CANDIDATE_FROM: &str = " FROM marketing_lead_candidates c JOIN marketing_assets a ON a.id=c.form_id AND a.company_id=c.company_id";
const SCOPE_TAIL: &str =
    " AND ($2::text IS NULL OR c.branch_id=$2) AND ($3::text IS NULL OR c.department_id=$3)";

/// Rendered workspace candidate list. `$4` is never emitted: the limit has
/// already been validated to `1..=5000` by the application layer, so inlining
/// it cannot introduce query text.
pub fn candidate_list_sql(enforced: bool, limit: Option<i64>) -> String {
    let guard = if enforced { PROVENANCE_GUARD } else { "TRUE" };
    let mut sql = format!(
        "SELECT {CANDIDATE_PROJECTION}{CANDIDATE_FROM} WHERE c.company_id=$1 AND {guard}{SCOPE_TAIL} ORDER BY c.updated_at DESC"
    );
    if let Some(limit) = limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    }
    sql
}

/// Rendered candidate detail. The detail reader is preceded by the caller's
/// purchased-source witness, so it keeps its original unguarded projection.
pub fn candidate_detail_sql() -> String {
    format!(
        "SELECT {CANDIDATE_PROJECTION}{CANDIDATE_FROM} WHERE c.id=$4 AND c.company_id=$1{SCOPE_TAIL}"
    )
}

/// Rendered engagement history. The join to the candidate makes the selected
/// Company authoritative even though the retained reader bound only the id.
pub fn candidate_history_sql() -> String {
    // Engagement rows have no company column; the joined candidate supplies
    // the authoritative Company in the response for the owner scope check.
    concat!(
        "SELECT (to_jsonb(e) || jsonb_build_object('company_id',c.company_id))::text AS value ",
        "FROM marketing_engagement_events e ",
        "JOIN marketing_lead_candidates c ON c.id=e.candidate_id AND c.company_id=$2 ",
        "WHERE e.candidate_id=$1 ORDER BY e.version"
    )
    .to_owned()
}

/// Rendered attribution aggregate. The `COALESCE` defaults and status
/// partitioning are byte-equivalent to the retired TypeScript statement;
/// Rust revalidates the partition before the response is returned.
pub fn campaign_summary_sql(enforced: bool) -> String {
    let guard = if enforced { PROVENANCE_GUARD } else { "TRUE" };
    format!(
        "SELECT COALESCE(NULLIF(BTRIM(c.attribution_json::jsonb->>'campaign'),''),'Unspecified') campaign,\
         COALESCE(NULLIF(BTRIM(c.attribution_json::jsonb->>'source'),''),NULLIF(BTRIM(c.attribution_json::jsonb->>'utmSource'),''),'Unspecified') source,\
         COUNT(*)::bigint captured,COUNT(*) FILTER (WHERE c.status='pending')::bigint pending,\
         COUNT(*) FILTER (WHERE c.status='handed_off')::bigint handed_off,\
         COUNT(*) FILTER (WHERE c.status='rejected')::bigint rejected,\
         $1::text company_id\
         FROM marketing_lead_candidates c WHERE c.company_id=$1 AND {guard}{SCOPE_TAIL}\
         GROUP BY 1,2 ORDER BY COUNT(*) DESC,campaign,source"
    )
}

/// Wave 2: the active-form source projection. The single JSON text column
/// carries `company_id` in the response so the owner's company-scope invariant
/// covers every returned row; `fetch_text` only decodes the first column.
pub fn active_form_source_sql() -> String {
    concat!(
        "SELECT (jsonb_build_object('configuration_json',a.configuration_json,'company_id',a.company_id))::text AS value ",
        "FROM marketing_assets a ",
        "WHERE a.id=$2 AND a.company_id=$1 AND a.version=$3 AND a.kind='form' AND a.status='active'"
    )
    .to_owned()
}

/// Wave 2: the active form-content row projection with the same company scope.
pub fn form_content_row_sql() -> String {
    concat!(
        "SELECT (jsonb_build_object('id',a.id,'version',a.version,'preview_json',a.preview_json,'company_id',a.company_id))::text AS value ",
        "FROM marketing_assets a ",
        "WHERE a.id=$2 AND a.company_id=$1 AND a.version=$3 AND a.kind='content' AND a.status='active'"
    )
    .to_owned()
}

/// The public-submission receipt read. The receipt table has no company
/// column, so the joined candidate supplies the authoritative Company.
pub fn public_receipt_sql() -> String {
    concat!(
        "SELECT (jsonb_build_object('request_hash',receipt.request_hash,'candidate_id',receipt.candidate_id,'company_id',candidate.company_id))::text AS value ",
        "FROM marketing_public_receipts receipt ",
        "JOIN marketing_lead_candidates candidate ON candidate.id=receipt.candidate_id ",
        "WHERE receipt.key=$2 AND candidate.company_id=$1"
    )
    .to_owned()
}

/// Pool-backed owner reader. Every read runs on an owner transaction leased
/// from the pool; there is no advisory lock and no cross-owner table access.
pub struct PgLane40Reader {
    pool: DatabasePool,
}

impl PgLane40Reader {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn from_transaction(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

async fn fetch_text(
    connection: &mut PgConnection,
    sql: String,
    binds: Vec<Option<String>>,
) -> Result<Vec<Value>, BusinessError> {
    let mut statement = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql));
    for bind in binds {
        statement = statement.bind(bind);
    }
    statement
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| unavailable())?
        .into_iter()
        .map(|raw| serde_json::from_str(&raw).map_err(|_| unavailable()))
        .collect()
}

async fn fetch_summary(
    connection: &mut PgConnection,
    sql: String,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
) -> Result<Vec<Value>, BusinessError> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(company_id)
        .bind(branch_id)
        .bind(department_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| unavailable())?
        .into_iter()
        .map(|row| {
            Ok(json!({
                "company_id": row.try_get::<String, _>("company_id").map_err(|_| unavailable())?,
                "campaign": row.try_get::<String, _>("campaign").map_err(|_| unavailable())?,
                "source": row.try_get::<String, _>("source").map_err(|_| unavailable())?,
                "captured": row.try_get::<i64, _>("captured").map_err(|_| unavailable())?,
                "pending": row.try_get::<i64, _>("pending").map_err(|_| unavailable())?,
                "handed_off": row.try_get::<i64, _>("handed_off").map_err(|_| unavailable())?,
                "rejected": row.try_get::<i64, _>("rejected").map_err(|_| unavailable())?,
            }))
        })
        .collect()
}

impl Lane40Reader for PgLane40Reader {
    fn read<'a>(&'a self, request: Lane40Request) -> Lane40ReadFuture<'a, Lane40ReadResult> {
        Box::pin(async move {
            let kind = request.kind;
            let company_id = request.company_id.clone();
            let unit = PgUnitOfWork::new(self.pool.clone());
            let rows = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let request = request.clone();
                    Box::pin(async move {
                        let company = request.company_id.clone();
                        let branch = request.branch_id.clone();
                        let department = request.department_id.clone();
                        let rows = match request.kind {
                            RD_CANDIDATE_LIST => {
                                fetch_text(
                                    connection,
                                    candidate_list_sql(request.enforced, request.limit),
                                    vec![Some(company), branch, department],
                                )
                                .await
                            }
                            RD_CANDIDATE_DETAIL => {
                                fetch_text(
                                    connection,
                                    candidate_detail_sql(),
                                    vec![Some(company), branch, department, request.record_id],
                                )
                                .await
                            }
                            RD_CANDIDATE_HISTORY => {
                                fetch_text(
                                    connection,
                                    candidate_history_sql(),
                                    vec![request.record_id, Some(company)],
                                )
                                .await
                            }
                            RD_CAMPAIGN_SUMMARY => {
                                fetch_summary(
                                    connection,
                                    campaign_summary_sql(request.enforced),
                                    &company,
                                    branch.as_deref(),
                                    department.as_deref(),
                                )
                                .await
                            }
                            RD_ACTIVE_FORM_SOURCE | RD_FORM_CONTENT_ROW => {
                                let sql = if request.kind == RD_ACTIVE_FORM_SOURCE {
                                    active_form_source_sql()
                                } else {
                                    form_content_row_sql()
                                };
                                fetch_text(
                                    connection,
                                    sql,
                                    vec![
                                        Some(company),
                                        request.asset_id,
                                        request.version.map(|version| version.to_string()),
                                    ],
                                )
                                .await
                            }
                            RD_PUBLIC_RECEIPT => {
                                fetch_text(
                                    connection,
                                    public_receipt_sql(),
                                    vec![Some(company), request.record_id],
                                )
                                .await
                            }
                            _ => Err(unavailable()),
                        };
                        rows.map_err(PgTransactionError::Business)
                    })
                })
                .await
                .map_err(from_transaction)?;
            Ok(Lane40ReadResult {
                kind,
                company_id,
                rows,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Count distinct bind slots, not textual occurrences: `$2::text` and
    /// `=$2` are the same positional bind and must be counted once.
    fn bind_slots(sql: &str) -> usize {
        let bytes = sql.as_bytes();
        let mut slots = std::collections::BTreeSet::new();
        for index in 0..bytes.len() {
            if bytes[index] == b'$' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
                let mut end = index + 1;
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
                slots.insert(sql[index + 1..end].to_owned());
            }
        }
        slots.len()
    }

    #[test]
    fn candidate_list_oracle_pins_predicates_bind_order_and_limit() {
        let plain = candidate_list_sql(false, None);
        assert_eq!(
            plain,
            concat!(
                "SELECT (to_jsonb(c) || jsonb_build_object('form_name',a.name))::text AS value",
                " FROM marketing_lead_candidates c JOIN marketing_assets a ON a.id=c.form_id AND a.company_id=c.company_id",
                " WHERE c.company_id=$1 AND TRUE",
                " AND ($2::text IS NULL OR c.branch_id=$2) AND ($3::text IS NULL OR c.department_id=$3)",
                " ORDER BY c.updated_at DESC"
            )
        );
        assert_eq!(bind_slots(&plain), 3);
        let enforced = candidate_list_sql(true, Some(25));
        assert!(enforced.contains(PROVENANCE_GUARD));
        assert!(enforced.ends_with("LIMIT 25"));
        assert_eq!(bind_slots(&enforced), 3);
    }

    #[test]
    fn detail_history_and_summary_pin_bind_order_and_owner_tables() {
        let detail = candidate_detail_sql();
        assert!(detail.contains("c.id=$4"));
        assert!(detail.contains("c.company_id=$1"));
        assert_eq!(bind_slots(&detail), 4);
        let history = candidate_history_sql();
        assert_eq!(
            history,
            "SELECT (to_jsonb(e) || jsonb_build_object('company_id',c.company_id))::text AS value FROM marketing_engagement_events e JOIN marketing_lead_candidates c ON c.id=e.candidate_id AND c.company_id=$2 WHERE e.candidate_id=$1 ORDER BY e.version"
        );
        assert!(history.contains("e.candidate_id=$1"));
        assert!(history.contains("c.company_id=$2"));
        assert_eq!(bind_slots(&history), 2);
        let summary = campaign_summary_sql(false);
        assert!(summary.contains("COUNT(*) FILTER (WHERE c.status='pending')"));
        assert!(summary.contains("'Unspecified'"));
        assert!(summary.contains("GROUP BY 1,2"));
        assert!(summary.contains("company_id"));
        assert_eq!(bind_slots(&summary), 3);
        assert!(campaign_summary_sql(true).contains(PROVENANCE_GUARD));
        let plain = plain();
        for sql in [&plain, &detail, &history, &summary] {
            assert!(!sql.contains(" FOR UPDATE"));
            assert!(!sql.contains("LOCK TABLE"));
            for table in [
                "crm_leads",
                "command_receipts",
                "commerce_",
                "finance_",
                "organization_",
                "pms_",
            ] {
                assert!(!sql.contains(table), "{table}");
            }
        }
    }

    #[test]
    fn active_form_sql_oracles_pin_company_asset_version_bind_order() {
        let source = active_form_source_sql();
        assert_eq!(
            source,
            "SELECT (jsonb_build_object('configuration_json',a.configuration_json,'company_id',a.company_id))::text AS value FROM marketing_assets a WHERE a.id=$2 AND a.company_id=$1 AND a.version=$3 AND a.kind='form' AND a.status='active'"
        );
        assert_eq!(bind_slots(&source), 3);
        let content = form_content_row_sql();
        assert_eq!(
            content,
            "SELECT (jsonb_build_object('id',a.id,'version',a.version,'preview_json',a.preview_json,'company_id',a.company_id))::text AS value FROM marketing_assets a WHERE a.id=$2 AND a.company_id=$1 AND a.version=$3 AND a.kind='content' AND a.status='active'"
        );
        assert_eq!(bind_slots(&content), 3);
        for sql in [&source, &content] {
            assert!(sql.contains("::text AS value"));
            assert!(sql.contains("'company_id',a.company_id"));
            assert!(sql.contains("a.company_id=$1"));
            assert!(sql.contains("a.status='active'"));
            assert!(!sql.contains(" FOR UPDATE"));
        }
    }

    #[test]
    fn public_receipt_oracle_pins_company_scoped_key_lookup() {
        let sql = public_receipt_sql();
        assert_eq!(
            sql,
            "SELECT (jsonb_build_object('request_hash',receipt.request_hash,'candidate_id',receipt.candidate_id,'company_id',candidate.company_id))::text AS value FROM marketing_public_receipts receipt JOIN marketing_lead_candidates candidate ON candidate.id=receipt.candidate_id WHERE receipt.key=$2 AND candidate.company_id=$1"
        );
        assert_eq!(bind_slots(&sql), 2);
        assert!(sql.contains("::text AS value"));
        assert!(sql.contains("'company_id',candidate.company_id"));
        assert!(sql.contains("candidate.company_id=$1"));
    }

    fn plain() -> String {
        candidate_list_sql(false, None)
    }
}
