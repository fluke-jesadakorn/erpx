//! Pure Field contribution to the shared Work projection.
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceWorkItem {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub href: String,
    pub status: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub updated_at: i64,
    pub queue: String,
    pub can_act: bool,
    pub responsible_queue: String,
    pub relationships: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub next_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional, type = "number | null"))]
    pub due_at: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub blocker: Option<Option<String>>,
}
fn text<'a>(r: &'a Value, k: &str) -> Option<&'a str> {
    r.get(k)?.as_str()
}
fn num(r: &Value, k: &str) -> Option<i64> {
    r.get(k)?.as_i64()
}
fn optional_text<'a>(r: &'a Value, k: &str) -> Option<Option<&'a str>> {
    match r.get(k)? {
        Value::Null => Some(None),
        Value::String(s) => Some(Some(s)),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub struct WorkRights {
    pub can_write: bool,
    pub can_review: bool,
    pub can_parts: bool,
    pub settle: bool,
}
pub fn field_service_work_items(
    agreements: &[Value],
    orders: &[Value],
    actor: &str,
    aliases: &[String],
    rights: WorkRights,
) -> Option<Vec<FieldServiceWorkItem>> {
    let WorkRights {
        can_write,
        can_review,
        can_parts,
        settle,
    } = rights;
    let same = |s: &str| s == actor || aliases.iter().any(|a| a == s);
    let mut items = Vec::new();
    for g in agreements {
        if text(g, "status")? != "draft" {
            continue;
        }
        let id = text(g, "id")?;
        let creator = text(g, "created_by")?;
        let asset =
            optional_text(g, "asset_id")?.is_some() || num(g, "requires_asset_capability")? > 0;
        let act = can_write
            && can_review
            && num(g, "can_review")? != 0
            && !same(creator)
            && (!asset || can_parts)
            && settle;
        items.push(FieldServiceWorkItem {
            id: id.into(),
            kind: "field_service".into(),
            title: format!("Agreement review · {}", text(g, "title")?),
            detail: format!(
                "{} · {} to {}",
                text(g, "customer_name")?,
                text(g, "start_date")?,
                text(g, "end_date")?
            ),
            href: "/field-service".into(),
            status: "draft".into(),
            updated_at: num(g, "updated_at")?,
            queue: if act { "todo" } else { "waiting" }.into(),
            can_act: act,
            responsible_queue: "Independent service manager".into(),
            relationships: vec![
                if creator == actor {
                    "requested"
                } else {
                    "approval"
                }
                .into(),
            ],
            next_action: None,
            due_at: None,
            blocker: None,
        });
    }
    for o in orders {
        let id = text(o, "id")?;
        let status = text(o, "status")?;
        let tech = text(o, "technician_id")?;
        let creator = text(o, "created_by")?;
        let completed = optional_text(o, "completed_by")?;
        let eligible = num(o, "technician_eligible")? != 0;
        let manage = can_review && num(o, "can_review")? > 0;
        let own = tech == actor && eligible;
        let terminal = status == "cancelled" || status == "billed";
        let asset =
            optional_text(o, "asset_id")?.is_some() || num(o, "requires_asset_capability")? > 0;
        let act = can_write
            && !terminal
            && (own || manage)
            && (!asset || can_parts)
            && (status != "completed"
                || manage
                    && !same(tech)
                    && !completed.is_some_and(same)
                    && num(o, "independent_billing")? != 0)
            && settle;
        let due = match o.get("scheduled_at")? {
            Value::Null => None,
            Value::Number(n) => Some(n.as_i64()?).filter(|n| *n != 0),
            _ => return None,
        };
        let blocker = if eligible {
            optional_text(o, "exception_reason")?.map(str::to_owned)
        } else {
            Some(
                "Assigned technician is no longer eligible; reviewer reassignment is required"
                    .into(),
            )
        };
        items.push(FieldServiceWorkItem {
            id: id.into(),
            kind: "field_service".into(),
            title: format!("Field visit · {}", text(o, "ticket_title")?),
            detail: format!("{} · {}", text(o, "site")?, text(o, "customer_name")?),
            href: format!("/field-service/{id}"),
            status: status.into(),
            updated_at: num(o, "updated_at")?,
            queue: if terminal {
                "completed"
            } else if act {
                "todo"
            } else {
                "waiting"
            }
            .into(),
            can_act: act,
            responsible_queue: if status == "completed" {
                "Independent billing reviewer"
            } else {
                "Assigned technician"
            }
            .into(),
            next_action: Some(
                if status == "completed" {
                    "Review visit evidence and release billing"
                } else if status == "exception" {
                    "Resolve site exception"
                } else {
                    "Continue scheduled field visit"
                }
                .into(),
            ),
            due_at: Some(due),
            blocker: Some(blocker),
            relationships: vec![
                if tech == actor {
                    "assigned"
                } else if creator == actor {
                    "requested"
                } else {
                    "team"
                }
                .into(),
            ],
        });
    }
    Some(items)
}
