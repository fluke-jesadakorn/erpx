//! Native Maintenance projection ports for Work, Collaboration and Executive.
//! These are presentation facts derived from owner rows; they grant no read,
//! write, purchase or command authority. Consumers still resolve the current
//! session, scope and package facts before calling.
use crate::domain::read::{MaintenancePlanRow, MaintenanceWorkRow};
use serde_json::{Value, json};

const DAY_MS: i64 = 86_400_000;

/// Current purchase visibility for the shared work projections. The retained
/// consumers hide history entirely and downgrade action hints when the purchased
/// add-on is draining; composition supplies those package facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaintenanceProjectionMode {
    pub history: bool,
    pub new_work: bool,
    pub settle_existing: bool,
}

impl Default for MaintenanceProjectionMode {
    fn default() -> Self {
        Self {
            history: true,
            new_work: true,
            settle_existing: true,
        }
    }
}

/// Exact `preventivePlanWork` item shape used by the Work projection.
pub fn preventive_plan_work(
    plans: &[MaintenancePlanRow],
    actor_id: &str,
    can_write: bool,
    can_review: bool,
    now: i64,
    mode: MaintenanceProjectionMode,
) -> Vec<Value> {
    if !mode.history {
        return Vec::new();
    }
    plans
        .iter()
        .map(|plan| {
            let terminal = plan.status == "cancelled";
            let own = plan.owner_id == actor_id && plan.owner_eligible != 0;
            let due = plan.next_due_at
                <= now.saturating_add(i64::from(plan.lead_days).saturating_mul(DAY_MS));
            let can_act = can_write
                && mode.new_work
                && !terminal
                && if matches!(plan.status.as_str(), "draft" | "paused") {
                    can_review && plan.edited_by != actor_id
                } else {
                    plan.status == "active" && due && (own || can_review)
                };
            let href = format!("/maintenance/plans/{}", plan.id);
            let next_action = if plan.status == "active" {
                "Generate due work"
            } else if plan.status == "cancelled" {
                "No further action"
            } else {
                "Review and activate preventive plan"
            };
            json!({
                "source": {
                    "kind": "maintenance_plan",
                    "id": plan.id,
                    "version": plan.version,
                    "href": href,
                    "label": format!("Preventive maintenance · {}", plan.title),
                },
                "id": plan.id,
                "type": "maintenance_plan",
                "title": format!("{} · {}", plan.asset_code, plan.title),
                "detail": format!(
                    "Preventive · every {} days · {}",
                    plan.interval_days, plan.time_zone
                ),
                "href": href,
                "status": plan.status,
                "updatedAt": plan.updated_at,
                "dueAt": plan.next_due_at,
                "queue": if terminal {
                    "completed"
                } else if can_act {
                    "todo"
                } else {
                    "waiting"
                },
                "canAct": can_act,
                "responsibleQueue": "Preventive maintenance owner",
                "nextAction": next_action,
                "blocker": if plan.owner_eligible != 0 {
                    Value::Null
                } else {
                    json!("Assigned maintenance owner is no longer eligible; pause and reassign the plan")
                },
                "relationships": [if plan.owner_id == actor_id {
                    "assigned"
                } else if plan.created_by == actor_id {
                    "requested"
                } else {
                    "team"
                }],
            })
        })
        .collect()
}

/// Exact `maintenanceWork` item shape used by the Work projection.
pub fn maintenance_work_items(
    works: &[MaintenanceWorkRow],
    actor_id: &str,
    can_review: bool,
    mode: MaintenanceProjectionMode,
) -> Vec<Value> {
    if !mode.history {
        return Vec::new();
    }
    works
        .iter()
        .map(|work| {
            let terminal = matches!(work.status.as_str(), "completed" | "cancelled");
            let manage = can_review && work.can_review != 0;
            let own = work.owner_id == actor_id && work.owner_eligible != 0;
            let can_act = !terminal && (own || manage) && mode.settle_existing;
            let next_action = if work.status == "blocked" {
                "Resolve blocker and resume"
            } else if work.status == "in_progress" {
                "Record completion evidence"
            } else {
                "Schedule or start work"
            };
            json!({
                "id": work.id,
                "type": "maintenance_work",
                "title": format!("{} · {}", work.asset_code, work.title),
                "detail": format!("{} · {} priority", work.asset_name, work.priority),
                "href": format!("/maintenance/{}", work.id),
                "status": work.status,
                "updatedAt": work.updated_at,
                "dueAt": work.scheduled_at,
                "queue": if terminal {
                    "completed"
                } else if can_act {
                    "todo"
                } else {
                    "waiting"
                },
                "canAct": can_act,
                "responsibleQueue": "Assigned maintenance owner",
                "nextAction": next_action,
                "blocker": if work.owner_eligible != 0 {
                    work.blocked_reason.clone().map_or(Value::Null, Value::String)
                } else {
                    json!("Assigned maintenance owner is no longer eligible; reviewer reassignment is required")
                },
                "relationships": [if work.owner_id == actor_id {
                    "assigned"
                } else if work.created_by == actor_id {
                    "requested"
                } else {
                    "team"
                }],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::read::{MaintenancePlanRow, MaintenanceWorkRow};

    fn plan() -> MaintenancePlanRow {
        MaintenancePlanRow {
            id: "plan".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            asset_id: "asset".into(),
            title: "Daily safety inspection".into(),
            priority: "normal".into(),
            owner_id: "owner".into(),
            interval_days: 1,
            time_zone: "Asia/Bangkok".into(),
            first_due_local: "2026-03-07T02:30".into(),
            next_due_local: "2026-03-07T02:30".into(),
            next_due_at: 1_772_868_600_000,
            lead_days: 0,
            status: "active".into(),
            version: 2,
            created_by: "owner".into(),
            edited_by: "reviewer".into(),
            approved_by: Some("reviewer".into()),
            approved_at: Some(1),
            created_at: 0,
            updated_at: 9,
            asset_code: "AST".into(),
            asset_name: "Asset".into(),
            owner_name: None,
            owner_eligible: 1,
        }
    }

    fn work() -> MaintenanceWorkRow {
        MaintenanceWorkRow {
            id: "work".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            asset_id: "asset".into(),
            title: "Inspection".into(),
            priority: "normal".into(),
            status: "in_progress".into(),
            owner_id: "owner".into(),
            scheduled_at: Some(1),
            downtime_started_at: None,
            downtime_ended_at: None,
            blocked_reason: None,
            completion_evidence: None,
            resulting_condition: None,
            version: 1,
            created_by: "owner".into(),
            created_at: 0,
            updated_at: 4,
            asset_code: "AST".into(),
            asset_name: "Asset".into(),
            owner_name: None,
            owner_eligible: 1,
            can_review: 0,
        }
    }

    #[test]
    fn plan_projection_preserves_queue_action_and_blocker_semantics() {
        let items = preventive_plan_work(
            &[plan()],
            "owner",
            true,
            false,
            1_772_868_600_000,
            MaintenanceProjectionMode::default(),
        );
        let item = &items[0];
        assert_eq!(item["source"]["kind"], "maintenance_plan");
        assert_eq!(item["title"], "AST · Daily safety inspection");
        assert_eq!(item["detail"], "Preventive · every 1 days · Asia/Bangkok");
        assert_eq!(item["queue"], "todo");
        assert_eq!(item["canAct"], true);
        assert_eq!(item["relationships"], json!(["assigned"]));
        let hidden = preventive_plan_work(
            &[plan()],
            "owner",
            true,
            false,
            0,
            MaintenanceProjectionMode {
                history: false,
                ..Default::default()
            },
        );
        assert!(hidden.is_empty());
        let draining = preventive_plan_work(
            &[plan()],
            "owner",
            true,
            false,
            1_772_868_600_000,
            MaintenanceProjectionMode {
                new_work: false,
                ..Default::default()
            },
        );
        assert_eq!(draining[0]["canAct"], false);
        assert_eq!(draining[0]["queue"], "waiting");
        let mut ineligible = plan();
        ineligible.owner_eligible = 0;
        let blocked = preventive_plan_work(
            &[ineligible],
            "owner",
            true,
            false,
            1_772_868_600_000,
            MaintenanceProjectionMode::default(),
        );
        assert!(blocked[0]["blocker"].as_str().unwrap().contains("no longer eligible"));
    }

    #[test]
    fn work_projection_keeps_owner_reviewer_and_blocker_fields() {
        let items = maintenance_work_items(
            &[work()],
            "owner",
            false,
            MaintenanceProjectionMode::default(),
        );
        assert_eq!(items[0]["type"], "maintenance_work");
        assert_eq!(items[0]["queue"], "todo");
        assert_eq!(items[0]["nextAction"], "Record completion evidence");
        let mut blocked = work();
        blocked.status = "blocked".into();
        blocked.blocked_reason = Some("Waiting for part".into());
        let items = maintenance_work_items(
            &[blocked],
            "owner",
            false,
            MaintenanceProjectionMode::default(),
        );
        assert_eq!(items[0]["blocker"], "Waiting for part");
        assert_eq!(items[0]["nextAction"], "Resolve blocker and resume");
        let items = maintenance_work_items(
            &[work()],
            "other",
            false,
            MaintenanceProjectionMode::default(),
        );
        assert_eq!(items[0]["canAct"], false);
        assert_eq!(items[0]["queue"], "waiting");
        assert_eq!(items[0]["relationships"], json!(["team"]));
    }
}
