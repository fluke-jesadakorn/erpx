//! sqlx execution for the lane-24 Organization evaluation-reference family.
//!
//! Every statement below is the byte-identical predicate/projection/bind order
//! of the retired TypeScript read, with only the placeholder syntax changed
//! from the TypeScript runtime's `?` to sqlx/Postgres `$n`. The owner owns the
//! SQL, the scoped bind of Company and the typed projection; no SQL text ever
//! crosses the transport. These are organization-owned tables only.

use crate::adapters::native_lane_24::application::{
    EvaluationFactsReader, FactsQuery, ReadFuture,
};
use crate::adapters::native_lane_24::domain::{
    AccessNode, BranchOption, EVAL_LEVEL_ACTIVE, EVAL_MARKET_LEVELS, EVAL_POSITION_BY_LEVEL,
    EVAL_WORKSPACE_LEVELS, EVAL_WORKSPACE_POSITIONS, LevelRef, MarketLevel, ORG_ACCESS_NODES,
    ORG_BRANCH_OPTIONS_ALL, ORG_BRANCH_OPTIONS_POS, ORG_BRANCH_OPTIONS_SCOPED, PositionRef,
    ReadResult, WorkspacePosition,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

// Predicates/projections are byte-identical to the retired TS rendered SQL
// (oracle pins in the tests below); `$n` is sqlx's placeholder syntax.
const SQL_POSITION_BY_LEVEL: &str =
    "SELECT id FROM organization_positions WHERE id=$1 AND company_id=$2 AND level_id=$3 AND active=1";
const SQL_LEVEL_ACTIVE: &str =
    "SELECT 1 FROM staff_levels WHERE id=$1 AND company_id=$2 AND active=1";
const SQL_WORKSPACE_POSITIONS: &str =
    "SELECT id,title,level_id FROM organization_positions WHERE company_id=$1 AND active=1 ORDER BY title";
const SQL_WORKSPACE_LEVELS: &str =
    "SELECT id,name FROM staff_levels WHERE company_id=$1 AND active=1 ORDER BY rank";
const SQL_MARKET_LEVELS: &str =
    "SELECT id FROM staff_levels WHERE company_id=$1 AND active=1";
const SQL_ACCESS_NODES: &str =
    "SELECT id,parent_id,kind,display_name,status FROM organization_nodes WHERE company_id=$1";
const SQL_BRANCH_OPTIONS_ALL: &str =
    "SELECT id,display_name name FROM organization_nodes WHERE company_id=$1 AND kind='branch' AND status='active'";
const SQL_BRANCH_OPTIONS_SCOPED: &str =
    "SELECT id,display_name name FROM organization_nodes WHERE company_id=$1 AND kind='branch' AND status='active' AND id=$2";
const SQL_BRANCH_OPTIONS_POS: &str =
    "SELECT id,display_name name FROM organization_nodes WHERE company_id=$1 AND kind='branch' AND ($2::text IS NULL OR id=$3) ORDER BY display_name";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_LANE24_READ_UNAVAILABLE",
        "The native Organization evaluation read is unavailable.",
    )
}

/// Pg-backed owner reader. Composition injects one `DatabasePool` for the
/// registered owner; the router never sees the pool or a connection.
pub struct PgEvaluationFacts {
    pool: DatabasePool,
}

impl PgEvaluationFacts {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl EvaluationFactsReader for PgEvaluationFacts {
    fn read(&self, query: FactsQuery) -> ReadFuture<'_, ReadResult> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let company = query.company.clone();
            let values = query.values.clone();
            match query.kind.as_str() {
                EVAL_POSITION_BY_LEVEL => {
                    let row = sqlx::query(SQL_POSITION_BY_LEVEL)
                        .bind(values[0].as_deref())
                        .bind(&company)
                        .bind(values[1].as_deref())
                        .fetch_optional(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    let row = row
                        .map(|r| {
                            r.try_get::<String, _>("id")
                                .map(|id| PositionRef { id })
                                .map_err(|_| unavailable())
                        })
                        .transpose()?;
                    Ok(ReadResult::Position(row))
                }
                EVAL_LEVEL_ACTIVE => {
                    let row = sqlx::query_scalar::<_, i32>(SQL_LEVEL_ACTIVE)
                        .bind(values[0].as_deref())
                        .bind(&company)
                        .fetch_optional(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(ReadResult::Exists(row.is_some()))
                }
                EVAL_WORKSPACE_POSITIONS => {
                    let rows = sqlx::query(SQL_WORKSPACE_POSITIONS)
                        .bind(&company)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(|r| {
                            Ok(WorkspacePosition {
                                id: r.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                title: r.try_get::<String, _>("title").map_err(|_| unavailable())?,
                                level_id: r
                                    .try_get::<String, _>("level_id")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(ReadResult::WorkspacePositions(rows))
                }
                EVAL_WORKSPACE_LEVELS => {
                    let rows = sqlx::query(SQL_WORKSPACE_LEVELS)
                        .bind(&company)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(|r| {
                            Ok(LevelRef {
                                id: r.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                name: r.try_get::<String, _>("name").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(ReadResult::Levels(rows))
                }
                EVAL_MARKET_LEVELS => {
                    let rows = sqlx::query(SQL_MARKET_LEVELS)
                        .bind(&company)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(|r| {
                            r.try_get::<String, _>("id")
                                .map(|id| MarketLevel { id })
                                .map_err(|_| unavailable())
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(ReadResult::MarketLevels(rows))
                }
                ORG_ACCESS_NODES => {
                    let rows = sqlx::query(SQL_ACCESS_NODES)
                        .bind(&company)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(|r| {
                            Ok(AccessNode {
                                id: r.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                parent_id: r.try_get::<Option<String>, _>("parent_id").map_err(|_| unavailable())?,
                                kind: r.try_get::<String, _>("kind").map_err(|_| unavailable())?,
                                display_name: r.try_get::<Option<String>, _>("display_name").map_err(|_| unavailable())?,
                                status: r.try_get::<String, _>("status").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(ReadResult::AccessNodes(rows))
                }
                ORG_BRANCH_OPTIONS_ALL | ORG_BRANCH_OPTIONS_SCOPED | ORG_BRANCH_OPTIONS_POS => {
                    let statement = match query.kind.as_str() {
                        ORG_BRANCH_OPTIONS_ALL => sqlx::query(SQL_BRANCH_OPTIONS_ALL).bind(&company),
                        ORG_BRANCH_OPTIONS_SCOPED => sqlx::query(SQL_BRANCH_OPTIONS_SCOPED)
                            .bind(&company)
                            .bind(values[0].as_deref()),
                        _ => sqlx::query(SQL_BRANCH_OPTIONS_POS)
                            .bind(&company)
                            .bind(values[0].as_deref())
                            .bind(values[1].as_deref()),
                    };
                    let rows = statement.fetch_all(pool.raw()).await.map_err(|_| unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(|r| {
                            Ok(BranchOption {
                                id: r.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                name: r.try_get::<String, _>("name").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(ReadResult::BranchOptions(rows))
                }
                _ => Err(unavailable()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_24::domain::envelope;
    use sha2::{Digest, Sha256};

    fn sha(value: &str) -> String {
        let mut h = Sha256::new();
        h.update(value.as_bytes());
        h.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Collapse sqlx `$n` placeholders back to the TS runtime's `?` so the byte
    /// oracle compares predicate/projection/bind order, not placeholder syntax.
    fn placeholders(sql: &str) -> String {
        let bytes = sql.as_bytes();
        let mut out = String::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                out.push('?');
                i += 1; // skip the '$' before the digits
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                continue;
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    #[test]
    fn sql_oracles_match_retired_typescript_rendered_sql() {
        // sha256 of the byte-identical SQL rendered by the retired TS helpers.
        let cases = [
            (
                EVAL_POSITION_BY_LEVEL,
                SQL_POSITION_BY_LEVEL,
                "f69e9fc60c847c2a32cd5d75adbb100f7dfc06132b430fb259363b5b6b7398ee",
            ),
            (
                EVAL_LEVEL_ACTIVE,
                SQL_LEVEL_ACTIVE,
                "3d6cc0a99d1bb6baf196588605287d8379407544fcfcb61478df179b0bf44553",
            ),
            (
                EVAL_WORKSPACE_POSITIONS,
                SQL_WORKSPACE_POSITIONS,
                "a77bb52eb08faace3a91a6c99ae7240cf63fdb76323bd14fe5584524012784a2",
            ),
            (
                EVAL_WORKSPACE_LEVELS,
                SQL_WORKSPACE_LEVELS,
                "2109f80d74ea61d5a35cb5fe2fd04db65ddb6a2159a1e7fde479faa13c841cec",
            ),
            (
                EVAL_MARKET_LEVELS,
                SQL_MARKET_LEVELS,
                "a9d3489791c762811c53749984d10816a6c462a657b2e7a252e26780086a7c79",
            ),
            (
                ORG_ACCESS_NODES,
                SQL_ACCESS_NODES,
                "0655d1518f407dd6667d1ecaf1a39a224b7d3a71c1ffb54e35b6e46bcf553608",
            ),
            (
                ORG_BRANCH_OPTIONS_ALL,
                SQL_BRANCH_OPTIONS_ALL,
                "29245ae97d9143bd83f7dbd381e3d1648f0eb64d62f3d464a3826e6b708cf89d",
            ),
            (
                ORG_BRANCH_OPTIONS_SCOPED,
                SQL_BRANCH_OPTIONS_SCOPED,
                "02231dfc98748e2457bedd350bd91da09237993253161b17e37c8a4d8812a5f0",
            ),
            (
                ORG_BRANCH_OPTIONS_POS,
                SQL_BRANCH_OPTIONS_POS,
                "b663cbb10506a5efb73b8a9f53d9d6ca84af2e246917c42a942db4f2321f1fff",
            ),
        ];
        for (kind, sql, pinned) in cases {
            assert_eq!(sha(&placeholders(sql)), pinned, "SQL oracle for {kind}");
        }
    }

    #[test]
    fn result_oracles_pin_the_response_shape() {
        let cases: Vec<(&str, ReadResult, &str)> = vec![
            (
                EVAL_POSITION_BY_LEVEL,
                ReadResult::Position(Some(PositionRef { id: "p1".into() })),
                "27abf79f4d5b4fb1b557dfaebae251c058ae64bba9e493ed6a36099923e1c267",
            ),
            (
                EVAL_POSITION_BY_LEVEL,
                ReadResult::Position(None),
                "afe4e8288c358cfbd9daabebb61e265e184da4916a886befa077c14b481ca0b2",
            ),
            (
                EVAL_LEVEL_ACTIVE,
                ReadResult::Exists(true),
                // serde_json orders map keys, so `exists` precedes `kind` here.
                "cd8e54006cce98d0c6cd5ccad04995dab34c1431cccb64960d230da2ddd69d91",
            ),
            (
                EVAL_WORKSPACE_POSITIONS,
                ReadResult::WorkspacePositions(vec![WorkspacePosition {
                    id: "p1".into(),
                    title: "Engineer".into(),
                    level_id: "l1".into(),
                }]),
                "3755921fe07383c1c980ff9001581ac74032d05e7e156d275de95593aa31d28f",
            ),
            (
                EVAL_WORKSPACE_LEVELS,
                ReadResult::Levels(vec![LevelRef {
                    id: "l1".into(),
                    name: "L1".into(),
                }]),
                "0c635e7f42d507bdcd34707f8e00d64d19ce8891fa8f9a91a3cc5bbd2e346b67",
            ),
            (
                EVAL_MARKET_LEVELS,
                ReadResult::MarketLevels(vec![MarketLevel { id: "l1".into() }]),
                "edb1bf23d9fe71c92da5374762962d8554ceb10cd3882f548943816a64ca79e8",
            ),
            (
                ORG_ACCESS_NODES,
                ReadResult::AccessNodes(vec![AccessNode {
                    id: "n1".into(),
                    parent_id: None,
                    kind: "branch".into(),
                    display_name: Some("Main".into()),
                    status: "active".into(),
                }]),
                "9363e900a1afcabcc9394fa2a846c74ad0d3c0af795bc9b43ca59618c0d722d1",
            ),
            (
                ORG_BRANCH_OPTIONS_ALL,
                ReadResult::BranchOptions(vec![BranchOption { id: "b1".into(), name: "Main".into() }]),
                "4e49d66cf8ce06afb1dc6e194bdb5678126eb6470c85d407597cc9ecbd6e2af8",
            ),
            (
                ORG_BRANCH_OPTIONS_SCOPED,
                ReadResult::BranchOptions(vec![BranchOption { id: "b1".into(), name: "Main".into() }]),
                "61f7138cc2680406b05da8b16d08f35c8071fb4e9bb33b06d05d38544b956444",
            ),
            (
                ORG_BRANCH_OPTIONS_POS,
                ReadResult::BranchOptions(vec![BranchOption { id: "b1".into(), name: "Main".into() }]),
                "89d924b78af317e985bd03c14951293ee327720bbbc3c263c3b4bac94e41ba39",
            ),
        ];
        for (kind, result, pinned) in cases {
            let value = serde_json::to_value(envelope(kind, result).unwrap()).unwrap();
            assert_eq!(sha(&value.to_string()), pinned, "result oracle for {kind}");
        }
    }
}
