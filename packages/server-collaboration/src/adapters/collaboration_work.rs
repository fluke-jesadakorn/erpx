//! Collaboration-owned row snapshots; no cross-owner source or IAM queries.
use crate::domain::collaboration_work::CurrentRelation;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::record_source_contract::{SourceKind, SourceReference};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("COLLABORATION_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Acquire before the first selected IAM snapshot. Ledger/other owner locks
/// must be acquired in the common composition ordering before this lock.
pub async fn lock_collaboration_work_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("COLLABORATION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE collaboration_record_relations,collaboration_relation_events,collaboration_action_cards,collaboration_action_events,staff_notifications IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}
#[derive(Clone, Copy)]
enum Family {
    Relation,
    Card,
}
fn missing(family: Family) -> Error {
    let (code, message) = match family {
        Family::Relation => (
            "RECORD_RELATION_NOT_FOUND",
            "The relation is unavailable in this work scope.",
        ),
        Family::Card => ("ACTION_CARD_NOT_FOUND", "Action card not found."),
    };
    BusinessError::new(FailureKind::NotFound, code, message).into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "COLLABORATION_SOURCE_CHANGED",
        "The collaboration record changed. Refresh and retry.",
    )
    .into()
}
async fn image(
    c: &mut PgConnection,
    family: Family,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<Option<String>, Error> {
    let sql = match family {
        Family::Relation => {
            "WITH s AS MATERIALIZED (SELECT * FROM collaboration_record_relations WHERE id=$1 AND company_id=$2), p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p"
        }
        Family::Card => {
            "WITH s AS MATERIALIZED (SELECT * FROM collaboration_action_cards WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4)), p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p"
        }
    };
    let mut q = sqlx::query(sql).bind(id).bind(company.as_str());
    if matches!(family, Family::Card) {
        q = q.bind(branch).bind(department);
    }
    match q.fetch_optional(c).await? {
        None => Ok(None),
        Some(row) => Ok(Some(
            row.try_get::<Option<String>, _>(0)?.ok_or_else(changed)?,
        )),
    }
}
pub struct CurrentCollaborationRow {
    family: Family,
    company: CompanyId,
    id: String,
    branch: Option<String>,
    department: Option<String>,
    unit: Unit,
    image: String,
}
impl CurrentCollaborationRow {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(
                c,
                self.family,
                &self.company,
                &self.id,
                self.branch.as_deref(),
                self.department.as_deref(),
            )
            .await?
            .as_deref()
                != Some(self.image.as_str())
        {
            return Err(changed());
        }
        Ok(())
    }
}
async fn capture(
    c: &mut PgConnection,
    family: Family,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<(Value, CurrentCollaborationRow), Error> {
    let current_unit = unit(c).await?;
    let raw = image(c, family, company, id, branch, department)
        .await?
        .ok_or_else(|| missing(family))?;
    let value = serde_json::from_str(&raw).map_err(|_| changed())?;
    Ok((
        value,
        CurrentCollaborationRow {
            family,
            company: company.clone(),
            id: id.into(),
            branch: branch.map(str::to_owned),
            department: department.map(str::to_owned),
            unit: current_unit,
            image: raw,
        },
    ))
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v.get(key).and_then(Value::as_str).ok_or_else(changed)
}
fn optional_text(v: &Value, key: &str) -> Result<Option<String>, Error> {
    match v.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(changed()),
    }
}
fn version(v: &Value) -> Result<i64, Error> {
    v.get("version")
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .ok_or_else(changed)
}
fn source(v: &Value, kind: &str, id: &str) -> Result<SourceReference, Error> {
    Ok(SourceReference {
        kind: SourceKind::parse(text(v, kind)?)?,
        id: text(v, id)?.into(),
        expected_version: None,
    })
}
/// Relation removals/replays classify both persisted endpoints. Stored versions
/// describe creation; current owner versions are recaptured for this command.
pub async fn read_relation(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<(CurrentRelation, CurrentCollaborationRow), Error> {
    let (v, w) = capture(c, Family::Relation, company, id, None, None).await?;
    Ok((
        CurrentRelation {
            id: id.into(),
            version: version(&v)?,
            active: text(&v, "status")? == "active",
            left: source(&v, "source_type", "source_id")?,
            right: source(&v, "target_type", "target_id")?,
        },
        w,
    ))
}
/// Stored data only: it confers no owner or People eligibility.
pub struct StoredCard {
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub id: String,
    pub version: i64,
    pub open: bool,
    pub owner_id: String,
    pub due_at: i64,
    pub source: SourceReference,
}
/// Owner eligibility comes from the separate current IAM and People ports.
pub async fn read_card(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<(StoredCard, CurrentCollaborationRow), Error> {
    let (v, w) = capture(c, Family::Card, company, id, branch, department).await?;
    Ok((
        StoredCard {
            branch_id: optional_text(&v, "branch_id")?,
            department_id: optional_text(&v, "department_id")?,
            id: id.into(),
            version: version(&v)?,
            open: text(&v, "status")? == "open",
            owner_id: text(&v, "owner_id")?.into(),
            due_at: v
                .get("due_at")
                .and_then(Value::as_i64)
                .ok_or_else(changed)?,
            source: source(&v, "source_type", "source_id")?,
        },
        w,
    ))
}

/// Stage only Collaboration-owned effects in the existing owner transaction.
/// Workflow owns the receipt/audit/outbox; callers must retain these effects,
/// source witnesses, assignee authority and selected IAM after deferred flush.
pub async fn apply_plan(
    c: &mut PgConnection,
    actor: &crate::domain::collaboration_work::AuthorizedActor,
    key: &erpx_server_shared::public::CommandKey,
    plan: &crate::domain::collaboration_work::OwnerPlan,
    now: i64,
) -> Result<crate::domain::collaboration_work::Receipt, Error> {
    use crate::domain::collaboration_work::{
        CardAction, OwnerPlan, Receipt, RelationKind, plan_receipt,
    };
    unit(c).await?;
    let expected = plan_receipt(key, plan)?;
    let command = key.as_str();
    let result: Result<Receipt, Error> = match plan {
        OwnerPlan::CreateRelation {
            left,
            right,
            relation,
            reason,
        } => {
            let id = format!("record-relation-{command}");
            let event = format!("record-relation-event-{command}");
            let kind = match relation {
                RelationKind::Related => "related",
                RelationKind::Duplicates => "duplicates",
                RelationKind::Blocks => "blocks",
                RelationKind::DependsOn => "depends_on",
            };
            sqlx::query("INSERT INTO collaboration_record_relations(id,company_id,branch_id,department_id,source_type,source_id,source_version,target_type,target_id,target_version,relation,reason,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$14)")
                .bind(&id).bind(&actor.company_id).bind(&left.branch_id).bind(&left.department_id).bind(left.kind.as_str()).bind(&left.id).bind(left.version).bind(right.kind.as_str()).bind(&right.id).bind(right.version).bind(kind).bind(reason).bind(&actor.user_id).bind(now).execute(&mut *c).await?;
            sqlx::query("INSERT INTO collaboration_relation_events VALUES($1,$2,1,'create',$3,$4,'active',$5)").bind(&event).bind(&id).bind(&actor.user_id).bind(reason).bind(now).execute(c).await?;
            Ok(Receipt {
                id,
                version: 1,
                status: "active",
            })
        }
        OwnerPlan::RemoveRelation { current, reason } => {
            let next = current.version.checked_add(1).ok_or_else(changed)?;
            let n=sqlx::query("UPDATE collaboration_record_relations SET status='removed',removed_by=$1,removed_at=$2,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND version=$5 AND status='active'").bind(&actor.user_id).bind(now).bind(&current.id).bind(&actor.company_id).bind(current.version).execute(&mut *c).await?.rows_affected();
            if n != 1 {
                return Err(changed());
            }
            sqlx::query("INSERT INTO collaboration_relation_events VALUES($1,$2,$3,'remove',$4,$5,'removed',$6)").bind(format!("record-relation-event-{command}")).bind(&current.id).bind(next).bind(&actor.user_id).bind(reason).bind(now).execute(c).await?;
            Ok(Receipt {
                id: current.id.clone(),
                version: next,
                status: "removed",
            })
        }
        OwnerPlan::CreateCard {
            source,
            title,
            owner_id,
            due_at,
            reason,
        } => {
            let id = format!("action-card-{command}");
            sqlx::query("INSERT INTO collaboration_action_cards(id,company_id,branch_id,department_id,source_type,source_id,source_version,title,owner_id,due_at,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$12)").bind(&id).bind(&actor.company_id).bind(&source.branch_id).bind(&source.department_id).bind(source.kind.as_str()).bind(&source.id).bind(source.version).bind(title).bind(owner_id).bind(due_at).bind(&actor.user_id).bind(now).execute(&mut *c).await?;
            sqlx::query("INSERT INTO collaboration_action_events VALUES($1,$2,1,'create',$3,$4,'open',$5,$6,$7)").bind(format!("action-card-event-{command}")).bind(&id).bind(&actor.user_id).bind(reason).bind(owner_id).bind(due_at).bind(now).execute(&mut *c).await?;
            insert_notice(
                c,
                command,
                &id,
                "New cross-record action card: review the linked source",
            )
            .await?;
            Ok(Receipt {
                id,
                version: 1,
                status: "open",
            })
        }
        OwnerPlan::ChangeCard {
            current,
            action,
            owner_id,
            due_at,
            evidence,
            reason,
        } => {
            let next = current.version.checked_add(1).ok_or_else(changed)?;
            let (action_name, status, owner, due) = match action {
                CardAction::Complete => (
                    "complete",
                    "completed",
                    current.owner_id.as_str(),
                    current.due_at,
                ),
                CardAction::Cancel => (
                    "cancel",
                    "cancelled",
                    current.owner_id.as_str(),
                    current.due_at,
                ),
                CardAction::Reassign => (
                    "reassign",
                    "open",
                    owner_id.as_deref().ok_or_else(changed)?,
                    due_at.ok_or_else(changed)?,
                ),
            };
            let n=sqlx::query("UPDATE collaboration_action_cards SET owner_id=$1,due_at=$2,status=$3,completion_evidence=$4,version=version+1,updated_at=$5 WHERE id=$6 AND company_id=$7 AND version=$8 AND status='open'").bind(owner).bind(due).bind(status).bind(evidence).bind(now).bind(&current.id).bind(&actor.company_id).bind(current.version).execute(&mut *c).await?.rows_affected();
            if n != 1 {
                return Err(changed());
            }
            sqlx::query(
                "INSERT INTO collaboration_action_events VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(format!("action-card-event-{command}"))
            .bind(&current.id)
            .bind(next)
            .bind(action_name)
            .bind(&actor.user_id)
            .bind(reason)
            .bind(status)
            .bind(owner)
            .bind(due)
            .bind(now)
            .execute(&mut *c)
            .await?;
            insert_notice(
                c,
                command,
                &current.id,
                "Cross-record action card updated: review the next action",
            )
            .await?;
            Ok(Receipt {
                id: current.id.clone(),
                version: next,
                status,
            })
        }
    };
    let result = result?;
    if result != expected {
        return Err(changed());
    }
    Ok(result)
}
async fn insert_notice(
    c: &mut PgConnection,
    key: &str,
    id: &str,
    message: &str,
) -> Result<(), Error> {
    let n=sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT $1,company_id,owner_id,'collaboration_action',id,version,$2,updated_at FROM collaboration_action_cards WHERE id=$3").bind(format!("action-card-notice-{key}")).bind(message).bind(id).execute(c).await?.rows_affected();
    if n != 1 {
        return Err(changed());
    }
    Ok(())
}

/// Exact post-effect images are sealed before deferred triggers are forced.
/// Revalidation compares the same owner unit, row, event and optional notice.
pub struct CurrentCollaborationEffects {
    row: CurrentCollaborationRow,
    unit: Unit,
    event_table: &'static str,
    event_id: String,
    event_image: String,
    notice_id: Option<String>,
    notice_image: Option<String>,
}
async fn owned_image(
    c: &mut PgConnection,
    table: &'static str,
    id: &str,
) -> Result<Option<String>, Error> {
    let sql = match table {
        "collaboration_relation_events" => {
            "WITH s AS MATERIALIZED (SELECT * FROM collaboration_relation_events WHERE id=$1),p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p"
        }
        "collaboration_action_events" => {
            "WITH s AS MATERIALIZED (SELECT * FROM collaboration_action_events WHERE id=$1),p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p"
        }
        "staff_notifications" => {
            "WITH s AS MATERIALIZED (SELECT * FROM staff_notifications WHERE id=$1),p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p"
        }
        _ => return Err(PersistenceError::new("COLLABORATION_OWNER_IMAGE_INVALID").into()),
    };
    match sqlx::query(sql).bind(id).fetch_optional(c).await? {
        None => Ok(None),
        Some(row) => Ok(Some(
            row.try_get::<Option<String>, _>(0)?.ok_or_else(changed)?,
        )),
    }
}
impl CurrentCollaborationEffects {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        self.row.require_current(c).await?;
        if owned_image(c, self.event_table, &self.event_id)
            .await?
            .as_deref()
            != Some(self.event_image.as_str())
        {
            return Err(changed());
        }
        if let Some(id) = &self.notice_id {
            if owned_image(c, "staff_notifications", id).await? != self.notice_image {
                return Err(changed());
            }
        }
        Ok(())
    }
}
/// Check the exact intended owner event and notice, then retain their full
/// images. This function does not validate Workflow or source-owner evidence.
pub async fn retain_effects(
    c: &mut PgConnection,
    actor: &crate::domain::collaboration_work::AuthorizedActor,
    key: &erpx_server_shared::public::CommandKey,
    plan: &crate::domain::collaboration_work::OwnerPlan,
    receipt: &crate::domain::collaboration_work::Receipt,
    now: i64,
) -> Result<CurrentCollaborationEffects, Error> {
    use crate::domain::collaboration_work::{CardAction, OwnerPlan};
    let current_unit = unit(c).await?;
    let company = CompanyId::new(actor.company_id.clone())?;
    let (family, event_table, event_id, notice_id, action, reason, owner, due) = match plan {
        OwnerPlan::CreateRelation { reason, .. } => (
            Family::Relation,
            "collaboration_relation_events",
            format!("record-relation-event-{}", key.as_str()),
            None,
            "create",
            reason.as_str(),
            None,
            None,
        ),
        OwnerPlan::RemoveRelation { reason, .. } => (
            Family::Relation,
            "collaboration_relation_events",
            format!("record-relation-event-{}", key.as_str()),
            None,
            "remove",
            reason.as_str(),
            None,
            None,
        ),
        OwnerPlan::CreateCard {
            reason,
            owner_id,
            due_at,
            ..
        } => (
            Family::Card,
            "collaboration_action_events",
            format!("action-card-event-{}", key.as_str()),
            Some(format!("action-card-notice-{}", key.as_str())),
            "create",
            reason.as_str(),
            Some(owner_id.as_str()),
            Some(*due_at),
        ),
        OwnerPlan::ChangeCard {
            current,
            action,
            owner_id,
            due_at,
            reason,
            ..
        } => {
            let (name, owner, due) = match action {
                CardAction::Complete => ("complete", current.owner_id.as_str(), current.due_at),
                CardAction::Cancel => ("cancel", current.owner_id.as_str(), current.due_at),
                CardAction::Reassign => (
                    "reassign",
                    owner_id.as_deref().ok_or_else(changed)?,
                    due_at.ok_or_else(changed)?,
                ),
            };
            (
                Family::Card,
                "collaboration_action_events",
                format!("action-card-event-{}", key.as_str()),
                Some(format!("action-card-notice-{}", key.as_str())),
                name,
                reason.as_str(),
                Some(owner),
                Some(due),
            )
        }
    };
    let (row_value, row) = capture(c, family, &company, &receipt.id, None, None).await?;
    if text(&row_value, "id")? != receipt.id
        || text(&row_value, "company_id")? != actor.company_id
        || version(&row_value)? != receipt.version
        || text(&row_value, "status")? != receipt.status
    {
        return Err(changed());
    }
    match plan {
        OwnerPlan::CreateRelation {
            left,
            right,
            relation,
            reason,
        } => {
            use crate::domain::collaboration_work::RelationKind;
            let relation = match relation {
                RelationKind::Related => "related",
                RelationKind::Duplicates => "duplicates",
                RelationKind::Blocks => "blocks",
                RelationKind::DependsOn => "depends_on",
            };
            for (field, expected) in [
                ("branch_id", serde_json::json!(left.branch_id)),
                ("department_id", serde_json::json!(left.department_id)),
                ("source_type", serde_json::json!(left.kind.as_str())),
                ("source_id", serde_json::json!(left.id)),
                ("source_version", serde_json::json!(left.version)),
                ("target_type", serde_json::json!(right.kind.as_str())),
                ("target_id", serde_json::json!(right.id)),
                ("target_version", serde_json::json!(right.version)),
                ("relation", serde_json::json!(relation)),
                ("reason", serde_json::json!(reason)),
                ("created_by", serde_json::json!(actor.user_id)),
                ("created_at", serde_json::json!(now)),
                ("updated_at", serde_json::json!(now)),
            ] {
                if row_value.get(field) != Some(&expected) {
                    return Err(changed());
                }
            }
        }
        OwnerPlan::RemoveRelation { current, .. } => {
            if text(&row_value, "source_type")? != current.left.kind.as_str()
                || text(&row_value, "source_id")? != current.left.id
                || text(&row_value, "target_type")? != current.right.kind.as_str()
                || text(&row_value, "target_id")? != current.right.id
            {
                return Err(changed());
            }
            if row_value.get("removed_by") != Some(&serde_json::json!(actor.user_id))
                || row_value.get("removed_at") != Some(&serde_json::json!(now))
                || row_value.get("updated_at") != Some(&serde_json::json!(now))
            {
                return Err(changed());
            }
        }
        OwnerPlan::CreateCard {
            source,
            title,
            owner_id,
            due_at,
            ..
        } => {
            for (field, expected) in [
                ("branch_id", serde_json::json!(source.branch_id)),
                ("department_id", serde_json::json!(source.department_id)),
                ("source_type", serde_json::json!(source.kind.as_str())),
                ("source_id", serde_json::json!(source.id)),
                ("source_version", serde_json::json!(source.version)),
                ("title", serde_json::json!(title)),
                ("owner_id", serde_json::json!(owner_id)),
                ("due_at", serde_json::json!(due_at)),
                ("created_by", serde_json::json!(actor.user_id)),
                ("created_at", serde_json::json!(now)),
                ("updated_at", serde_json::json!(now)),
            ] {
                if row_value.get(field) != Some(&expected) {
                    return Err(changed());
                }
            }
        }
        OwnerPlan::ChangeCard {
            current, evidence, ..
        } => {
            if text(&row_value, "source_type")? != current.source.kind.as_str()
                || text(&row_value, "source_id")? != current.source.id
            {
                return Err(changed());
            }
            if row_value.get("completion_evidence") != Some(&serde_json::json!(evidence))
                || row_value.get("updated_at") != Some(&serde_json::json!(now))
            {
                return Err(changed());
            }
        }
    }
    let event_image = owned_image(c, event_table, &event_id)
        .await?
        .ok_or_else(changed)?;
    let event: Value = serde_json::from_str(&event_image).map_err(|_| changed())?;
    let foreign = if matches!(family, Family::Relation) {
        "relation_id"
    } else {
        "card_id"
    };
    if text(&event, "id")? != event_id
        || text(&event, foreign)? != receipt.id
        || version(&event)? != receipt.version
        || text(&event, "action")? != action
        || text(&event, "actor_id")? != actor.user_id
        || text(&event, "reason")? != reason
        || text(&event, "status")? != receipt.status
        || event.get("created_at").and_then(Value::as_i64) != Some(now)
    {
        return Err(changed());
    }
    if let Some(owner) = owner {
        if text(&event, "owner_id")? != owner
            || event.get("due_at").and_then(Value::as_i64) != due
            || text(&row_value, "owner_id")? != owner
            || row_value.get("due_at").and_then(Value::as_i64) != due
        {
            return Err(changed());
        }
    }
    let notice_image = if let Some(id) = &notice_id {
        let raw = owned_image(c, "staff_notifications", id)
            .await?
            .ok_or_else(changed)?;
        let notice: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
        let expected_message = if action == "create" {
            "New cross-record action card: review the linked source"
        } else {
            "Cross-record action card updated: review the next action"
        };
        if text(&notice, "id")? != id
            || text(&notice, "company_id")? != actor.company_id
            || Some(text(&notice, "user_id")?) != owner
            || text(&notice, "record_type")? != "collaboration_action"
            || text(&notice, "record_id")? != receipt.id
            || notice.get("record_version").and_then(Value::as_i64) != Some(receipt.version)
            || text(&notice, "message")? != expected_message
            || notice.get("created_at").and_then(Value::as_i64) != Some(now)
        {
            return Err(changed());
        }
        Some(raw)
    } else {
        None
    };
    Ok(CurrentCollaborationEffects {
        row,
        unit: current_unit,
        event_table,
        event_id,
        event_image,
        notice_id,
        notice_image,
    })
}

/// Native Field partition only. Other source kinds have separate ownership and
/// are never retried through a retained writer when this partition is selected.
pub struct FieldWorkRows {
    pub relations: Vec<Value>,
    pub cards: Vec<Value>,
    witnesses: Vec<CurrentCollaborationRow>,
}
impl FieldWorkRows {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        for witness in &self.witnesses {
            witness.require_current(c).await?;
        }
        Ok(())
    }
}
pub async fn read_field_work_rows(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<FieldWorkRows, Error> {
    unit(c).await?;
    let relations:Vec<String>=sqlx::query_scalar("SELECT id FROM collaboration_record_relations WHERE company_id=$1 AND status='active' AND ((source_type='field_service_order' AND target_type IN ('field_service_order','support_ticket')) OR (source_type='support_ticket' AND target_type='field_service_order')) ORDER BY updated_at DESC,id LIMIT 100").bind(company.as_str()).fetch_all(&mut *c).await?;
    let cards:Vec<String>=sqlx::query_scalar("SELECT id FROM collaboration_action_cards WHERE company_id=$1 AND source_type='field_service_order' AND ($2::text IS NULL OR branch_id=$2) AND ($3::text IS NULL OR department_id=$3) ORDER BY updated_at DESC,id LIMIT 100").bind(company.as_str()).bind(branch).bind(department).fetch_all(&mut *c).await?;
    let mut rows = FieldWorkRows {
        relations: vec![],
        cards: vec![],
        witnesses: vec![],
    };
    for id in relations {
        let (row, witness) = capture(c, Family::Relation, company, &id, None, None).await?;
        rows.relations.push(row);
        rows.witnesses.push(witness);
        if rows.witnesses.iter().map(|w| w.image.len()).sum::<usize>() > 2 * 1024 * 1024 {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "COLLABORATION_WORKSPACE_LIMIT",
                "The Collaboration workspace exceeds this bounded read.",
            )
            .into());
        }
    }
    for id in cards {
        let (row, witness) = capture(c, Family::Card, company, &id, branch, department).await?;
        rows.cards.push(row);
        rows.witnesses.push(witness);
        if rows.witnesses.iter().map(|w| w.image.len()).sum::<usize>() > 2 * 1024 * 1024 {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "COLLABORATION_WORKSPACE_LIMIT",
                "The Collaboration workspace exceeds this bounded read.",
            )
            .into());
        }
    }
    Ok(rows)
}
