//! Native CRM activity workspace read rules (owner-side port prep).
//!
//! Pure view/work projections of CRM-owned activity rows. No SQL, no purchase
//! check and no cross-owner authority happens here. The rules are a literal
//! port of the retained TypeScript `crmActivityWorkspaceView`,
//! `crmActivityWorkItems` and `crmActivityCanChange` so the native reader can
//! replace that path without changing the wire shape. The wire keys keep the
//! TypeScript casing (snake_case row fields plus camelCase derived flags).
use crate::domain::activities_native::ActivityActor;
use serde::Serialize;

/// Selected CRM lead. `company_id`/`branch_id`/`department_id` are scope
/// witnesses; only `id`/`title`/`status`/`stage`/`version` reach the view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CrmLeadRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub title: String,
    pub status: String,
    pub stage: Option<String>,
    pub version: i64,
}

/// One activity row as produced by the owner read SQL. Field names and types
/// mirror the TypeScript `CRMActivity` type exactly, including the optional
/// cross-owner `owner_name` and numeric `owner_eligible`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CrmActivityRow {
    pub id: String,
    pub company_id: String,
    pub lead_id: String,
    pub activity_type: String,
    pub subject: String,
    pub details: String,
    pub owner_id: String,
    pub due_date: String,
    pub status: String,
    pub outcome: Option<String>,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub lead_title: String,
    pub lead_status: String,
    pub lead_stage: Option<String>,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub owner_name: Option<String>,
    pub owner_eligible: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActivityRecipient {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkspaceLeadView {
    pub id: String,
    pub title: String,
    pub status: String,
    pub stage: Option<String>,
    pub version: i64,
}

impl From<&CrmLeadRow> for WorkspaceLeadView {
    fn from(lead: &CrmLeadRow) -> Self {
        Self {
            id: lead.id.clone(),
            title: lead.title.clone(),
            status: lead.status.clone(),
            stage: lead.stage.clone(),
            version: lead.version,
        }
    }
}

/// Activity row with the TypeScript derived control flags. The flattened row
/// keeps snake_case keys; the flags use the camelCase keys of the old reader.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActivityItemView {
    #[serde(flatten)]
    pub row: CrmActivityRow,
    #[serde(rename = "ownerEligible")]
    pub owner_eligible_flag: bool,
    #[serde(rename = "canComplete")]
    pub can_complete: bool,
    #[serde(rename = "canManage")]
    pub can_manage: bool,
    #[serde(rename = "canChange")]
    pub can_change: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActivityWorkspaceView {
    pub lead: WorkspaceLeadView,
    pub items: Vec<ActivityItemView>,
    #[serde(rename = "canCreate")]
    pub can_create: bool,
    #[serde(rename = "canManage")]
    pub can_manage: bool,
}

/// Full workspace payload. The TypeScript reader spreads the view and appends
/// `history` and `recipients`; `purchase` is a composition concern and is not
/// emitted by this owner port.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActivityWorkspaceData {
    #[serde(flatten)]
    pub view: ActivityWorkspaceView,
    pub history: Vec<serde_json::Value>,
    pub recipients: Vec<ActivityRecipient>,
}

/// Work projection row. `#[serde(rename_all = "camelCase")]` reproduces the
/// TypeScript `WorkItem` keys (`updatedAt`, `responsibleQueue`, ...).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrmWorkItem {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub title: String,
    pub detail: String,
    pub href: String,
    pub status: String,
    pub updated_at: i64,
    pub queue: &'static str,
    pub responsible_queue: String,
    pub next_action: String,
    pub due_at: i64,
    pub blocker: Option<String>,
    pub relationships: Vec<&'static str>,
}

/// Effective manager flag used by the old reader: sales authority is required
/// before the `commerce.approve` manager flag is honoured.
pub fn effective_manager(actor: &ActivityActor) -> bool {
    actor.can_sales && actor.can_manage
}

/// `crmActivityCanChange`: open + sales + (creator, owner or manager), where
/// creator/owner use the same-person directory supplied by composition.
pub fn activity_can_change(row: &CrmActivityRow, actor: &ActivityActor, manager: bool) -> bool {
    row.status == "open"
        && actor.can_sales
        && (actor.is_same_person(&row.created_by)
            || actor.is_same_person(&row.owner_id)
            || manager)
}

fn can_complete(row: &CrmActivityRow, actor: &ActivityActor) -> bool {
    row.status == "open"
        && actor.can_sales
        && row.owner_id == actor.user_id
        && row.owner_eligible > 0
}

/// `crmActivityWorkspaceView`: lead summary, enriched items and the two
/// create/manage controls.
pub fn workspace_view(
    lead: &CrmLeadRow,
    items: &[CrmActivityRow],
    actor: &ActivityActor,
) -> ActivityWorkspaceView {
    let manager = effective_manager(actor);
    let can_create = actor.can_sales
        && lead.status != "disqualified"
        && !matches!(lead.stage.as_deref(), Some("won") | Some("lost"));
    let views = items
        .iter()
        .map(|row| ActivityItemView {
            owner_eligible_flag: row.owner_eligible > 0,
            can_complete: can_complete(row, actor),
            can_manage: row.status == "open" && manager,
            can_change: activity_can_change(row, actor, manager),
            row: row.clone(),
        })
        .collect();
    ActivityWorkspaceView {
        lead: lead.into(),
        items: views,
        can_create,
        can_manage: manager,
    }
}

/// `crmActivityWorkItems`: the Work queue projection of activity rows.
pub fn work_items(all: &[CrmActivityRow], actor: &ActivityActor) -> Vec<CrmWorkItem> {
    all.iter()
        .map(|row| {
            let open = row.status == "open";
            let eligible_owner = row.owner_eligible > 0;
            let can_act = open
                && eligible_owner
                && row.owner_id == actor.user_id
                && actor.can_sales;
            let relationship = if row.owner_id == actor.user_id {
                "assigned"
            } else if row.created_by == actor.user_id {
                "requested"
            } else {
                "team"
            };
            CrmWorkItem {
                id: row.id.clone(),
                kind: "crm_activity",
                title: format!("CRM activity · {}", row.subject),
                detail: row.details.clone(),
                href: format!("/crm/{}#crm-activities", row.lead_id),
                status: row.status.clone(),
                updated_at: row.updated_at,
                queue: if !open {
                    "completed"
                } else if can_act {
                    "todo"
                } else {
                    "waiting"
                },
                responsible_queue: row
                    .owner_name
                    .clone()
                    .unwrap_or_else(|| "Assigned salesperson".to_owned()),
                next_action: if open {
                    row.subject.clone()
                } else {
                    "Activity outcome recorded".to_owned()
                },
                due_at: due_date_millis(&row.due_date),
                blocker: if open && !eligible_owner {
                    Some(
                        "Assigned owner is no longer eligible; a sales manager must reassign with a reason"
                            .to_owned(),
                    )
                } else {
                    None
                },
                relationships: vec![relationship],
            }
        })
        .collect()
}

/// `Date.parse(`${dueDate}T00:00:00Z`)` for the ISO `YYYY-MM-DD` dates stored
/// by the native activity writer. Uses the proleptic Gregorian epoch so year
/// zero and leap boundaries match JavaScript. Malformed dates yield 0; the
/// write path validates ISO dates before persistence.
pub fn due_date_millis(date: &str) -> i64 {
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return 0;
    }
    let number = |from: usize, to: usize| -> Option<i64> {
        date[from..to]
            .bytes()
            .try_fold(0_i64, |accumulator, byte| {
                if byte.is_ascii_digit() {
                    Some(accumulator * 10 + i64::from(byte - b'0'))
                } else {
                    None
                }
            })
    };
    let (Some(year), Some(month), Some(day)) = (number(0, 4), number(5, 7), number(8, 10)) else {
        return 0;
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return 0;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    days * 86_400_000
}
