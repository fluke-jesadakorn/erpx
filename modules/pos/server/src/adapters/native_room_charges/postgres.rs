//! PostgreSQL execution for the native POS room-charge reversal queue read.
//!
//! The source statement is byte-identical to the TypeScript literal retired
//! from `room-charge-reversal-queue.ts`, including the bounded PMS
//! `pms_properties` projection it read. A fixed text/int/bool wrapper lets sqlx
//! decode the exact wire row; no row lock, advisory lock or mutation is added
//! and the query is scoped only by the caller's Company/Branch/Property values.

use crate::adapters::native_room_charges::application::{
    PosRoomChargeReader, RoomChargeFuture,
};
use crate::adapters::native_room_charges::domain::{
    RoomChargeQuery, RoomChargeRow, valid_row,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Arguments, Row, postgres::PgArguments};

/// Exact retired TypeScript source statement (`?` before bind numbering).
pub const ROOM_CHARGE_SQL: &str = r#"SELECT q.*,s.number,s.created_by,(s.version=q.sale_version AND s.total_minor=q.amount_minor AND p.version=q.property_version AND p.state='open') source_current,
    (SELECT actor_id FROM pos_audit WHERE operation='sale.charge_to_room' AND record_id=s.id ORDER BY created_at DESC LIMIT 1) charged_by
    FROM pos_room_charge_reversal_requests q JOIN pos_sales s ON s.id=q.sale_id AND s.company_id=q.company_id JOIN (SELECT "id","company_id","branch_id","name","business_date","state","config_json","version","created_by","updated_at" FROM pms_properties) p ON p.id=q.property_id AND p.company_id=q.company_id
    WHERE q.company_id=? AND (?::text IS NULL OR q.branch_id=?) AND (?::text IS NULL OR q.property_id=?)
    AND q.state IN ('requested','pos_approved') ORDER BY q.created_at,q.id LIMIT 100"#;

/// Fixed projection of the 19 source columns into uniform decode types: text
/// for identities, bigint for versions/amounts/timestamps, boolean for the
/// source-current discriminator. Numeric casts preserve the retired JSON types.
const PROJECTION: &str = "SELECT id::text AS id,company_id::text AS company_id,\
 branch_id::text AS branch_id,sale_id::text AS sale_id,property_id::text AS property_id,\
 sale_version::bigint AS sale_version,property_version::bigint AS property_version,\
 restock_line_ids_json::text AS restock_line_ids_json,number::text AS number,state::text AS state,\
 version::bigint AS version,reason::text AS reason,amount_minor::bigint AS amount_minor,\
 requested_by::text AS requested_by,pos_approved_by::text AS pos_approved_by,\
 created_by::text AS created_by,charged_by::text AS charged_by,updated_at::bigint AS updated_at,\
 source_current::boolean AS source_current";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Bind {
    Text(String),
    Opt(Option<String>),
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_ROOM_CHARGES_UNAVAILABLE",
        "The native POS room-charge service is unavailable.",
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

/// Render the owner SQL and its five positional binds in the retired order:
/// `(company, branch, branch, property, property)`.
pub fn render(query: &RoomChargeQuery) -> (String, Vec<Bind>) {
    let sql = format!("{PROJECTION} FROM ({}) d", numbered(ROOM_CHARGE_SQL));
    let binds = vec![
        Bind::Text(query.company.clone()),
        Bind::Opt(query.branch.clone()),
        Bind::Opt(query.branch.clone()),
        Bind::Opt(query.property.clone()),
        Bind::Opt(query.property.clone()),
    ];
    (sql, binds)
}

fn args(binds: &[Bind]) -> Result<PgArguments, BusinessError> {
    let mut arguments = PgArguments::default();
    for bind in binds {
        match bind {
            Bind::Text(value) => {
                arguments.add(value.as_str()).map_err(|_| unavailable())?;
            }
            Bind::Opt(value) => {
                arguments.add(value.as_deref()).map_err(|_| unavailable())?;
            }
        }
    }
    Ok(arguments)
}

fn text(row: &sqlx::postgres::PgRow, key: &str) -> Result<String, BusinessError> {
    row.try_get::<Option<String>, _>(key)
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)
}

fn opt_text(row: &sqlx::postgres::PgRow, key: &str) -> Result<Option<String>, BusinessError> {
    row.try_get::<Option<String>, _>(key).map_err(|_| unavailable())
}

fn integer(row: &sqlx::postgres::PgRow, key: &str) -> Result<i64, BusinessError> {
    row.try_get::<Option<i64>, _>(key)
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)
}

fn boolean(row: &sqlx::postgres::PgRow, key: &str) -> Result<bool, BusinessError> {
    row.try_get::<Option<bool>, _>(key)
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)
}

fn projection(row: &sqlx::postgres::PgRow) -> Result<RoomChargeRow, BusinessError> {
    let row = RoomChargeRow {
        id: text(row, "id")?,
        company_id: text(row, "company_id")?,
        branch_id: text(row, "branch_id")?,
        sale_id: text(row, "sale_id")?,
        property_id: text(row, "property_id")?,
        sale_version: integer(row, "sale_version")?,
        property_version: integer(row, "property_version")?,
        restock_line_ids_json: opt_text(row, "restock_line_ids_json")?,
        number: text(row, "number")?,
        state: text(row, "state")?,
        version: integer(row, "version")?,
        reason: text(row, "reason")?,
        amount_minor: integer(row, "amount_minor")?,
        requested_by: text(row, "requested_by")?,
        pos_approved_by: opt_text(row, "pos_approved_by")?,
        created_by: text(row, "created_by")?,
        charged_by: opt_text(row, "charged_by")?,
        updated_at: integer(row, "updated_at")?,
        source_current: boolean(row, "source_current")?,
    };
    if !valid_row(&row) {
        return Err(unavailable());
    }
    Ok(row)
}

/// Pool-backed owner reader. The router mounts one of these per registered
/// Holding target; no caller transaction or SQL from TypeScript is required.
pub struct PgRoomChargeReader {
    pool: DatabasePool,
}

impl PgRoomChargeReader {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PosRoomChargeReader for PgRoomChargeReader {
    fn rows<'a>(&'a self, query: &'a RoomChargeQuery) -> RoomChargeFuture<'a, Vec<RoomChargeRow>> {
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
    use crate::adapters::native_room_charges::domain::{
        ActorDecision, PackageDecision,
    };

    fn query() -> RoomChargeQuery {
        RoomChargeQuery {
            company: "co".into(),
            branch: Some("br".into()),
            property: Some("prop".into()),
            actor: ActorDecision {
                pos_approve: true,
                pms_approve: false,
                department: false,
                self_ids: Vec::new(),
            },
            package: PackageDecision {
                enforced: false,
                history: false,
                settle_existing: false,
            },
        }
    }

    #[test]
    fn source_statement_is_owner_local_lock_free_and_byte_stable() {
        assert!(ROOM_CHARGE_SQL.starts_with("SELECT q.*,s.number,s.created_by,"));
        assert!(ROOM_CHARGE_SQL.contains("q.state IN ('requested','pos_approved') ORDER BY q.created_at,q.id LIMIT 100"));
        assert_eq!(ROOM_CHARGE_SQL.matches('?').count(), 5);
        for forbidden in [
            "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
            "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pg_try_advisory",
            "finance_", "inventory_", "identity_access",
        ] {
            assert!(
                !ROOM_CHARGE_SQL.contains(forbidden),
                "room-charge SQL must not mutate, lock or cross business owners: {forbidden}"
            );
        }
        assert_eq!(
            ROOM_CHARGE_SQL.matches("pms_properties").count(),
            1,
            "the bounded PMS projection appears exactly once"
        );
    }

    #[test]
    fn render_preserves_bind_order_and_numbering() {
        let (sql, binds) = render(&query());
        assert!(sql.starts_with("SELECT id::text AS id,company_id::text AS company_id,"));
        assert!(sql.contains(" FROM (SELECT q.*,s.number,s.created_by,"));
        assert_eq!(binds.len(), 5);
        assert_eq!(binds[0], Bind::Text("co".into()));
        assert_eq!(binds[1], Bind::Opt(Some("br".into())));
        assert_eq!(binds[2], Bind::Opt(Some("br".into())));
        assert_eq!(binds[3], Bind::Opt(Some("prop".into())));
        assert_eq!(binds[4], Bind::Opt(Some("prop".into())));
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

        let no_scope = RoomChargeQuery {
            branch: None,
            property: None,
            ..query()
        };
        let binds = render(&no_scope).1;
        assert_eq!(binds[1], Bind::Opt(None));
        assert_eq!(binds[3], Bind::Opt(None));
    }
}
