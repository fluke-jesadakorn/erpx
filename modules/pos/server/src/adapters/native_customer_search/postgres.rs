//! PostgreSQL execution for the native POS customer-search read.
//!
//! The source statement is byte-identical to the TypeScript literal retired
//! from `PosDayFinanceFacts.readFinanceParties2`, including the escaped
//! `ILIKE` pattern and the deterministic `ORDER BY name,id LIMIT 100` tail.
//! A fixed text/bigint wrapper lets sqlx decode the exact wire row. No row
//! lock, advisory lock or mutation is added and the query is scoped only by
//! the caller's Company value.

use crate::adapters::native_customer_search::application::{
    CustomerFuture, PosCustomerReader,
};
use crate::adapters::native_customer_search::domain::{
    CustomerRow, CustomerSearchQuery, valid_row,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Arguments, Row, postgres::PgArguments};

/// Exact retired TypeScript source statement (`?` before bind numbering).
pub const CUSTOMER_SEARCH_SQL: &str = r#"SELECT id,name,phone,notification_email,address_json,version FROM finance_parties
  WHERE company_id=? AND kind='customer' AND status='active'
  AND (?='' OR name ILIKE ? OR phone ILIKE ? OR notification_email ILIKE ?)
  ORDER BY name,id LIMIT 100"#;

/// Fixed projection of the six source columns into uniform decode types:
/// text for identities/JSON, bigint for the record version. `address_json`
/// stays raw text because the retained runtime registers an identity parser
/// for JSONB, so the wire field is a JSON string on both sides.
const PROJECTION: &str = "SELECT id::text AS id,name::text AS name,phone::text AS phone,\
  notification_email::text AS notification_email,address_json::text AS address_json,\
  version::bigint AS version";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Bind {
    Text(String),
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_POS_CUSTOMERS_UNAVAILABLE",
        "The native POS customer service is unavailable.",
    )
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

/// Mirror the retired pattern: trim, take 100 Unicode scalars, escape every
/// `\`, `%` and `_`, then wrap in `%...%`. The empty query keeps the retired
/// `?=''` short-circuit and therefore needs no pattern at all.
fn pattern(query: &str) -> String {
    let bounded: String = query.trim().chars().take(100).collect();
    if bounded.is_empty() {
        return "%".into();
    }
    let mut escaped = String::with_capacity(bounded.len() + 2);
    escaped.push('%');
    for ch in bounded.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped.push('%');
    escaped
}

/// Render the owner SQL and its five positional binds in the retired order:
/// `(company, q, pattern, pattern, pattern)`.
pub fn render(query: &CustomerSearchQuery) -> (String, Vec<Bind>) {
    let sql = format!("{PROJECTION} FROM ({}) d", numbered(CUSTOMER_SEARCH_SQL));
    let pattern = pattern(&query.q);
    let binds = vec![
        Bind::Text(query.company.clone()),
        Bind::Text(query.q.trim().chars().take(100).collect()),
        Bind::Text(pattern.clone()),
        Bind::Text(pattern.clone()),
        Bind::Text(pattern),
    ];
    (sql, binds)
}

fn args(binds: &[Bind]) -> Result<PgArguments, BusinessError> {
    let mut arguments = PgArguments::default();
    for Bind::Text(value) in binds {
        arguments.add(value.as_str()).map_err(|_| unavailable())?;
    }
    Ok(arguments)
}

fn text(row: &sqlx::postgres::PgRow, key: &str) -> Result<Option<String>, BusinessError> {
    row.try_get::<Option<String>, _>(key)
        .map_err(|_| unavailable())
}

fn required(row: &sqlx::postgres::PgRow, key: &str) -> Result<String, BusinessError> {
    text(row, key)?.ok_or_else(unavailable)
}

fn integer(row: &sqlx::postgres::PgRow, key: &str) -> Result<i64, BusinessError> {
    row.try_get::<Option<i64>, _>(key)
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)
}

fn projection(row: &sqlx::postgres::PgRow) -> Result<CustomerRow, BusinessError> {
    let row = CustomerRow {
        id: required(row, "id")?,
        name: required(row, "name")?,
        phone: text(row, "phone")?,
        notification_email: text(row, "notification_email")?,
        address_json: required(row, "address_json")?,
        version: integer(row, "version")?,
    };
    if !valid_row(&row) {
        return Err(unavailable());
    }
    Ok(row)
}

/// Pool-backed owner reader. The router mounts one of these per registered
/// Holding target; no caller transaction or SQL from TypeScript is required.
pub struct PgCustomerSearchReader {
    pool: DatabasePool,
}

impl PgCustomerSearchReader {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PosCustomerReader for PgCustomerSearchReader {
    fn rows<'a>(&'a self, query: &'a CustomerSearchQuery) -> CustomerFuture<'a, Vec<CustomerRow>> {
        Box::pin(async move {
            let (sql, binds) = render(query);
            let arguments = args(&binds)?;
            let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), arguments)
                .fetch_all(self.pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter().map(projection).collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_customer_search::domain::{
        CustomerActorDecision, CustomerPackageDecision,
    };

    fn query(q: &str) -> CustomerSearchQuery {
        CustomerSearchQuery {
            company: "co".into(),
            q: q.into(),
            actor: CustomerActorDecision {
                read: true,
                department: false,
            },
            package: CustomerPackageDecision {
                enforced: false,
                history: false,
            },
        }
    }

    #[test]
    fn source_statement_is_owner_local_lock_free_and_byte_stable() {
        assert!(CUSTOMER_SEARCH_SQL.starts_with("SELECT id,name,phone,notification_email,"));
        assert!(CUSTOMER_SEARCH_SQL.contains("kind='customer' AND status='active'"));
        assert!(CUSTOMER_SEARCH_SQL.contains("ORDER BY name,id LIMIT 100"));
        assert_eq!(CUSTOMER_SEARCH_SQL.matches('?').count(), 5);
        for forbidden in [
            "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
            "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pg_try_advisory",
        ] {
            assert!(
                !CUSTOMER_SEARCH_SQL.contains(forbidden),
                "customer-search SQL must not mutate or lock: {forbidden}"
            );
        }
    }

    #[test]
    fn render_preserves_bind_order_and_retired_pattern() {
        let (sql, binds) = render(&query(" 50%_a\\b "));
        assert!(sql.starts_with("SELECT id::text AS id,name::text AS name,"));
        assert!(sql.contains(" FROM (SELECT id,name,phone,notification_email,"));
        assert_eq!(binds.len(), 5);
        assert_eq!(binds[0], Bind::Text("co".into()));
        assert_eq!(binds[1], Bind::Text("50%_a\\b".into()));
        assert_eq!(binds[2], Bind::Text("%50\\%\\_a\\\\b%".into()));
        assert_eq!(binds[3], binds[2]);
        assert_eq!(binds[4], binds[2]);
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
        assert_eq!(max, 5);

        let (_, empty) = render(&query(""));
        assert_eq!(empty[1], Bind::Text("".into()));
        assert_eq!(empty[2], Bind::Text("%".into()));
    }
}
