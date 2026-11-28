//! Pure Work Projection page-layout contract.
//!
//! Mirrors the retained TypeScript `pageLayoutSchema`, `pageLayoutInputSchema`
//! and `LayoutSnapshot` wire shape. No SQL and no authority live here: the
//! allowed widget ids are supplied by the caller's navigation authority read.

use erpx_platform_shared_server::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// Exact `configurablePageKeys` from `page-layouts-view.ts`.
pub const CONFIGURABLE_PAGE_KEYS: &[&str] = &[
    "people-directory",
    "people-directory-detail",
    "people-organization",
    "people-roster",
    "people-settings",
    "people-payroll",
    "people-payroll-detail",
    "people-clearance-detail",
    "overtime",
    "overtime-detail",
    "executive",
];

pub fn is_configurable_page(page: &str) -> bool {
    CONFIGURABLE_PAGE_KEYS.contains(&page)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LayoutItem {
    pub id: String,
    pub columns: i64,
    pub height: Option<i64>,
    pub hidden: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLayout {
    pub schema_version: i64,
    pub widgets: Vec<LayoutItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageLayoutAction {
    Save,
    Reset,
    Publish,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageLayoutInput {
    pub action: PageLayoutAction,
    pub expected_version: i64,
    pub layout: Option<PageLayout>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireLayoutItem {
    id: String,
    columns: i64,
    height: Option<i64>,
    hidden: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePageLayout {
    #[serde(rename = "schemaVersion")]
    schema_version: i64,
    widgets: Vec<WireLayoutItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireInput {
    action: String,
    #[serde(rename = "expectedVersion")]
    expected_version: i64,
    layout: Option<WirePageLayout>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Strict parse of `pageLayoutInputSchema` including Zod's bounds.
pub fn parse_page_layout_input(value: &Value) -> Result<PageLayoutInput, BusinessError> {
    let invalid_input = || invalid("LAYOUT_INPUT_INVALID", "Provide a valid layout command.");
    if !value.is_object()
        || !value
            .as_object()
            .is_some_and(|map| ["action", "expectedVersion", "layout"].iter().all(|key| map.contains_key(*key)))
    {
        return Err(invalid_input());
    }
    let wire: WireInput = serde_json::from_value(value.clone())
        .map_err(|_| invalid("LAYOUT_INPUT_INVALID", "Provide a valid layout command."))?;
    let action = match wire.action.as_str() {
        "save" => PageLayoutAction::Save,
        "reset" => PageLayoutAction::Reset,
        "publish" => PageLayoutAction::Publish,
        _ => return Err(invalid("LAYOUT_INPUT_INVALID", "Provide a valid layout command.")),
    };
    if wire.expected_version < 0 {
        return Err(invalid("LAYOUT_INPUT_INVALID", "Provide a valid layout command."));
    }
    let layout = wire
        .layout
        .map(|layout| {
            if layout.schema_version != 1 || layout.widgets.len() > 120 {
                return Err(invalid("LAYOUT_INPUT_INVALID", "Provide a valid layout command."));
            }
            let mut seen = BTreeSet::new();
            let mut widgets = Vec::with_capacity(layout.widgets.len());
            for widget in layout.widgets {
                if widget.id.is_empty()
                    || widget.id.len() > 160
                    || !(3..=12).contains(&widget.columns)
                    || widget
                        .height
                        .is_some_and(|height| !(160..=1600).contains(&height))
                    || !seen.insert(widget.id.clone())
                {
                    return Err(invalid("LAYOUT_INPUT_INVALID", "Provide a valid layout command."));
                }
                widgets.push(LayoutItem {
                    id: widget.id,
                    columns: widget.columns,
                    height: widget.height,
                    hidden: widget.hidden,
                });
            }
            Ok(PageLayout {
                schema_version: 1,
                widgets,
            })
        })
        .transpose()?;
    Ok(PageLayoutInput {
        action,
        expected_version: wire.expected_version,
        layout,
    })
}

/// Decodes one stored `layout_json`, filtering out widgets no longer available
/// for the current role, exactly as the reader in `application/page-layouts.ts`.
pub fn decode_layout(
    raw: Option<&str>,
    allowed_ids: &BTreeSet<String>,
) -> Result<Option<PageLayout>, BusinessError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let layout: PageLayout = serde_json::from_str(raw)
        .map_err(|_| invalid("LAYOUT_STORED_INVALID", "The saved layout is unavailable."))?;
    if layout.schema_version != 1 {
        return Err(invalid("LAYOUT_STORED_INVALID", "The saved layout is unavailable."));
    }
    Ok(Some(PageLayout {
        schema_version: 1,
        widgets: layout
            .widgets
            .into_iter()
            .filter(|widget| allowed_ids.contains(&widget.id))
            .collect(),
    }))
}

/// Renders the exact `LayoutSnapshot` wire shape.
pub fn layout_snapshot_json(
    personal: Option<&PageLayout>,
    organization: Option<&PageLayout>,
    version: i64,
    organization_version: i64,
    can_publish: bool,
) -> Value {
    serde_json::to_value(personal)
        .and_then(|personal| {
            serde_json::to_value(organization).map(|organization| {
                serde_json::json!({
                    "personal": personal,
                    "organization": organization,
                    "version": version,
                    "organizationVersion": organization_version,
                    "canPublish": can_publish,
                })
            })
        })
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn page_allow_list_is_exact() {
        assert_eq!(CONFIGURABLE_PAGE_KEYS.len(), 11);
        assert!(is_configurable_page("people-directory-detail"));
        assert!(is_configurable_page("executive"));
        assert!(!is_configurable_page("platform-home"));
        assert!(!is_configurable_page("account-home"));
    }

    #[test]
    fn input_is_strict_and_bounded() {
        let valid = json!({
            "action": "save",
            "expectedVersion": 2,
            "layout": {"schemaVersion": 1, "widgets": [{"id": "w1", "columns": 12, "height": null, "hidden": false}]}
        });
        assert_eq!(
            parse_page_layout_input(&valid).unwrap(),
            PageLayoutInput {
                action: PageLayoutAction::Save,
                expected_version: 2,
                layout: Some(PageLayout {
                    schema_version: 1,
                    widgets: vec![LayoutItem {
                        id: "w1".into(),
                        columns: 12,
                        height: None,
                        hidden: false,
                    }],
                }),
            }
        );
        assert!(parse_page_layout_input(&json!({"action":"save","expectedVersion":1,"layout":null,"extra":1})).is_err());
        assert!(parse_page_layout_input(&json!({"action":"publish","expectedVersion":-1,"layout":null})).is_err());
        assert!(parse_page_layout_input(&json!({"action":"save","expectedVersion":1,"layout":{"schemaVersion":2,"widgets":[]}})).is_err());
        assert!(parse_page_layout_input(&json!({"action":"save","expectedVersion":1,"layout":{"schemaVersion":1,"widgets":[{"id":"w","columns":2,"height":null,"hidden":false}]}})).is_err());
        assert!(parse_page_layout_input(&json!({"action":"save","expectedVersion":1,"layout":{"schemaVersion":1,"widgets":[{"id":"w","columns":12,"height":null,"hidden":false},{"id":"w","columns":12,"height":null,"hidden":true}]}})).is_err());
    }

    #[test]
    fn decode_filters_disallowed_widgets_and_keeps_versions() {
        let allowed = BTreeSet::from(["w1".to_owned()]);
        let raw = r#"{"schemaVersion":1,"widgets":[{"id":"w1","columns":6,"height":null,"hidden":false},{"id":"w2","columns":6,"height":null,"hidden":false}]}"#;
        let layout = decode_layout(Some(raw), &allowed).unwrap().unwrap();
        assert_eq!(layout.widgets.len(), 1);
        assert_eq!(layout.widgets[0].id, "w1");
        assert!(decode_layout(None, &allowed).unwrap().is_none());
    }

    #[test]
    fn snapshot_wire_keys_match_the_browser_contract() {
        let wire = layout_snapshot_json(None, None, 2, 3, true);
        assert_eq!(
            wire,
            json!({
                "personal": null,
                "organization": null,
                "version": 2,
                "organizationVersion": 3,
                "canPublish": true
            })
        );
    }
}
