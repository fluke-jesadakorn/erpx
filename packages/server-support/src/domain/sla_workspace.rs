//! Portable Support SLA workspace read model. No database, HTTP, clock or
//! notification effect is reachable here. The projection mirrors the retained
//! TypeScript `supportSlaWorkspace` payload field for field.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Already-authorized reader facts. Purchase gates are supplied by the caller;
/// row visibility is still enforced by the SQL predicates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlaWorkspaceScope {
    pub company_id: String,
    pub user_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub support_manage: bool,
    pub support_read: bool,
    /// `support.sla` purchase `newWork`.
    pub new_work: bool,
    /// `support.sla` purchase `settleExisting`.
    pub settle_existing: bool,
}

impl SlaWorkspaceScope {
    /// Base `support.manage` Company-level authority, before purchase gates.
    pub fn can_manage(&self) -> bool {
        self.support_manage && self.branch_id.is_none() && self.department_id.is_none()
    }

    pub fn same_person(&self, user_id: &str) -> bool {
        self.user_id == user_id
    }
}

/// Owner rows handed from the SQL adapter to the application layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlaPolicyRow {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub status: String,
    pub created_by: String,
    pub policy_json: String,
    pub actor_eligible: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlaTicketRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub owner_id: Option<String>,
    pub policy_id: Option<String>,
    pub policy_version: Option<i64>,
    pub clock_json: Option<String>,
}

/// `canApprove` is the independent Company-manager review fact before the
/// `newWork` purchase gate is applied.
pub fn can_approve(scope: &SlaWorkspaceScope, policy: &SlaPolicyRow) -> bool {
    scope.can_manage()
        && !scope.same_person(&policy.created_by)
        && policy.status == "draft"
        && policy.actor_eligible > 0
}

/// TypeScript `String(value)` coerces a database NULL to the literal "null".
fn text(value: Option<&str>) -> String {
    value.unwrap_or("null").to_owned()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlaPolicyView {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub status: String,
    pub created_by: String,
    pub policy: Value,
    pub can_approve: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlaTicketView {
    pub id: String,
    pub title: String,
    pub status: String,
    pub owner_id: String,
    pub policy_id: String,
    pub policy_version: i64,
    pub clock: Option<Value>,
}

/// Complete native replacement for the TypeScript SLA workspace read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlaWorkspaceView {
    pub alerts: Vec<Value>,
    pub policies: Vec<SlaPolicyView>,
    pub items: Vec<SlaTicketView>,
    pub can_manage: bool,
    pub now: i64,
    pub can_create: bool,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Pure mapping from owner rows to the wire payload. JSON columns are parsed
/// verbatim so retained field additions survive without a second projection.
pub fn project(
    scope: &SlaWorkspaceScope,
    policies: Vec<SlaPolicyRow>,
    tickets: Vec<SlaTicketRow>,
    alerts: Vec<Value>,
    now: i64,
) -> Result<SlaWorkspaceView, BusinessError> {
    let base_manage = scope.can_manage();
    let mut views = Vec::with_capacity(policies.len());
    for row in policies {
        let eligible = can_approve(scope, &row);
        let policy = serde_json::from_str::<Value>(&row.policy_json)
            .map_err(|_| invalid("SUPPORT_SLA_RESPONSE_INVALID", "Invalid Support policy snapshot."))?;
        views.push(SlaPolicyView {
            id: row.id,
            name: row.name,
            version: row.version,
            status: row.status,
            created_by: row.created_by,
            policy,
            can_approve: eligible && scope.new_work,
        });
    }
    let mut items = Vec::with_capacity(tickets.len());
    for row in tickets {
        let clock = match row.clock_json {
            Some(raw) => Some(serde_json::from_str::<Value>(&raw).map_err(|_| {
                invalid("SUPPORT_SLA_RESPONSE_INVALID", "Invalid Support clock snapshot.")
            })?),
            None => None,
        };
        items.push(SlaTicketView {
            id: row.id,
            title: row.title,
            status: row.status,
            owner_id: text(row.owner_id.as_deref()),
            policy_id: text(row.policy_id.as_deref()),
            policy_version: row.policy_version.unwrap_or(0),
            clock,
        });
    }
    Ok(SlaWorkspaceView {
        alerts,
        policies: views,
        items,
        can_manage: base_manage && scope.settle_existing,
        now,
        can_create: base_manage && scope.new_work,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scope() -> SlaWorkspaceScope {
        SlaWorkspaceScope {
            company_id: "company".into(),
            user_id: "reviewer".into(),
            branch_id: None,
            department_id: None,
            support_manage: true,
            support_read: true,
            new_work: true,
            settle_existing: true,
        }
    }

    #[test]
    fn project_matches_the_retained_payload_shape() {
        let policy = SlaPolicyRow {
            id: "support-policy-1".into(),
            name: "Standard support".into(),
            version: 2,
            status: "draft".into(),
            created_by: "creator".into(),
            policy_json: r#"{"calendar":{"timezone":"Asia/Bangkok"},"targetMinutes":60,"pauseWhileWaiting":true}"#.into(),
            actor_eligible: 1,
        };
        let ticket = SlaTicketRow {
            id: "ticket-1".into(),
            title: "Printer offline".into(),
            status: "open".into(),
            owner_id: Some("manager".into()),
            policy_id: Some("support-policy-1".into()),
            policy_version: Some(2),
            clock_json: Some(r#"{"state":"running","remainingMs":3600000,"dueAt":1793757600000}"#.into()),
        };
        let view = project(
            &scope(),
            vec![policy],
            vec![ticket],
            vec![json!({"id":"alert-1","can_ack":1})],
            1_793_757_600_000,
        )
        .unwrap();
        let value = serde_json::to_value(&view).unwrap();
        assert_eq!(value["alerts"][0]["id"], "alert-1");
        assert_eq!(value["policies"][0]["createdBy"], "creator");
        assert_eq!(value["policies"][0]["policy"]["targetMinutes"], 60);
        assert_eq!(value["policies"][0]["canApprove"], true);
        assert_eq!(value["items"][0]["ownerId"], "manager");
        assert_eq!(value["items"][0]["policyVersion"], 2);
        assert_eq!(value["items"][0]["clock"]["state"], "running");
        assert_eq!(value["canManage"], true);
        assert_eq!(value["canCreate"], true);
        assert_eq!(value["now"], 1_793_757_600_000i64);
    }

    #[test]
    fn purchase_gates_and_scope_narrow_the_capabilities() {
        let policy = SlaPolicyRow {
            id: "p".into(),
            name: "n".into(),
            version: 1,
            status: "draft".into(),
            created_by: "reviewer".into(),
            policy_json: "{}".into(),
            actor_eligible: 1,
        };
        let mut scoped = scope();
        scoped.new_work = false;
        scoped.settle_existing = false;
        let view = project(&scoped, vec![policy], vec![], vec![], 0).unwrap();
        assert_eq!(view.can_create, false);
        assert_eq!(view.can_manage, false);
        assert_eq!(view.policies[0].can_approve, false);
        let mut branch = scope();
        branch.branch_id = Some("branch".into());
        let view = project(&branch, vec![], vec![], vec![], 0).unwrap();
        assert_eq!(view.can_create, false);
        assert_eq!(view.can_manage, false);
    }
}
