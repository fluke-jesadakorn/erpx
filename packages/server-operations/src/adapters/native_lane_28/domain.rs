//! Pure domain types, validation and response invariants for the native
//! Operations staff-level acceptance facts read (lane 28).
//!
//! This capability owns two owner-scoped reads that were formerly executed as
//! TypeScript SQL in `staff-facts.ts` and `company-acceptance-facts.ts`:
//! active level ranks and the count of levels in the 1..6 acceptance band. The
//! Rust owner executes the statements with sqlx, applies the aggregation and
//! produces the response shape; TypeScript only transports the facts.

use erpx_server_shared::public::{BusinessError, RecordId};
use serde::Serialize;

/// Byte-identical successor of the retired active-ranks literal. The only
/// deliberate differences from the TypeScript rendering are the Postgres
/// bind marker (`$1` instead of `?`) and the deterministic `ORDER BY`, which
/// makes the returned projection stable for the response oracle.
pub const ACTIVE_RANKS_SQL: &str = concat!(
    "SELECT id,rank FROM (SELECT ",
    "\"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\"",
    " FROM staff_levels) staff_levels WHERE company_id=$1 AND active=1 ORDER BY rank,id",
);

/// Byte-identical successor of the retired company-acceptance count literal.
pub const ACCEPTANCE_COUNT_SQL: &str = concat!(
    "SELECT count(*)::bigint active_count FROM (SELECT ",
    "\"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\"",
    " FROM staff_levels) staff_levels WHERE company_id=$1 AND active=1 AND level_number BETWEEN 1 AND 6",
);

/// Byte-identical successor of the retired level-history literal in
/// `people-level-history.ts`, with `$1` bind and deterministic ORDER BY.
pub const LEVEL_HISTORY_SQL: &str = concat!(
    "SELECT h.created_at,h.reason,l.name AS level_name,old.name AS previous_level_name FROM (SELECT ",
    "\"id\",\"engagement_id\",\"previous_level_id\",\"level_id\",\"actor_id\",\"reason\",\"command_id\",\"created_at\",\"insertion_order\"",
    " FROM staff_level_history) h JOIN (SELECT ",
    "\"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\"",
    " FROM staff_levels) l ON l.id=h.level_id LEFT JOIN (SELECT ",
    "\"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\"",
    " FROM staff_levels) old ON old.id=h.previous_level_id WHERE h.engagement_id=$1 ORDER BY h.created_at DESC",
);

/// SHA-256 of the exact rendered statements; pinned by the response oracle.
pub const ACTIVE_RANKS_SQL_SHA256: &str =
    "0f76f4b04f07e67557e64c05f74e6d6abdb0280feb7a36f699111aaa5a02a540";
pub const ACCEPTANCE_COUNT_SQL_SHA256: &str =
    "2acd64c84a200b674c8df3f6e6c44a091a34c67a81e0c047cc5afa710f411636";
pub const LEVEL_HISTORY_SQL_SHA256: &str =
    "aa0c8174c2c50c8dfdf5822391ee8d12376112805d236372348114eb49fdddac";

/// Deterministic 64-bit FNV-1a used by the Rust and TypeScript contract tests
/// as a dependency-free rendered-SQL oracle (SHA-256 of the same bytes is
/// pinned above and reported with the migration slice).
pub fn sql_oracle(sql: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in sql.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Max accepted company identity bytes; matches the owner's other native
/// read ports and prevents unbounded input reaching Postgres.
pub const MAX_COMPANY_BYTES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffLevel {
    pub id: String,
    pub rank: i64,
    pub level_number: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffLevelFacts {
    pub levels: Vec<StaffLevel>,
    pub active_count: i64,
}

/// A staff-level history row. Keys are deliberately snake_case: the retired
/// TypeScript consumer forwards these rows unchanged to its presentation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LevelHistoryRow {
    pub created_at: i64,
    pub reason: String,
    pub level_name: Option<String>,
    pub previous_level_name: Option<String>,
}

pub fn invalid_company() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_LANE28_COMPANY_INVALID",
        "Use one bounded Operations company identity.",
    )
}

pub fn validate_company(company: &str) -> Result<(), BusinessError> {
    if company.is_empty()
        || company.len() > MAX_COMPANY_BYTES
        || !RecordId::new(company).is_ok()
        || !company
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
    {
        return Err(invalid_company());
    }
    Ok(())
}

pub fn invalid_engagement() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_LANE28_ENGAGEMENT_INVALID",
        "Use one bounded Operations engagement identity.",
    )
}

pub fn validate_engagement(engagement: &str) -> Result<(), BusinessError> {
    if engagement.is_empty()
        || engagement.len() > MAX_COMPANY_BYTES
        || !RecordId::new(engagement).is_ok()
        || !engagement
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
    {
        return Err(invalid_engagement());
    }
    Ok(())
}

/// Aggregation moved out of SQL where safe: the acceptance count is the count
/// of projected active levels already inside the 1..6 band. The index is
/// derived, never trusted from a caller.
pub fn acceptance_count(levels: &[StaffLevel]) -> i64 {
    levels
        .iter()
        .filter(|level| (1..=6).contains(&level.level_number))
        .count() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn company_validation_is_bounded() {
        assert!(validate_company("company").is_ok());
        assert!(validate_company("acme:001").is_ok());
        assert!(validate_company("").is_err());
        assert!(validate_company("bad company!").is_err());
        assert!(validate_company(&"x".repeat(MAX_COMPANY_BYTES + 1)).is_err());
    }

    #[test]
    fn acceptance_count_is_derived_in_rust() {
        let levels = vec![
            StaffLevel { id: "a".into(), rank: 1, level_number: 1 },
            StaffLevel { id: "b".into(), rank: 2, level_number: 6 },
            StaffLevel { id: "c".into(), rank: 3, level_number: 9 },
        ];
        assert_eq!(acceptance_count(&levels), 2);
    }

    #[test]
    fn response_shape_is_camel_case() {
        let facts = StaffLevelFacts { levels: vec![], active_count: 2 };
        let value = serde_json::to_value(&facts).unwrap();
        assert_eq!(value, serde_json::json!({"levels": [], "activeCount": 2}));
    }

    #[test]
    fn rendered_sql_matches_byte_oracle() {
        // Oracles fixed before the migration from the byte-identical TS SQL.
        assert_eq!(ACTIVE_RANKS_SQL, concat!(
            "SELECT id,rank FROM (SELECT ",
            "\"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\"",
            " FROM staff_levels) staff_levels WHERE company_id=$1 AND active=1 ORDER BY rank,id",
        ));
        assert_eq!(sql_oracle(ACTIVE_RANKS_SQL), 0x195a854ad7fad16f);
        assert_eq!(sql_oracle(ACCEPTANCE_COUNT_SQL), 0xfd6ce958369b433e);
        assert_eq!(sql_oracle(LEVEL_HISTORY_SQL), 0xf523fc8eb6805a52);
    }
}
