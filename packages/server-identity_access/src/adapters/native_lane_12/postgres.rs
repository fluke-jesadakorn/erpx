//! sqlx execution for lane 12. The three statements are byte-identical to the
//! rendered TypeScript literals they replace; `company_id` stays in the WHERE
//! clause and bind order is fixed. Projection into the response envelope and
//! authorization-safe company scope are owned here, not by the TS client.

use super::application::{AttributeReadFuture, PeopleTemplateRead};
use super::domain::{AttributePage, AttributeRead, ReadPage};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::Row;

/// `SELECT id,version,definition FROM um_attributes WHERE company_id=$1 ORDER BY id`
pub const ATTRIBUTES_SQL: &str =
    "SELECT id,version,definition FROM um_attributes WHERE company_id=$1 ORDER BY id";
/// `activeIamRoleProbeSql(false)` rendered with the fixed active-role head.
pub const ACTIVE_ROLE_SQL: &str =
    "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'";
/// `SELECT 1 FROM um_groups ... EXISTS(jsonbArrayHeadSql(...))` fully rendered.
pub const GROUP_REFERENCES_SQL: &str =
    "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean AND EXISTS(SELECT 1 FROM jsonb_array_elements(definition->'rule'->'conditions') x WHERE x->>'field'=$2)";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LANE_12_READ_UNAVAILABLE",
        "The native People-template attribute read is unavailable.",
    )
}

/// Pg-backed owner reader. All three relations are identity_access owned; no
/// other owner's permission, membership or workflow authority is inferred.
pub struct PgPeopleTemplateRead {
    pool: DatabasePool,
}

impl PgPeopleTemplateRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PeopleTemplateRead for PgPeopleTemplateRead {
    fn read<'a>(&'a self, read: &'a AttributeRead) -> AttributeReadFuture<'a, AttributePage> {
        let pool = self.pool.clone();
        let read = read.clone();
        Box::pin(async move {
            match read {
                AttributeRead::Attributes { company_id } => {
                    let rows = sqlx::query(ATTRIBUTES_SQL)
                        .bind(&company_id)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    let rows = rows
                        .into_iter()
                        .map(|row| {
                            Ok(json!({
                                "id": row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                "version": i64::from(row.try_get::<i32, _>("version").map_err(|_| unavailable())?),
                                "definition": row.try_get::<Value, _>("definition").map_err(|_| unavailable())?,
                            }))
                        })
                        .collect::<Result<Vec<Value>, BusinessError>>()?;
                    Ok(ReadPage::new(rows))
                }
                AttributeRead::ActiveRole { company_id, role } => {
                    let rows = sqlx::query(ACTIVE_ROLE_SQL)
                        .bind(&role)
                        .bind(&company_id)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(ReadPage::new(vec![json!({"one": 1}); rows.len()]))
                }
                AttributeRead::GroupReferences { company_id, value } => {
                    let rows = sqlx::query(GROUP_REFERENCES_SQL)
                        .bind(&company_id)
                        .bind(&value)
                        .fetch_all(pool.raw())
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(ReadPage::new(vec![json!({"one": 1}); rows.len()]))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn native_lane_12_sql_oracle_hashes() {
        // Byte-identical rendered statements retired from the TS bucket.
        assert_eq!(
            hash(ATTRIBUTES_SQL),
            "fc2fca4285b7a6baf1681610ba50fec70a983b65c922160ea46c2e7637a7e396"
        );
        assert_eq!(
            hash(ACTIVE_ROLE_SQL),
            "dcb70327c0b64b8492d55fc9333fd992697b01c5b792e576a2e54aad0250522d"
        );
        assert_eq!(
            hash(GROUP_REFERENCES_SQL),
            "5ae2987e3ed8e9d9d67b77ac23f7fabbdb5d6b5499560f7617ad18bf9b1364e2"
        );
    }

    #[test]
    fn native_lane_12_bind_order_is_company_scoped() {
        // The company predicate is always first on the company-only reads and
        // second on the id+company active-role probe, matching pg's $n order.
        assert!(ATTRIBUTES_SQL.contains("company_id=$1 ORDER BY id"));
        assert!(ACTIVE_ROLE_SQL.contains("id=$1 AND company_id IN ('*',$2)"));
        assert!(GROUP_REFERENCES_SQL.contains("company_id=$1"));
        assert!(GROUP_REFERENCES_SQL.contains("x->>'field'=$2"));
        for sql in [ATTRIBUTES_SQL, ACTIVE_ROLE_SQL, GROUP_REFERENCES_SQL] {
            assert!(sql.starts_with("SELECT "));
        }
    }

    #[test]
    fn native_lane_12_result_oracle() {
        let attributes = ReadPage::new(vec![
            json!({"id": "custom:a", "version": 2, "definition": {"type": "text"}}),
            json!({"id": "custom:b", "version": 1, "definition": {"type": "role"}}),
        ]);
        let rendered = serde_json::to_string(&attributes).unwrap();
        assert_eq!(
            hash(&rendered),
            hash(&serde_json::to_string(&ReadPage::new(vec![
                json!({"id": "custom:a", "version": 2, "definition": {"type": "text"}}),
                json!({"id": "custom:b", "version": 1, "definition": {"type": "role"}}),
            ]))
            .unwrap())
        );
        assert_eq!(attributes.row_count, 2);
        let probes: AttributePage = ReadPage::new(vec![json!({"one": 1})]);
        assert_eq!(serde_json::to_value(&probes).unwrap(), json!({"rows": [{"one": 1}], "rowCount": 1}));
    }
}
