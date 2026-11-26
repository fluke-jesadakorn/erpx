//! Owner SQL for the native Work page-layout read. The caller owns one
//! transaction and all authority; this adapter executes the exact retained
//! read statement and returns typed rows for the pure projection.

use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Exact retained SQL from `adapters/page-layouts.ts` `unit(db).layouts`.
pub const PAGE_LAYOUTS_READ_SQL: &str = "SELECT owner_id,layout_json,version FROM page_layouts WHERE company_id=$1 AND page_key=$2 AND ((owner_id=$3 AND workspace_base=$4) OR (owner_id=$5 AND workspace_base=$6))";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageLayoutRow {
    pub owner_id: String,
    pub layout_json: Option<String>,
    pub version: i64,
}

/// Executes the read on the caller's retained unit. No transaction is opened
/// here and no authority is granted by this function.
pub async fn read_page_layout_rows(
    connection: &mut PgConnection,
    company: &str,
    page_key: &str,
    user: &str,
    base: &str,
) -> Result<Vec<PageLayoutRow>, Error> {
    let rows = sqlx::query(PAGE_LAYOUTS_READ_SQL)
        .bind(company)
        .bind(page_key)
        .bind(user)
        .bind(base)
        .bind("organization")
        .bind("*")
        .fetch_all(&mut *connection)
        .await?;
    Ok(rows
        .iter()
        .map(|row| PageLayoutRow {
            owner_id: row.try_get("owner_id").unwrap_or_default(),
            layout_json: row.try_get::<Option<String>, _>("layout_json").ok().flatten(),
            version: row.try_get::<i64, _>("version").unwrap_or(0),
        })
        .collect())
}

/// Selects the personal and organization rows exactly as the retained reader.
pub fn select_layouts<'a>(
    rows: &'a [PageLayoutRow],
    user: &str,
) -> (Option<&'a PageLayoutRow>, Option<&'a PageLayoutRow>) {
    (
        rows.iter().find(|row| row.owner_id == user),
        rows.iter().find(|row| row.owner_id == "organization"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::page_layouts::{decode_layout, layout_snapshot_json};
    use std::collections::BTreeSet;

    fn rows() -> Vec<PageLayoutRow> {
        vec![
            PageLayoutRow {
                owner_id: "organization".into(),
                layout_json: Some(
                    r#"{"schemaVersion":1,"widgets":[{"id":"w1","columns":6,"height":null,"hidden":false},{"id":"removed","columns":6,"height":null,"hidden":false}]}"#
                        .into(),
                ),
                version: 3,
            },
            PageLayoutRow {
                owner_id: "user-1".into(),
                layout_json: Some(
                    r#"{"schemaVersion":1,"widgets":[{"id":"w2","columns":12,"height":400,"hidden":true}]}"#
                        .into(),
                ),
                version: 2,
            },
            PageLayoutRow {
                owner_id: "user-1".into(),
                layout_json: None,
                version: 9,
            },
        ]
    }

    #[test]
    fn selected_rows_project_exact_snapshot_with_only_allowed_widgets() {
        let rows = rows();
        let (personal_row, organization_row) = select_layouts(&rows, "user-1");
        let personal_row = personal_row.expect("personal row");
        let organization_row = organization_row.expect("organization row");
        assert_eq!((personal_row.version, organization_row.version), (2, 3));

        let allowed = BTreeSet::from(["w1".to_owned(), "w2".to_owned()]);
        let personal = decode_layout(personal_row.layout_json.as_deref(), &allowed)
            .expect("decoded personal layout");
        let organization = decode_layout(organization_row.layout_json.as_deref(), &allowed)
            .expect("decoded organization layout");
        let organization = organization.expect("organization layout");
        assert_eq!(organization.widgets.len(), 1);
        assert_eq!(organization.widgets[0].id, "w1");

        let wire = layout_snapshot_json(
            personal.as_ref(),
            Some(&organization),
            personal_row.version,
            organization_row.version,
            true,
        );
        assert_eq!(wire["version"], 2);
        assert_eq!(wire["organizationVersion"], 3);
        assert_eq!(wire["canPublish"], true);
        assert_eq!(wire["personal"]["widgets"][0]["id"], "w2");
        assert_eq!(wire["personal"]["widgets"][0]["hidden"], true);
        assert_eq!(wire["organization"]["widgets"][0]["id"], "w1");
        assert_eq!(wire.as_object().unwrap().len(), 5);
    }

    #[test]
    fn missing_personal_row_is_absent_not_fabricated() {
        let rows = rows();
        let (personal, organization) = select_layouts(&rows, "unknown-user");
        assert!(personal.is_none());
        assert!(organization.is_some());
    }
}
