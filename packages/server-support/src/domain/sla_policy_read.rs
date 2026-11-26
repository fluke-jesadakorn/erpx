//! Portable Support SLA policy read model. No SQL, HTTP, clock or notification
//! effect is reachable here. The projections mirror the retained TypeScript
//! `getSupportPolicy` and `supportPolicyWork` payloads field for field.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Already-authorized reader facts. Row visibility is enforced by owner SQL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlaPolicyReadScope {
    pub company_id: String,
    pub user_id: String,
    pub support_manage: bool,
    pub support_read: bool,
    /// `support.sla` purchase `newWork`.
    pub new_work: bool,
}

impl SlaPolicyReadScope {
    pub fn can_manage(&self) -> bool {
        self.support_manage
    }

    pub fn same_person(&self, user_id: &str) -> bool {
        self.user_id == user_id
    }
}

/// Owner rows handed from the SQL adapter to the application layer. The two
/// read statements return the same policy columns; the work statement adds the
/// independent-reviewer eligibility and reviewer-count projections.
#[derive(Clone, Debug, PartialEq)]
pub struct SlaPolicyRow {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub policy_json: Value,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub approved_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub actor_eligible: i64,
    pub reviewers: i64,
}

/// Complete `getSupportPolicy` payload: the retained row with its exact
/// snake_case column keys, including nullable approval facts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlaPolicyPointView {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub policy_json: Value,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub approved_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Map one owner policy row to the complete point payload.
pub fn point(row: &SlaPolicyRow) -> Result<SlaPolicyPointView, BusinessError> {
    if row.id.is_empty() || row.company_id.is_empty() {
        return Err(invalid(
            "SUPPORT_SLA_POLICY_INVALID",
            "Provide a valid policy row.",
        ));
    }
    Ok(SlaPolicyPointView {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        name: row.name.clone(),
        policy_json: row.policy_json.clone(),
        status: row.status.clone(),
        version: row.version,
        created_by: row.created_by.clone(),
        approved_by: row.approved_by.clone(),
        approved_at: row.approved_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Complete `supportPolicyWork` work-item payload. `toString` coercions of the
/// TypeScript projection are preserved; a NULL reviewer count is zero.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlaPolicyWorkItem {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub href: String,
    pub status: String,
    pub updated_at: i64,
    pub queue: String,
    pub can_act: bool,
    pub responsible_queue: String,
    pub next_action: String,
    pub blocker: Option<String>,
    pub relationships: Vec<String>,
}

/// Pure mapping from owner rows to work items. The purchase `newWork` gate is
/// applied after independent-reviewer eligibility, exactly as the retained
/// TypeScript adapter applies its purchase envelope.
pub fn work_items(
    scope: &SlaPolicyReadScope,
    rows: Vec<SlaPolicyRow>,
) -> Result<Vec<SlaPolicyWorkItem>, BusinessError> {
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        if row.id.is_empty() {
            return Err(invalid(
                "SUPPORT_SLA_POLICY_INVALID",
                "Provide a valid policy row.",
            ));
        }
        let pending = row.status == "draft";
        let independent =
            scope.can_manage() && !scope.same_person(&row.created_by) && row.actor_eligible > 0;
        let base_can_act = pending && independent;
        let can_act = base_can_act && scope.new_work;
        let queue = if !pending {
            "completed"
        } else if can_act {
            "todo"
        } else {
            "waiting"
        };
        let relationships = if scope.same_person(&row.created_by) {
            "requested"
        } else if base_can_act {
            "approval"
        } else {
            "team"
        };
        items.push(SlaPolicyWorkItem {
            id: row.id,
            kind: "support_policy".into(),
            title: format!("Resolution-time policy · {}", row.name),
            detail: "Review the proposed working calendar and target before use.".into(),
            href: "/support/sla".into(),
            status: row.status,
            updated_at: row.updated_at,
            queue: queue.into(),
            can_act,
            responsible_queue: "Independent Company Support manager".into(),
            next_action: if pending {
                "Review or retire the proposed policy".into()
            } else {
                "Read the retained policy snapshot".into()
            },
            blocker: (pending && row.reviewers == 0).then(|| {
                "No independent eligible Company Support manager; administrator coverage is required"
                    .into()
            }),
            relationships: vec![relationships.into()],
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scope() -> SlaPolicyReadScope {
        SlaPolicyReadScope {
            company_id: "company".into(),
            user_id: "reviewer".into(),
            support_manage: true,
            support_read: true,
            new_work: true,
        }
    }

    fn row(status: &str, created_by: &str) -> SlaPolicyRow {
        SlaPolicyRow {
            id: "support-policy-1".into(),
            company_id: "company".into(),
            name: "Standard support".into(),
            policy_json: json!({"targetMinutes": 60}),
            status: status.into(),
            version: 1,
            created_by: created_by.into(),
            approved_by: None,
            approved_at: None,
            created_at: 1_793_757_000_000,
            updated_at: 1_793_757_600_000,
            actor_eligible: 1,
            reviewers: 1,
        }
    }

    #[test]
    fn point_keeps_every_retained_row_field() {
        let view = point(&row("draft", "creator")).unwrap();
        let value = serde_json::to_value(&view).unwrap();
        assert_eq!(value["id"], "support-policy-1");
        assert_eq!(value["company_id"], "company");
        assert_eq!(value["name"], "Standard support");
        assert_eq!(value["status"], "draft");
        assert_eq!(value["version"], 1);
        assert_eq!(value["created_by"], "creator");
        assert_eq!(value["policy_json"]["targetMinutes"], 60);
        assert_eq!(value["created_at"], 1_793_757_000_000_i64);
        assert_eq!(value["updated_at"], 1_793_757_600_000_i64);
        assert!(value["approved_by"].is_null());
        assert!(value["approved_at"].is_null());
        assert_eq!(value.as_object().unwrap().len(), 11);
    }

    #[test]
    fn work_items_match_the_retained_gates_and_shape() {
        let items = work_items(&scope(), vec![row("draft", "creator")]).unwrap();
        let value = serde_json::to_value(&items).unwrap();
        assert_eq!(value[0]["id"], "support-policy-1");
        assert_eq!(value[0]["type"], "support_policy");
        assert_eq!(value[0]["title"], "Resolution-time policy · Standard support");
        assert_eq!(value[0]["href"], "/support/sla");
        assert_eq!(value[0]["status"], "draft");
        assert_eq!(value[0]["updatedAt"], 1_793_757_600_000_i64);
        assert_eq!(value[0]["queue"], "todo");
        assert_eq!(value[0]["canAct"], true);
        assert_eq!(value[0]["relationships"][0], "approval");
        assert!(value[0]["blocker"].is_null());

        let completed = work_items(&scope(), vec![row("approved", "creator")]).unwrap();
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value[0]["queue"], "completed");
        assert_eq!(value[0]["canAct"], false);
        assert_eq!(value[0]["nextAction"], "Read the retained policy snapshot");
    }

    #[test]
    fn purchase_manager_scope_and_reviewers_narrow_actions() {
        let mut gated = scope();
        gated.new_work = false;
        let items = work_items(&gated, vec![row("draft", "creator")]).unwrap();
        let value = serde_json::to_value(&items).unwrap();
        assert_eq!(value[0]["canAct"], false);
        assert_eq!(value[0]["queue"], "waiting");

        let mut branch = scope();
        branch.support_manage = false;
        let items = work_items(&branch, vec![row("draft", "creator")]).unwrap();
        assert_eq!(serde_json::to_value(&items).unwrap()[0]["queue"], "waiting");

        let mut unreviewed = scope();
        let mut empty = row("draft", "creator");
        empty.reviewers = 0;
        let items = work_items(&unreviewed, vec![empty]).unwrap();
        assert!(serde_json::to_value(&items).unwrap()[0]["blocker"].is_string());
        unreviewed.user_id = "creator".into();
        let own = work_items(&unreviewed, vec![row("draft", "creator")]).unwrap();
        assert_eq!(
            serde_json::to_value(&own).unwrap()[0]["relationships"][0],
            "requested"
        );
    }
}
