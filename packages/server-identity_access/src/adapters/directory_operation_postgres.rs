//! PostgreSQL adapter and owner orchestration for Directory automation.
//!
//! SQL is Identity-owned (`iam_connections`, `iam_identities`, `iam_members`,
//! `iam_reviews`, `iam_audit`, `people_directory_state`,
//! `people_directory_operations`, `people_directory_membership_sources`,
//! `command_receipts`) plus read-only Organization/People facts. The caller
//! supplies the open owner transaction and actor; this adapter never commits.
//! Provider effects are delegated to the installed connector port.
use crate::domain::{
    directory_operation::*,
    user_management::{attribute_access, UserManagementActor},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{json, Map, Value};
use sqlx::{Connection as _, PgConnection, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;

type Error = PgTransactionError<BusinessError>;

pub type DirectoryOperationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// One bounded installed-connector effect. The profile, not this copied
/// action, decides whether the provider write is approved.
pub trait DirectoryOperationConnector: Send + Sync {
    fn inventory<'a>(
        &'a self,
        connection: &'a Connection,
    ) -> DirectoryOperationFuture<'a, DirectoryInventory>;
    fn apply<'a>(
        &'a self,
        connection: &'a Connection,
        operation_id: &'a str,
        action: &'a DirectoryAction,
        before: &'a DirectoryInventory,
    ) -> DirectoryOperationFuture<'a, Value>;
}

/// Current fresh-MFA facts for a deferred direct operation. Composition owns
/// the control-database session proof.
pub trait DirectoryDirectFreshness: Send + Sync {
    fn fresh<'a>(
        &'a self,
        user_id: &'a str,
        session_id: &'a str,
        now: i64,
    ) -> DirectoryOperationFuture<'a, bool>;
}

#[derive(Clone, Debug)]
pub struct Connection {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub provider: String,
    pub config: Value,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
struct State {
    version: i64,
    tested_version: Option<i64>,
    write_enabled: bool,
    inventory: Option<DirectoryInventory>,
    definition: Value,
}

impl State {
    fn mappings(&self) -> Vec<ConnectorMapping> {
        self.definition
            .get("mappings")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
struct Job {
    id: String,
    company_id: String,
    connection_id: String,
    connection_version: i64,
    state_version: i64,
    status: String,
    definition: DirectoryChangeInput,
    evidence: PreviewEvidence,
    review_id: Option<String>,
    created_by: String,
    assignment_id: String,
    approval_mode: String,
    mfa_session_id: Option<String>,
    steps: Vec<Value>,
    provision_id: Option<String>,
}

#[derive(Clone, Debug)]
struct IdentityLink {
    id: String,
    external_id: String,
    user_id: Option<String>,
    person_id: Option<String>,
    company_id: String,
}

#[derive(Clone, Debug)]
struct PositionFact {
    id: String,
    company_id: String,
    title: String,
}

fn unavailable_row() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "DIRECTORY_STATE_UNAVAILABLE",
        "The directory connection state is unavailable.",
    )
}
fn not_found_operation() -> BusinessError {
    conflict(
        "DIRECTORY_RETRY_UNAVAILABLE",
        "A stale or uncertain change needs a new preview. Only the original operator can resume a transport or MFA retry.",
    )
}
fn operation_unavailable() -> BusinessError {
    conflict(
        "DIRECTORY_CANCEL_UNAVAILABLE",
        "Only work that has not started can be cancelled.",
    )
}
fn beneficiary_review() -> BusinessError {
    forbidden(
        "DIRECTORY_BENEFICIARY_REVIEW",
        "A person affected by this access change cannot approve it.",
    )
}
fn review_scope() -> BusinessError {
    forbidden(
        "DIRECTORY_REVIEW_SCOPE",
        "The reviewer must cover every affected Company.",
    )
}
fn apply_scope() -> BusinessError {
    forbidden(
        "DIRECTORY_APPLY_SCOPE",
        "You must administer every affected Company.",
    )
}
fn impact_ack_required() -> BusinessError {
    conflict(
        "DIRECTORY_IMPACT_ACK_REQUIRED",
        "Acknowledge every affected Company before disabling the whole account.",
    )
}
fn protected_account() -> BusinessError {
    forbidden(
        "PROTECTED_DIRECTORY_ACCOUNT",
        "This operation would disable your own account or a recovery administrator.",
    )
}
fn verified_lifecycle_required() -> BusinessError {
    conflict(
        "VERIFIED_LIFECYCLE_REQUIRED",
        "Choose the lifecycle case for this explicitly verified account.",
    )
}
fn lifecycle_not_effective() -> BusinessError {
    conflict(
        "LIFECYCLE_NOT_EFFECTIVE",
        "The reviewed lifecycle cutoff has not taken effect.",
    )
}
fn membership_source_retained() -> BusinessError {
    conflict(
        "DIRECTORY_MEMBERSHIP_SOURCE_RETAINED",
        "Remove the supplying organization policy or access package before removing this membership.",
    )
}
fn readback_failed() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "DIRECTORY_READBACK_FAILED",
        "The provider has not verified this operation.",
    )
}

fn sha256(value: &Value) -> String {
    hash_value(value)
}

async fn ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("DIRECTORY_OPERATION_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}

async fn scoped(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    now: i64,
) -> Result<(), Error> {
    if actor.platform {
        return Err(company_scope_required().into());
    }
    if now >= actor.expires_at {
        return Err(version_conflict().into());
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
        return Err(company_scope_required().into());
    }
    Ok(())
}

async fn allowed(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    permission: &str,
    now: i64,
) -> Result<(), Error> {
    scoped(connection, actor, now).await?;
    if !actor.has(permission) {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "IAM_PERMISSION_REQUIRED",
            "The current role does not grant this Directory operation.",
        )
        .into());
    }
    Ok(())
}

async fn connection_row(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    id: &str,
    read_only: bool,
) -> Result<Connection, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,name,provider,config,status,version FROM iam_connections \
         WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Err(connection_unavailable().into());
    };
    let status: String = row.try_get("status")?;
    let allowed = if read_only {
        matches!(status.as_str(), "draft" | "tested" | "active")
    } else {
        status == "active"
    };
    if !allowed {
        return Err(connection_unavailable().into());
    }
    Ok(Connection {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        name: row.try_get("name")?,
        provider: row.try_get("provider")?,
        config: row.try_get("config")?,
        status,
        version: row.try_get("version")?,
    })
}

fn state_from_row(row: &sqlx::postgres::PgRow) -> Result<State, Error> {
    let inventory: Option<Value> = row.try_get("inventory")?;
    Ok(State {
        version: row.try_get("version")?,
        tested_version: row.try_get("tested_version")?,
        write_enabled: row.try_get("write_enabled")?,
        inventory: inventory
            .filter(|value| !value.is_null())
            .map(DirectoryInventory::checked)
            .transpose()?,
        definition: row.try_get("definition")?,
    })
}

async fn state(
    connection: &mut PgConnection,
    conn: &Connection,
    lock: bool,
) -> Result<Option<State>, Error> {
    let sql = if lock {
        "SELECT * FROM people_directory_state WHERE connection_id=$1 FOR UPDATE"
    } else {
        "SELECT * FROM people_directory_state WHERE connection_id=$1"
    };
    let row = sqlx::query(sql)
        .bind(&conn.id)
        .fetch_optional(&mut *connection)
        .await?;
    row.as_ref().map(state_from_row).transpose()
}

async fn receipt(
    connection: &mut PgConnection,
    key: &str,
    operation: &str,
    actor: &DirectoryOperationActor,
    request_hash: &str,
    record_id: &str,
) -> Result<Option<Value>, Error> {
    let row = sqlx::query(
        "SELECT request_hash,result_json,operation,actor_id,record_id \
         FROM command_receipts WHERE command_id=$1",
    )
    .bind(key)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored: String = row.try_get("request_hash")?;
    let stored_operation: String = row.try_get("operation")?;
    let stored_actor: String = row.try_get("actor_id")?;
    let stored_record: String = row.try_get("record_id")?;
    if stored != request_hash
        || stored_operation != operation
        || stored_actor != actor.user_id
        || stored_record != record_id
    {
        return Err(idempotency_conflict().into());
    }
    let result: Option<String> = row.try_get("result_json")?;
    Ok(result
        .and_then(|value| serde_json::from_str(&value).ok())
        .filter(|value: &Value| !value.is_null()))
}

#[allow(clippy::too_many_arguments)]
async fn record_receipt(
    connection: &mut PgConnection,
    key: &str,
    operation: &str,
    actor: &DirectoryOperationActor,
    request_hash: &str,
    record_id: &str,
    result: &Value,
    now: i64,
) -> Result<(), Error> {
    let authority = json!({
        "assignmentId": actor.assignment_id,
        "companyId": actor.company_id,
    })
    .to_string();
    let inserted = sqlx::query(
        "INSERT INTO command_receipts\
         (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
    )
    .bind(key)
    .bind(operation)
    .bind(&actor.user_id)
    .bind(request_hash)
    .bind(record_id)
    .bind(result.to_string())
    .bind(now)
    .bind(authority)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if inserted != 1 {
        return Err(idempotency_conflict().into());
    }
    Ok(())
}

fn optional_string(row: &sqlx::postgres::PgRow, key: &str) -> Result<Option<String>, Error> {
    Ok(row.try_get::<Option<String>, _>(key)?)
}

async fn lifecycle(
    connection: &mut PgConnection,
    action: &DirectoryAction,
    actor: &DirectoryOperationActor,
    now: i64,
) -> Result<Option<Value>, Error> {
    let case_id = match action {
        DirectoryAction::AccountEnable { case_id, .. } => Some(case_id.clone()),
        DirectoryAction::AccountDisable { case_id, .. } => case_id.clone(),
        _ => None,
    };
    let Some(case_id) = case_id else {
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT c.*,(people_case_active(c.id,$3) OR c.workflow='placement' AND c.status='completed' \
         AND c.effective_at<=$3 AND EXISTS(SELECT 1 FROM people_applied_packages ap \
         WHERE ap.company_id=c.company_id AND ap.person_id=c.person_id \
         AND people_application_active(ap.id,$3))) active \
         FROM people_lifecycle_cases c WHERE c.id=$1 AND c.company_id=$2",
    )
    .bind(case_id)
    .bind(&actor.company_id)
    .bind(now)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|row| {
        let mut value = Map::new();
        for key in ["id", "person_id", "status", "kind", "effective_at", "active"] {
            if let Ok(v) = row.try_get::<Value, _>(key) {
                value.insert(key.to_owned(), v);
            }
        }
        Value::Object(value)
    }))
}

fn json_i64(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}
fn json_str<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
fn json_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

async fn verified_lifecycle_account(
    connection: &mut PgConnection,
    conn: &Connection,
    action: &DirectoryAction,
    actor: &DirectoryOperationActor,
    lifecycle: &Value,
) -> Result<bool, Error> {
    let (Some(object_id), Some(person_id)) = (
        action.object_id(),
        lifecycle.get("person_id").and_then(Value::as_str),
    ) else {
        return Ok(false);
    };
    let row: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id \
         AND m.user_id=i.user_id AND m.company_id=i.company_id \
         WHERE i.connection_id=$1 AND i.external_id=$2 AND m.company_id=$3 AND m.person_id=$4 \
         AND m.status='active'",
    )
    .bind(&conn.id)
    .bind(object_id)
    .bind(&actor.company_id)
    .bind(person_id)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.is_some())
}

async fn identity_links(
    connection: &mut PgConnection,
    conn: &Connection,
    ids: &[String],
) -> Result<Vec<IdentityLink>, Error> {
    let authority = conn.config.get("authority").and_then(Value::as_str).unwrap_or("");
    let rows = sqlx::query(
        "SELECT i.id,i.external_id,i.user_id,i.person_id,i.company_id \
         FROM iam_identities i JOIN iam_connections c ON c.id=i.connection_id \
         WHERE c.config->>'authority'=$1 AND i.external_id=ANY($2::text[]) \
         ORDER BY i.company_id,i.id",
    )
    .bind(authority)
    .bind(ids.to_vec())
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(IdentityLink {
                id: row.try_get("id")?,
                external_id: row.try_get("external_id")?,
                user_id: row.try_get("user_id")?,
                person_id: row.try_get("person_id")?,
                company_id: row.try_get("company_id")?,
            })
        })
        .collect()
}

async fn related_connections(
    connection: &mut PgConnection,
    conn: &Connection,
) -> Result<Vec<(String, String)>, Error> {
    let authority = conn.config.get("authority").and_then(Value::as_str).unwrap_or("");
    let rows = sqlx::query(
        "SELECT id,company_id FROM iam_connections WHERE config->>'authority'=$1",
    )
    .bind(authority)
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| Ok((row.try_get("id")?, row.try_get("company_id")?)))
        .collect()
}

async fn affected_positions(
    connection: &mut PgConnection,
    related: &[(String, String)],
    ids: &[String],
) -> Result<Vec<PositionFact>, Error> {
    let connections = related
        .iter()
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let rows = sqlx::query(
        "SELECT DISTINCT p.id,p.company_id,p.title FROM organization_positions p \
         JOIN people_position_packages b ON b.position_id=p.id AND b.company_id=p.company_id \
         JOIN people_package_revisions r ON r.package_id=b.package_id AND r.revision=b.revision \
         AND r.company_id=b.company_id \
         WHERE r.definition->>'connectionId'=ANY($1::text[]) AND \
         (EXISTS(SELECT 1 FROM jsonb_array_elements_text(r.definition->'directoryGroupIds') g \
          WHERE g=ANY($2::text[])) OR EXISTS(SELECT 1 FROM jsonb_array_elements(r.definition->'windowsPolicies') w \
          WHERE w->>'id'=ANY($2::text[]))) ORDER BY p.title",
    )
    .bind(connections)
    .bind(ids.to_vec())
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(PositionFact {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                title: row.try_get("title")?,
            })
        })
        .collect()
}

async fn company_names(
    connection: &mut PgConnection,
    company_ids: &[String],
) -> Result<BTreeMap<String, String>, Error> {
    let rows = sqlx::query(
        "SELECT company_id id,display_name name FROM organization_nodes \
         WHERE company_id=ANY($1::text[]) AND kind='company' ORDER BY company_id",
    )
    .bind(company_ids.to_vec())
    .fetch_all(&mut *connection)
    .await?;
    let mut names = BTreeMap::new();
    for row in rows {
        names.insert(row.try_get("id")?, row.try_get("name")?);
    }
    Ok(names)
}

async fn affected_people(
    connection: &mut PgConnection,
    identities: &[IdentityLink],
    positions: &[PositionFact],
) -> Result<Vec<String>, Error> {
    let user_ids = identities
        .iter()
        .filter_map(|i| i.user_id.clone())
        .collect::<Vec<_>>();
    let position_ids = positions.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
    let rows = sqlx::query(
        "SELECT DISTINCT p.legal_name FROM hr_people p \
         LEFT JOIN iam_members m ON m.person_id=p.id AND m.company_id=p.company_id \
         WHERE p.user_id=ANY($1::text[]) OR m.user_id=ANY($1::text[]) OR EXISTS(\
         SELECT 1 FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id \
         AND e.position_id=ANY($2::text[]) AND e.status='active') ORDER BY p.legal_name",
    )
    .bind(&user_ids)
    .bind(position_ids)
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| Ok(row.try_get::<String, _>("legal_name")?))
        .collect()
}

async fn preview_membership_claims(
    connection: &mut PgConnection,
    conn: &Connection,
    object_id: &str,
    member_ids: &[String],
) -> Result<Vec<(String, String)>, Error> {
    let rows = sqlx::query(
        "SELECT s.member_id,s.source_id FROM people_directory_membership_sources s \
         LEFT JOIN people_applied_packages ap ON ap.id=s.application_id \
         WHERE s.connection_id=$1 AND s.group_id=$2 AND s.member_id=ANY($3::text[]) \
         AND s.application_id IS NOT NULL AND ap.ended_at IS NULL",
    )
    .bind(&conn.id)
    .bind(object_id)
    .bind(member_ids.to_vec())
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| Ok((row.try_get("member_id")?, row.try_get("source_id")?)))
        .collect()
}

async fn protected_accounts(
    connection: &mut PgConnection,
    identities: &[IdentityLink],
    actor: &DirectoryOperationActor,
    now: i64,
) -> Result<bool, Error> {
    for identity in identities {
        let Some(user_id) = &identity.user_id else {
            continue;
        };
        let row = sqlx::query(
            "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$1 AND a.company_id=$2 \
             AND a.role='holding_owner' AND iam_assignment_active(a.id,$3)) OR EXISTS(\
             SELECT 1 FROM iam_members m WHERE m.user_id=$1 AND m.company_id=$2 \
             AND m.login_policy='emergency' AND m.status='active') protected,\
             iam_same_person($1,$4,$2) self",
        )
        .bind(user_id)
        .bind(&identity.company_id)
        .bind(now)
        .bind(&actor.user_id)
        .fetch_one(&mut *connection)
        .await?;
        if row.try_get::<bool, _>("protected")? || row.try_get::<bool, _>("self")? {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn direct_company_authority(
    connection: &mut PgConnection,
    company_id: &str,
    actor: &DirectoryOperationActor,
    now: i64,
) -> Result<bool, Error> {
    let row: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 \
         AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) \
         AND iam_role_allows(a.role,a.company_id,'iam.directory.apply')",
    )
    .bind(company_id)
    .bind(&actor.user_id)
    .bind(now)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.is_some())
}

async fn review_company_authority(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    now: i64,
) -> Result<bool, Error> {
    let row: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 \
         AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) \
         AND iam_role_allows(a.role,a.company_id,'iam.directory.manage')",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(now)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.is_some())
}

async fn current_review_authority(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    now: i64,
) -> Result<bool, Error> {
    let row: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 \
         AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) \
         AND iam_role_allows(a.role,a.company_id,'iam.review') \
         AND iam_role_allows(a.role,a.company_id,'iam.directory.manage')",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(now)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.is_some())
}

async fn same_person(
    connection: &mut PgConnection,
    left: &str,
    right: &str,
    company_id: &str,
) -> Result<bool, Error> {
    let row: Option<bool> = sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)")
        .bind(left)
        .bind(right)
        .bind(company_id)
        .fetch_optional(&mut *connection)
        .await?;
    Ok(row == Some(true))
}

async fn insert_review(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    kind: &str,
    payload: Value,
    reason: &str,
    now: i64,
) -> Result<String, Error> {
    let id = canonical_uuid();
    let mut payload = payload.as_object().cloned().unwrap_or_default();
    payload.insert(
        "authorizingAssignmentId".into(),
        Value::String(actor.assignment_id.clone()),
    );
    sqlx::query(
        "INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,reason) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(&id)
    .bind(&actor.company_id)
    .bind(kind)
    .bind(Value::Object(payload))
    .bind(&actor.user_id)
    .bind(&actor.user_id)
    .bind(now)
    .bind(reason)
    .execute(&mut *connection)
    .await?;
    Ok(id)
}

/// Pure simulation of the batch before any provider effect.
fn simulate(
    input: &DirectoryChangeInput,
    inventory: &DirectoryInventory,
    provider: &str,
) -> Result<(), BusinessError> {
    let mut simulated = inventory.clone();
    let actions = input.actions().collect::<Vec<_>>();
    for action in &actions {
        validate_object_target(action, &simulated)?;
        if provider == "univention"
            && matches!(
                action,
                DirectoryAction::ObjectMove { object_id, .. }
                    if simulated
                        .object(object_id)
                        .is_some_and(|object| object.kind == "ou")
                        && simulated
                            .objects
                            .iter()
                            .any(|object| object.parent_id.as_deref() == Some(object_id.as_str()))
            )
        {
            return Err(conflict(
                "DIRECTORY_CONTAINER_NOT_EMPTY",
                "UCS recreates populated containers when moving them. Move the children first, then move the empty container and return the children to preserve permanent identities.",
            ));
        }
        match action {
            DirectoryAction::ObjectMove {
                object_id,
                parent_id,
            } => {
                if let Some(object) = simulated
                    .objects
                    .iter_mut()
                    .find(|object| &object.id == object_id)
                {
                    object.parent_id = Some(parent_id.clone());
                }
            }
            DirectoryAction::GroupMembers {
                object_id,
                member_ids,
                action,
            } => {
                if let Some(group) = simulated
                    .objects
                    .iter_mut()
                    .find(|object| &object.id == object_id)
                {
                    if action == "add" {
                        for id in member_ids {
                            if !group.member_ids.contains(id) {
                                group.member_ids.push(id.clone());
                            }
                        }
                    } else {
                        group.member_ids.retain(|id| !member_ids.contains(id));
                    }
                }
            }
            _ => {}
        }
    }
    if actions.len() > 1
        && actions
            .iter()
            .any(|action| matches!(action, DirectoryAction::AccountEnable { .. } | DirectoryAction::AccountDisable { .. }))
    {
        return Err(conflict(
            "DIRECTORY_BATCH_DEPENDENCY",
            "Apply account activation or suspension separately to review its full impact.",
        ));
    }
    let mut targets = BTreeSet::new();
    for action in &actions {
        if let Some(id) = action.object_id() {
            if !targets.insert(id) {
                return Err(conflict(
                    "DIRECTORY_BATCH_DEPENDENCY",
                    "Apply one change per object, then refresh before dependent changes.",
                ));
            }
        }
    }
    Ok(())
}

/// Build the exact preview a reviewer sees. The transaction is read-only from
/// the provider's perspective: no command receipt and no operation row is
/// written here.
async fn prepare_preview(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    input: &DirectoryChangeInput,
    now: i64,
) -> Result<(PreparedPreview, Connection), Error> {
    let conn = connection_row(connection, actor, &input.connection_id, true).await?;
    let state = state(connection, &conn, false)
        .await?
        .ok_or_else(directory_test_required)?;
    if conn.version != input.expected_version {
        return Err(connection_changed().into());
    }
    let inventory = state
        .inventory
        .clone()
        .filter(|_| state.tested_version == Some(conn.version))
        .ok_or_else(directory_test_required)?;
    simulate(input, &inventory, &conn.provider)?;
    let actions = input.actions().collect::<Vec<_>>();
    let mut objects = Vec::new();
    let mut seen = BTreeSet::new();
    for action in &actions {
        for object in affected_objects(action, &inventory) {
            if seen.insert(object.id.clone()) {
                objects.push(object);
            }
        }
    }
    let mut ids = Vec::new();
    for object in &objects {
        ids.push(object.id.clone());
        if let Some(identity) = windows_identity(object) {
            ids.push(identity.to_owned());
        }
    }
    let identities = identity_links(connection, &conn, &ids).await?;
    if matches!(input.action, DirectoryAction::AccountEnable { .. })
        || matches!(
            input.action,
            DirectoryAction::AccountDisable { case_id: Some(_), .. }
        )
    {
        let lifecycle_value = lifecycle(connection, &input.action, actor, now)
            .await?
            .ok_or_else(verified_lifecycle_required)?;
        let action_id = input.action.object_id().unwrap_or_default();
        let person_id = json_str(&lifecycle_value, "person_id");
        let linked = identities.iter().any(|identity| {
            identity.external_id == action_id
                && identity.user_id.is_some()
                && identity.company_id == actor.company_id
                && identity.person_id.as_deref() == Some(person_id)
        });
        let verified =
            verified_lifecycle_account(connection, &conn, &input.action, actor, &lifecycle_value)
                .await?;
        if !linked && !verified || !verified {
            return Err(verified_lifecycle_required().into());
        }
        if matches!(input.action, DirectoryAction::AccountEnable { .. })
            && (!json_bool(&lifecycle_value, "active")
                || json_str(&lifecycle_value, "status") != "completed")
        {
            return Err(lifecycle_not_effective().into());
        }
        if matches!(input.action, DirectoryAction::AccountDisable { .. })
            && (!["offboarding", "suspend"].contains(&json_str(&lifecycle_value, "kind"))
                || !["scheduled", "completed"].contains(&json_str(&lifecycle_value, "status"))
                || json_i64(&lifecycle_value, "effective_at") > now)
        {
            return Err(lifecycle_not_effective().into());
        }
    }
    let related = related_connections(connection, &conn).await?;
    let positions = affected_positions(connection, &related, &ids).await?;
    let mut company_ids = BTreeSet::new();
    company_ids.insert(actor.company_id.clone());
    for identity in &identities {
        company_ids.insert(identity.company_id.clone());
    }
    for position in &positions {
        company_ids.insert(position.company_id.clone());
    }
    let company_ids = company_ids.into_iter().collect::<Vec<_>>();
    let names = company_names(connection, &company_ids).await?;
    if matches!(input.action, DirectoryAction::AccountDisable { .. }) {
        let target = input.action.object_id().unwrap_or_default();
        let target_object = inventory.object(target).ok_or_else(|| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "DIRECTORY_OBJECT_MISSING",
                "Refresh the directory inventory.",
            )
        })?;
        let aliases = [
            Some(target.to_owned()),
            windows_identity(target_object).map(str::to_owned),
        ];
        let accounts = identities
            .iter()
            .filter(|identity| {
                aliases
                    .iter()
                    .flatten()
                    .any(|alias| alias == &identity.external_id)
                    && identity.user_id.is_some()
            })
            .collect::<Vec<_>>();
        if protected_accounts(connection, &accounts.into_iter().cloned().collect::<Vec<_>>(), actor, now)
            .await?
        {
            return Err(protected_account().into());
        }
    }
    let people = affected_people(connection, &identities, &positions).await?;
    let mut warnings = Vec::new();
    for action in &actions {
        if let DirectoryAction::GroupMembers {
            object_id,
            member_ids,
            action,
        } = action
        {
            if action == "remove" {
                for (member, source) in
                    preview_membership_claims(connection, &conn, object_id, member_ids).await?
                {
                    warnings.push(format!(
                        "Membership {member} is supplied by applied package {source}."
                    ));
                }
            }
        }
    }
    let evidence = PreviewEvidence {
        connection_version: conn.version,
        state_version: state.version,
        inventory: inventory.clone(),
        objects: ids,
        companies: company_ids.clone(),
        identity_links: identities
            .iter()
            .map(|identity| {
                (
                    identity.id.clone(),
                    identity.user_id.clone(),
                    identity.company_id.clone(),
                )
            })
            .collect(),
    };
    let token = preview_token(input, &evidence);
    let companies = company_ids
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "name": names.get(id).cloned().unwrap_or_else(|| id.clone()),
            })
        })
        .collect::<Vec<_>>();
    let computer_names = objects
        .iter()
        .filter(|object| object.kind == "computer")
        .map(|object| object.name.clone())
        .collect::<Vec<_>>();
    if !state.write_enabled {
        warnings.push(
            "Read-only connection: IT can preview work; enable tested write-back before submission."
                .into(),
        );
    }
    if matches!(input.action, DirectoryAction::ObjectMove { .. }) {
        warnings.push("Moving an OU placement changes inherited Windows policies.".into());
    }
    if matches!(input.action, DirectoryAction::AccountDisable { .. }) && company_ids.len() > 1 {
        warnings.push("Disabling this directory account affects every listed Company.".into());
    }
    if matches!(input.action, DirectoryAction::GpoPublish { .. }) {
        warnings.push(
            "Published GPO configuration is separate from confirmed results on endpoints.".into(),
        );
    }
    let result = json!({
        "token": token,
        "objects": objects.iter().map(|object| format!("{} · {}", object.name, object.kind)).collect::<Vec<_>>(),
        "companies": companies,
        "people": people,
        "computers": computer_names,
        "positions": positions.iter().map(|position| position.title.clone()).collect::<Vec<_>>(),
        "warnings": warnings,
    });
    Ok((
        PreparedPreview {
            token,
            result,
            evidence,
            state_version: state.version,
            write_enabled: state.write_enabled,
        },
        conn,
    ))
}

fn job_from_row(row: &sqlx::postgres::PgRow) -> Result<Job, Error> {
    let definition: Value = row.try_get("definition")?;
    let evidence: Value = row.try_get("evidence")?;
    let steps: Option<Value> = row.try_get("steps").ok();
    Ok(Job {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        connection_id: row.try_get("connection_id")?,
        connection_version: row.try_get("connection_version")?,
        state_version: row.try_get("state_version")?,
        status: row.try_get("status")?,
        definition: serde_json::from_value(definition).map_err(|_| unavailable_row())?,
        evidence: evidence_from_value(evidence),
        review_id: optional_string(row, "review_id")?,
        created_by: row.try_get("created_by")?,
        assignment_id: row.try_get("assignment_id")?,
        approval_mode: optional_string(row, "approval_mode")?.unwrap_or_else(|| "reviewed".into()),
        mfa_session_id: optional_string(row, "mfa_session_id")?,
        steps: steps
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default(),
        provision_id: optional_string(row, "provision_id")?,
    })
}

fn evidence_from_value(value: Value) -> PreviewEvidence {
    let objects = value
        .get("objects")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let companies = value
        .get("companies")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let identity_links = value
        .get("identityLinks")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    Some((
                        row.first()?.as_str()?.to_owned(),
                        row.get(1).and_then(Value::as_str).map(str::to_owned),
                        row.get(2)?.as_str()?.to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    PreviewEvidence {
        connection_version: json_i64(&value, "connectionVersion"),
        state_version: json_i64(&value, "stateVersion"),
        inventory: DirectoryInventory::checked(
            value.get("inventory").cloned().unwrap_or(Value::Null),
        )
        .unwrap_or(DirectoryInventory {
            objects: vec![],
            fields: vec![],
            provider_version: String::new(),
            observed_at: 0,
            complete: true,
        }),
        objects,
        companies,
        identity_links,
    }
}

/// Directory automation overview for the technology/people Directory screen.
pub async fn directory_automation_data(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    allowed(connection, actor, READ_PERMISSION, now).await?;
    let connections = sqlx::query(
        "SELECT c.*,s.version state_version,s.tested_version automation_tested,s.write_enabled,\
         s.inventory,s.definition FROM iam_connections c \
         LEFT JOIN people_directory_state s ON s.connection_id=c.id \
         WHERE c.company_id=$1 AND c.status IN ('draft','tested','active') \
         AND c.provider IN ('univention','active_directory') ORDER BY c.name",
    )
    .bind(&actor.company_id)
    .fetch_all(&mut *connection)
    .await?;
    let connections = connections
        .into_iter()
        .map(|row| {
            let inventory: Option<Value> = row.try_get("inventory")?;
            let definition: Option<Value> = row.try_get("definition")?;
            let mappings = definition
                .as_ref()
                .and_then(|value| value.get("mappings"))
                .cloned()
                .unwrap_or_else(|| json!([]));
            Ok(json!({
                "id": row.try_get::<String,_>("id")?,
                "name": row.try_get::<String,_>("name")?,
                "provider": row.try_get::<String,_>("provider")?,
                "version": row.try_get::<i64,_>("version")?,
                "writeEnabled": row.try_get::<Option<bool>,_>("write_enabled")?.unwrap_or(false),
                "testedVersion": row.try_get::<Option<i64>,_>("automation_tested")?,
                "inventory": inventory.filter(|value| !value.is_null()),
                "mappings": mappings,
            }))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let jobs = sqlx::query(
        "SELECT id,connection_id,status,version,definition,last_error,result,review_id,created_at,\
         approval_mode,steps FROM people_directory_operations WHERE company_id=$1 \
         ORDER BY created_at DESC LIMIT 200",
    )
    .bind(&actor.company_id)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        Ok(json!({
            "id": row.try_get::<String,_>("id")?,
            "connection_id": row.try_get::<String,_>("connection_id")?,
            "status": row.try_get::<String,_>("status")?,
            "version": row.try_get::<i64,_>("version")?,
            "definition": row.try_get::<Value,_>("definition")?,
            "last_error": optional_string(&row, "last_error")?,
            "result": row.try_get::<Option<Value>,_>("result")?,
            "review_id": optional_string(&row, "review_id")?,
            "created_at": row.try_get::<i64,_>("created_at")?,
            "approval_mode": optional_string(&row, "approval_mode")?,
            "steps": row.try_get::<Option<Value>,_>("steps")?,
        }))
    })
    .collect::<Result<Vec<_>, Error>>()?;
    let cases = sqlx::query(
        "SELECT c.id,p.legal_name name,c.kind,c.effective_at,\
         ARRAY(SELECT i.external_id FROM iam_identities i \
         JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id \
         WHERE m.person_id=c.person_id AND m.company_id=c.company_id AND m.status='active') account_ids \
         FROM people_lifecycle_cases c JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id \
         WHERE c.company_id=$1 AND c.status IN ('scheduled','completed') \
         ORDER BY c.updated_at DESC LIMIT 200",
    )
    .bind(&actor.company_id)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| {
        Ok(json!({
            "id": row.try_get::<String,_>("id")?,
            "name": row.try_get::<String,_>("name")?,
            "kind": row.try_get::<String,_>("kind")?,
            "effectiveAt": row.try_get::<i64,_>("effective_at")?,
            "accountIds": row.try_get::<Vec<String>,_>("account_ids")?,
        }))
    })
    .collect::<Result<Vec<_>, Error>>()?;
    Ok(json!({
        "cases": cases,
        "connections": connections,
        "jobs": jobs,
        "canApply": actor.has(APPLY_PERMISSION),
        "canManage": actor.has(MANAGE_PERMISSION),
        "canReview": actor.has(REVIEW_PERMISSION),
    }))
}

pub async fn refresh_inventory(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    connector: &dyn DirectoryOperationConnector,
    id: &str,
    version: i64,
    key: &str,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    allowed(connection, actor, MANAGE_PERMISSION, now).await?;
    let request = json!({"id": id, "version": version});
    if let Some(result) = receipt(
        connection,
        key,
        "directory.inventory.read",
        actor,
        &sha256(&request),
        id,
    )
    .await?
    {
        return Ok(result);
    }
    let conn = connection_row(connection, actor, id, true).await?;
    if conn.version != version {
        return Err(connection_changed().into());
    }
    let inventory = connector.inventory(&conn).await?;
    let current = connection_row(connection, actor, id, true).await?;
    if current.version != version {
        return Err(connection_changed().into());
    }
    sqlx::query(
        "INSERT INTO people_directory_state(connection_id,company_id,tested_version,inventory,observed_at) \
         VALUES($1,$2,$3,$4,$5) ON CONFLICT(connection_id) DO UPDATE SET \
         tested_version=excluded.tested_version,inventory=excluded.inventory,observed_at=excluded.observed_at,\
         write_enabled=CASE WHEN people_directory_state.tested_version=excluded.tested_version \
         AND people_directory_state.inventory->>'providerVersion'=excluded.inventory->>'providerVersion' \
         THEN people_directory_state.write_enabled ELSE false END",
    )
    .bind(&conn.id)
    .bind(&actor.company_id)
    .bind(version)
    .bind(json!(inventory))
    .bind(now)
    .execute(&mut *connection)
    .await?;
    let result = json!({
        "count": inventory.objects.len(),
        "providerVersion": inventory.provider_version,
    });
    record_receipt(
        connection,
        key,
        "directory.inventory.read",
        actor,
        &sha256(&request),
        id,
        &result,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn configure_automation(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    input: AutomationConfiguration,
    key: &str,
    fresh: bool,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    allowed(connection, actor, MANAGE_PERMISSION, now).await?;
    if !fresh {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "FRESH_MFA_REQUIRED",
            "Verify MFA before configuring Directory write-back.",
        )
        .into());
    }
    if input.expected_version < 1 {
        return Err(request_invalid().into());
    }
    let request = serde_json::to_value(&input).map_err(|_| request_invalid())?;
    if let Some(result) = receipt(
        connection,
        key,
        "directory.automation.configure",
        actor,
        &sha256(&request),
        &input.connection_id,
    )
    .await?
    {
        return Ok(result);
    }
    let conn = connection_row(connection, actor, &input.connection_id, false).await?;
    let current = state(connection, &conn, true)
        .await?
        .ok_or_else(directory_test_required)?;
    if conn.version != input.expected_version
        || current.inventory.is_none()
        || current.tested_version != Some(conn.version)
    {
        return Err(directory_test_required().into());
    }
    let mappings = input
        .mappings
        .clone()
        .unwrap_or_else(|| current.mappings());
    if mappings
        .iter()
        .map(|mapping| mapping.attribute_id.clone())
        .collect::<BTreeSet<_>>()
        .len()
        != mappings.len()
        || mappings
            .iter()
            .map(|mapping| mapping.field.clone())
            .collect::<BTreeSet<_>>()
            .len()
            != mappings.len()
    {
        return Err(invalid(
            "MAPPING_DUPLICATE",
            "Map each local and external field once.",
        )
        .into());
    }
    let inventory = current.inventory.as_ref().ok_or_else(directory_test_required)?;
    for mapping in &mappings {
        let row = sqlx::query("SELECT definition FROM um_attributes WHERE company_id=$1 AND id=$2")
            .bind(&actor.company_id)
            .bind(&mapping.attribute_id)
            .fetch_optional(&mut *connection)
            .await?;
        let Some(row) = row else {
            return Err(invalid(
                "MAPPING_ATTRIBUTE_INVALID",
                "Choose an active compatible person field.",
            )
            .into());
        };
        let definition: Value = row.try_get("definition")?;
        if definition.get("target").and_then(Value::as_str) != Some("person")
            || definition
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            || !matches!(
                definition.get("type").and_then(Value::as_str),
                Some("text") | Some("checkbox")
            )
        {
            return Err(invalid(
                "MAPPING_ATTRIBUTE_INVALID",
                "Choose an active compatible person field.",
            )
            .into());
        }
        let (can_view, can_edit) = attribute_access(&definition, actor);
        if !can_view
            || (mapping.direction == "read"
                && (!can_edit || !actor.has(PEOPLE_MANAGE_PERMISSION)))
        {
            return Err(forbidden(
                "MAPPING_FIELD_FORBIDDEN",
                "The mapping requires permission to view or update the owned field.",
            )
            .into());
        }
        let supported = inventory.fields.iter().any(|field| {
            field.name == mapping.field
                && match mapping.direction.as_str() {
                    "read" => field.read,
                    _ => field.write,
                }
                && field.kind == definition.get("type").and_then(Value::as_str).unwrap_or("")
        });
        if !supported {
            return Err(invalid(
                "MAPPING_SCHEMA_UNSUPPORTED",
                "This connector schema does not support the field and direction.",
            )
            .into());
        }
    }
    sqlx::query(
        "UPDATE people_directory_state SET write_enabled=$1,definition=$2,version=version+1 \
         WHERE connection_id=$3",
    )
    .bind(input.write_enabled)
    .bind(json!({"mappings": mappings}))
    .bind(&conn.id)
    .execute(&mut *connection)
    .await?;
    let result = json!({"writeEnabled": input.write_enabled});
    record_receipt(
        connection,
        key,
        "directory.automation.configure",
        actor,
        &sha256(&request),
        &input.connection_id,
        &result,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn preview_change(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    input: DirectoryChangeInput,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    allowed(connection, actor, MANAGE_PERMISSION, now).await?;
    let input = input.validate()?;
    let (preview, _) = prepare_preview(connection, actor, &input, now).await?;
    Ok(preview.result)
}

pub async fn submit_change(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    input: DirectoryChangeInput,
    token: &str,
    key: &str,
    direct: bool,
    fresh: bool,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    allowed(
        connection,
        actor,
        if direct {
            APPLY_PERMISSION
        } else {
            MANAGE_PERMISSION
        },
        now,
    )
    .await?;
    if !fresh {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "FRESH_MFA_REQUIRED",
            "Verify MFA before submitting a Directory operation.",
        )
        .into());
    }
    let input = input.validate()?;
    // Inventory is part of the reviewed evidence, but the provider read is
    // never taken here: submit only accepts the exact tested snapshot.
    let operation = if direct {
        "directory.operation.apply"
    } else {
        "directory.operation.propose"
    };
    let request = serde_json::to_value(&input).map_err(|_| request_invalid())?;
    if let Some(result) = receipt(
        connection,
        key,
        operation,
        actor,
        &sha256(&request),
        &input.connection_id,
    )
    .await?
    {
        return Ok(result);
    }
    let (preview, conn) = prepare_preview(connection, actor, &input, now).await?;
    if preview.token != token {
        return Err(preview_changed().into());
    }
    if !preview.write_enabled || conn.status != "active" {
        return Err(read_only().into());
    }
    if matches!(input.action, DirectoryAction::AccountDisable { .. })
        && preview
            .result
            .get("companies")
            .and_then(Value::as_array)
            .is_some_and(|companies| {
                companies.iter().any(|company| {
                    company
                        .get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| !input.affected_company_ids.iter().any(|ack| ack == id))
                })
            })
    {
        return Err(impact_ack_required().into());
    }
    if direct {
        for company_id in &preview.evidence.companies {
            if !direct_company_authority(connection, company_id, actor, now).await? {
                return Err(apply_scope().into());
            }
        }
    }
    let id = canonical_uuid();
    let review_id = if direct {
        None
    } else {
        Some(
            insert_review(
                connection,
                actor,
                "directory.operation",
                json!({"operationId": id}),
                &input.reason,
                now,
            )
            .await?,
        )
    };
    sqlx::query(
        "INSERT INTO people_directory_operations(id,company_id,connection_id,connection_version,\
         state_version,status,definition,evidence,review_id,created_by,assignment_id,created_at,\
         updated_at,approval_mode,mfa_session_id) \
         VALUES($1,$2,$3,$4,$5,$9,$6,$7,$8,$10,$11,$12,$12,$13,$14)",
    )
    .bind(&id)
    .bind(&actor.company_id)
    .bind(&conn.id)
    .bind(conn.version)
    .bind(preview.state_version)
    .bind(json!(input))
    .bind(preview.evidence.value())
    .bind(&review_id)
    .bind(if direct { "queued" } else { "pending_review" })
    .bind(&actor.user_id)
    .bind(&actor.assignment_id)
    .bind(now)
    .bind(if direct { "direct" } else { "reviewed" })
    .bind(if direct {
        Some(actor.session_id.clone())
    } else {
        None
    })
    .execute(&mut *connection)
    .await?;
    let result = json!({
        "id": id,
        "reviewId": review_id,
        "status": if direct { "queued" } else { "pending_review" },
    });
    record_receipt(
        connection,
        key,
        operation,
        actor,
        &sha256(&request),
        &conn.id,
        &result,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn apply_operation_review(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    payload: &Value,
    decision: &str,
    key: &str,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    allowed(connection, actor, MANAGE_PERMISSION, now).await?;
    let operation_id = payload
        .get("operationId")
        .and_then(Value::as_str)
        .ok_or_else(version_conflict)?;
    let request = json!({"payload": payload, "decision": decision});
    if let Some(result) = receipt(
        connection,
        key,
        "directory.operation.review",
        actor,
        &sha256(&request),
        operation_id,
    )
    .await?
    {
        return Ok(result);
    }
    let row = sqlx::query(
        "SELECT * FROM people_directory_operations WHERE id=$1 AND company_id=$2 \
         AND status='pending_review' FOR UPDATE",
    )
    .bind(operation_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *connection)
    .await?;
    let job = row.as_ref().map(job_from_row).transpose()?.ok_or_else(version_conflict)?;
    if !matches!(decision, "approve" | "reject") {
        return Err(request_invalid().into());
    }
    if decision == "approve" {
        allowed(connection, actor, MANAGE_PERMISSION, now).await?;
        let (current, _) = prepare_preview(connection, actor, &job.definition, now).await?;
        if sha256(&current.evidence.value()) != sha256(&job.evidence.value())
            || !current.write_enabled
        {
            return Err(preview_changed().into());
        }
        if matches!(
            job.definition.action,
            DirectoryAction::AccountEnable { .. } | DirectoryAction::GroupMembers { .. }
        ) {
            for (_, user_id, company_id) in &current.evidence.identity_links {
                if let Some(user_id) = user_id {
                    if same_person(connection, user_id, &actor.user_id, company_id).await? {
                        return Err(beneficiary_review().into());
                    }
                }
            }
        }
        for company_id in &current.evidence.companies {
            if company_id != &actor.company_id
                && !review_company_authority(connection, company_id, &actor.user_id, now).await?
            {
                return Err(review_scope().into());
            }
        }
    }
    sqlx::query(
        "UPDATE people_directory_operations SET status=$1,version=version+1,updated_at=$2 WHERE id=$3",
    )
    .bind(if decision == "approve" {
        "queued"
    } else {
        "rejected"
    })
    .bind(now)
    .bind(&job.id)
    .execute(&mut *connection)
    .await?;
    let result = json!({"id": job.id, "status": if decision == "approve" { "queued" } else { "rejected" }});
    record_receipt(
        connection,
        key,
        "directory.operation.review",
        actor,
        &sha256(&request),
        operation_id,
        &result,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn retry_operation(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    id: &str,
    version: i64,
    key: &str,
    fresh: bool,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    allowed(connection, actor, APPLY_PERMISSION, now).await?;
    if !fresh {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "FRESH_MFA_REQUIRED",
            "Verify MFA before resuming a Directory operation.",
        )
        .into());
    }
    let request = json!({"id": id, "version": version});
    if let Some(result) = receipt(
        connection,
        key,
        "directory.operation.retry",
        actor,
        &sha256(&request),
        id,
    )
    .await?
    {
        return Ok(result);
    }
    let row = sqlx::query(
        "UPDATE people_directory_operations SET status='queued',mfa_session_id=$5,next_attempt_at=0,\
         version=version+1 WHERE id=$1 AND company_id=$2 AND version=$3 AND created_by=$4 \
         AND approval_mode='direct' AND (status='retry' OR status='blocked' \
         AND last_error='FRESH_MFA_REQUIRED') RETURNING provision_id",
    )
    .bind(id)
    .bind(&actor.company_id)
    .bind(version)
    .bind(&actor.user_id)
    .bind(&actor.session_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Err(not_found_operation().into());
    };
    let provision_id: Option<String> = row.try_get("provision_id")?;
    if provision_id.is_some() {
        sqlx::query(
            "UPDATE people_provision_operations SET status='queued',session_id=$2,last_error=NULL \
             WHERE id=$1 AND status IN ('blocked','running')",
        )
        .bind(provision_id)
        .bind(&actor.session_id)
        .execute(&mut *connection)
        .await?;
    }
    let result = json!({"id": id, "status": "queued"});
    record_receipt(
        connection,
        key,
        "directory.operation.retry",
        actor,
        &sha256(&request),
        id,
        &result,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn cancel_operation(
    connection: &mut PgConnection,
    actor: &DirectoryOperationActor,
    id: &str,
    version: i64,
    key: &str,
    now: i64,
) -> Result<Value, Error> {
    ready(connection).await?;
    allowed(connection, actor, MANAGE_PERMISSION, now).await?;
    let request = json!({"id": id, "version": version});
    if let Some(result) = receipt(
        connection,
        key,
        "directory.operation.cancel",
        actor,
        &sha256(&request),
        id,
    )
    .await?
    {
        return Ok(result);
    }
    let row = sqlx::query(
        "UPDATE people_directory_operations SET status='cancelled',version=version+1,updated_at=$1 \
         WHERE id=$2 AND company_id=$3 AND version=$4 AND attempts=0 \
         AND status IN ('pending_review','queued','blocked') RETURNING review_id",
    )
    .bind(now)
    .bind(id)
    .bind(&actor.company_id)
    .bind(version)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Err(operation_unavailable().into());
    };
    if let Some(review_id) = optional_string(&row, "review_id")? {
        sqlx::query("UPDATE iam_reviews SET status='failed',version=version+1 WHERE id=$1 AND status='pending'")
            .bind(review_id)
            .execute(&mut *connection)
            .await?;
    }
    let result = json!({"id": id, "status": "cancelled"});
    record_receipt(
        connection,
        key,
        "directory.operation.cancel",
        actor,
        &sha256(&request),
        id,
        &result,
        now,
    )
    .await?;
    Ok(result)
}

async fn due_operations(
    connection: &mut PgConnection,
    now: i64,
    limit: i64,
) -> Result<Vec<(String, String)>, Error> {
    let rows = sqlx::query(
        "SELECT o.id,o.connection_id FROM people_directory_operations o \
         LEFT JOIN iam_reviews r ON r.id=o.review_id \
         WHERE (o.status IN ('queued','retry','running') AND o.next_attempt_at<=$1) \
         OR (o.status='pending_review' AND r.status='approved') \
         ORDER BY o.created_at LIMIT $2",
    )
    .bind(now)
    .bind(limit)
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| Ok((row.try_get("id")?, row.try_get("connection_id")?)))
        .collect()
}

async fn load_job(
    connection: &mut PgConnection,
    id: &str,
) -> Result<Option<Job>, Error> {
    let row = sqlx::query("SELECT * FROM people_directory_operations WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?;
    row.as_ref().map(job_from_row).transpose()
}

async fn fail_operation(
    connection: &mut PgConnection,
    job: &Job,
    retry: bool,
    code: &str,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "UPDATE people_directory_operations SET status=$1,last_error=$2,\
         next_attempt_at=$3+LEAST(300000,1000*power(2,LEAST(attempts,8))),version=version+1,\
         updated_at=$3 WHERE id=$4 AND status NOT IN ('completed','cancelled','rejected')",
    )
    .bind(if retry { "retry" } else { "blocked" })
    .bind(code)
    .bind(now)
    .bind(&job.id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Drain due operations under the connection advisory lock. The caller owns
/// the transaction; every provider effect is verified by readback.
pub async fn drain_operations(
    connection: &mut PgConnection,
    connector: &dyn DirectoryOperationConnector,
    freshness: &dyn DirectoryDirectFreshness,
    now: i64,
    limit: i64,
) -> Result<usize, Error> {
    ready(connection).await?;
    let due = due_operations(connection, now, limit).await?;
    let mut completed = 0usize;
    for (id, _) in due {
        let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtext($1))")
            .bind(format!("people-directory:{id}"))
            .fetch_one(&mut *connection)
            .await?;
        if !locked {
            continue;
        }
        let outcome = drain_one(connection, connector, freshness, &id, now).await;
        let _ = sqlx::query("SELECT pg_advisory_unlock(hashtext($1))")
            .bind(format!("people-directory:{id}"))
            .fetch_optional(&mut *connection)
            .await;
        match outcome {
            Ok(true) => completed += 1,
            Ok(false) => {}
            Err(error) => {
                let (retry, code) = match &error {
                    PgTransactionError::Business(error) => (
                        ["DIRECTORY_UNAVAILABLE", "DIRECTORY_REQUEST_FAILED", "DIRECTORY_READBACK_FAILED"]
                            .contains(&error.code),
                        error.code,
                    ),
                    PgTransactionError::Persistence(_) => (false, "DIRECTORY_UNAVAILABLE"),
                };
                if let Some(job) = load_job(connection, &id).await? {
                    fail_operation(connection, &job, retry, code, now).await?;
                }
            }
        }
    }
    Ok(completed)
}

async fn drain_one(
    connection: &mut PgConnection,
    connector: &dyn DirectoryOperationConnector,
    freshness: &dyn DirectoryDirectFreshness,
    id: &str,
    now: i64,
) -> Result<bool, Error> {
    let Some(job) = load_job(connection, id).await? else {
        return Ok(false);
    };
    if job.status == "pending_review" {
        let approved: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM iam_reviews WHERE id=$1 AND status='approved'")
                .bind(job.review_id.as_deref())
                .fetch_optional(&mut *connection)
                .await?;
        if approved.is_none() {
            return Ok(false);
        }
        sqlx::query(
            "UPDATE people_directory_operations SET status='queued',version=version+1,updated_at=$1 \
             WHERE id=$2 AND status='pending_review'",
        )
        .bind(now)
        .bind(&job.id)
        .execute(&mut *connection)
        .await?;
    } else if !matches!(job.status.as_str(), "queued" | "retry" | "running") {
        return Ok(false);
    }
    let mut company = actor_for(&job.company_id, &job.created_by, &job.assignment_id);
    company.session_id = job.mfa_session_id.clone().unwrap_or_default();
    if job.provision_id.is_some() {
        let operation = sqlx::query("SELECT status FROM people_provision_operations WHERE id=$1")
            .bind(job.provision_id.as_deref())
            .fetch_optional(&mut *connection)
            .await?;
        match operation {
            Some(row) => {
                let status: String = row.try_get("status")?;
                if !matches!(status.as_str(), "queued" | "running" | "retry") {
                    return Err(conflict(
                        "PROVISIONING_CHANGED",
                        "This provisioning operation is no longer executable.",
                    )
                    .into());
                }
            }
            None => {
                return Err(conflict(
                    "PROVISIONING_CHANGED",
                    "This provisioning operation is no longer executable.",
                )
                .into())
            }
        }
    }
    let conn = connection_row(connection, &company, &job.connection_id, false).await?;
    let current_state = state(connection, &conn, false)
        .await?
        .ok_or_else(unavailable_row)?;
    let allowed_execution: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM assignments a WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 \
         AND iam_assignment_active(a.id,$4) AND iam_role_allows(a.role,a.company_id,$5)",
    )
    .bind(&job.assignment_id)
    .bind(&job.created_by)
    .bind(&job.company_id)
    .bind(now)
    .bind(if job.approval_mode == "direct" {
        "iam.directory.apply"
    } else {
        "iam.directory.manage"
    })
    .fetch_optional(&mut *connection)
    .await?;
    if allowed_execution.is_none()
        || !current_state.write_enabled
        || conn.version != job.connection_version
        || current_state.version != job.state_version
    {
        return Err(conflict(
            "DIRECTORY_AUTHORITY_CHANGED",
            "The authorizing role or connector configuration changed.",
        )
        .into());
    }
    let impact = prepare_preview(connection, &company, &job.definition, now).await?;
    if job.approval_mode == "direct" {
        if !freshness
            .fresh(
                &job.created_by,
                job.mfa_session_id.as_deref().unwrap_or_default(),
                now,
            )
            .await?
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Forbidden,
                "FRESH_MFA_REQUIRED",
                "Verify MFA before resuming this Directory operation.",
            )
            .into());
        }
        for company_id in &impact.0.evidence.companies {
            let authority: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 \
                 AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) \
                 AND iam_role_allows(a.role,a.company_id,'iam.directory.apply')",
            )
            .bind(company_id)
            .bind(&job.created_by)
            .bind(now)
            .fetch_optional(&mut *connection)
            .await?;
            if authority.is_none() {
                return Err(apply_scope().into());
            }
        }
    } else {
        let reviewed_by: Option<String> = job.review_id.as_deref().map(|_| ()).and_then(|_| None);
        let _ = reviewed_by;
        let review = sqlx::query(
            "SELECT reviewed_by FROM iam_reviews WHERE id=$1 AND status='approved'",
        )
        .bind(job.review_id.as_deref())
        .fetch_optional(&mut *connection)
        .await?;
        let Some(review) = review else {
            return Err(conflict(
                "DIRECTORY_REVIEW_AUTHORITY_CHANGED",
                "The directory reviewer's authority changed.",
            )
            .into());
        };
        let reviewer: String = review.try_get("reviewed_by")?;
        for company_id in &impact.0.evidence.companies {
            if !current_review_authority(connection, company_id, &reviewer, now).await? {
                return Err(conflict(
                    "DIRECTORY_REVIEW_AUTHORITY_CHANGED",
                    "The directory reviewer's authority changed.",
                )
                .into());
            }
        }
    }
    if sha256(&json!(impact.0.evidence.identity_links))
        != sha256(&json!(job.evidence.identity_links))
        || sha256(&json!(impact.0.evidence.companies)) != sha256(&json!(job.evidence.companies))
    {
        return Err(conflict(
            "DIRECTORY_IDENTITY_CHANGED",
            "Account links or affected Companies changed after review.",
        )
        .into());
    }
    sqlx::query(
        "UPDATE people_directory_operations SET status='running',attempts=attempts+1,\
         version=version+1,updated_at=$1 WHERE id=$2",
    )
    .bind(now)
    .bind(&job.id)
    .execute(&mut *connection)
    .await?;
    let actions = job
        .definition
        .actions()
        .cloned()
        .collect::<Vec<DirectoryAction>>();
    let mut steps = job.steps.clone();
    for (index, action) in actions.iter().enumerate() {
        if steps.iter().any(|step| {
            step.get("index").and_then(Value::as_u64) == Some(index as u64)
                && step.get("status").and_then(Value::as_str) == Some("completed")
        }) {
            continue;
        }
        let mut executable = action.clone();
        let mut retained_member_ids: Vec<String> = vec![];
        let mut replaced_source_ids: Vec<String> = vec![];
        if let DirectoryAction::GroupMembers {
            object_id,
            member_ids,
            action: membership,
        } = action
        {
            if membership == "remove" {
                let sources = membership_sources(connection, &conn, object_id, member_ids).await?;
                if let Some(provision_id) = &job.provision_id {
                    let provision = sqlx::query(
                        "SELECT evidence FROM people_provision_operations WHERE id=$1",
                    )
                    .bind(provision_id)
                    .fetch_optional(&mut *connection)
                    .await?;
                    if let Some(provision) = provision {
                        let evidence: Value = provision.try_get("evidence")?;
                        replaced_source_ids = provision_old_ids(&evidence);
                        let replaced = replaced_source_ids.iter().cloned().collect::<BTreeSet<_>>();
                        retained_member_ids = member_ids
                            .iter()
                            .filter(|member| {
                                let claims = sources
                                    .iter()
                                    .filter(|(m, _, _, _)| m == *member)
                                    .collect::<Vec<_>>();
                                !claims.is_empty()
                                    && claims.iter().any(|(_, application, _, _)| {
                                        application
                                            .as_ref()
                                            .is_none_or(|id| !replaced.contains(id))
                                    })
                            })
                            .cloned()
                            .collect();
                        executable = DirectoryAction::GroupMembers {
                            object_id: object_id.clone(),
                            member_ids: member_ids
                                .iter()
                                .filter(|member| !retained_member_ids.contains(member))
                                .cloned()
                                .collect(),
                            action: membership.clone(),
                        };
                    }
                } else if sources.iter().any(|(_, _, package, gpo)| *package || *gpo) {
                    return Err(membership_source_retained().into());
                }
            }
        }
        let result = match &executable {
            DirectoryAction::GroupMembers {
                member_ids,
                action: membership,
                ..
            } if membership == "remove" && member_ids.is_empty() => json!({
                "verified": true,
                "retainedMemberIds": retained_member_ids,
            }),
            _ => {
                let operation_id = if actions.len() == 1 {
                    job.id.clone()
                } else {
                    format!("{}:{}", job.id, index)
                };
                let applied = connector
                    .apply(&conn, &operation_id, &executable, &job.evidence.inventory)
                    .await?;
                if applied.get("verified").and_then(Value::as_bool) != Some(true) {
                    return Err(readback_failed().into());
                }
                applied
            }
        };
        steps.push(json!({"index": index, "status": "completed", "result": result}));
        let withdraws = job.provision_id.is_some()
            && matches!(
                action,
                DirectoryAction::GroupMembers {
                    action,
                    ..
                } if action == "remove"
            );
        if withdraws {
            if let DirectoryAction::GroupMembers {
                object_id,
                member_ids,
                ..
            } = action
            {
                sqlx::query(
                    "DELETE FROM people_directory_membership_sources WHERE connection_id=$1 \
                     AND group_id=$2 AND member_id=ANY($3::text[]) AND application_id=ANY($4::text[])",
                )
                .bind(&conn.id)
                .bind(object_id)
                .bind(member_ids.clone())
                .bind(replaced_source_ids.clone())
                .execute(&mut *connection)
                .await?;
            }
        }
        sqlx::query("UPDATE people_directory_operations SET steps=$2 WHERE id=$1")
            .bind(&job.id)
            .bind(json!(steps))
            .execute(&mut *connection)
            .await?;
    }
    let result = {
        let last = steps
            .last()
            .and_then(|step| step.get("result"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mut result = last.as_object().cloned().unwrap_or_default();
        result.insert("verified".into(), Value::Bool(true));
        result.insert("endpointConfirmed".into(), Value::Bool(false));
        result.insert("steps".into(), Value::Array(steps.clone()));
        Value::Object(result)
    };
    sqlx::query(
        "UPDATE people_directory_operations SET status='completed',result=$1,last_error=NULL,\
         version=version+1,updated_at=$2 WHERE id=$3",
    )
    .bind(&result)
    .bind(now)
    .bind(&job.id)
    .execute(&mut *connection)
    .await?;
    if let Some(provision_id) = &job.provision_id {
        let provision = sqlx::query("SELECT evidence FROM people_provision_operations WHERE id=$1")
            .bind(provision_id)
            .fetch_optional(&mut *connection)
            .await?;
        if let Some(provision) = provision {
            let evidence: Value = provision.try_get("evidence")?;
            for action in &actions {
                if let DirectoryAction::GroupMembers {
                    object_id,
                    member_ids,
                    action: membership,
                } = action
                {
                    if membership != "add" {
                        continue;
                    }
                    for plan in provision_plans(&evidence) {
                        if plan.connection_id == conn.id
                            && plan.group_ids.contains(object_id)
                        {
                            for member in member_ids
                                .iter()
                                .filter(|member| plan.directory_members.contains(member))
                            {
                                let application_id = format!("{provision_id}:{}", plan.id);
                                sqlx::query(
                                    "INSERT INTO people_directory_membership_sources VALUES($1,$2,$3,$4,$4,$5) \
                                     ON CONFLICT DO NOTHING",
                                )
                                .bind(&conn.id)
                                .bind(object_id)
                                .bind(member)
                                .bind(application_id)
                                .bind(now)
                                .execute(&mut *connection)
                                .await?;
                            }
                        }
                    }
                }
            }
        }
    } else {
        for action in &actions {
            if let DirectoryAction::GroupMembers {
                object_id,
                member_ids,
                action: membership,
            } = action
            {
                if membership != "add" {
                    continue;
                }
                for member in member_ids {
                    sqlx::query(
                        "INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) \
                         VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
                    )
                    .bind(&conn.id)
                    .bind(object_id)
                    .bind(member)
                    .bind(format!("direct:{}", job.id))
                    .bind(now)
                    .execute(&mut *connection)
                    .await?;
                }
            }
        }
    }
    sqlx::query(
        "INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(canonical_uuid())
    .bind(&job.company_id)
    .bind(&job.created_by)
    .bind(&job.assignment_id)
    .bind("directory.operation.verified")
    .bind(&job.id)
    .bind(json!({"result": result}))
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(true)
}

fn actor_for(company_id: &str, user_id: &str, assignment_id: &str) -> DirectoryOperationActor {
    UserManagementActor {
        user_id: user_id.to_owned(),
        session_id: String::new(),
        holding_id: String::new(),
        company_id: company_id.to_owned(),
        assignment_id: assignment_id.to_owned(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        role: String::new(),
        permissions: vec![],
        grantable_permissions: vec![],
        platform: false,
        expires_at: i64::MAX,
    }
}

struct Plan {
    id: String,
    connection_id: String,
    group_ids: Vec<String>,
    directory_members: Vec<String>,
}
fn provision_plans(evidence: &Value) -> Vec<Plan> {
    evidence
        .get("plans")
        .and_then(Value::as_array)
        .map(|plans| {
            plans
                .iter()
                .filter_map(|plan| {
                    Some(Plan {
                        id: plan.get("id")?.as_str()?.to_owned(),
                        connection_id: plan
                            .get("definition")?
                            .get("connectionId")?
                            .as_str()?
                            .to_owned(),
                        group_ids: plan
                            .get("definition")?
                            .get("directoryGroupIds")?
                            .as_array()?
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect(),
                        directory_members: plan
                            .get("directoryMembers")?
                            .as_array()?
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}
fn provision_old_ids(evidence: &Value) -> Vec<String> {
    evidence
        .get("plans")
        .and_then(Value::as_array)
        .map(|plans| {
            plans
                .iter()
                .flat_map(|plan| {
                    plan.get("oldIds")
                        .and_then(Value::as_array)
                        .map(|ids| ids.iter().filter_map(Value::as_str).map(str::to_owned).collect::<Vec<_>>())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn membership_sources(
    connection: &mut PgConnection,
    conn: &Connection,
    object_id: &str,
    member_ids: &[String],
) -> Result<Vec<(String, Option<String>, bool, bool)>, Error> {
    let rows = sqlx::query(
        "SELECT s.member_id,s.application_id,\
         (s.application_id IS NOT NULL AND ap.ended_at IS NULL) package_owned,\
         EXISTS(SELECT 1 FROM organization_gpo_membership_claims g WHERE \
         g.connection_id=s.connection_id AND g.group_id=s.group_id \
         AND g.member_id=s.member_id AND g.source_id=s.source_id) gpo_owned \
         FROM people_directory_membership_sources s \
         LEFT JOIN people_applied_packages ap ON ap.id=s.application_id \
         WHERE s.connection_id=$1 AND s.group_id=$2 AND s.member_id=ANY($3::text[])",
    )
    .bind(&conn.id)
    .bind(object_id)
    .bind(member_ids.to_vec())
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("member_id")?,
                optional_string(&row, "application_id")?,
                row.try_get("package_owned")?,
                row.try_get("gpo_owned")?,
            ))
        })
        .collect()
}
