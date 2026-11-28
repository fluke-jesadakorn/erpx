//! Pure query contract and projection for the R3 page-layout read. No SQL and
//! no I/O live here: the owner reader supplies rows, composition supplies the
//! scope and widget allow-list, and this layer renders the exact retained
//! `LayoutSnapshot` wire value.

use crate::adapters::page_layouts_read::{select_layouts, PageLayoutRow};
use crate::domain::page_layouts::{decode_layout, is_configurable_page, layout_snapshot_json};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::collections::BTreeSet;

/// All facts the owner needs to execute the retained read. `company`, `user`
/// and `base` are the exact bind values of the retired TypeScript statement;
/// `allowed_widget_ids` and `can_publish` are composition-resolved authority
/// facts, never caller input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadQuery {
    pub company: String,
    pub page_key: String,
    pub user: String,
    pub base: String,
    pub allowed_widget_ids: BTreeSet<String>,
    pub can_publish: bool,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PAGE_LAYOUT_READ_INVALID",
        "Use one bounded page-layout read.",
    )
}

/// Bounded owner-scope identifier, identical shape to the other Work reads.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// Fail closed before any SQL is planned: the page key must be one owner
/// allow-list entry, scope identifiers must be bounded, and at least one widget
/// must be available to the current role (the retained 403 parity).
pub fn validate(query: &ReadQuery) -> Result<(), BusinessError> {
    if !valid_id(&query.company)
        || !valid_id(&query.user)
        || !is_configurable_page(&query.page_key)
        || query.page_key.len() > 128
        || query.base.is_empty()
        || query.base.len() > 200
        || query.allowed_widget_ids.is_empty()
        || query
            .allowed_widget_ids
            .iter()
            .any(|id| !valid_id(id))
    {
        return Err(invalid());
    }
    Ok(())
}

/// Project the owner rows into the exact retained `LayoutSnapshot`. Personal
/// and organization rows are selected by `owner_id`; versions come from the
/// rows, not from a decoded layout, and stored widgets outside the current
/// role's allow-list are dropped exactly as the retired reader did.
pub fn project_snapshot(rows: &[PageLayoutRow], query: &ReadQuery) -> Result<Value, BusinessError> {
    validate(query)?;
    let (personal_row, organization_row) = select_layouts(rows, &query.user);
    let version = personal_row.map_or(0, |row| row.version);
    let organization_version = organization_row.map_or(0, |row| row.version);
    let personal = decode_layout(
        personal_row.and_then(|row| row.layout_json.as_deref()),
        &query.allowed_widget_ids,
    )?;
    let organization = decode_layout(
        organization_row.and_then(|row| row.layout_json.as_deref()),
        &query.allowed_widget_ids,
    )?;
    Ok(layout_snapshot_json(
        personal.as_ref(),
        organization.as_ref(),
        version,
        organization_version,
        query.can_publish,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn query() -> ReadQuery {
        ReadQuery {
            company: "acme-co".into(),
            page_key: "people-directory".into(),
            user: "user-1".into(),
            base: "/holdings/acme/people".into(),
            allowed_widget_ids: BTreeSet::from(["w1".to_owned(), "w2".to_owned()]),
            can_publish: true,
        }
    }

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
        ]
    }

    #[test]
    fn projection_matches_the_retained_snapshot_wire_shape() {
        let value = project_snapshot(&rows(), &query()).unwrap();
        assert_eq!(
            value,
            json!({
                "personal": {"schemaVersion":1,"widgets":[{"id":"w2","columns":12,"height":400,"hidden":true}]},
                "organization": {"schemaVersion":1,"widgets":[{"id":"w1","columns":6,"height":null,"hidden":false}]},
                "version": 2,
                "organizationVersion": 3,
                "canPublish": true
            })
        );
        assert_eq!(value.as_object().unwrap().len(), 5);
    }

    #[test]
    fn missing_rows_fabricate_zero_versions_and_null_layouts() {
        let value = project_snapshot(&[], &query()).unwrap();
        assert_eq!(
            value,
            json!({"personal":null,"organization":null,"version":0,"organizationVersion":0,"canPublish":true})
        );
    }

    #[test]
    fn validation_fails_closed_on_scope_and_allow_list() {
        assert!(validate(&query()).is_ok());
        let mut bad = query();
        bad.page_key = "platform-home".into();
        assert_eq!(validate(&bad).unwrap_err().code, "PAGE_LAYOUT_READ_INVALID");
        let mut bad = query();
        bad.allowed_widget_ids.clear();
        assert!(validate(&bad).is_err());
        let mut bad = query();
        bad.user = "../bad".into();
        assert!(validate(&bad).is_err());
    }
}
