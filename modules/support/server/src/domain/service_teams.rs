//! Pure service-team input normalization and locally decidable validation.
//! The caller supplies the current People-owned category catalogue. This does
//! not establish owner authority, membership, scope, version, or persistence.
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceTeamInput {
    pub name: String,
    pub active: bool,
    pub expected_version: i64,
    pub members: Vec<Member>,
    pub coverage: Vec<Coverage>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub user_id: String,
    pub responsibility: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub category: String,
    pub scope_kind: String,
    pub scope_node_id: String,
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'))
}
fn bounded(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.chars().count())
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, &'static str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or("VALIDATION_FAILED")
}

/// Mirrors Zod parsing and checks that require no database facts. The caller
/// must perform owner/current-version checks first, before the returned local
/// decisions, matching `createValidateServiceTeam`'s ordering.
pub fn normalize_service_team(
    raw: &Value,
    categories: &[&str],
) -> Result<ServiceTeamInput, &'static str> {
    let object = raw.as_object().ok_or("VALIDATION_FAILED")?;
    let name = js_trim(string(raw, "name")?);
    if !bounded(name, 2, 160) {
        return Err("VALIDATION_FAILED");
    }
    let active = match object.get("active") {
        None => true,
        Some(v) => v.as_bool().ok_or("VALIDATION_FAILED")?,
    };
    let n = match object.get("expectedVersion") {
        None => 1.0,
        Some(v) => v.as_f64().ok_or("VALIDATION_FAILED")?,
    };
    if !n.is_finite() || n.fract() != 0.0 || !(1.0..=MAX_SAFE_INTEGER).contains(&n) {
        return Err("VALIDATION_FAILED");
    }
    let member_values = object
        .get("members")
        .and_then(Value::as_array)
        .ok_or("VALIDATION_FAILED")?;
    if member_values.len() > 100 {
        return Err("VALIDATION_FAILED");
    }
    let mut members = Vec::with_capacity(member_values.len());
    for item in member_values {
        let id = string(item, "userId")?;
        let responsibility = string(item, "responsibility")?;
        if !bounded(id, 1, 160) || !matches!(responsibility, "lead" | "member") {
            return Err("VALIDATION_FAILED");
        }
        members.push(Member {
            user_id: id.into(),
            responsibility: responsibility.into(),
        });
    }
    let coverage_values = object
        .get("coverage")
        .and_then(Value::as_array)
        .ok_or("VALIDATION_FAILED")?;
    if !(1..=50).contains(&coverage_values.len()) {
        return Err("VALIDATION_FAILED");
    }
    let mut coverage = Vec::with_capacity(coverage_values.len());
    for item in coverage_values {
        let category = string(item, "category")?;
        let kind = string(item, "scopeKind")?;
        let node = string(item, "scopeNodeId")?;
        if !categories.contains(&category)
            || !matches!(kind, "holding" | "branch" | "department")
            || !bounded(node, 1, 160)
        {
            return Err("VALIDATION_FAILED");
        }
        coverage.push(Coverage {
            category: category.into(),
            scope_kind: kind.into(),
            scope_node_id: node.into(),
        });
    }
    Ok(ServiceTeamInput {
        name: name.into(),
        active,
        expected_version: n as i64,
        members,
        coverage,
    })
}

pub fn validate_local_team_rules(input: &ServiceTeamInput) -> Result<(), &'static str> {
    let mut members = HashSet::new();
    if input
        .members
        .iter()
        .any(|member| !members.insert(&member.user_id))
    {
        return Err("DUPLICATE_MEMBER");
    }
    let mut coverage = HashSet::new();
    // JS builds exactly this colon-delimited string; do not replace its
    // equality with tuple equality when source identifiers contain ':'.
    if input.coverage.iter().any(|c| {
        !coverage.insert(format!(
            "{}:{}:{}",
            c.category, c.scope_kind, c.scope_node_id
        ))
    }) {
        return Err("DUPLICATE_COVERAGE");
    }
    if input.active
        && !input
            .members
            .iter()
            .any(|member| member.responsibility == "lead")
    {
        return Err("TEAM_LEAD_REQUIRED");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATEGORIES: &[&str] = &[
        "it",
        "engineering",
        "maintenance",
        "facilities",
        "housekeeping",
        "people",
        "finance",
        "purchasing",
        "operations",
        "sales",
    ];

    fn normalize(raw: &Value) -> Result<ServiceTeamInput, &'static str> {
        normalize_service_team(raw, CATEGORIES)
    }

    fn decided(raw: &Value) -> Result<ServiceTeamInput, &'static str> {
        let input = normalize(raw)?;
        validate_local_team_rules(&input)?;
        Ok(input)
    }

    #[test]
    fn normalizes_the_retained_payload_and_strips_unknown_fields() {
        let input = decided(&serde_json::json!({
            "name": " \u{feff}Team 😀 ",
            "members": [{"userId": "a", "responsibility": "lead", "extra": 1}],
            "coverage": [{"category": "it", "scopeKind": "holding", "scopeNodeId": "c", "extra": 1}],
            "extra": true
        }))
        .unwrap();
        assert_eq!(input.name, "Team 😀");
        assert!(input.active);
        assert_eq!(input.expected_version, 1);
        assert_eq!(
            input.members,
            vec![Member {
                user_id: "a".into(),
                responsibility: "lead".into()
            }]
        );
        assert_eq!(
            input.coverage,
            vec![Coverage {
                category: "it".into(),
                scope_kind: "holding".into(),
                scope_node_id: "c".into()
            }]
        );
        let inactive = normalize(&serde_json::json!({
            "name": "Team",
            "active": false,
            "expectedVersion": 2,
            "members": [],
            "coverage": [{"category": "it", "scopeKind": "branch", "scopeNodeId": "b"}]
        }))
        .unwrap();
        assert!(!inactive.active);
        assert_eq!(inactive.expected_version, 2);
        assert_eq!(validate_local_team_rules(&inactive), Ok(()));
    }

    #[test]
    fn enforces_duplicate_and_lead_rules() {
        assert_eq!(
            decided(&serde_json::json!({
                "name": "Team",
                "members": [
                    {"userId": "a", "responsibility": "lead"},
                    {"userId": "a", "responsibility": "member"}
                ],
                "coverage": [{"category": "it", "scopeKind": "holding", "scopeNodeId": "c"}]
            })),
            Err("DUPLICATE_MEMBER")
        );
        assert_eq!(
            decided(&serde_json::json!({
                "name": "Team",
                "members": [{"userId": "a", "responsibility": "lead"}],
                "coverage": [
                    {"category": "it", "scopeKind": "holding", "scopeNodeId": "c"},
                    {"category": "it", "scopeKind": "holding", "scopeNodeId": "c"}
                ]
            })),
            Err("DUPLICATE_COVERAGE")
        );
        assert_eq!(
            decided(&serde_json::json!({
                "name": "Team",
                "members": [{"userId": "a", "responsibility": "member"}],
                "coverage": [{"category": "it", "scopeKind": "holding", "scopeNodeId": "c"}]
            })),
            Err("TEAM_LEAD_REQUIRED")
        );
    }

    #[test]
    fn rejects_the_same_schema_bounds_as_the_retained_zod_schema() {
        for raw in [
            serde_json::json!({"name": "T", "members": [], "coverage": [{"category": "it", "scopeKind": "holding", "scopeNodeId": "c"}]}),
            serde_json::json!({"name": "Team", "active": null, "members": [], "coverage": [{"category": "it", "scopeKind": "holding", "scopeNodeId": "c"}]}),
            serde_json::json!({"name": "Team", "expectedVersion": 9007199254740992_u64, "members": [], "coverage": [{"category": "it", "scopeKind": "holding", "scopeNodeId": "c"}]}),
            serde_json::json!({"name": "Team", "members": [], "coverage": [{"category": "unknown", "scopeKind": "holding", "scopeNodeId": "c"}]}),
            serde_json::json!({"name": "Team", "members": [{"userId": "😀".repeat(161), "responsibility": "lead"}], "coverage": [{"category": "it", "scopeKind": "holding", "scopeNodeId": "c"}]}),
            serde_json::json!({"name": "😀", "members": [{"userId": "a", "responsibility": "lead"}], "coverage": [{"category": "it", "scopeKind": "holding", "scopeNodeId": "c"}]}),
            serde_json::json!({"name": "Team", "members": [], "coverage": []}),
        ] {
            assert_eq!(normalize(&raw), Err("VALIDATION_FAILED"));
        }
        let long_user = normalize(&serde_json::json!({
            "name": "Team",
            "members": [{"userId": "😀".repeat(81), "responsibility": "lead"}],
            "coverage": [{"category": "it", "scopeKind": "holding", "scopeNodeId": "c"}]
        }))
        .unwrap();
        assert_eq!(long_user.members[0].user_id.chars().count(), 81);
    }
}
