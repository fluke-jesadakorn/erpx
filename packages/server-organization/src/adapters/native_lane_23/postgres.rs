//! sqlx execution for the native Organization directory hierarchy read.
//! The three statements are byte-identical to the SQL retired from
//! `adapters/directory-projection-facts.ts`; their SHA-256 is pinned by a unit
//! test. Projection into typed response objects happens here, never in TypeScript.
use super::application::{DirectoryHierarchyFuture, DirectoryHierarchyRead};
use super::domain::{
    DirectoryHierarchy, DirectoryLevel, DirectoryNode, DirectoryPosition,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::Row;

pub const NODES_SQL: &str = "SELECT n.id,n.company_id,n.kind,n.slug,n.display_name,n.parent_id,n.branch_id
      FROM organization_nodes n JOIN organization_nodes c
        ON c.id=n.company_id AND c.kind='company' AND c.status='active'
      WHERE n.status='active' ORDER BY n.company_id,n.kind,n.display_name,n.id";

pub const LEVELS_SQL: &str = "SELECT l.id,l.company_id,l.code,l.name,l.level_number
      FROM staff_levels l JOIN organization_nodes c
        ON c.id=l.company_id AND c.kind='company' AND c.status='active'
      WHERE l.active=1 ORDER BY l.company_id,l.level_number NULLS LAST,l.name,l.id";

pub const POSITIONS_SQL: &str = "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id,
        p.reports_to_position_id,p.is_ceo
      FROM organization_positions p JOIN organization_nodes c
        ON c.id=p.company_id AND c.kind='company' AND c.status='active'
      WHERE p.active=1 ORDER BY p.company_id,p.title,p.id";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_DIRECTORY_HIERARCHY_UNAVAILABLE",
        "The native Organization directory hierarchy is unavailable.",
    )
}

fn hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Pg-backed owner reader. All three queries run on one fresh owner unit so
/// the holding-wide directory is a consistent snapshot; no lock is taken and
/// no other owner's table is read.
pub struct PgDirectoryHierarchyRead {
    pool: DatabasePool,
}

impl PgDirectoryHierarchyRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl DirectoryHierarchyRead for PgDirectoryHierarchyRead {
    fn read<'a>(&'a self) -> DirectoryHierarchyFuture<'a, DirectoryHierarchy> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let nodes = sqlx::query(NODES_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(DirectoryNode {
                                id: row.try_get("id").map_err(|_| unavailable())?,
                                company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                                kind: row.try_get("kind").map_err(|_| unavailable())?,
                                slug: row.try_get("slug").map_err(|_| unavailable())?,
                                display_name: row.try_get("display_name").map_err(|_| unavailable())?,
                                parent_id: row.try_get("parent_id").map_err(|_| unavailable())?,
                                branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let levels = sqlx::query(LEVELS_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(DirectoryLevel {
                                id: row.try_get("id").map_err(|_| unavailable())?,
                                company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                                code: row.try_get("code").map_err(|_| unavailable())?,
                                name: row.try_get("name").map_err(|_| unavailable())?,
                                level_number: row
                                    .try_get::<Option<i32>, _>("level_number")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let positions = sqlx::query(POSITIONS_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(DirectoryPosition {
                                id: row.try_get("id").map_err(|_| unavailable())?,
                                company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                                branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                                department_id: row
                                    .try_get("department_id")
                                    .map_err(|_| unavailable())?,
                                title: row.try_get("title").map_err(|_| unavailable())?,
                                level_id: row.try_get("level_id").map_err(|_| unavailable())?,
                                reports_to_position_id: row
                                    .try_get("reports_to_position_id")
                                    .map_err(|_| unavailable())?,
                                is_ceo: row.try_get::<i32, _>("is_ceo").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(DirectoryHierarchy {
                        nodes,
                        levels,
                        positions,
                    })
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_sql_is_byte_identical() {
        assert_eq!(
            hash(NODES_SQL),
            "3209884176699884550af2180cbd61c52e95bc0551d804b8ddd218e9596201ff"
        );
        assert_eq!(
            hash(LEVELS_SQL),
            "3ec6b658baae8dfb915c182e229783d147d3ee30133b899a96e5e444e5641f53"
        );
        assert_eq!(
            hash(POSITIONS_SQL),
            "00e25fe99020172a9ba538ced87e7c32eea4220f09ebe07c3ec061bf67a1fee9"
        );
    }

    #[test]
    fn no_statement_escapes_the_owner_read() {
        for sql in [NODES_SQL, LEVELS_SQL, POSITIONS_SQL] {
            let upper = sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT "));
            assert!(!upper.contains("INSERT") && !upper.contains("DELETE") && !upper.contains("UPDATE"));
            assert!(!upper.contains("FOR UPDATE") && !upper.contains("LOCK TABLE"));
        }
    }

    #[test]
    fn result_oracle_is_stable() {
        let facts = DirectoryHierarchy {
            nodes: vec![
                DirectoryNode {
                    id: "node-1".into(),
                    company_id: "company-1".into(),
                    kind: "company".into(),
                    slug: "acme".into(),
                    display_name: "Acme".into(),
                    parent_id: None,
                    branch_id: None,
                },
                DirectoryNode {
                    id: "node-2".into(),
                    company_id: "company-1".into(),
                    kind: "branch".into(),
                    slug: "nyc".into(),
                    display_name: "New York".into(),
                    parent_id: Some("node-1".into()),
                    branch_id: Some("node-2".into()),
                },
            ],
            levels: vec![DirectoryLevel {
                id: "level-1".into(),
                company_id: "company-1".into(),
                code: "L1".into(),
                name: "Level 1".into(),
                level_number: Some(1),
            }],
            positions: vec![DirectoryPosition {
                id: "pos-1".into(),
                company_id: "company-1".into(),
                branch_id: Some("node-2".into()),
                department_id: None,
                title: "CEO".into(),
                level_id: "level-1".into(),
                reports_to_position_id: None,
                is_ceo: 1,
            }],
        };
        facts.validate().unwrap();
        let json = serde_json::to_string(&facts).unwrap();
        assert_eq!(
            hash(&json),
            "f48ac71e156c9a4172977bb8e60d785ef2a5bc17b081137cb6cb36efdab12d08"
        );
    }
}
