//! sqlx execution for the native Organization directory hierarchy read.
//! The three statements are byte-identical to the SQL retired from
//! `adapters/directory-projection-facts.ts`; their SHA-256 is pinned by a unit
//! test. Projection into typed response objects happens here, never in TypeScript.
use super::application::{
    DirectoryHierarchyFuture, DirectoryHierarchyRead, ExecutiveEvidenceRead,
    ExecutiveEvidenceRequest, NodeReferenceRead,
};
use super::domain::{
    AttachmentNodeFact, AudienceNode, DirectoryHierarchy, DirectoryLevel, DirectoryNode,
    DirectoryPosition, ExecutiveEvidence, ExecutiveEvidenceKind, NodeReferences, UnitReference,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgRow;
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

/// Byte-identical to the retired `collaboration-inbox-sources.ts` audience read.
pub const AUDIENCE_NODE_SQL: &str = "SELECT id,kind,branch_id FROM organization_nodes WHERE id=? AND company_id=? AND status='active'";

/// Byte-identical to the retired `iam-unit-facts.ts` unit list.
pub const UNIT_REFERENCE_SQL: &str = "SELECT id,display_name AS name,kind,branch_id AS \"branchId\" FROM organization_nodes
    WHERE company_id=? AND status='active' AND kind IN ('branch','department') ORDER BY display_name,id";

/// Byte-identical to the retired `attachments-finance-facts.ts` owner read.
pub const ATTACHMENT_NODE_SQL: &str = "SELECT company_id,CASE WHEN kind='branch' THEN id ELSE branch_id END branch_id,NULL::text department_id,hierarchy_revision AS version,kind
      FROM organization_nodes WHERE id=? AND company_id=? AND kind IN ('company','headquarters','branch','department')";

/// Byte-identical to the retired `executive-connection-facts.ts` scope read.
pub const CONNECTION_SCOPES_SQL: &str = "SELECT n.id,n.kind,n.display_name,n.branch_id,b.display_name branch_name
    FROM organization_nodes n LEFT JOIN organization_nodes b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.status='active'
    WHERE n.company_id=? AND n.status='active' AND n.kind IN ('company','branch','department')
      AND (n.branch_id IS NULL OR b.id IS NOT NULL)
      AND (?::text IS NULL OR n.id=? OR n.branch_id=?)
      AND (?::text IS NULL OR n.id=?) ORDER BY n.kind,n.display_name,n.id";

/// Byte-identical to the retired `executive-context-sources.ts` node read.
pub const CONTEXT_NODES_SQL: &str = "SELECT id,kind,display_name,parent_id,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND (kind='company' OR ((?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=? OR (kind='branch' AND id=(SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=?))))) ORDER BY kind,display_name,id LIMIT 501";

/// Byte-identical to the retired `executive-context-sources.ts` position read.
pub const CONTEXT_POSITIONS_SQL: &str = "SELECT p.id,p.title,p.branch_id,p.department_id,p.level_id,p.reports_to_position_id,l.level_number FROM organization_positions p JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id WHERE p.company_id=? AND p.active=1 AND l.active=1 AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL) AND (?::text IS NULL OR p.department_id=? OR p.department_id IS NULL) ORDER BY l.level_number,p.title,p.id LIMIT 501";

/// Byte-identical to the rendered `nodeIdentityHeadSql` source in the retired
/// `executive-main-evidence-sources.ts` node-evidence read.
pub const NODE_EVIDENCE_SQL: &str = "SELECT n.id,n.kind,n.slug,n.display_name,n.status,n.hierarchy_revision,p.display_name parent_name,
  b.display_name branch_name FROM organization_nodes n
  LEFT JOIN organization_nodes p ON p.id=n.parent_id AND p.company_id=n.company_id
  LEFT JOIN organization_nodes b ON b.id=CASE WHEN n.kind='branch' THEN n.id ELSE n.branch_id END AND b.company_id=n.company_id
  WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active'
    AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?)";

/// Byte-identical to the retired `executive-main-evidence-sources.ts` pending
/// structure-evidence source.
pub const PENDING_EVIDENCE_SQL: &str = "SELECT c.id,c.action,c.node_kind,c.display_name,c.state,c.version,c.created_at,c.updated_at,
  COALESCE(b.display_name,'Company-wide') branch_name FROM organization_node_changes c
  LEFT JOIN organization_nodes b ON b.id=c.branch_id AND b.company_id=c.company_id
  WHERE c.company_id=? AND c.state='submitted' AND (?::text IS NULL OR c.branch_id=?)";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_DIRECTORY_HIERARCHY_UNAVAILABLE",
        "The native Organization directory hierarchy is unavailable.",
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

impl NodeReferenceRead for PgDirectoryHierarchyRead {
    fn node_references<'a>(
        &'a self,
        company_id: &'a str,
        node_id: Option<&'a str>,
    ) -> DirectoryHierarchyFuture<'a, NodeReferences> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company_id.to_owned();
        let node = node_id.map(str::to_owned);
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let node = node.clone();
                Box::pin(async move {
                    let audience = match &node {
                        Some(id) => {
                            let row = sqlx::query(AUDIENCE_NODE_SQL)
                                .bind(id.as_str())
                                .bind(company.as_str())
                                .fetch_optional(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?;
                            row.map(|row| {
                                Ok::<AudienceNode, BusinessError>(AudienceNode {
                                    id: row.try_get("id").map_err(|_| unavailable())?,
                                    kind: row.try_get("kind").map_err(|_| unavailable())?,
                                    branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                                })
                            })
                            .transpose()?
                        }
                        None => None,
                    };
                    let units = sqlx::query(UNIT_REFERENCE_SQL)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(UnitReference {
                                id: row.try_get("id").map_err(|_| unavailable())?,
                                name: row.try_get("name").map_err(|_| unavailable())?,
                                kind: row.try_get("kind").map_err(|_| unavailable())?,
                                branch_id: row.try_get("branchId").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let attachment = match &node {
                        Some(id) => {
                            let row = sqlx::query(ATTACHMENT_NODE_SQL)
                                .bind(id.as_str())
                                .bind(company.as_str())
                                .fetch_optional(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?;
                            row.map(|row| {
                                Ok::<AttachmentNodeFact, BusinessError>(AttachmentNodeFact {
                                    company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                                    branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                                    department_id: row.try_get("department_id").map_err(|_| unavailable())?,
                                    version: row.try_get("version").map_err(|_| unavailable())?,
                                    kind: row.try_get("kind").map_err(|_| unavailable())?,
                                })
                            })
                            .transpose()?
                        }
                        None => None,
                    };
                    Ok(NodeReferences {
                        node: audience,
                        units,
                        attachment,
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

/// Owned binds keep the retry future `'static`: no request borrow escapes the
/// transaction callback.
enum EvidenceBind {
    Text(Option<String>),
    Number(i64),
}

fn connection_row(row: PgRow) -> Result<Value, BusinessError> {
    Ok(json!({
        "id": row.try_get::<String,_>("id").map_err(|_| unavailable())?,
        "kind": row.try_get::<String,_>("kind").map_err(|_| unavailable())?,
        "display_name": row.try_get::<String,_>("display_name").map_err(|_| unavailable())?,
        "branch_id": row.try_get::<Option<String>,_>("branch_id").map_err(|_| unavailable())?,
        "branch_name": row.try_get::<Option<String>,_>("branch_name").map_err(|_| unavailable())?,
    }))
}

fn context_node_row(row: PgRow) -> Result<Value, BusinessError> {
    Ok(json!({
        "id": row.try_get::<String,_>("id").map_err(|_| unavailable())?,
        "kind": row.try_get::<String,_>("kind").map_err(|_| unavailable())?,
        "display_name": row.try_get::<String,_>("display_name").map_err(|_| unavailable())?,
        "parent_id": row.try_get::<Option<String>,_>("parent_id").map_err(|_| unavailable())?,
        "branch_id": row.try_get::<Option<String>,_>("branch_id").map_err(|_| unavailable())?,
    }))
}

fn context_position_row(row: PgRow) -> Result<Value, BusinessError> {
    Ok(json!({
        "id": row.try_get::<String,_>("id").map_err(|_| unavailable())?,
        "title": row.try_get::<String,_>("title").map_err(|_| unavailable())?,
        "branch_id": row.try_get::<Option<String>,_>("branch_id").map_err(|_| unavailable())?,
        "department_id": row.try_get::<Option<String>,_>("department_id").map_err(|_| unavailable())?,
        "level_id": row.try_get::<Option<String>,_>("level_id").map_err(|_| unavailable())?,
        "reports_to_position_id": row.try_get::<Option<String>,_>("reports_to_position_id").map_err(|_| unavailable())?,
        "level_number": row.try_get::<Option<i32>,_>("level_number").map_err(|_| unavailable())?,
    }))
}

fn node_evidence_row(row: PgRow) -> Result<Value, BusinessError> {
    Ok(json!({
        "id": row.try_get::<String,_>("id").map_err(|_| unavailable())?,
        "kind": row.try_get::<String,_>("kind").map_err(|_| unavailable())?,
        "slug": row.try_get::<String,_>("slug").map_err(|_| unavailable())?,
        "display_name": row.try_get::<String,_>("display_name").map_err(|_| unavailable())?,
        "parent_name": row.try_get::<Option<String>,_>("parent_name").map_err(|_| unavailable())?,
        "branch_name": row.try_get::<Option<String>,_>("branch_name").map_err(|_| unavailable())?,
        "status": row.try_get::<String,_>("status").map_err(|_| unavailable())?,
        "hierarchy_revision": row.try_get::<i64,_>("hierarchy_revision").map_err(|_| unavailable())?,
    }))
}

fn pending_evidence_row(row: PgRow) -> Result<Value, BusinessError> {
    Ok(json!({
        "id": row.try_get::<String,_>("id").map_err(|_| unavailable())?,
        "action": row.try_get::<String,_>("action").map_err(|_| unavailable())?,
        "node_kind": row.try_get::<String,_>("node_kind").map_err(|_| unavailable())?,
        "display_name": row.try_get::<String,_>("display_name").map_err(|_| unavailable())?,
        "state": row.try_get::<String,_>("state").map_err(|_| unavailable())?,
        "version": row.try_get::<i64,_>("version").map_err(|_| unavailable())?,
        "created_at": row.try_get::<i64,_>("created_at").map_err(|_| unavailable())?,
        "updated_at": row.try_get::<i64,_>("updated_at").map_err(|_| unavailable())?,
        "branch_name": row.try_get::<String,_>("branch_name").map_err(|_| unavailable())?,
    }))
}

async fn evidence_rows(
    connection: &mut sqlx::PgConnection,
    sql: String,
    binds: Vec<EvidenceBind>,
    project: fn(PgRow) -> Result<Value, BusinessError>,
) -> Result<Vec<Value>, BusinessError> {
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    for bind in &binds {
        query = match bind {
            EvidenceBind::Text(value) => query.bind(value.as_deref()),
            EvidenceBind::Number(value) => query.bind(*value),
        };
    }
    query
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| unavailable())?
        .into_iter()
        .map(project)
        .collect()
}

impl ExecutiveEvidenceRead for PgDirectoryHierarchyRead {
    fn evidence<'a>(
        &'a self,
        request: &'a ExecutiveEvidenceRequest,
    ) -> DirectoryHierarchyFuture<'a, ExecutiveEvidence> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        // Own the request before any future is built: a borrowed `&'a` would
        // otherwise escape through the retry callback and require `'static`.
        let request = request.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let request = request.clone();
                Box::pin(async move {
                    let company = request.company_id.clone();
                    let rows = match request.kind {
                        ExecutiveEvidenceKind::ConnectionScopes => {
                            let mut binds = vec![EvidenceBind::Text(Some(company))];
                            binds.extend(request.values.into_iter().map(EvidenceBind::Text));
                            evidence_rows(connection, CONNECTION_SCOPES_SQL.to_owned(), binds, connection_row).await?
                        }
                        ExecutiveEvidenceKind::ContextNodes => {
                            let mut binds = vec![EvidenceBind::Text(Some(company))];
                            binds.extend(request.values.into_iter().map(EvidenceBind::Text));
                            evidence_rows(connection, CONTEXT_NODES_SQL.to_owned(), binds, context_node_row).await?
                        }
                        ExecutiveEvidenceKind::ContextPositions => {
                            let mut binds = vec![EvidenceBind::Text(Some(company))];
                            binds.extend(request.values.into_iter().map(EvidenceBind::Text));
                            evidence_rows(connection, CONTEXT_POSITIONS_SQL.to_owned(), binds, context_position_row).await?
                        }
                        ExecutiveEvidenceKind::NodeEvidence | ExecutiveEvidenceKind::PendingEvidence => {
                            let pending = request.kind == ExecutiveEvidenceKind::PendingEvidence;
                            let mut sql = if pending { PENDING_EVIDENCE_SQL } else { NODE_EVIDENCE_SQL }.to_owned();
                            let mut binds = vec![EvidenceBind::Text(Some(company))];
                            binds.extend(request.values.into_iter().map(EvidenceBind::Text));
                            if let Some(node) = &request.node_id {
                                if pending {
                                    sql.push_str(" AND c.id=? LIMIT 1");
                                } else {
                                    sql.push_str(" AND n.id=? LIMIT 1");
                                }
                                binds.push(EvidenceBind::Text(Some(node.clone())));
                            } else {
                                if let Some(query) = &request.query {
                                    if pending {
                                        sql.push_str(" AND position(lower(?) in lower(concat_ws(' ',c.action,c.node_kind,c.display_name,c.state,b.display_name)))>0");
                                    } else {
                                        sql.push_str(" AND position(lower(?) in lower(concat_ws(' ',n.display_name,n.kind,n.slug,b.display_name)))>0");
                                    }
                                    binds.push(EvidenceBind::Text(Some(query.clone())));
                                }
                                if let Some(name) = &request.cursor_name {
                                    sql.push_str(" AND (n.display_name>? OR (n.display_name=? AND n.id>?))");
                                    binds.push(EvidenceBind::Text(Some(name.clone())));
                                    binds.push(EvidenceBind::Text(Some(name.clone())));
                                    binds.push(EvidenceBind::Text(Some(request.cursor_id.clone().unwrap_or_default())));
                                }
                                if let Some(at) = &request.cursor_at {
                                    sql.push_str(" AND (c.updated_at<? OR (c.updated_at=? AND c.id>?))");
                                    binds.push(EvidenceBind::Number(*at));
                                    binds.push(EvidenceBind::Number(*at));
                                    binds.push(EvidenceBind::Text(Some(request.cursor_id.clone().unwrap_or_default())));
                                }
                                if pending {
                                    sql.push_str(" ORDER BY c.updated_at DESC,c.id ASC LIMIT ?");
                                } else {
                                    sql.push_str(" ORDER BY n.display_name,n.id LIMIT ?");
                                }
                                binds.push(EvidenceBind::Number(request.limit.unwrap_or(31)));
                            }
                            if pending {
                                evidence_rows(connection, sql, binds, pending_evidence_row).await?
                            } else {
                                evidence_rows(connection, sql, binds, node_evidence_row).await?
                            }
                        }
                    };
                    Ok(ExecutiveEvidence { rows })
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
    fn retired_node_reference_sql_is_byte_identical() {
        assert_eq!(
            hash(AUDIENCE_NODE_SQL),
            "f735e1687f8991d098fb440a9b42ac848db5e518ba2d55897bffc2b07c814f09"
        );
        assert_eq!(
            hash(UNIT_REFERENCE_SQL),
            "4487d2a30b78b49145077dc6003ffbbd729d64b0f72dbc66238724cfb903f336"
        );
        assert_eq!(
            hash(ATTACHMENT_NODE_SQL),
            "1a550056dc4a644104b5b7fadcdfd2581362ad20ae24a30bfad8a7914021e7c3"
        );
    }

    #[test]
    fn retired_executive_evidence_sql_is_byte_identical() {
        assert_eq!(
            hash(CONNECTION_SCOPES_SQL),
            "270124c0154df0b1f42c8a76baa46168719ff009c88362a3ea46ed875af54f4a"
        );
        assert_eq!(
            hash(CONTEXT_NODES_SQL),
            "217fcab9daf0f888e232b67e858e8536754a0e5a839b0a21664e969494dd6170"
        );
        assert_eq!(
            hash(CONTEXT_POSITIONS_SQL),
            "13d40c34f8423ea329375fac2892fc96d31f56b7687a4416100393ba5979ff49"
        );
        assert_eq!(
            hash(NODE_EVIDENCE_SQL),
            "1f2cc5c5dff8a615f55fdfd0d0334c75006d0e69f856f56ec9e121f1c5ae4f0b"
        );
        assert_eq!(
            hash(PENDING_EVIDENCE_SQL),
            "00ac378a2a99356eb5a98f4b6ff6ad020baa19dc2c9e04fab09f1b0d189664f5"
        );
    }

    #[test]
    fn no_statement_escapes_the_owner_read() {
        for sql in [
            NODES_SQL,
            LEVELS_SQL,
            POSITIONS_SQL,
            AUDIENCE_NODE_SQL,
            UNIT_REFERENCE_SQL,
            ATTACHMENT_NODE_SQL,
            CONNECTION_SCOPES_SQL,
            CONTEXT_NODES_SQL,
            CONTEXT_POSITIONS_SQL,
            NODE_EVIDENCE_SQL,
            PENDING_EVIDENCE_SQL,
        ] {
            let upper = format!(" {} ", sql.to_ascii_uppercase());
            assert!(upper.starts_with(" SELECT "));
            // Column names such as `updated_at` contain UPDATE as a substring;
            // reject only standalone write statements and row locks.
            assert!(!upper.contains(" INSERT ") && !upper.contains(" DELETE ") && !upper.contains(" UPDATE "));
            assert!(!upper.contains(" FOR UPDATE ") && !upper.contains(" LOCK TABLE "));
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
