//! PostgreSQL execution for the native POS executive evidence search.
//!
//! The three source legs and the paged outer query are byte-identical to the
//! TypeScript literals retired from `executive-evidence-sources.ts`. At runtime
//! the `?` placeholders become numbered `$n` binds and numeric columns are
//! rendered as `text` so sqlx can decode the exact wire row uniformly. No row
//! lock, advisory lock or cross-owner projection is introduced.

use crate::adapters::native_lane_25::application::{
    EvidenceFuture, PosEvidenceReader,
};
use crate::adapters::native_lane_25::domain::{
    DAY_CLOSES_KEYS, DETAIL_KIND_PAYMENT, DETAIL_KIND_REFUND, DayClosesQuery, DayClosesRow,
    EvidenceDetailQuery, EvidenceDetailRow, METRIC_NET_POS_SALES, METRIC_POS_PAYMENT_MIX,
    METRIC_POS_SALES, EvidenceQuery, EvidenceRow, valid_day_closes_row, valid_detail_row,
    valid_row,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
#[cfg(test)]
use serde_json::json;
#[cfg(test)]
use sha2::{Digest, Sha256};
use sqlx::{Arguments, Row, postgres::PgArguments};

/// Exact retired TypeScript source leg: POS sales.
pub const LEG_POS_SALES: &str = r#"SELECT 'sale:'||s.id id,s.id source_id,'sale' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,s.state status,s.state sale_status,s.currency,NULL::text method,
      s.total_minor::numeric amount_minor,s.total_minor::numeric contribution_minor,0::numeric refunded_minor,
      s.version revision,s.version sale_version
    FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed'
      AND (?='' OR position(lower(?) in lower(s.number))>0)"#;

/// Exact retired TypeScript source leg: refund rows of the net-sales union.
pub const LEG_REFUND: &str = r#"SELECT 'refund:'||r.id id,r.id source_id,'refund' source_type,s.id sale_id,r.accounting_date event_date,
      s.number receipt_number,r.state status,s.state sale_status,s.currency,NULL::text method,
      r.amount_minor::numeric amount_minor,-r.amount_minor::numeric contribution_minor,0::numeric refunded_minor,
      r.version revision,s.version sale_version
    FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id
    WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?
      AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed'
      AND (?='' OR position(lower(?) in lower(s.number))>0)"#;

/// Exact retired TypeScript source leg: payment mix with refunded sums.
pub const LEG_PAYMENT: &str = r#"SELECT 'payment:'||p.id id,p.id source_id,'payment' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,p.state status,s.state sale_status,s.currency,p.method,
      p.amount_minor::numeric amount_minor,
      (p.amount_minor::numeric-COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)) contribution_minor,
      COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0) refunded_minor,
      p.version revision,s.version sale_version
    FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
    WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed' AND p.state='succeeded'
      AND (?='' OR position(lower(?) in lower(s.number))>0 OR position(lower(?) in lower(p.method))>0)"#;

/// Exact retired TypeScript paged outer query shape (`?` before translation).
pub const OUTER_TEMPLATE: &str = "WITH events AS ({source}) \
    SELECT * FROM events {cursor} ORDER BY event_date DESC,id DESC LIMIT ?";

/// Explicit text projection of the fixed 15-key `EvidenceRow`.
const OUTER_COLUMNS: &str = "SELECT id::text AS id,source_id::text AS source_id,\
 source_type::text AS source_type,sale_id::text AS sale_id,event_date::text AS event_date,\
 receipt_number::text AS receipt_number,status::text AS status,sale_status::text AS sale_status,\
 currency::text AS currency,method::text AS method,amount_minor::text AS amount_minor,\
 contribution_minor::text AS contribution_minor,refunded_minor::text AS refunded_minor,\
 revision::text AS revision,sale_version::text AS sale_version";

// ---- Wave 2: exact retired TypeScript evidence detail literals ----------

pub const REFUND_DETAIL_SQL: &str = r#"SELECT r.id source_id,s.id sale_id,'refund' source_type,s.number receipt_number,
     s.business_date,r.accounting_date,s.currency,r.state,r.amount_minor::text amount_minor,
     (-r.amount_minor::numeric)::text contribution_minor,'0'::text refunded_minor,r.version revision,s.version sale_version,r.branch_id
   FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id
   WHERE r.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?
     AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed' LIMIT 1"#;

pub const PAYMENT_DETAIL_SQL: &str = r#"SELECT p.id source_id,'payment' source_type,s.id sale_id,s.number receipt_number,
     s.business_date,NULL::text accounting_date,s.currency,s.state sale_state,p.state,p.method,p.amount_minor::text amount_minor,
     COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
       WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)::text refunded_minor,
     (p.amount_minor::numeric-COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
       WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0))::text contribution_minor,
     p.version revision,s.version sale_version,s.branch_id,p.created_at
   FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
   WHERE p.id=? AND p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
     AND s.business_date BETWEEN ? AND ? AND s.state='completed' AND p.state='succeeded' LIMIT 1"#;

// ---- Wave 3: exact retired TypeScript day-close report list literal ------

/// Byte-identical retired `readPosDayCloses1` literal from `pos.ts`.
pub const DAY_CLOSES_SQL: &str = "SELECT id,company_id,branch_id,business_date,currency,state,prepared_by,confirmed_by,version,snapshot_json::text FROM pos_day_closes WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY business_date DESC LIMIT 90";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Bind {
    Text(String),
    Opt(Option<String>),
    Int(i64),
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_EVIDENCE_READ_UNAVAILABLE",
        "The native POS evidence service is unavailable.",
    )
}

#[cfg(test)]
fn hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Translate only the `?` placeholders into numbered Postgres binds. The
/// selected SQL contains no `?` inside a literal, so the mapping is exact.
fn numbered(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut index = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            index += 1;
            out.push('$');
            out.push_str(&index.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

fn source_leg(metric: &str) -> (&'static str, usize) {
    match metric {
        METRIC_POS_SALES => (LEG_POS_SALES, 8),
        METRIC_NET_POS_SALES => (LEG_REFUND, 8),
        METRIC_POS_PAYMENT_MIX => (LEG_PAYMENT, 9),
        _ => ("", 0),
    }
}

/// Render the owner SQL and its positional binds in bind order. The two legs
/// of `net-pos-sales` are concatenated with `UNION ALL`, preserving each leg's
/// base/search values and the exact placeholder order of the retired TS code.
pub fn render(query: &EvidenceQuery) -> Result<(String, Vec<Bind>), BusinessError> {
    if !matches!(
        query.metric.as_str(),
        METRIC_POS_SALES | METRIC_NET_POS_SALES | METRIC_POS_PAYMENT_MIX
    ) {
        return Err(unavailable());
    }
    let base = || -> Vec<Bind> {
        vec![
            Bind::Text(query.company.clone()),
            Bind::Opt(query.branch.clone()),
            Bind::Opt(query.branch.clone()),
            Bind::Text(query.currency.clone()),
            Bind::Text(query.from.clone()),
            Bind::Text(query.to.clone()),
        ]
    };
    let search = |n: usize| -> Vec<Bind> {
        (0..n).map(|_| Bind::Text(query.query.clone())).collect()
    };

    let source = if query.metric == METRIC_NET_POS_SALES {
        // One `?` sequence spans both legs of the union, so numbering stays
        // contiguous and the board/base/search bind order is preserved.
        let sql = format!("{LEG_POS_SALES}\n    UNION ALL\n    {LEG_REFUND}");
        let mut values = base();
        values.extend(search(2));
        values.extend(base());
        values.extend(search(2));
        (numbered(&sql), values)
    } else {
        let (leg, searches) = source_leg(&query.metric);
        let mut values = base();
        values.extend(search(searches - 6));
        (numbered(leg), values)
    };
    let (source_sql, mut values) = source;

    let offset = values.len();
    let cursor_sql = if query.cursor.is_some() {
        values.push(Bind::Text(query.cursor.as_ref().unwrap().event_date.clone()));
        values.push(Bind::Text(query.cursor.as_ref().unwrap().event_date.clone()));
        values.push(Bind::Text(query.cursor.as_ref().unwrap().id.clone()));
        format!(
            "WHERE (event_date < ${}::date OR (event_date = ${}::date AND id < ${}))",
            offset + 1,
            offset + 2,
            offset + 3
        )
    } else {
        String::new()
    };
    // The retained application detects `hasMore` from a `limit + 1` row page
    // and truncates to `limit`; keep that exact pagination contract.
    values.push(Bind::Int(query.limit as i64 + 1));
    let limit_index = values.len();
    let sql = format!(
        "WITH events AS ({source_sql}) {OUTER_COLUMNS} FROM events {cursor_sql} \
         ORDER BY event_date DESC,id DESC LIMIT ${limit_index}"
    );
    Ok((sql, values))
}

/// Render one bounded detail read: byte-identical retired SQL with numbered
/// binds in the exact retired order `(id, company, branch, branch, currency,
/// from, to)`.
pub fn render_detail(query: &EvidenceDetailQuery) -> Result<(String, Vec<Bind>), BusinessError> {
    let sql = match query.kind.as_str() {
        DETAIL_KIND_REFUND => REFUND_DETAIL_SQL,
        DETAIL_KIND_PAYMENT => PAYMENT_DETAIL_SQL,
        _ => return Err(unavailable()),
    };
    // The retired SQL returns a mix of text, bigint and timestamp columns.
    // Cast every projected column to text in the wrapper so sqlx can decode the
    // exact fixed key set uniformly; the oracle above stays byte-identical.
    let keys: &[&str] = match query.kind.as_str() {
        DETAIL_KIND_REFUND => &crate::adapters::native_lane_25::domain::REFUND_DETAIL_KEYS,
        _ => &crate::adapters::native_lane_25::domain::PAYMENT_DETAIL_KEYS,
    };
    let projection = keys
        .iter()
        .map(|key| format!("{key}::text AS {key}"))
        .collect::<Vec<_>>()
        .join(",");
    Ok((
        format!("SELECT {projection} FROM ({}) d", numbered(sql)),
        vec![
            Bind::Text(query.id.clone()),
            Bind::Text(query.company.clone()),
            Bind::Opt(query.branch.clone()),
            Bind::Opt(query.branch.clone()),
            Bind::Text(query.currency.clone()),
            Bind::Text(query.from.clone()),
            Bind::Text(query.to.clone()),
        ],
    ))
}

fn detail_projection(
    kind: &str,
    row: &sqlx::postgres::PgRow,
) -> Result<EvidenceDetailRow, BusinessError> {
    let keys: &[&str] = match kind {
        DETAIL_KIND_REFUND => &crate::adapters::native_lane_25::domain::REFUND_DETAIL_KEYS,
        DETAIL_KIND_PAYMENT => &crate::adapters::native_lane_25::domain::PAYMENT_DETAIL_KEYS,
        _ => return Err(unavailable()),
    };
    let mut values = Vec::with_capacity(keys.len());
    for key in keys {
        let value = row
            .try_get::<Option<String>, _>(*key)
            .map_err(|_| unavailable())?;
        values.push(((*key).to_owned(), value));
    }
    let row = EvidenceDetailRow { values };
    if !valid_detail_row(kind, &row) {
        return Err(unavailable());
    }
    Ok(row)
}

/// Render the day-close report list: byte-identical retired SQL wrapped in a
/// uniform text projection, binds in the exact order `(company, branch, branch)`.
pub fn render_day_closes(query: &DayClosesQuery) -> (String, Vec<Bind>) {
    let projection = DAY_CLOSES_KEYS
        .iter()
        .map(|key| format!("{key}::text AS {key}"))
        .collect::<Vec<_>>()
        .join(",");
    (
        format!("SELECT {projection} FROM ({}) d", numbered(DAY_CLOSES_SQL)),
        vec![
            Bind::Text(query.company.clone()),
            Bind::Opt(query.branch.clone()),
            Bind::Opt(query.branch.clone()),
        ],
    )
}

fn day_closes_projection(row: &sqlx::postgres::PgRow) -> Result<DayClosesRow, BusinessError> {
    let mut values = Vec::with_capacity(DAY_CLOSES_KEYS.len());
    for key in DAY_CLOSES_KEYS {
        let value = row
            .try_get::<Option<String>, _>(key)
            .map_err(|_| unavailable())?;
        values.push((key.to_owned(), value));
    }
    let row = DayClosesRow { values };
    if !valid_day_closes_row(&row) {
        return Err(unavailable());
    }
    Ok(row)
}

fn args(binds: &[Bind]) -> Result<PgArguments, BusinessError> {
    let mut args = PgArguments::default();
    for bind in binds {
        match bind {
            Bind::Text(value) => {
                args.add(value.as_str()).map_err(|_| unavailable())?;
            }
            Bind::Opt(value) => {
                args.add(value.as_deref()).map_err(|_| unavailable())?;
            }
            Bind::Int(value) => {
                args.add(*value).map_err(|_| unavailable())?;
            }
        }
    }
    Ok(args)
}

fn projection(row: &sqlx::postgres::PgRow) -> Result<EvidenceRow, BusinessError> {
    let get = |name: &str| -> Result<String, BusinessError> {
        row.try_get::<String, _>(name).map_err(|_| unavailable())
    };
    let row = EvidenceRow {
        id: get("id")?,
        source_id: get("source_id")?,
        source_type: get("source_type")?,
        sale_id: get("sale_id")?,
        event_date: get("event_date")?,
        receipt_number: get("receipt_number")?,
        status: get("status")?,
        sale_status: get("sale_status")?,
        currency: get("currency")?,
        method: row
            .try_get::<Option<String>, _>("method")
            .map_err(|_| unavailable())?,
        amount_minor: get("amount_minor")?,
        contribution_minor: get("contribution_minor")?,
        refunded_minor: get("refunded_minor")?,
        revision: get("revision")?,
        sale_version: get("sale_version")?,
    };
    if !valid_row(&row) {
        return Err(unavailable());
    }
    Ok(row)
}

/// Pool-backed owner reader. The router mounts one of these per registered
/// Holding target; no caller transaction or SQL from TypeScript is required.
pub struct PgEvidenceReader {
    pool: DatabasePool,
}

impl PgEvidenceReader {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PosEvidenceReader for PgEvidenceReader {
    fn search<'a>(&'a self, query: &'a EvidenceQuery) -> EvidenceFuture<'a, Vec<EvidenceRow>> {
        Box::pin(async move {
            let (sql, binds) = render(query)?;
            let arguments = args(&binds)?;
            let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), arguments)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter().map(projection).collect()
        })
    }

    fn detail<'a>(
        &'a self,
        query: &'a EvidenceDetailQuery,
    ) -> EvidenceFuture<'a, Option<EvidenceDetailRow>> {
        Box::pin(async move {
            let (sql, binds) = render_detail(query)?;
            let arguments = args(&binds)?;
            let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), arguments)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            if rows.len() > 1 {
                return Err(unavailable());
            }
            rows.first()
                .map(|row| detail_projection(&query.kind, row))
                .transpose()
        })
    }

    fn day_closes<'a>(&'a self, query: &'a DayClosesQuery) -> EvidenceFuture<'a, Vec<DayClosesRow>> {
        Box::pin(async move {
            let (sql, binds) = render_day_closes(query);
            let arguments = args(&binds)?;
            let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), arguments)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter().map(day_closes_projection).collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_25::domain::{EvidenceCursor, ROW_KEYS};

    fn query(metric: &str) -> EvidenceQuery {
        EvidenceQuery {
            company: "co".into(),
            branch: Some("br".into()),
            metric: metric.into(),
            from: "2026-01-01".into(),
            to: "2026-01-31".into(),
            currency: "USD".into(),
            query: "R1".into(),
            limit: 25,
            cursor: None,
        }
    }

    #[test]
    fn source_legs_are_select_only_and_owner_local() {
        for leg in [LEG_POS_SALES, LEG_REFUND, LEG_PAYMENT] {
            assert!(leg.starts_with("SELECT "));
            for forbidden in [
                "FOR UPDATE",
                "FOR SHARE",
                "LOCK TABLE",
                "pg_advisory",
                "INSERT ",
                "UPDATE ",
                "DELETE ",
                "pms_sources",
                "pms_properties",
                "finance_",
                "inventory_",
                "identity_access",
            ] {
                assert!(
                    !leg.contains(forbidden),
                    "evidence leg must not lock, mutate or cross owners: {forbidden}"
                );
            }
        }
    }

    #[test]
    fn render_uses_numbered_binds_in_exact_order() {
        let cases = [
            (METRIC_POS_SALES, 8),
            (METRIC_NET_POS_SALES, 16),
            (METRIC_POS_PAYMENT_MIX, 9),
        ];
        for (metric, leg_binds) in cases {
            let (sql, binds) = render(&query(metric)).unwrap();
            assert!(sql.starts_with("WITH events AS (SELECT "));
            assert!(sql.contains("ORDER BY event_date DESC,id DESC LIMIT $"));
            assert_eq!(binds.len(), leg_binds + 1);
            assert_eq!(binds[0], Bind::Text("co".into()));
            assert_eq!(binds[1], Bind::Opt(Some("br".into())));
            assert_eq!(binds.last(), Some(&Bind::Int(26)));
            let max = sql
                .split("$")
                .skip(1)
                .filter_map(|part| part.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<usize>().ok())
                .max()
                .unwrap_or(0);
            assert_eq!(max, binds.len());
        }
    }

    #[test]
    fn render_adds_cursor_predicate_before_limit() {
        let mut q = query(METRIC_POS_SALES);
        q.cursor = Some(EvidenceCursor {
            event_date: "2026-01-02".into(),
            id: "sale:s1".into(),
        });
        let (sql, binds) = render(&q).unwrap();
        assert!(sql.contains("WHERE (event_date < $9::date OR (event_date = $10::date AND id < $11))"));
        assert_eq!(binds.len(), 12);
        assert_eq!(binds[8], Bind::Text("2026-01-02".into()));
        assert_eq!(binds[10], Bind::Text("sale:s1".into()));
        assert_eq!(binds[11], Bind::Int(26));
    }

    #[test]
    fn unknown_metric_fails_closed() {
        assert!(render(&query("other")).is_err());
    }

    #[test]
    fn oracle_hashes_pin_sql_and_result_shape() {
        let sql_hashes: Vec<String> = [LEG_POS_SALES, LEG_REFUND, LEG_PAYMENT, OUTER_TEMPLATE]
            .iter()
            .map(|sql| hash(sql))
            .collect();
        assert_eq!(
            sql_hashes,
            vec![
                "bb7d6a9715b214378648b4e02a92b0785006d696f1ea5bdcedcfdb4665cab2fc",
                "d7f8a728675380073c41737d3039f923af1cbb6433790b23aabb89e1b9801f0c",
                "884d6b57c718cea724ddfb722bda04c9a825fb9a53ce52e6db91c5a12507a49b",
                "09c32a0244f20aa378cb53ac0d25f3f075f5e88f4a6c5dcb4b39cf7d3adfb212",
            ]
        );
        let result = json!([{
            "id":"sale:s1","source_id":"s1","source_type":"sale","sale_id":"s1",
            "event_date":"2026-01-02","receipt_number":"R1","status":"completed",
            "sale_status":"completed","currency":"USD","method":null,
            "amount_minor":"100","contribution_minor":"100","refunded_minor":"0",
            "revision":"1","sale_version":"1"
        }]);
        let object = result[0].as_object().unwrap();
        assert_eq!(object.len(), ROW_KEYS.len());
        assert_eq!(
            hash(&serde_json::to_string(&result).unwrap()),
            "d559c7a0ad2adc21773e380d6a812bdf2c749892b0609bd813fc4ebf2307137f"
        );
    }
}

#[cfg(test)]
mod detail_tests {
    use super::*;

    fn query(kind: &str) -> EvidenceDetailQuery {
        EvidenceDetailQuery {
            company: "co".into(),
            branch: Some("br".into()),
            kind: kind.into(),
            id: "r1".into(),
            from: "2026-01-01".into(),
            to: "2026-01-31".into(),
            currency: "USD".into(),
        }
    }

    #[test]
    fn detail_sql_is_owner_local_lock_free_and_byte_stable() {
        for sql in [REFUND_DETAIL_SQL, PAYMENT_DETAIL_SQL] {
            assert!(sql.starts_with("SELECT "));
            assert!(sql.contains(" LIMIT 1"));
            for forbidden in [
                "FOR UPDATE", "FOR SHARE", "LOCK TABLE", "pg_advisory", "INSERT ",
                "UPDATE ", "DELETE ", "pms_sources", "pms_properties", "commerce_",
                "finance_", "inventory_",
            ] {
                assert!(!sql.contains(forbidden), "detail SQL must stay owner-local: {forbidden}");
            }
        }
        assert_eq!(
            hash(REFUND_DETAIL_SQL),
            "4ba411c8e3a749cdd0322fa80bfc6d7a9532768356443c2f7dd67201c368d8b1"
        );
        assert_eq!(
            hash(PAYMENT_DETAIL_SQL),
            "5299bafa4da2d4476f7df990b4214281e0ca2152148c92a11f214b8c506636d5"
        );
    }

    #[test]
    fn detail_render_preserves_bind_order_and_casts_projection() {
        for (kind, binds) in [(DETAIL_KIND_REFUND, 7), (DETAIL_KIND_PAYMENT, 7)] {
            let (sql, values) = render_detail(&query(kind)).unwrap();
            assert!(sql.starts_with("SELECT source_id::text AS source_id,"));
            assert!(sql.contains(" FROM (SELECT "));
            assert_eq!(values.len(), binds);
            assert_eq!(values[0], Bind::Text("r1".into()));
            assert_eq!(values[1], Bind::Text("co".into()));
            assert_eq!(values[2], Bind::Opt(Some("br".into())));
            assert_eq!(values[6], Bind::Text("2026-01-31".into()));
            let max = sql
                .split('$')
                .skip(1)
                .filter_map(|part| part.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<usize>().ok())
                .max()
                .unwrap_or(0);
            assert_eq!(max, binds);
        }
    }

    #[test]
    fn detail_unknown_kind_fails_closed() {
        assert!(render_detail(&query("other")).is_err());
    }
}

#[cfg(test)]
mod day_closes_tests {
    use super::*;

    fn query() -> DayClosesQuery {
        DayClosesQuery {
            company: "co".into(),
            branch: Some("br".into()),
        }
    }

    #[test]
    fn day_closes_sql_is_owner_local_lock_free_and_byte_stable() {
        assert!(DAY_CLOSES_SQL.starts_with("SELECT id,company_id,branch_id,"));
        assert!(DAY_CLOSES_SQL.contains(" ORDER BY business_date DESC LIMIT 90"));
        for forbidden in [
            "FOR UPDATE",
            "FOR SHARE",
            "LOCK TABLE",
            "pg_advisory",
            "INSERT ",
            "UPDATE ",
            "DELETE ",
            "pms_",
            "commerce_",
            "finance_",
            "inventory_",
        ] {
            assert!(
                !DAY_CLOSES_SQL.contains(forbidden),
                "day-close SQL must stay owner-local: {forbidden}"
            );
        }
        assert_eq!(
            hash(DAY_CLOSES_SQL),
            "70c1300ca8e63dd32d0c5763af42c84d27d25504618cbf23f546ffc3725e7807"
        );
    }

    #[test]
    fn day_closes_render_preserves_bind_order_and_casts_projection() {
        let (sql, binds) = render_day_closes(&query());
        assert!(sql.starts_with("SELECT id::text AS id,company_id::text AS company_id,"));
        assert!(sql.contains(" FROM (SELECT "));
        assert_eq!(binds.len(), 3);
        assert_eq!(binds[0], Bind::Text("co".into()));
        assert_eq!(binds[1], Bind::Opt(Some("br".into())));
        assert_eq!(binds[2], Bind::Opt(Some("br".into())));
        let max = sql
            .split('$')
            .skip(1)
            .filter_map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<usize>()
                    .ok()
            })
            .max()
            .unwrap_or(0);
        assert_eq!(max, 3);

        let without_branch = render_day_closes(&DayClosesQuery {
            company: "co".into(),
            branch: None,
        })
        .1;
        assert_eq!(without_branch[1], Bind::Opt(None));
        assert_eq!(without_branch[2], Bind::Opt(None));
    }
}
