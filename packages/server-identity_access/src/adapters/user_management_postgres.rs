//! PostgreSQL adapter for the native user-management command family.
//!
//! SQL is Identity-owned (`um_attributes`, `um_groups`, `um_memberships`,
//! `um_values`, `um_policies`, `um_attachments`, `um_candidates`, `um_history`,
//! `iam_audit`, `iam_receipts`) plus the selected role/assignment/review facts.
//! `hr_people`, `hr_engagements`, `organization_nodes` and the people lifecycle
//! package tables are read-only source facts. The caller supplies the open
//! owner transaction and actor; no commit is performed here.
use crate::domain::user_management::{
    ATTRIBUTE_MANAGE_PERMISSION, AttachmentInput, AttributeInput, GroupInput, MANAGE_PERMISSION,
    ManagementChange, MembershipInput, PolicyInput, REVIEW_PERMISSION, UserManagementActor,
    ValuesInput, active_group_required, active_unit_required, attachment_out_of_scope,
    attribute_access, attribute_acl_invalid, attribute_in_use, attribute_options_retained,
    attribute_permanent, attribute_role_invalid, attribute_write_forbidden, candidate_missing,
    changed, command_permission, company_scope_required, content_hash, delegation_exceeded,
    exclusion_out_of_scope, grant_expiry_invalid, grant_role_changed, grant_scope_mismatch,
    group_children_present, group_children_scope, group_cycle, group_manual_required,
    group_parent_invalid, group_parent_scope, group_rule_contains, group_rule_field,
    group_rule_order, group_rule_text, group_rule_value, group_unavailable, idempotency_conflict,
    inherit_invalid, management_impact, matching_people, membership_out_of_scope,
    permission_required, person_rule_field_type, person_unavailable, policy_out_of_scope,
    policy_required, preview_changed, protected_grant, request_invalid, review_changed,
    scope_forbidden, transaction_required, unit_required, validate_attribute_value, values_changed,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Map, Value, json};
use sqlx::{Connection, PgConnection, Row};
use std::collections::{BTreeMap, BTreeSet};

type Error = PgTransactionError<BusinessError>;

const MANAGEMENT_TABLES: [(&str, &str); 4] = [
    ("attribute", "um_attributes"),
    ("group", "um_groups"),
    ("policy", "um_policies"),
    ("attachment", "um_attachments"),
];

fn table(kind: &str) -> &'static str {
    MANAGEMENT_TABLES
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, table)| *table)
        .unwrap_or("um_attributes")
}

async fn ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("USER_MANAGEMENT_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}

async fn scoped(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    now: i64,
) -> Result<(), Error> {
    if actor.platform {
        return Err(company_scope_required().into());
    }
    if now >= actor.expires_at {
        return Err(changed().into());
    }
    let active: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM assignments a WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 \
         AND iam_assignment_active(a.id,$4)",
    )
    .bind(&actor.assignment_id)
    .bind(&actor.user_id)
    .bind(&actor.company_id)
    .bind(now)
    .fetch_optional(&mut *connection)
    .await?;
    if active.is_none() {
        return Err(changed().into());
    }
    Ok(())
}

async fn require(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    permission: &str,
    now: i64,
) -> Result<(), Error> {
    scoped(connection, actor, now).await?;
    if !actor.has(permission) {
        return Err(permission_required().into());
    }
    Ok(())
}

async fn allowed_scope(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    node: Option<&str>,
) -> Result<(), Error> {
    if actor.platform {
        return Err(company_scope_required().into());
    }
    if let Some(node) = node {
        let active: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM organization_nodes WHERE company_id=$1 AND id=$2 AND status='active'",
        )
        .bind(&actor.company_id)
        .bind(node)
        .fetch_optional(&mut *connection)
        .await?;
        if active.is_none() {
            return Err(active_unit_required().into());
        }
    }
    if actor.scope_kind != "holding" {
        let node = node.ok_or_else(scope_forbidden)?;
        let ok: Option<bool> = sqlx::query_scalar("SELECT um_descendant($1,$2,$3) ok")
            .bind(&actor.company_id)
            .bind(node)
            .bind(actor.scope_node_id.as_deref())
            .fetch_optional(&mut *connection)
            .await?;
        if ok != Some(true) {
            return Err(scope_forbidden().into());
        }
    }
    Ok(())
}

async fn visible_person(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    id: &str,
) -> Result<(), Error> {
    let exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM hr_people WHERE id=$1 AND company_id=$2 AND merged_into_id IS NULL",
    )
    .bind(id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *connection)
    .await?;
    if exists.is_none() {
        return Err(person_unavailable().into());
    }
    if actor.scope_kind != "holding" {
        let ok: Option<bool> = sqlx::query_scalar("SELECT um_person_in_unit($1,$2,$3) ok")
            .bind(&actor.company_id)
            .bind(id)
            .bind(actor.scope_node_id.as_deref())
            .fetch_optional(&mut *connection)
            .await?;
        if ok != Some(true) {
            return Err(person_unavailable().into());
        }
    }
    Ok(())
}

async fn group_row(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    id: &str,
) -> Result<(i64, Value), Error> {
    let row = sqlx::query("SELECT version,definition FROM um_groups WHERE company_id=$1 AND id=$2")
        .bind(&actor.company_id)
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?;
    let Some(row) = row else {
        return Err(group_unavailable().into());
    };
    let version: i32 = row.get("version");
    let definition: Value = row.get("definition");
    let node = definition
        .get("scopeNodeId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    allowed_scope(connection, actor, node.as_deref()).await?;
    Ok((version as i64, definition))
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn id_of(value: &Value) -> String {
    value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

async fn snapshot(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    now: i64,
) -> Result<Value, Error> {
    let company = actor.company_id.as_str();
    let scope = (actor.scope_kind != "holding")
        .then(|| actor.scope_node_id.clone())
        .flatten();

    let mut groups: Vec<Value> = sqlx::query(
        "SELECT g.id,g.version,g.definition FROM um_groups g WHERE company_id=$1 \
         AND ($2::text IS NULL OR g.definition->>'scopeNodeId' IS NOT NULL \
         AND um_descendant($1,g.definition->>'scopeNodeId',$2)) \
         ORDER BY g.definition->>'name',g.id",
    )
    .bind(company)
    .bind(scope.as_deref())
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        let version: i32 = row.get("version");
        let definition: Value = row.get("definition");
        let mut value = definition;
        if let Some(object) = value.as_object_mut() {
            object.insert("id".into(), json!(row.get::<String, _>("id")));
            object.insert("version".into(), json!(version));
            object.insert("members".into(), json!([]));
        }
        value
    })
    .collect();

    let people_rows = sqlx::query(
        "SELECT p.id,p.legal_name name,p.user_id,um_person_facts($1,p.id) definition,\
         COALESCE((SELECT jsonb_agg(DISTINCT COALESCE(e.department_id,e.branch_id)) \
         FROM hr_engagements e WHERE e.company_id=$1 AND e.person_id=p.id AND e.status='active'),'[]'::jsonb) groups_json \
         FROM hr_people p WHERE p.company_id=$1 AND p.merged_into_id IS NULL \
         AND ($2::text IS NULL OR um_person_in_unit($1,p.id,$2)) ORDER BY p.legal_name,p.id",
    )
    .bind(company)
    .bind(scope.as_deref())
    .fetch_all(&mut *connection)
    .await?;
    let mut people: Vec<Value> = people_rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<String, _>("id"),
                "name": row.get::<String, _>("name"),
                "userId": row.get::<Option<String>, _>("user_id"),
                "facts": row.get::<Value, _>("definition"),
                "nodes": row.get::<Value, _>("groups_json").as_array().cloned().unwrap_or_default()
                    .into_iter().filter_map(|value| value.as_str().map(str::to_owned)).collect::<Vec<_>>(),
            })
        })
        .collect();

    let memberships = sqlx::query(
        "SELECT g.id,p.id person_id,\
         EXISTS(SELECT 1 FROM um_memberships m WHERE m.company_id=$1 AND m.group_id=g.id AND m.person_id=p.id) direct,\
         g.definition->>'kind'='automatic' AND um_rule_matches(g.definition->'rule',um_person_facts($1,p.id)) rule,\
         EXISTS(SELECT 1 FROM um_groups child WHERE child.company_id=$1 AND child.definition->>'parentId'=g.id AND um_group_member($1,p.id,child.id)) nested,\
         (EXISTS(SELECT 1 FROM people_lifecycle_cases lc \
           JOIN people_package_revisions pr ON pr.company_id=lc.company_id AND pr.package_id=lc.package_id AND pr.revision=lc.package_revision \
           WHERE lc.company_id=$1 AND lc.person_id=p.id \
           AND people_case_active(lc.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND pr.definition->'groupIds' @> jsonb_build_array(g.id)) \
          OR EXISTS(SELECT 1 FROM people_applied_packages ap WHERE ap.company_id=$1 AND ap.person_id=p.id \
           AND people_application_active(ap.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND ap.definition->'groupIds' @> jsonb_build_array(g.id))) position \
         FROM um_groups g JOIN hr_people p ON p.company_id=$1 \
         WHERE g.company_id=$1 AND um_group_member($1,p.id,g.id)",
    )
    .bind(company)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "id": row.get::<String, _>("id"),
            "personId": row.get::<String, _>("person_id"),
            "direct": row.get::<bool, _>("direct"),
            "rule": row.get::<bool, _>("rule"),
            "nested": row.get::<bool, _>("nested"),
            "position": row.get::<bool, _>("position"),
        })
    })
    .collect::<Vec<Value>>();

    let people_ids: BTreeSet<String> = people
        .iter()
        .filter_map(|person| person.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    for group in &mut groups {
        let id = id_of(group);
        let members: Vec<String> = memberships
            .iter()
            .filter(|row| {
                row.get("id").and_then(Value::as_str) == Some(id.as_str())
                    && row
                        .get("personId")
                        .and_then(Value::as_str)
                        .is_some_and(|person| people_ids.contains(person))
            })
            .filter_map(|row| {
                row.get("personId")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect();
        let sources: Map<String, Value> = memberships
            .iter()
            .filter(|row| {
                row.get("id").and_then(Value::as_str) == Some(id.as_str())
                    && row
                        .get("personId")
                        .and_then(Value::as_str)
                        .is_some_and(|person| members.contains(&person.to_owned()))
            })
            .map(|row| {
                let mut labels = Vec::new();
                if row.get("direct").and_then(Value::as_bool) == Some(true) {
                    labels.push("Added directly");
                }
                if row.get("rule").and_then(Value::as_bool) == Some(true) {
                    labels.push("Rule");
                }
                if row.get("nested").and_then(Value::as_bool) == Some(true) {
                    labels.push("Nested group");
                }
                if row.get("position").and_then(Value::as_bool) == Some(true) {
                    labels.push("Position access package");
                }
                (
                    row.get("personId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    json!(labels),
                )
            })
            .collect();
        if let Some(object) = group.as_object_mut() {
            object.insert("members".into(), json!(members));
            object.insert("memberSources".into(), Value::Object(sources));
        }
    }

    let nodes: Vec<Value> = sqlx::query(
        "SELECT id,parent_id,display_name,kind FROM organization_nodes WHERE company_id=$1 \
         AND status='active' AND ($2::text IS NULL OR um_descendant($1,id,$2)) \
         ORDER BY display_name,id",
    )
    .bind(company)
    .bind(scope.as_deref())
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "id": row.get::<String, _>("id"),
            "parentId": row.get::<Option<String>, _>("parent_id"),
            "label": row.get::<String, _>("display_name"),
            "kind": row.get::<String, _>("kind"),
        })
    })
    .collect();
    let node_ids: BTreeSet<String> = nodes
        .iter()
        .filter_map(|node| node.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    let group_ids: BTreeSet<String> = groups.iter().map(id_of).collect();

    let mut attachments: Vec<Value> = sqlx::query(
        "SELECT id,version,definition FROM um_attachments WHERE company_id=$1 ORDER BY id",
    )
    .bind(company)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        let version: i32 = row.get("version");
        let mut definition: Value = row.get("definition");
        if let Some(object) = definition.as_object_mut() {
            object.insert("id".into(), json!(row.get::<String, _>("id")));
            object.insert("version".into(), json!(version));
        }
        definition
    })
    .filter(|attachment| {
        let target = attachment.get("target").and_then(Value::as_str);
        let target_id = attachment
            .get("targetId")
            .and_then(Value::as_str)
            .unwrap_or("");
        if target == Some("group") {
            group_ids.contains(target_id)
        } else {
            node_ids.contains(target_id)
        }
    })
    .collect();
    let attachment_ids: BTreeSet<String> = attachments.iter().map(id_of).collect();

    let policies: Vec<Value> = sqlx::query(
        "SELECT id,version,definition FROM um_policies WHERE company_id=$1 ORDER BY definition->>'name',id",
    )
    .bind(company)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        let version: i32 = row.get("version");
        let mut definition: Value = row.get("definition");
        if let Some(object) = definition.as_object_mut() {
            object.insert("id".into(), json!(row.get::<String, _>("id")));
            object.insert("version".into(), json!(version));
        }
        definition
    })
    .filter(|policy| {
        actor.scope_kind == "holding"
            || policy
                .get("grants")
                .and_then(Value::as_array)
                .is_none_or(|grants| {
                    grants.iter().all(|grant| {
                        grant
                            .get("scopeNodeId")
                            .and_then(Value::as_str)
                            .is_none_or(|node| node_ids.contains(node))
                    })
                })
    })
    .collect();
    let policy_ids: BTreeSet<String> = policies.iter().map(id_of).collect();

    let all_attributes: Vec<Value> = sqlx::query(
        "SELECT id,version,definition FROM um_attributes WHERE company_id=$1 ORDER BY definition->>'label',id",
    )
    .bind(company)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        let version: i32 = row.get("version");
        let mut definition: Value = row.get("definition");
        if let Some(object) = definition.as_object_mut() {
            object.insert("id".into(), json!(row.get::<String, _>("id")));
            object.insert("version".into(), json!(version));
        }
        definition
    })
    .collect();

    let can_attribute_manage = actor.has(ATTRIBUTE_MANAGE_PERMISSION);
    let attributes: Vec<Value> = all_attributes
        .into_iter()
        .filter_map(|mut attribute| {
            let (can_view, can_edit) = attribute_access(&attribute, actor);
            let company_it = attribute.get("target").and_then(Value::as_str) == Some("person")
                && attribute.get("ownerRole").and_then(Value::as_str).is_none()
                && actor.role == "it_admin"
                && actor.scope_kind == "holding"
                && actor.has("people.custom_attributes.write");
            let can_view = can_view || company_it;
            let can_edit = company_it
                || (can_edit
                    && (attribute.get("target").and_then(Value::as_str) != Some("person")
                        || actor.has("people.custom_attributes.write")));
            if let Some(object) = attribute.as_object_mut() {
                object.insert("canView".into(), json!(can_view));
                object.insert("canEdit".into(), json!(can_edit));
            }
            (can_view || can_attribute_manage).then_some(attribute)
        })
        .collect();
    let visible: BTreeSet<String> = attributes
        .iter()
        .filter(|attribute| attribute.get("canView").and_then(Value::as_bool) == Some(true))
        .map(id_of)
        .collect();

    for person in &mut people {
        if let Some(facts) = person.get_mut("facts").and_then(Value::as_object_mut) {
            facts.retain(|key, _| {
                !key.starts_with("custom:") || visible.contains(key.trim_start_matches("custom:"))
            });
        }
    }
    for group in &mut groups {
        let hidden = group
            .get("rule")
            .and_then(|rule| rule.get("conditions"))
            .and_then(Value::as_array)
            .is_some_and(|conditions| {
                conditions.iter().any(|condition| {
                    condition
                        .get("field")
                        .and_then(Value::as_str)
                        .is_some_and(|field| {
                            field
                                .strip_prefix("custom:")
                                .is_some_and(|id| !visible.contains(id))
                        })
                })
            });
        if hidden {
            if let Some(object) = group.as_object_mut() {
                object.insert("rule".into(), Value::Null);
            }
        }
    }

    let all_values: Vec<Value> = sqlx::query(
        "SELECT target,object_id,version,definition FROM um_values WHERE company_id=$1",
    )
    .bind(company)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "target": row.get::<String, _>("target"),
            "objectId": row.get::<String, _>("object_id"),
            "version": row.get::<i32, _>("version"),
            "definition": row.get::<Value, _>("definition"),
        })
    })
    .collect();
    let object_ids = |target: &str| -> BTreeSet<String> {
        match target {
            "person" => people_ids.clone(),
            "group" => group_ids.clone(),
            _ => node_ids.clone(),
        }
    };
    let mut values: Vec<Value> = all_values
        .iter()
        .filter(|row| {
            let target = row.get("target").and_then(Value::as_str).unwrap_or("");
            let object = row.get("objectId").and_then(Value::as_str).unwrap_or("");
            object_ids(target).contains(object)
        })
        .map(|row| {
            let filtered: Map<String, Value> = row
                .get("definition")
                .and_then(Value::as_object)
                .map(|values| {
                    values
                        .iter()
                        .filter(|(key, _)| visible.contains(key.as_str()))
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect()
                })
                .unwrap_or_default();
            json!({
                "target": row.get("target"),
                "objectId": row.get("objectId"),
                "values": filtered,
                "version": row.get("version"),
                "expectedVersion": row.get("version"),
                "reason": "",
            })
        })
        .collect();

    let ancestry: Vec<(String, Option<String>)> =
        sqlx::query("SELECT id,parent_id FROM organization_nodes WHERE company_id=$1")
            .bind(company)
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("id"),
                    row.get::<Option<String>, _>("parent_id"),
                )
            })
            .collect();
    let ancestry: BTreeMap<String, Option<String>> = ancestry.into_iter().collect();
    for node in &nodes {
        let id = id_of(node);
        if !values.iter().any(|value| {
            value.get("target").and_then(Value::as_str) == Some("unit")
                && value.get("objectId").and_then(Value::as_str) == Some(id.as_str())
        }) {
            values.push(json!({
                "target": "unit", "objectId": id, "expectedVersion": 0, "version": 0,
                "values": {}, "reason": "",
            }));
        }
        let record = values
            .iter_mut()
            .find(|value| {
                value.get("target").and_then(Value::as_str) == Some("unit")
                    && value.get("objectId").and_then(Value::as_str) == Some(id.as_str())
            })
            .expect("inserted above");
        let mut inherited_from = Map::new();
        let mut parent = ancestry.get(&id).cloned().flatten();
        let mut seen = BTreeSet::from([id.clone()]);
        while let Some(current) = parent {
            if !seen.insert(current.clone()) {
                break;
            }
            if let Some(row) = all_values.iter().find(|row| {
                row.get("target").and_then(Value::as_str) == Some("unit")
                    && row.get("objectId").and_then(Value::as_str) == Some(current.as_str())
            }) {
                if let Some(inherited) = row.get("definition").and_then(Value::as_object) {
                    let values_object = record
                        .get_mut("values")
                        .and_then(Value::as_object_mut)
                        .expect("values object");
                    for (field, value) in inherited {
                        if visible.contains(field) && !values_object.contains_key(field) {
                            values_object.insert(field.clone(), value.clone());
                            inherited_from.insert(field.clone(), json!(current));
                        }
                    }
                }
            }
            parent = ancestry.get(&current).cloned().flatten();
        }
        if let Some(object) = record.as_object_mut() {
            object.insert("inheritedFrom".into(), Value::Object(inherited_from));
        }
    }

    let candidate_rows = sqlx::query(
        "SELECT c.*,r.status review_status,r.version review_version,\
         CASE WHEN c.status='active' THEN iam_assignment_active(c.assignment_id,$2) ELSE true END enabled \
         FROM um_candidates c LEFT JOIN iam_reviews r ON r.id=c.review_id \
         WHERE c.company_id=$1 ORDER BY c.created_at DESC,c.id",
    )
    .bind(company)
    .bind(now)
    .fetch_all(&mut *connection)
    .await?;
    let mut candidates = Vec::new();
    for row in candidate_rows {
        let person_id: String = row.get("person_id");
        let attachment_id: String = row.get("attachment_id");
        if !people_ids.contains(&person_id) || !attachment_ids.contains(&attachment_id) {
            continue;
        }
        let definition: Value = row.get("definition");
        let status: String = row.get("status");
        let review_status: Option<String> = row.get("review_status");
        let enabled: bool = row.get("enabled");
        let valid_until = definition
            .get("validUntil")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let status = if review_status.as_deref() == Some("rejected") {
            "rejected".to_owned()
        } else if status == "active" && valid_until <= now {
            "expired".to_owned()
        } else if status == "active" && !enabled {
            "revoked".to_owned()
        } else {
            status.clone()
        };
        let attachment = attachments
            .iter()
            .find(|attachment| id_of(attachment) == attachment_id)
            .cloned()
            .unwrap_or(Value::Null);
        let policy_name = attachment
            .get("policyId")
            .and_then(Value::as_str)
            .and_then(|policy| policies.iter().find(|row| id_of(row) == policy))
            .and_then(|policy| policy.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("Access policy");
        let target = attachment
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("");
        let target_id = attachment
            .get("targetId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let target_name = if target == "group" {
            groups
                .iter()
                .find(|group| id_of(group) == target_id)
                .and_then(|group| group.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
        } else {
            nodes
                .iter()
                .find(|node| id_of(node) == target_id)
                .and_then(|node| node.get("label"))
                .and_then(Value::as_str)
                .unwrap_or("")
        };
        let explanation = format!(
            "{}: {target_name} · {}",
            if target == "group" {
                "Group"
            } else {
                "Organization"
            },
            if status == "pending" {
                "Independent review required".to_owned()
            } else {
                definition
                    .get("blockedReason")
                    .and_then(Value::as_str)
                    .unwrap_or(status.as_str())
                    .to_owned()
            }
        );
        candidates.push(json!({
            "id": row.get::<String, _>("id"),
            "personId": person_id,
            "userId": row.get::<String, _>("user_id"),
            "attachmentId": attachment_id,
            "policyName": policy_name,
            "roleId": definition.get("roleId").cloned().unwrap_or(Value::Null),
            "status": status,
            "reviewId": row.get::<Option<String>, _>("review_id"),
            "reviewVersion": row.get::<Option<i32>, _>("review_version").unwrap_or(0),
            "assignmentId": row.get::<Option<String>, _>("assignment_id"),
            "validUntil": valid_until,
            "explanation": explanation,
        }));
    }
    let policies = policies
        .into_iter()
        .filter(|policy| policy_ids.contains(&id_of(policy)))
        .collect::<Vec<_>>();

    Ok(json!({
        "attributes": attributes,
        "groups": groups,
        "policies": policies,
        "attachments": attachments,
        "values": values,
        "people": people,
        "nodes": nodes,
        "candidates": candidates,
        "canManage": actor.has(MANAGE_PERMISSION),
        "canManageGroups": actor.has(MANAGE_PERMISSION) || actor.has(crate::domain::user_management::DIRECTORY_MANAGE_PERMISSION),
        "canDefine": actor.scope_kind == "holding" && actor.has(ATTRIBUTE_MANAGE_PERMISSION),
        "canManagePeople": actor.has("people.manage"),
        "canManagePersonAttributes": actor.has("people.custom_attributes.write"),
        "canManageOrganization": actor.has("organization.manage"),
        "canReview": actor.has(REVIEW_PERMISSION),
    }))
}

async fn command_snapshot(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    change: &ManagementChange,
    now: i64,
) -> Result<Value, Error> {
    let narrow = matches!(
        change,
        ManagementChange::Values(input) if input.target == "person" && !actor.has("iam.read")
    );
    if !narrow {
        return snapshot(connection, actor, now).await;
    }
    let (ValuesInput { object_id, .. },) = match change {
        ManagementChange::Values(input) => (input.clone(),),
        _ => unreachable!(),
    };
    require(connection, actor, "people.custom_attributes.write", now).await?;
    if actor.platform {
        return Err(company_scope_required().into());
    }
    visible_person(connection, actor, &object_id).await?;
    let attributes: Vec<Value> = sqlx::query(
        "SELECT id,version,definition FROM um_attributes WHERE company_id=$1 \
         ORDER BY definition->>'label',id",
    )
    .bind(&actor.company_id)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .filter_map(|row| {
        let version: i32 = row.get("version");
        let mut definition: Value = row.get("definition");
        if definition.get("target").and_then(Value::as_str) != Some("person") {
            return None;
        }
        if let Some(object) = definition.as_object_mut() {
            object.insert("id".into(), json!(row.get::<String, _>("id")));
            object.insert("version".into(), json!(version));
        }
        let (can_view, can_edit) = attribute_access(&definition, actor);
        let company_it = definition
            .get("ownerRole")
            .and_then(Value::as_str)
            .is_none()
            && actor.role == "it_admin"
            && actor.scope_kind == "holding";
        let can_view = can_view || company_it;
        let can_edit = can_edit || company_it;
        if let Some(object) = definition.as_object_mut() {
            object.insert("canView".into(), json!(can_view));
            object.insert("canEdit".into(), json!(can_edit));
        }
        (can_view || can_edit).then_some(definition)
    })
    .collect();
    let visible: BTreeSet<String> = attributes
        .iter()
        .filter(|attribute| attribute.get("canView").and_then(Value::as_bool) == Some(true))
        .map(id_of)
        .collect();
    let row = sqlx::query(
        "SELECT version,definition FROM um_values WHERE company_id=$1 AND target='person' \
         AND object_id=$2 FOR UPDATE",
    )
    .bind(&actor.company_id)
    .bind(&object_id)
    .fetch_optional(&mut *connection)
    .await?;
    let values: Vec<Value> = row
        .map(|row| {
            let version: i32 = row.get("version");
            let definition: Value = row.get("definition");
            let filtered: Map<String, Value> = definition
                .as_object()
                .map(|values| {
                    values
                        .iter()
                        .filter(|(key, _)| visible.contains(key.as_str()))
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect()
                })
                .unwrap_or_default();
            vec![json!({
                "target": "person", "objectId": object_id, "version": version,
                "expectedVersion": version, "values": filtered, "reason": "",
            })]
        })
        .unwrap_or_default();
    Ok(json!({
        "attributes": attributes,
        "groups": [], "policies": [], "attachments": [], "people": [], "nodes": [],
        "candidates": [], "values": values,
        "canManage": false, "canManageGroups": false, "canDefine": false,
        "canManagePeople": actor.has("people.manage"),
        "canManagePersonAttributes": true, "canManageOrganization": false, "canReview": false,
    }))
}

async fn save_object(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    kind: &str,
    id: &str,
    expected_version: i64,
    definition: &Value,
    extra: Option<(&str, &str)>,
    now: i64,
) -> Result<(), Error> {
    let table = table(kind);
    let old = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT version FROM {table} WHERE company_id=$1 AND id=$2 FOR UPDATE"
    )))
    .bind(&actor.company_id)
    .bind(id)
    .fetch_optional(&mut *connection)
    .await?;
    let found = old.map(|row| row.get::<i32, _>("version") as i64);
    if found.unwrap_or(0) != expected_version {
        return Err(changed().into());
    }
    let version = expected_version + 1;
    if let Some((author, parent)) = extra {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {table}(company_id,id,version,definition,author_id,parent_id) \
             VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(company_id,id) DO UPDATE SET \
             version=excluded.version,definition=excluded.definition,\
             author_id=excluded.author_id,parent_id=excluded.parent_id"
        )))
        .bind(&actor.company_id)
        .bind(id)
        .bind(version as i32)
        .bind(definition)
        .bind(author)
        .bind(parent)
        .execute(&mut *connection)
        .await?;
    } else {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {table}(company_id,id,version,definition) VALUES($1,$2,$3,$4) \
             ON CONFLICT(company_id,id) DO UPDATE SET version=excluded.version,definition=excluded.definition"
        )))
        .bind(&actor.company_id)
        .bind(id)
        .bind(version as i32)
        .bind(definition)
        .execute(&mut *connection)
        .await?;
    }
    sqlx::query(
        "INSERT INTO um_history(company_id,entity,object_id,version,definition,created_at) \
         VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(&actor.company_id)
    .bind(table)
    .bind(id)
    .bind(version as i32)
    .bind(definition)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

fn definition_of(change: &ManagementChange) -> Result<Value, BusinessError> {
    match change {
        ManagementChange::Attribute(input) => {
            let mut value = serde_json::to_value(input).map_err(|_| request_invalid())?;
            if let Some(object) = value.as_object_mut() {
                object.remove("expectedVersion");
            }
            Ok(value)
        }
        ManagementChange::Group(input) => {
            let mut value = serde_json::to_value(input).map_err(|_| request_invalid())?;
            if let Some(object) = value.as_object_mut() {
                object.remove("expectedVersion");
            }
            Ok(value)
        }
        ManagementChange::Policy(input) => {
            let mut value = serde_json::to_value(input).map_err(|_| request_invalid())?;
            if let Some(object) = value.as_object_mut() {
                object.remove("expectedVersion");
            }
            Ok(value)
        }
        ManagementChange::Attachment(input) => {
            let mut value = serde_json::to_value(input).map_err(|_| request_invalid())?;
            if let Some(object) = value.as_object_mut() {
                object.remove("expectedVersion");
            }
            Ok(value)
        }
        _ => Err(request_invalid()),
    }
}

async fn validate_policy(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    policy: &PolicyInput,
    now: i64,
) -> Result<(), Error> {
    let parent_until: Option<i64> =
        sqlx::query_scalar("SELECT valid_until FROM assignments WHERE id=$1 AND company_id=$2")
            .bind(&actor.assignment_id)
            .bind(&actor.company_id)
            .fetch_optional(&mut *connection)
            .await?;
    let parent_until = parent_until.ok_or_else(scope_forbidden)?;
    for grant in &policy.grants {
        allowed_scope(connection, actor, grant.scope_node_id.as_deref()).await?;
        if (grant.scope_kind == "holding" && actor.scope_kind != "holding")
            || (grant.scope_kind != "holding" && grant.scope_node_id.is_none())
        {
            return Err(unit_required().into());
        }
        if grant.scope_kind != "holding" {
            let node: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM organization_nodes WHERE company_id=$1 AND id=$2 AND kind=$3",
            )
            .bind(&actor.company_id)
            .bind(grant.scope_node_id.as_deref())
            .bind(&grant.scope_kind)
            .fetch_optional(&mut *connection)
            .await?;
            if node.is_none() {
                return Err(grant_scope_mismatch().into());
            }
        }
        if grant.valid_until <= now || grant.valid_until > parent_until.min(now + 90 * 86_400_000) {
            return Err(grant_expiry_invalid().into());
        }
        let role = sqlx::query(
            "SELECT v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r \
             JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision \
             WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.published_revision=$3 AND r.status='active'",
        )
        .bind(&actor.company_id)
        .bind(&grant.role_id)
        .bind(grant.expected_role_revision as i32)
        .fetch_optional(&mut *connection)
        .await?;
        let Some(role) = role else {
            return Err(grant_role_changed().into());
        };
        let scopes: Value = role.get("scopes_json");
        let permissions: Value = role.get("permissions_json");
        let grantable: Value = role.get("grantable_json");
        if !scopes.as_array().is_some_and(|scopes| {
            scopes
                .iter()
                .any(|scope| scope.as_str() == Some(grant.scope_kind.as_str()))
        }) {
            return Err(grant_role_changed().into());
        }
        let mut delegated = Vec::new();
        for value in permissions
            .as_array()
            .into_iter()
            .flatten()
            .chain(grantable.as_array().into_iter().flatten())
        {
            if let Some(permission) = value.as_str() {
                delegated.push(permission.to_owned());
            }
        }
        if delegated
            .iter()
            .any(|permission| !actor.grantable_permissions.contains(permission))
        {
            return Err(delegation_exceeded().into());
        }
        if delegated
            .iter()
            .any(|permission| permission == "company.owner" || permission == "iam.recovery")
        {
            return Err(protected_grant().into());
        }
    }
    Ok(())
}

async fn apply_change(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    change: &ManagementChange,
    now: i64,
) -> Result<String, Error> {
    let permission = command_permission(change, actor);
    require(connection, actor, permission, now).await?;
    match change {
        ManagementChange::Attribute(input) => apply_attribute(connection, actor, input, now).await,
        ManagementChange::Group(input) => apply_group(connection, actor, input).await,
        ManagementChange::Membership(input) => apply_membership(connection, actor, input).await,
        ManagementChange::Values(input) => apply_values(connection, actor, input, now).await,
        ManagementChange::Policy(input) => apply_policy(connection, actor, input, now).await,
        ManagementChange::Attachment(input) => {
            apply_attachment(connection, actor, input, now).await
        }
    }
}

async fn apply_attribute(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    input: &AttributeInput,
    _now: i64,
) -> Result<String, Error> {
    if actor.scope_kind != "holding" {
        return Err(scope_forbidden().into());
    }
    let id = input.id.clone().unwrap_or_else(new_id);
    let old = sqlx::query("SELECT definition FROM um_attributes WHERE company_id=$1 AND id=$2")
        .bind(&actor.company_id)
        .bind(&id)
        .fetch_optional(&mut *connection)
        .await?
        .map(|row| row.get::<Value, _>("definition"));
    let roles: BTreeSet<&String> = input
        .read_roles
        .iter()
        .chain(input.write_roles.iter())
        .flatten()
        .chain(input.owner_role.iter())
        .collect();
    for role in roles {
        let active: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'",
        )
        .bind(role)
        .bind(&actor.company_id)
        .fetch_optional(&mut *connection)
        .await?;
        if active.is_none() {
            return Err(attribute_role_invalid().into());
        }
    }
    if let (Some(read), Some(write)) = (&input.read_roles, &input.write_roles) {
        if !read.is_empty()
            && write
                .iter()
                .any(|role| !read.iter().any(|candidate| candidate == role))
        {
            return Err(attribute_acl_invalid().into());
        }
    }
    if let Some(old) = &old {
        if old.get("type").and_then(Value::as_str) != Some(input.field_type.as_str())
            || old.get("target").and_then(Value::as_str) != Some(input.target.as_str())
        {
            return Err(attribute_permanent().into());
        }
    }
    let used: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean \
         AND EXISTS(SELECT 1 FROM jsonb_array_elements(definition->'rule'->'conditions') x \
         WHERE x->>'field'=$2)",
    )
    .bind(&actor.company_id)
    .bind(format!("custom:{id}"))
    .fetch_optional(&mut *connection)
    .await?;
    if used.is_some() {
        let options_changed = old.as_ref().is_some_and(|old| {
            old.get("options").cloned().unwrap_or(json!([]))
                != serde_json::to_value(&input.options).unwrap_or(json!([]))
        });
        if input.archived || options_changed {
            return Err(attribute_in_use().into());
        }
    }
    if let Some(old) = &old {
        let old_options = old
            .get("options")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if old_options
            .iter()
            .filter_map(Value::as_str)
            .any(|option| !input.options.iter().any(|next| next == option))
        {
            return Err(attribute_options_retained().into());
        }
    }
    let mut definition = serde_json::to_value(input).map_err(|_| request_invalid())?;
    if let Some(object) = definition.as_object_mut() {
        object.remove("expectedVersion");
    }
    save_object(
        connection,
        actor,
        "attribute",
        &id,
        input.expected_version,
        &definition,
        None,
        _now,
    )
    .await?;
    Ok(id)
}

async fn apply_group(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    input: &GroupInput,
) -> Result<String, Error> {
    if let Some(id) = &input.id {
        group_row(connection, actor, id).await?;
    }
    allowed_scope(connection, actor, input.scope_node_id.as_deref()).await?;
    let mut parent = input.parent_id.clone();
    let mut seen = BTreeSet::from([input.id.clone().unwrap_or_else(new_id)]);
    while let Some(current) = parent {
        if !seen.insert(current.clone()) || seen.len() > 32 {
            return Err(group_cycle().into());
        }
        let (_, row) = group_row(connection, actor, &current).await?;
        if flag(&row, "archived") || text(&row, "kind").as_deref() != Some("manual") {
            return Err(group_parent_invalid().into());
        }
        if let Some(scope) = text(&row, "scopeNodeId") {
            let Some(input_scope) = input.scope_node_id.as_deref() else {
                return Err(group_parent_scope().into());
            };
            let ok: Option<bool> = sqlx::query_scalar("SELECT um_descendant($1,$2,$3) ok")
                .bind(&actor.company_id)
                .bind(input_scope)
                .bind(scope)
                .fetch_optional(&mut *connection)
                .await?;
            if ok != Some(true) {
                return Err(group_parent_scope().into());
            }
        }
        parent = text(&row, "parentId");
    }
    let children: Vec<(String, Value)> = sqlx::query(
        "SELECT id,definition FROM um_groups WHERE company_id=$1 AND definition->>'parentId'=$2",
    )
    .bind(&actor.company_id)
    .bind(input.id.as_deref())
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("id"),
            row.get::<Value, _>("definition"),
        )
    })
    .collect();
    for (child_id, child) in children
        .iter()
        .filter(|(_, child)| !flag(child, "archived"))
    {
        if let Some(scope) = input.scope_node_id.as_deref() {
            let Some(child_scope) = text(child, "scopeNodeId") else {
                return Err(group_children_scope().into());
            };
            let ok: Option<bool> = sqlx::query_scalar("SELECT um_descendant($1,$2,$3) ok")
                .bind(&actor.company_id)
                .bind(child_scope)
                .bind(scope)
                .fetch_optional(&mut *connection)
                .await?;
            if ok != Some(true) {
                let _ = child_id;
                return Err(group_children_scope().into());
            }
        }
    }
    if input.id.is_some()
        && (input.archived || input.kind == "automatic")
        && children.iter().any(|(_, child)| !flag(child, "archived"))
    {
        return Err(group_children_present().into());
    }
    if let Some(rule) = &input.rule {
        let attributes =
            sqlx::query("SELECT id,version,definition FROM um_attributes WHERE company_id=$1")
                .bind(&actor.company_id)
                .fetch_all(&mut *connection)
                .await?
                .into_iter()
                .map(|row| {
                    let mut definition: Value = row.get("definition");
                    if let Some(object) = definition.as_object_mut() {
                        object.insert("id".into(), json!(row.get::<String, _>("id")));
                    }
                    definition
                })
                .collect::<Vec<_>>();
        for condition in rule
            .get("conditions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let field = condition.get("field").and_then(Value::as_str).unwrap_or("");
            let custom = attributes.iter().find(|attribute| {
                format!("custom:{}", id_of(attribute)) == field
                    && attribute.get("target").and_then(Value::as_str) == Some("person")
                    && !flag(attribute, "archived")
            });
            let builtin = person_rule_field_type(field);
            let value_type = custom
                .and_then(|attribute| attribute.get("type"))
                .and_then(Value::as_str)
                .or(builtin);
            let Some(value_type) = value_type else {
                return Err(group_rule_field().into());
            };
            let operation = condition.get("op").and_then(Value::as_str).unwrap_or("");
            if !["empty", "not-empty"].contains(&operation) {
                let Some(value) = condition.get("value") else {
                    return Err(group_rule_value().into());
                };
                if ["gt", "gte", "lt", "lte"].contains(&operation)
                    && !["number", "date"].contains(&value_type)
                {
                    return Err(group_rule_order().into());
                }
                if (operation == "contains") != (value_type == "multi-select") {
                    return Err(group_rule_contains().into());
                }
                if let Some(custom) = custom {
                    let mut field = custom.clone();
                    if if let Some(object) = field.as_object_mut() {
                        object.insert(
                            "type".into(),
                            json!(if value_type == "multi-select" {
                                "single-select"
                            } else {
                                value_type
                            }),
                        );
                        true
                    } else {
                        false
                    } {
                        validate_attribute_value(&field, Some(value))?;
                    }
                } else if !value.is_string() {
                    return Err(group_rule_text().into());
                }
            }
        }
    }
    let id = input.id.clone().unwrap_or_else(new_id);
    let definition = definition_of(&ManagementChange::Group(input.clone()))?;
    save_object(
        connection,
        actor,
        "group",
        &id,
        input.expected_version,
        &definition,
        None,
        now_ms(),
    )
    .await?;
    Ok(id)
}

async fn apply_membership(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    input: &MembershipInput,
) -> Result<String, Error> {
    let (version, definition) = group_row(connection, actor, &input.group_id).await?;
    if version != input.expected_version {
        return Err(changed().into());
    }
    if text(&definition, "kind").as_deref() != Some("manual") || flag(&definition, "archived") {
        return Err(group_manual_required().into());
    }
    for person in &input.person_ids {
        visible_person(connection, actor, person).await?;
        let ok: Option<bool> = sqlx::query_scalar("SELECT um_person_in_unit($1,$2,$3) ok")
            .bind(&actor.company_id)
            .bind(person)
            .bind(text(&definition, "scopeNodeId"))
            .fetch_optional(&mut *connection)
            .await?;
        if ok != Some(true) {
            return Err(membership_out_of_scope().into());
        }
        if input.action == "add" {
            sqlx::query("INSERT INTO um_memberships VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(&actor.company_id)
                .bind(&input.group_id)
                .bind(person)
                .execute(&mut *connection)
                .await?;
        } else {
            sqlx::query(
                "DELETE FROM um_memberships WHERE company_id=$1 AND group_id=$2 AND person_id=$3",
            )
            .bind(&actor.company_id)
            .bind(&input.group_id)
            .bind(person)
            .execute(&mut *connection)
            .await?;
        }
    }
    save_object(
        connection,
        actor,
        "group",
        &input.group_id,
        input.expected_version,
        &definition,
        None,
        now_ms(),
    )
    .await?;
    Ok(input.group_id.clone())
}

async fn apply_values(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    input: &ValuesInput,
    now: i64,
) -> Result<String, Error> {
    match input.target.as_str() {
        "person" => visible_person(connection, actor, &input.object_id).await?,
        "group" => {
            group_row(connection, actor, &input.object_id).await?;
        }
        _ => allowed_scope(connection, actor, Some(&input.object_id)).await?,
    }
    let old = sqlx::query(
        "SELECT version,definition FROM um_values WHERE company_id=$1 AND target=$2 AND object_id=$3 FOR UPDATE",
    )
    .bind(&actor.company_id)
    .bind(&input.target)
    .bind(&input.object_id)
    .fetch_optional(&mut *connection)
    .await?;
    let old_version = old
        .as_ref()
        .map(|row| row.get::<i32, _>("version") as i64)
        .unwrap_or(0);
    if old_version != input.expected_version {
        return Err(values_changed().into());
    }
    let mut values: Map<String, Value> = old
        .as_ref()
        .map(|row| {
            row.get::<Value, _>("definition")
                .as_object()
                .cloned()
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let snapshot = snapshot(connection, actor, now).await?;
    let attributes = snapshot
        .get("attributes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(inherit) = &input.inherit {
        if !inherit.is_empty() && input.target != "unit" {
            return Err(inherit_invalid().into());
        }
        for field_id in inherit {
            let field = attributes.iter().find(|attribute| {
                id_of(attribute) == *field_id
                    && attribute.get("target").and_then(Value::as_str) == Some("unit")
            });
            let Some(field) = field else {
                return Err(attribute_write_forbidden().into());
            };
            if field.get("canEdit").and_then(Value::as_bool) != Some(true) {
                return Err(attribute_write_forbidden().into());
            }
            let inherited =
                inherited_value(connection, &actor.company_id, &input.object_id, field_id).await?;
            validate_attribute_value(field, inherited.as_ref())?;
            values.remove(field_id);
        }
    }
    for id in input.values.keys() {
        let allowed = attributes.iter().any(|attribute| {
            id_of(attribute) == *id
                && attribute.get("target").and_then(Value::as_str) == Some(input.target.as_str())
                && !flag(attribute, "archived")
                && attribute.get("canEdit").and_then(Value::as_bool) == Some(true)
        });
        if !allowed {
            return Err(attribute_write_forbidden().into());
        }
    }
    for field in attributes.iter().filter(|attribute| {
        attribute.get("target").and_then(Value::as_str) == Some(input.target.as_str())
            && !flag(attribute, "archived")
    }) {
        let id = id_of(field);
        if input
            .inherit
            .as_ref()
            .is_some_and(|inherit| inherit.contains(&id))
        {
            continue;
        }
        if input.target == "unit" && !input.values.contains_key(&id) && !values.contains_key(&id) {
            continue;
        }
        let value = input.values.get(&id).or_else(|| values.get(&id));
        let checked = validate_attribute_value(field, value)?;
        match checked {
            Some(value) => {
                values.insert(id, value);
            }
            None => {
                values.remove(&id);
            }
        }
    }
    let version = input.expected_version + 1;
    sqlx::query(
        "INSERT INTO um_values VALUES($1,$2,$3,$4,$5) ON CONFLICT(company_id,target,object_id) \
         DO UPDATE SET version=excluded.version,definition=excluded.definition",
    )
    .bind(&actor.company_id)
    .bind(&input.target)
    .bind(&input.object_id)
    .bind(version as i32)
    .bind(Value::Object(values.clone()))
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO um_history(company_id,entity,object_id,version,definition,created_at) \
         VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(&actor.company_id)
    .bind(format!("values:{}", input.target))
    .bind(&input.object_id)
    .bind(version as i32)
    .bind(Value::Object(values))
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(input.object_id.clone())
}

async fn inherited_value(
    connection: &mut PgConnection,
    company: &str,
    object: &str,
    field: &str,
) -> Result<Option<Value>, Error> {
    let value: Option<Value> = sqlx::query_scalar(
        "WITH RECURSIVE ancestry AS (\
           SELECT parent_id,ARRAY[id] path,0 depth FROM organization_nodes WHERE id=$1 AND company_id=$2 \
           UNION ALL SELECT n.parent_id,a.path||n.id,a.depth+1 FROM organization_nodes n \
           JOIN ancestry a ON n.id=a.parent_id WHERE n.company_id=$2 AND NOT n.id=ANY(a.path) AND a.depth<32) \
         SELECT v.definition->$3 value FROM ancestry a JOIN um_values v ON v.object_id=a.parent_id \
         AND v.company_id=$2 AND v.target='unit' WHERE v.definition ? $3 ORDER BY a.depth LIMIT 1",
    )
    .bind(object)
    .bind(company)
    .bind(field)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(value)
}

async fn apply_policy(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    input: &PolicyInput,
    now: i64,
) -> Result<String, Error> {
    if let Some(id) = &input.id {
        let exists: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM um_policies WHERE company_id=$1 AND id=$2")
                .bind(&actor.company_id)
                .bind(id)
                .fetch_optional(&mut *connection)
                .await?;
        if exists.is_none() {
            return Err(policy_out_of_scope().into());
        }
    }
    if !input.archived {
        validate_policy(connection, actor, input, now).await?;
    }
    let id = input.id.clone().unwrap_or_else(new_id);
    let definition = definition_of(&ManagementChange::Policy(input.clone()))?;
    save_object(
        connection,
        actor,
        "policy",
        &id,
        input.expected_version,
        &definition,
        None,
        now,
    )
    .await?;
    Ok(id)
}

async fn apply_attachment(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    input: &AttachmentInput,
    now: i64,
) -> Result<String, Error> {
    if let Some(id) = &input.id {
        let exists: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM um_attachments WHERE company_id=$1 AND id=$2")
                .bind(&actor.company_id)
                .bind(id)
                .fetch_optional(&mut *connection)
                .await?;
        if exists.is_none() {
            return Err(attachment_out_of_scope().into());
        }
    }
    let policy = sqlx::query("SELECT definition FROM um_policies WHERE company_id=$1 AND id=$2")
        .bind(&actor.company_id)
        .bind(&input.policy_id)
        .fetch_optional(&mut *connection)
        .await?;
    let Some(policy) = policy else {
        return Err(policy_required().into());
    };
    let policy_definition: Value = policy.get("definition");
    if flag(&policy_definition, "archived") && !input.archived {
        return Err(policy_required().into());
    }
    if !input.archived {
        let grants: Vec<crate::domain::user_management::PolicyGrantInput> = policy_definition
            .get("grants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|grant| {
                Some(crate::domain::user_management::PolicyGrantInput {
                    role_id: text(&grant, "roleId")?,
                    expected_role_revision: grant
                        .get("expectedRoleRevision")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    scope_kind: text(&grant, "scopeKind")?,
                    scope_node_id: text(&grant, "scopeNodeId"),
                    valid_until: grant.get("validUntil").and_then(Value::as_i64).unwrap_or(0),
                })
            })
            .collect();
        let policy = PolicyInput {
            id: text(&policy_definition, "id"),
            expected_version: 0,
            reason: text(&policy_definition, "reason").unwrap_or_else(|| "policy".into()),
            name: text(&policy_definition, "name").unwrap_or_else(|| "policy".into()),
            grants,
            archived: false,
        };
        validate_policy(connection, actor, &policy, now).await?;
    }
    if input.target == "group" {
        let (_, group) = group_row(connection, actor, &input.target_id).await?;
        if flag(&group, "archived") {
            return Err(active_group_required().into());
        }
    } else {
        allowed_scope(connection, actor, Some(&input.target_id)).await?;
    }
    for excluded in &input.exclusions {
        if input.target == "unit" {
            allowed_scope(connection, actor, Some(excluded)).await?;
            let ok: Option<bool> = sqlx::query_scalar("SELECT um_descendant($1,$2,$3) ok")
                .bind(&actor.company_id)
                .bind(excluded)
                .bind(&input.target_id)
                .fetch_optional(&mut *connection)
                .await?;
            if ok != Some(true) {
                return Err(exclusion_out_of_scope().into());
            }
        } else {
            let mut current = Some(excluded.clone());
            let mut found = false;
            let mut seen = BTreeSet::new();
            while let Some(id) = current {
                if !seen.insert(id.clone()) {
                    break;
                }
                let (_, row) = group_row(connection, actor, &id).await?;
                if id == input.target_id {
                    found = true;
                    break;
                }
                current = text(&row, "parentId");
            }
            if !found {
                return Err(exclusion_out_of_scope().into());
            }
        }
    }
    let id = input.id.clone().unwrap_or_else(new_id);
    let definition = definition_of(&ManagementChange::Attachment(input.clone()))?;
    save_object(
        connection,
        actor,
        "attachment",
        &id,
        input.expected_version,
        &definition,
        Some((&actor.user_id, &actor.assignment_id)),
        now,
    )
    .await?;
    Ok(id)
}

fn new_id() -> String {
    uuid_like()
}

/// The crate has no `uuid` dependency in production; generate a v4-shaped ID
/// from the OS entropy available through the shared runtime.
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut seed = now as u64;
    seed ^= std::process::id() as u64;
    let mut output = String::with_capacity(36);
    let hex = format!("{seed:016x}{:016x}", now.rotate_left(17) as u64);
    for (index, byte) in hex.bytes().take(32).enumerate() {
        if matches!(index, 8 | 12 | 16 | 20) {
            output.push('-');
        }
        output.push(byte as char);
    }
    output
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

async fn reconcile(connection: &mut PgConnection, company_id: &str, now: i64) -> Result<(), Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("user-management:{company_id}"))
        .execute(&mut *connection)
        .await?;
    sqlx::query(
        "UPDATE um_candidates c SET status='rejected' FROM assignments a \
         WHERE c.company_id=$1 AND c.assignment_id=a.id AND c.status='active' \
         AND a.revoked_at IS NOT NULL AND um_attachment_matches(c.company_id,c.person_id,c.attachment_id)",
    )
    .bind(company_id)
    .execute(&mut *connection)
    .await?;
    sqlx::query("SELECT um_invalidate($1)")
        .bind(company_id)
        .execute(&mut *connection)
        .await?;
    sqlx::query(
        "UPDATE um_candidates c SET status='rejected' FROM iam_reviews r \
         WHERE c.review_id=r.id AND c.company_id=$1 AND c.status='pending' AND r.status='rejected'",
    )
    .bind(company_id)
    .execute(&mut *connection)
    .await?;
    let matches = sqlx::query(
        "SELECT a.*,p.version policy_version,p.definition policy_data,h.id person_id,h.user_id \
         FROM um_attachments a JOIN um_policies p ON p.company_id=a.company_id AND p.id=a.definition->>'policyId' \
         JOIN hr_people h ON h.company_id=$1 WHERE a.company_id=$1 AND h.user_id IS NOT NULL AND h.status='active' \
         AND um_attachment_matches($1,h.id,a.id) AND iam_assignment_active(a.parent_id,$2) ORDER BY a.id,h.id",
    )
    .bind(company_id)
    .bind(now)
    .fetch_all(&mut *connection)
    .await?;
    for row in matches {
        let parent_id: String = row.get("parent_id");
        let author_id: String = row.get("author_id");
        let proposal = sqlx::query(
            "SELECT a.id,a.scope_kind,a.scope_node_id,a.role,v.grantable_json FROM assignments a \
             JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) \
             JOIN iam_role_revisions v ON v.role_id=r.id AND v.company_id=r.company_id AND v.revision=r.published_revision \
             WHERE a.id=$1 AND a.user_id=$2 AND iam_role_allows(a.role,a.company_id,'iam.manage')",
        )
        .bind(&parent_id)
        .bind(&author_id)
        .fetch_optional(&mut *connection)
        .await?;
        let Some(proposal) = proposal else {
            continue;
        };
        let policy: Value = row.get("policy_data");
        let grants = policy
            .get("grants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let person_id: String = row.get("person_id");
        let user_id: String = row.get("user_id");
        let attachment_id: String = row.get("id");
        let attachment_version: i32 = row.get("version");
        let policy_version: i32 = row.get("policy_version");
        let policy_name = text(&policy, "name").unwrap_or_else(|| "Access policy".into());
        for (index, spec) in grants.iter().enumerate() {
            let valid_until = spec.get("validUntil").and_then(Value::as_i64).unwrap_or(0);
            if valid_until <= now {
                continue;
            }
            let existing: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM um_candidates WHERE company_id=$1 AND person_id=$2 AND attachment_id=$3 \
                 AND grant_index=$4 AND attachment_version=$5 AND policy_version=$6 \
                 AND status IN ('pending','active','rejected','blocked')",
            )
            .bind(company_id)
            .bind(&person_id)
            .bind(&attachment_id)
            .bind(index as i32)
            .bind(attachment_version)
            .bind(policy_version)
            .fetch_optional(&mut *connection)
            .await?;
            if existing.is_some() {
                continue;
            }
            let grant = json!({
                "userId": user_id,
                "roleId": spec.get("roleId").cloned().unwrap_or(Value::Null),
                "expectedRoleRevision": spec.get("expectedRoleRevision").cloned().unwrap_or(Value::Null),
                "scopeKind": spec.get("scopeKind").cloned().unwrap_or(Value::Null),
                "scopeNodeId": spec.get("scopeNodeId").cloned().unwrap_or(Value::Null),
                "validUntil": valid_until,
                "reason": format!("Policy: {policy_name}"),
            });
            let proposer = UserManagementActor {
                user_id: author_id.clone(),
                session_id: String::new(),
                holding_id: String::new(),
                company_id: company_id.to_owned(),
                assignment_id: parent_id.clone(),
                scope_kind: row.get("scope_kind"),
                scope_node_id: row.get("scope_node_id"),
                role: row.get("role"),
                permissions: vec![MANAGE_PERMISSION.into()],
                grantable_permissions: proposal
                    .get::<Value, _>("grantable_json")
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect(),
                platform: false,
                expires_at: i64::MAX,
            };
            let blocked =
                validate_candidate_grant(connection, &proposer, &grant, &parent_id, now).await;
            let id = uuid_like();
            let (user_id_value, role_id, scope_kind, scope_node_id) = (
                user_id.clone(),
                text(&grant, "roleId").unwrap_or_default(),
                text(&grant, "scopeKind").unwrap_or_default(),
                text(&grant, "scopeNodeId"),
            );
            let _ = (&user_id_value, &role_id, &scope_kind, &scope_node_id);
            if let Err(error) = blocked {
                sqlx::query(
                    "INSERT INTO um_candidates(id,company_id,person_id,user_id,attachment_id,\
                     attachment_version,policy_version,grant_index,definition,status,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'blocked',$10)",
                )
                .bind(&id)
                .bind(company_id)
                .bind(&person_id)
                .bind(&user_id)
                .bind(&attachment_id)
                .bind(attachment_version)
                .bind(policy_version)
                .bind(index as i32)
                .bind({
                    let mut definition = grant.clone();
                    if let Some(object) = definition.as_object_mut() {
                        object.insert("blockedReason".into(), json!(error.message));
                    }
                    definition
                })
                .bind(now)
                .execute(&mut *connection)
                .await?;
                continue;
            }
            let review_id = uuid_like();
            let payload = json!({
                "userId": user_id,
                "roleId": grant.get("roleId"),
                "expectedRoleRevision": grant.get("expectedRoleRevision"),
                "scopeKind": grant.get("scopeKind"),
                "scopeNodeId": grant.get("scopeNodeId"),
                "validUntil": valid_until,
                "reason": format!("Policy: {policy_name}"),
                "candidateId": id,
                "parentId": parent_id,
                "authorizingAssignmentId": parent_id,
            });
            sqlx::query(
                "INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,reason) \
                 VALUES($1,$2,'policy.grant',$3,$4,COALESCE((SELECT person_id FROM iam_members WHERE company_id=$2 AND user_id=$4),$4),$5,$6)",
            )
            .bind(&review_id)
            .bind(company_id)
            .bind(&payload)
            .bind(&author_id)
            .bind(now)
            .bind(format!("Policy: {policy_name}"))
            .execute(&mut *connection)
            .await?;
            sqlx::query(
                "INSERT INTO um_candidates(id,company_id,person_id,user_id,attachment_id,\
                 attachment_version,policy_version,grant_index,definition,review_id,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            )
            .bind(&id)
            .bind(company_id)
            .bind(&person_id)
            .bind(&user_id)
            .bind(&attachment_id)
            .bind(attachment_version)
            .bind(policy_version)
            .bind(index as i32)
            .bind(&grant)
            .bind(&review_id)
            .bind(now)
            .execute(&mut *connection)
            .await?;
            sqlx::query(
                "INSERT INTO iam_audit VALUES($1,$2,$3,$4,'policy.grant.proposed',$5,$6,$7)",
            )
            .bind(uuid_like())
            .bind(company_id)
            .bind(&author_id)
            .bind(&parent_id)
            .bind(&id)
            .bind(&payload)
            .bind(now)
            .execute(&mut *connection)
            .await?;
        }
    }
    Ok(())
}

async fn validate_candidate_grant(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    grant: &Value,
    parent_id: &str,
    now: i64,
) -> Result<(), BusinessError> {
    let parent = sqlx::query(
        "SELECT company_id,scope_kind,scope_node_id,valid_until FROM assignments WHERE id=$1",
    )
    .bind(parent_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| transaction_required())?;
    let Some(parent) = parent else {
        return Err(changed());
    };
    let active: Option<bool> = sqlx::query_scalar("SELECT iam_assignment_active($1,$2) ok")
        .bind(parent_id)
        .bind(now)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| transaction_required())?;
    if active != Some(true) {
        return Err(changed());
    }
    let scope_kind = text(grant, "scopeKind").unwrap_or_default();
    let scope_node = text(grant, "scopeNodeId");
    let valid_until = grant.get("validUntil").and_then(Value::as_i64).unwrap_or(0);
    let parent_until: i64 = parent.get("valid_until");
    if valid_until <= now || valid_until > parent_until {
        return Err(grant_expiry_invalid());
    }
    let role_id = text(grant, "roleId").unwrap_or_default();
    let role = sqlx::query(
        "SELECT v.permissions_json,v.grantable_json,v.scopes_json,v.revision FROM iam_roles r \
         JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision \
         WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.status='active'",
    )
    .bind(&actor.company_id)
    .bind(&role_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| transaction_required())?;
    let Some(role) = role else {
        return Err(grant_role_changed());
    };
    let scopes: Value = role.get("scopes_json");
    if !scopes.as_array().is_some_and(|scopes| {
        scopes
            .iter()
            .any(|scope| scope.as_str() == Some(scope_kind.as_str()))
    }) {
        return Err(grant_role_changed());
    }
    if scope_kind == "holding" {
        if actor.scope_kind != "holding" {
            return Err(scope_forbidden());
        }
    } else {
        let node = sqlx::query(
            "SELECT kind,branch_id FROM organization_nodes WHERE company_id=$1 AND id=$2 AND status='active'",
        )
        .bind(&actor.company_id)
        .bind(scope_node.as_deref())
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| transaction_required())?;
        if node.is_none() {
            return Err(grant_scope_mismatch());
        }
    }
    Ok(())
}

async fn insert_grant(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    grant: &Value,
    parent_id: &str,
    source: &str,
    source_id: &str,
    now: i64,
) -> Result<String, Error> {
    let id = format!("iam-{}", uuid_like());
    let role_id = text(grant, "roleId").unwrap_or_default();
    let revisions = sqlx::query(
        "SELECT v.permissions_json FROM iam_roles r JOIN iam_role_revisions v \
         ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision \
         WHERE r.id=$1 AND r.company_id IN ('*',$2)",
    )
    .bind(&role_id)
    .bind(&actor.company_id)
    .fetch_all(&mut *connection)
    .await?;
    let mut modules = BTreeSet::new();
    for row in revisions {
        let permissions: Value = row.get("permissions_json");
        for permission in permissions.as_array().into_iter().flatten() {
            if let Some(permission) = permission.as_str() {
                if let Some(module) = permission.strip_prefix("module.") {
                    modules.insert(module.to_owned());
                }
            }
        }
    }
    let scope_node = text(grant, "scopeNodeId");
    let branch: Option<String> =
        match scope_node.as_deref() {
            Some(node) => sqlx::query_scalar(
                "SELECT CASE WHEN kind='branch' THEN id ELSE branch_id END FROM organization_nodes \
             WHERE id=$1 AND company_id=$2",
            )
            .bind(node)
            .bind(&actor.company_id)
            .fetch_optional(&mut *connection)
            .await?,
            None => None,
        };
    sqlx::query(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,\
         valid_until,scope_kind,scope_node_id,parent_id,source,source_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(&id)
    .bind(text(grant, "userId").unwrap_or_default())
    .bind(&role_id)
    .bind(json!(modules))
    .bind(&actor.company_id)
    .bind(branch)
    .bind(now)
    .bind(grant.get("validUntil").and_then(Value::as_i64).unwrap_or(0))
    .bind(text(grant, "scopeKind").unwrap_or_default())
    .bind(scope_node)
    .bind(parent_id)
    .bind(source)
    .bind(source_id)
    .execute(&mut *connection)
    .await?;
    Ok(id)
}

async fn existing_receipt(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    key: &str,
    digest: &str,
) -> Result<Option<Value>, Error> {
    let existing = sqlx::query(
        "SELECT company_id,user_id,request_hash,result_json FROM iam_receipts WHERE id=$1",
    )
    .bind(key)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(existing) = existing else {
        return Ok(None);
    };
    let company: String = existing.get("company_id");
    let user: String = existing.get("user_id");
    let hash: String = existing.get("request_hash");
    if company != actor.company_id || user != actor.user_id || hash != digest {
        return Err(idempotency_conflict().into());
    }
    Ok(Some(existing.get::<Value, _>("result_json")))
}

async fn record_receipt(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    key: &str,
    operation: &str,
    digest: &str,
    result: &Value,
    now: i64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO iam_receipts VALUES($1,$2,$3,$4,$5,$6)")
        .bind(key)
        .bind(&actor.company_id)
        .bind(&actor.user_id)
        .bind(digest)
        .bind(result)
        .bind(now)
        .execute(&mut *connection)
        .await?;
    sqlx::query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(uuid_like())
        .bind(&actor.company_id)
        .bind(&actor.user_id)
        .bind(&actor.assignment_id)
        .bind(operation)
        .bind(key)
        .bind(json!({"input": digest, "result": result, "role": actor.role, "scope": actor.scope_kind}))
        .bind(now)
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// Reviewed preview/apply. Returns the current snapshot for a read, the
/// preview token/impact for a preview and the applied result for a save.
pub async fn management_command(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    raw: Value,
    key: &str,
    preview: bool,
    token: Option<&str>,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    let change = ManagementChange::parse(raw)?;
    let permission = command_permission(&change, actor);
    require(connection, actor, permission, now).await?;
    let digest = content_hash(&change.to_value());
    let operation = format!("user-management.{}", change.operation());
    if !preview {
        if let Some(result) = existing_receipt(connection, actor, key, &digest).await? {
            return Ok(result);
        }
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("user-management:{}", actor.company_id))
        .execute(&mut *connection)
        .await?;
    let before = command_snapshot(connection, actor, &change, now).await?;
    let stamp = content_hash(&json!({"change": change.to_value(), "snapshot": before}));
    if !preview && token != Some(stamp.as_str()) {
        return Err(preview_changed().into());
    }
    let id = apply_change(connection, actor, &change, now).await?;
    reconcile(connection, &actor.company_id, now).await?;
    let after = command_snapshot(connection, actor, &change, now).await?;
    let mut impact = management_impact(&before, &after);
    if let ManagementChange::Group(input) = &change {
        if let Some(rule) = &input.rule {
            if let Some(object) = impact.as_object_mut() {
                object.insert(
                    "matchingPeople".into(),
                    json!(matching_people(&after, &id, rule)),
                );
            }
        }
    }
    let result = json!({"id": id, "previewToken": stamp, "impact": impact});
    if !preview {
        record_receipt(connection, actor, key, &operation, &digest, &result, now).await?;
    }
    Ok(result)
}

pub async fn management_read(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    require(connection, actor, "iam.read", now).await?;
    snapshot(connection, actor, now).await
}

pub async fn reconcile_management(
    connection: &mut PgConnection,
    company_id: &str,
    now: i64,
) -> Result<(), Error> {
    ready(connection).await?;
    reconcile(connection, company_id, now).await
}

pub async fn management_companies(connection: &mut PgConnection) -> Result<Vec<String>, Error> {
    Ok(
        sqlx::query_scalar("SELECT DISTINCT company_id FROM um_attachments ORDER BY company_id")
            .fetch_all(&mut *connection)
            .await?,
    )
}

pub async fn apply_management_review(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    payload: Value,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    if actor.platform {
        return Err(company_scope_required().into());
    }
    let candidate_id = text(&payload, "candidateId")
        .or_else(|| text(&payload, "id"))
        .ok_or_else(request_invalid)?;
    let parent_id = text(&payload, "parentId").ok_or_else(request_invalid)?;
    let row = sqlx::query("SELECT * FROM um_candidates WHERE company_id=$1 AND id=$2 FOR UPDATE")
        .bind(&actor.company_id)
        .bind(&candidate_id)
        .fetch_optional(&mut *connection)
        .await?;
    let Some(row) = row else {
        return Err(candidate_missing().into());
    };
    let status: String = row.get("status");
    let eligible: Option<bool> = sqlx::query_scalar("SELECT um_candidate_eligible($1,$2) ok")
        .bind(&candidate_id)
        .bind(now)
        .fetch_optional(&mut *connection)
        .await?;
    if status != "pending" || eligible != Some(true) {
        return Err(review_changed().into());
    }
    let definition: Value = row.get("definition");
    validate_candidate_grant(connection, actor, &definition, &parent_id, now).await?;
    let assignment_id = insert_grant(
        connection,
        actor,
        &definition,
        &parent_id,
        "policy",
        &candidate_id,
        now,
    )
    .await?;
    sqlx::query("UPDATE um_candidates SET assignment_id=$1,status='active' WHERE id=$2")
        .bind(&assignment_id)
        .bind(&candidate_id)
        .execute(&mut *connection)
        .await?;
    Ok(json!({"id": candidate_id, "assignmentId": assignment_id}))
}

/// Decide one `policy.grant` review inside the owner transaction.
pub async fn decide_management_review(
    connection: &mut PgConnection,
    actor: &UserManagementActor,
    review_id: &str,
    payload: Value,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    require(connection, actor, REVIEW_PERMISSION, now).await?;
    let expected_version = payload
        .get("expectedVersion")
        .and_then(Value::as_i64)
        .ok_or_else(request_invalid)?;
    let decision = text(&payload, "decision").ok_or_else(request_invalid)?;
    if !["approve", "reject"].contains(&decision.as_str()) {
        return Err(request_invalid().into());
    }
    let review = sqlx::query(
        "SELECT kind,payload,status,version,created_by,created_person FROM iam_reviews WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(review_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(review) = review else {
        return Err(candidate_missing().into());
    };
    let kind: String = review.get("kind");
    let status: String = review.get("status");
    let version: i32 = review.get("version");
    if kind != "policy.grant" {
        return Err(candidate_missing().into());
    }
    if status != "pending" || version as i64 != expected_version {
        return Err(review_changed().into());
    }
    let created_by: String = review.get("created_by");
    let created_person: String = review.get("created_person");
    if created_by == actor.user_id || created_person == actor.user_id {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_REVIEW_REQUIRED",
            "A different person must review this change.",
        )
        .into());
    }
    let proposal: Value = review.get("payload");
    let candidate_id = text(&proposal, "candidateId").ok_or_else(request_invalid)?;
    let parent_id = text(&proposal, "parentId").ok_or_else(request_invalid)?;
    let parent_active: Option<bool> = sqlx::query_scalar("SELECT iam_assignment_active($1,$2) ok")
        .bind(&parent_id)
        .bind(now)
        .fetch_optional(&mut *connection)
        .await?;
    if parent_active != Some(true) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PROPOSER_AUTHORITY_REVOKED",
            "The proposer no longer has active authority. Create a new proposal.",
        )
        .into());
    }
    if actor.scope_kind != "holding" {
        let scope_kind = text(&proposal, "scopeKind").unwrap_or_default();
        let scope_node = text(&proposal, "scopeNodeId");
        let covered =
            scope_kind == "holding" || scope_node.as_deref() == actor.scope_node_id.as_deref();
        if !covered {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "REVIEW_OUTSIDE_SCOPE",
                "This review is outside your delegated scope.",
            )
            .into());
        }
    }
    let candidate = sqlx::query(
        "SELECT status,definition FROM um_candidates WHERE company_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(&actor.company_id)
    .bind(&candidate_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(candidate) = candidate else {
        return Err(candidate_missing().into());
    };
    if decision == "approve" {
        let eligible: Option<bool> = sqlx::query_scalar("SELECT um_candidate_eligible($1,$2) ok")
            .bind(&candidate_id)
            .bind(now)
            .fetch_optional(&mut *connection)
            .await?;
        if candidate.get::<String, _>("status") != "pending" || eligible != Some(true) {
            return Err(review_changed().into());
        }
        let definition: Value = candidate.get("definition");
        validate_candidate_grant(connection, actor, &definition, &parent_id, now).await?;
        let assignment_id = insert_grant(
            connection,
            actor,
            &definition,
            &parent_id,
            "policy",
            &candidate_id,
            now,
        )
        .await?;
        sqlx::query("UPDATE um_candidates SET assignment_id=$1,status='active' WHERE id=$2")
            .bind(&assignment_id)
            .bind(&candidate_id)
            .execute(&mut *connection)
            .await?;
    } else {
        sqlx::query("UPDATE um_candidates SET status='rejected' WHERE id=$1")
            .bind(&candidate_id)
            .execute(&mut *connection)
            .await?;
    }
    let reason = text(&payload, "reason").unwrap_or_default();
    sqlx::query(
        "UPDATE iam_reviews SET status=$1,reviewed_by=$2,reviewed_at=$3,reason=reason||$4,version=version+1 WHERE id=$5",
    )
    .bind(if decision == "approve" { "approved" } else { "rejected" })
    .bind(&actor.user_id)
    .bind(now)
    .bind(format!("\n{reason}"))
    .bind(review_id)
    .execute(&mut *connection)
    .await?;
    sqlx::query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,'policy.grant.decided',$5,$6,$7)")
        .bind(uuid_like())
        .bind(&actor.company_id)
        .bind(&actor.user_id)
        .bind(&actor.assignment_id)
        .bind(&candidate_id)
        .bind(json!({"reviewId": review_id, "decision": decision, "reason": reason}))
        .bind(now)
        .execute(&mut *connection)
        .await?;
    Ok(
        json!({"id": review_id, "status": if decision == "approve" { "approved" } else { "rejected" }}),
    )
}
