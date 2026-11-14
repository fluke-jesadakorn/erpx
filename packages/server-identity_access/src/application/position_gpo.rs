//! Identity Access Position GPO mapping and delivery use cases.
//!
//! Ports supply current authority, the serializable owner transaction, replay
//! and persisted readback; [`PositionGpoDirectory`] supplies the approved
//! Directory provider evidence and effects. This layer never opens a
//! transaction, reads an HTTP request or talks to a Directory connector.
use crate::domain::iam_roles::IamRoleActor;
use crate::domain::position_gpo::{
    config_blocked, config_changed, connection_required, delivery_id, idempotency_conflict,
    inventory_filters, inventory_links, inventory_object, job_changed, mapping_changed,
    membership_key, position_not_found, preview_changed, readback_invalid, readback_required,
    revision_id, same_ids, target_invalid, version_conflict, waiting_key, write_not_ready,
    ClaimRow, ConfigRetryInput, ConfigRow, ConnectionRow, DesiredRow, DirectoryState, FreshMfa,
    GpoAction, GpoAuthority, HolderRow, Inventory, JobRetryInput, JobRow, JobStateRow, MappingRow,
    OwnershipRow, PolicyRow, PositionFacts, PositionGpoInput, PreviewResult, PublishInput,
    RuleStatusRow, UnmapInput, UnmapPublishInput, CONFIG_RETRY_OPERATION, INTERRUPTED_AFTER_MS,
    PUBLISH_OPERATION, UNMAP_OPERATION,
};
use erpx_server_shared::public::BusinessError;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;

pub type GpoFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct GpoReceiptSnapshot {
    pub request_hash: String,
    pub result_json: String,
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub authority_json: String,
}

#[derive(Clone, Debug)]
pub struct PublishPlan {
    pub company_id: String,
    pub position_id: String,
    pub expected_version: i64,
    pub version: i64,
    pub revision: i64,
    pub connection_id: String,
    pub gpo_id: String,
    pub user_filter_group_id: String,
    pub computer_target_ou_id: String,
    pub directory_gpo_version: String,
    pub person_ids: Vec<String>,
    pub user_id: String,
    pub reason: String,
    pub command_id: String,
    pub now: i64,
    /// Receipt payload; the adapter stores exactly the returned command result.
    pub result: Value,
}

#[derive(Clone, Debug)]
pub struct UnmapPlan {
    pub company_id: String,
    pub position_id: String,
    pub expected_version: i64,
    pub version: i64,
    pub revision: i64,
    pub person_ids: Vec<String>,
    pub user_id: String,
    pub reason: String,
    pub command_id: String,
    pub now: i64,
    /// Receipt payload; the adapter stores exactly the returned command result.
    pub result: Value,
}

/// One effective Position rule as read from the owner policy source.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveRule {
    pub id: String,
    pub company_id: String,
    pub revision: i64,
    pub connection_id: String,
    pub gpo_id: String,
    pub user_filter_group_id: String,
    pub computer_target_ou_id: String,
    pub legacy_position_id: Option<String>,
}

pub trait PositionGpoRepository {
    type Error: From<BusinessError>;

    /// Classify the current authority from the transaction's own facts.
    fn authority<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        now: i64,
    ) -> GpoFuture<'a, GpoAuthority, Self::Error>;
    fn failure_code(error: &Self::Error) -> &'static str;
    fn identifier(&mut self) -> String;
    fn lock_catalog<'a>(&'a mut self) -> GpoFuture<'a, (), Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> GpoFuture<'a, Option<GpoReceiptSnapshot>, Self::Error>;

    fn active_position<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Option<PositionFacts>, Self::Error>;
    fn policy<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Option<PolicyRow>, Self::Error>;
    fn connection<'a>(
        &'a mut self,
        company_id: &'a str,
        connection_id: &'a str,
    ) -> GpoFuture<'a, Option<ConnectionRow>, Self::Error>;
    fn directory_state<'a>(
        &'a mut self,
        company_id: &'a str,
        connection_id: &'a str,
    ) -> GpoFuture<'a, Option<DirectoryState>, Self::Error>;
    fn holders<'a>(
        &'a mut self,
        connection_id: &'a str,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Vec<HolderRow>, Self::Error>;
    fn shared_mappings<'a>(
        &'a mut self,
        company_id: &'a str,
        connection_id: &'a str,
        gpo_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Vec<Value>, Self::Error>;
    fn commit_publish<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a PublishPlan,
        request_hash: &'a str,
    ) -> GpoFuture<'a, Value, Self::Error>;

    fn mapping<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Option<MappingRow>, Self::Error>;
    fn unmap_holders<'a>(
        &'a mut self,
        connection_id: &'a str,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Vec<Value>, Self::Error>;
    fn commit_unmap<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a UnmapPlan,
        request_hash: &'a str,
    ) -> GpoFuture<'a, Value, Self::Error>;

    fn overview<'a>(&'a mut self, company_id: &'a str) -> GpoFuture<'a, Value, Self::Error>;
    fn job_state<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> GpoFuture<'a, Option<JobStateRow>, Self::Error>;
    fn commit_job_retry<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        input: &'a JobRetryInput,
        request_hash: &'a str,
        now: i64,
    ) -> GpoFuture<'a, Value, Self::Error>;
    fn config<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Option<ConfigRow>, Self::Error>;
    fn commit_config_retry<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        input: &'a ConfigRetryInput,
        request_hash: &'a str,
        now: i64,
    ) -> GpoFuture<'a, Value, Self::Error>;

    fn effective_rules<'a>(&'a mut self) -> GpoFuture<'a, Vec<EffectiveRule>, Self::Error>;
    fn config_ownership<'a>(&'a mut self) -> GpoFuture<'a, Vec<OwnershipRow>, Self::Error>;
    fn connection_company<'a>(
        &'a mut self,
        connection_id: &'a str,
    ) -> GpoFuture<'a, Option<String>, Self::Error>;
    fn set_ownership<'a>(
        &'a mut self,
        kind: &'a str,
        connection_id: &'a str,
        gpo_id: &'a str,
        target_id: &'a str,
        status: Option<&'a str>,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error>;
    fn mark_rule<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
        revision: i64,
        status: &'a str,
        error: Option<&'a str>,
    ) -> GpoFuture<'a, (), Self::Error>;
    fn mark_legacy<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
        revision: i64,
        status: &'a str,
        error: Option<&'a str>,
    ) -> GpoFuture<'a, (), Self::Error>;

    fn desired_facts<'a>(&'a mut self) -> GpoFuture<'a, Vec<DesiredRow>, Self::Error>;
    fn claims<'a>(&'a mut self) -> GpoFuture<'a, Vec<ClaimRow>, Self::Error>;
    fn pending_adds<'a>(&'a mut self) -> GpoFuture<'a, Vec<JobRow>, Self::Error>;
    fn waiting_identities<'a>(&'a mut self) -> GpoFuture<'a, Vec<JobRow>, Self::Error>;
    fn latest_delivery<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        desired_action: &'a str,
    ) -> GpoFuture<'a, Option<String>, Self::Error>;
    fn owned_membership<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        source_id: &'a str,
    ) -> GpoFuture<'a, bool, Self::Error>;
    fn enqueue<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        id: &'a str,
        desired_action: &'a str,
        status: &'a str,
        source_id: &'a str,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error>;
    fn supersede<'a>(
        &'a mut self,
        id: &'a str,
        from_status: &'a str,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error>;
    fn recover_interrupted<'a>(
        &'a mut self,
        now: i64,
        stale_before: i64,
    ) -> GpoFuture<'a, (), Self::Error>;
    fn due_deliveries<'a>(&'a mut self, limit: i64) -> GpoFuture<'a, Vec<JobRow>, Self::Error>;
    fn claim_delivery<'a>(
        &'a mut self,
        id: &'a str,
        version: i64,
        now: i64,
    ) -> GpoFuture<'a, bool, Self::Error>;
    fn finish_delivery<'a>(
        &'a mut self,
        id: &'a str,
        status: &'a str,
        observed_at: Option<i64>,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error>;
    fn block_delivery<'a>(
        &'a mut self,
        id: &'a str,
        error: &'a str,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error>;
    fn defer_replacement<'a>(&'a mut self, id: &'a str, now: i64)
        -> GpoFuture<'a, (), Self::Error>;
    fn membership<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        source_id: &'a str,
    ) -> GpoFuture<'a, bool, Self::Error>;
    fn existing_membership<'a>(
        &'a mut self,
        connection_id: &'a str,
        group_id: &'a str,
        member_id: &'a str,
    ) -> GpoFuture<'a, bool, Self::Error>;
    fn other_membership<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        source_id: &'a str,
    ) -> GpoFuture<'a, bool, Self::Error>;
    fn preserve_external<'a>(
        &'a mut self,
        connection_id: &'a str,
        group_id: &'a str,
        member_id: &'a str,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error>;
    fn record_claim<'a>(&'a mut self, job: &'a JobRow, now: i64) -> GpoFuture<'a, (), Self::Error>;
    fn delete_claim<'a>(&'a mut self, job: &'a JobRow) -> GpoFuture<'a, (), Self::Error>;
    fn delivery_rule<'a>(
        &'a mut self,
        company_id: &'a str,
        rule_id: &'a str,
        revision: i64,
    ) -> GpoFuture<'a, Option<RuleStatusRow>, Self::Error>;
}

/// The approved Directory provider profile. Installed providers decide whether
/// the requested effect is allowed; this port never bypasses that decision.
pub trait PositionGpoDirectory {
    fn inventory<'a>(
        &'a mut self,
        connection: &'a ConnectionRow,
    ) -> GpoFuture<'a, Inventory, BusinessError>;
    fn apply<'a>(
        &'a mut self,
        connection: &'a ConnectionRow,
        operation_id: &'a str,
        action: &'a GpoAction,
        before: &'a Inventory,
    ) -> GpoFuture<'a, (), BusinessError>;
}

fn receipt_result(
    receipt: Option<GpoReceiptSnapshot>,
    actor: &IamRoleActor,
    key: &str,
    operation: &str,
    record_id: &str,
    request_hash: &str,
) -> Result<Option<Value>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash
        || receipt.operation != operation
        || receipt.actor_id != actor.user_id.as_str()
        || receipt.record_id != record_id
    {
        return Err(idempotency_conflict());
    }
    let authority: Value =
        serde_json::from_str(&receipt.authority_json).map_err(|_| readback_invalid())?;
    if authority["assignmentId"].as_str() != Some(actor.assignment_id.as_str())
        || authority["companyId"].as_str() != Some(actor.company_id.as_str())
    {
        return Err(idempotency_conflict());
    }
    let result: Value =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    let _ = key;
    Ok(Some(result))
}

async fn connection<R: PositionGpoRepository>(
    repository: &mut R,
    company_id: &str,
    connection_id: &str,
) -> Result<ConnectionRow, R::Error> {
    repository
        .connection(company_id, connection_id)
        .await?
        .filter(|row| row.status == "active")
        .ok_or_else(|| connection_required().into())
}

async fn writable_connection<R: PositionGpoRepository>(
    repository: &mut R,
    company_id: &str,
    connection_id: &str,
) -> Result<ConnectionRow, R::Error> {
    let row = connection(repository, company_id, connection_id).await?;
    let Some(state) = repository
        .directory_state(company_id, connection_id)
        .await?
    else {
        return Err(write_not_ready().into());
    };
    if !state.write_ready() {
        return Err(write_not_ready().into());
    }
    Ok(row)
}

fn directory_objects(
    inventory: &Inventory,
    gpo_id: &str,
    group_id: &str,
    ou_id: &str,
) -> Result<(String, String, String, i64), BusinessError> {
    let gpo = inventory_object(inventory, gpo_id, "gpo").ok_or_else(target_invalid)?;
    let group = inventory_object(inventory, group_id, "group").ok_or_else(target_invalid)?;
    let ou = inventory_object(inventory, ou_id, "ou").ok_or_else(target_invalid)?;
    Ok((
        gpo.version.clone(),
        group.version.clone(),
        ou.version.clone(),
        inventory.observed_at,
    ))
}

/// Preview the exact Directory evidence a publish would commit to.
pub async fn preview_position_gpo<R: PositionGpoRepository, D: PositionGpoDirectory>(
    repository: &mut R,
    directory: &mut D,
    actor: &IamRoleActor,
    input: &PositionGpoInput,
    now: i64,
) -> Result<PreviewResult, R::Error> {
    let authority = repository.authority(actor, now).await?;
    if !authority.it_holding {
        return Err(crate::domain::position_gpo::it_authority_required().into());
    }
    let position = repository
        .active_position(actor.company_id.as_str(), &input.position_id)
        .await?
        .ok_or_else(position_not_found)?;
    let current = repository
        .policy(actor.company_id.as_str(), &input.position_id)
        .await?;
    if current.as_ref().map(|row| row.version).unwrap_or(0) != input.expected_version {
        return Err(version_conflict().into());
    }
    let row = connection(repository, actor.company_id.as_str(), &input.connection_id).await?;
    let state = repository
        .directory_state(actor.company_id.as_str(), &input.connection_id)
        .await?;
    if !state.is_some_and(|state| state.write_ready()) {
        return Err(write_not_ready().into());
    }
    let inventory = directory.inventory(&row).await?;
    let (gpo_version, group_version, ou_version, observed_at) = directory_objects(
        &inventory,
        &input.gpo_id,
        &input.user_filter_group_id,
        &input.computer_target_ou_id,
    )?;

    let holders = repository
        .holders(
            &input.connection_id,
            actor.company_id.as_str(),
            &input.position_id,
        )
        .await?;
    let mut deduplicated: BTreeMap<String, Value> = BTreeMap::new();
    for holder in holders {
        let key = format!(
            "{}:{}",
            holder.engagement_id,
            holder
                .account_id
                .clone()
                .unwrap_or_else(|| "awaiting".into())
        );
        deduplicated.insert(
            key,
            json!({
                "engagement_id": holder.engagement_id,
                "person_id": holder.person_id,
                "version": holder.version,
                "name": holder.name,
                "account_id": holder.account_id,
                "accountStatus": if holder.account_id.is_some() { "linked" } else { "awaiting_account_linkage" },
            }),
        );
    }
    let holder_rows: Vec<Value> = deduplicated.into_values().collect();
    let shared = repository
        .shared_mappings(
            actor.company_id.as_str(),
            &input.connection_id,
            &input.gpo_id,
            &input.position_id,
        )
        .await?;
    let previous_filter_group_id = current
        .as_ref()
        .and_then(|row| row.user_filter_group_id.clone());
    let reset_group = previous_filter_group_id
        .as_deref()
        .is_some_and(|value| value != input.user_filter_group_id);
    let accounts = |only: bool| -> Vec<Value> {
        holder_rows
            .iter()
            .filter_map(|holder| {
                let account = holder.get("account_id").and_then(Value::as_str)?;
                if only && !reset_group {
                    return None;
                }
                Some(Value::String(account.to_owned()))
            })
            .collect()
    };
    let version = input.expected_version + 1;
    let revision = current.as_ref().map(|row| row.revision).unwrap_or(0) + 1;
    let directory_value = json!({
        "connectionId": row.id,
        "gpo": {"id": input.gpo_id, "version": gpo_version},
        "userFilterGroup": {"id": input.user_filter_group_id, "version": group_version},
        "computerTargetOu": {"id": input.computer_target_ou_id, "version": ou_version},
        "observedAt": observed_at,
    });
    let mut value = json!({
        "position": {"id": position.id, "title": position.title},
        "version": version,
        "revision": revision,
        "directory": directory_value,
        "holders": holder_rows,
        "previousFilterGroupId": previous_filter_group_id,
        "sharedMappings": shared,
        "changes": {
            "directoryConfiguration": true,
            "membersToAdd": accounts(false),
            "membersToRemoveFromPreviousGroup": accounts(true),
        },
        "endpointStatus": "unconfirmed",
    });
    let token =
        crate::domain::position_gpo::preview_token(&input.to_json(), &value, &directory_value);
    if let Some(object) = value.as_object_mut() {
        object.insert("token".into(), Value::String(token.clone()));
    }
    let person_ids = holder_rows
        .iter()
        .filter_map(|holder| holder.get("person_id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let linked = holder_rows
        .iter()
        .filter(|holder| holder.get("account_id").and_then(Value::as_str).is_some())
        .count() as i64;
    Ok(PreviewResult {
        value,
        token,
        version,
        revision,
        person_ids,
        queued: linked,
        awaiting_linkage: holder_rows.len() as i64 - linked,
    })
}

pub async fn publish_position_gpo<R: PositionGpoRepository, D: PositionGpoDirectory>(
    repository: &mut R,
    directory: &mut D,
    actor: &IamRoleActor,
    fresh_mfa: FreshMfa,
    key: &str,
    input: &PublishInput,
    now: i64,
) -> Result<Value, R::Error> {
    let authority = repository.authority(actor, now).await?;
    authority.require_write(fresh_mfa)?;
    let request_hash = crate::domain::position_gpo::sha256_hex(&input.request_json());
    let record_id = input.input.position_id.as_str();
    if let Some(result) = receipt_result(
        repository.receipt(key).await?,
        actor,
        key,
        PUBLISH_OPERATION,
        record_id,
        &request_hash,
    )? {
        return Ok(result);
    }
    let preview = preview_position_gpo(repository, directory, actor, &input.input, now).await?;
    if preview.token != input.preview_token {
        return Err(preview_changed().into());
    }
    let plan = PublishPlan {
        company_id: actor.company_id.as_str().to_owned(),
        position_id: input.input.position_id.clone(),
        expected_version: input.input.expected_version,
        version: preview.version,
        revision: preview.revision,
        connection_id: input.input.connection_id.clone(),
        gpo_id: input.input.gpo_id.clone(),
        user_filter_group_id: input.input.user_filter_group_id.clone(),
        computer_target_ou_id: input.input.computer_target_ou_id.clone(),
        directory_gpo_version: preview.value["directory"]["gpo"]["version"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        person_ids: preview.person_ids,
        user_id: actor.user_id.as_str().to_owned(),
        reason: input.input.reason.clone(),
        command_id: revision_id("position-gpo", key),
        now,
        result: json!({
            "positionId": input.input.position_id,
            "version": preview.version,
            "revision": preview.revision,
            "queued": preview.queued,
            "awaitingAccountLinkage": preview.awaiting_linkage,
            "directoryStatus": "pending",
            "endpointStatus": "unconfirmed",
        }),
    };
    let result = plan.result.clone();
    repository
        .commit_publish(actor, key, &plan, &request_hash)
        .await?;
    Ok(result)
}

pub async fn preview_unmap_position_gpo<R: PositionGpoRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    input: &UnmapInput,
    now: i64,
) -> Result<Value, R::Error> {
    let authority = repository.authority(actor, now).await?;
    if !authority.it_holding {
        return Err(crate::domain::position_gpo::it_authority_required().into());
    }
    let policy = repository
        .mapping(actor.company_id.as_str(), &input.position_id)
        .await?;
    let Some(policy) = policy else {
        return Err(mapping_changed().into());
    };
    let Some(connection_id) = policy.connection_id.clone() else {
        return Err(mapping_changed().into());
    };
    if policy.version != input.expected_version {
        return Err(mapping_changed().into());
    }
    let holders = repository
        .unmap_holders(
            &connection_id,
            actor.company_id.as_str(),
            &input.position_id,
        )
        .await?;
    let shared = repository
        .shared_mappings(
            actor.company_id.as_str(),
            &connection_id,
            policy.gpo_id.as_deref().unwrap_or_default(),
            &input.position_id,
        )
        .await?;
    let mut result = json!({
        "positionId": input.position_id,
        "title": policy.title,
        "version": policy.version + 1,
        "revision": policy.revision + 1,
        "current": {
            "connectionId": policy.connection_id,
            "gpoId": policy.gpo_id,
            "userFilterGroupId": policy.user_filter_group_id,
            "computerTargetOuId": policy.computer_target_ou_id,
        },
        "holders": holders,
        "sharedMappings": shared,
        "accountMemberships": "queued_for_reconciliation",
    });
    let token = crate::domain::position_gpo::preview_token(&input.to_json(), &result, &Value::Null);
    if let Some(object) = result.as_object_mut() {
        object.insert("token".into(), Value::String(token));
    }
    Ok(result)
}

pub async fn unmap_position_gpo<R: PositionGpoRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    fresh_mfa: FreshMfa,
    key: &str,
    input: &UnmapPublishInput,
    now: i64,
) -> Result<Value, R::Error> {
    let authority = repository.authority(actor, now).await?;
    authority.require_write(fresh_mfa)?;
    let request_hash = crate::domain::position_gpo::sha256_hex(&input.input.to_json());
    let record_id = input.input.position_id.as_str();
    if let Some(result) = receipt_result(
        repository.receipt(key).await?,
        actor,
        key,
        UNMAP_OPERATION,
        record_id,
        &request_hash,
    )? {
        return Ok(result);
    }
    let preview = preview_unmap_position_gpo(repository, actor, &input.input, now).await?;
    if preview["token"].as_str() != Some(input.preview_token.as_str()) {
        return Err(preview_changed().into());
    }
    let holders = preview["holders"].as_array().cloned().unwrap_or_default();
    let person_ids = holders
        .iter()
        .filter_map(|holder| holder.get("person_id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let plan = UnmapPlan {
        company_id: actor.company_id.as_str().to_owned(),
        position_id: input.input.position_id.clone(),
        expected_version: input.input.expected_version,
        version: preview["version"].as_i64().ok_or_else(readback_invalid)?,
        revision: preview["revision"].as_i64().ok_or_else(readback_invalid)?,
        person_ids,
        user_id: actor.user_id.as_str().to_owned(),
        reason: input.input.reason.clone(),
        command_id: revision_id("position-gpo-unmap", key),
        now,
        result: json!({
            "positionId": input.input.position_id,
            "version": preview["version"].clone(),
            "revision": preview["revision"].clone(),
            "mapped": false,
            "queuedForReconciliation": true,
        }),
    };
    let result = plan.result.clone();
    repository
        .commit_unmap(actor, key, &plan, &request_hash)
        .await?;
    Ok(result)
}

pub async fn position_gpo_overview<R: PositionGpoRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    now: i64,
) -> Result<Value, R::Error> {
    let authority = repository.authority(actor, now).await?;
    if !authority.it_holding {
        return Err(crate::domain::position_gpo::it_authority_required().into());
    }
    repository.overview(actor.company_id.as_str()).await
}

pub async fn retry_position_gpo<R: PositionGpoRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    fresh_mfa: FreshMfa,
    key: &str,
    input: &JobRetryInput,
    now: i64,
) -> Result<Value, R::Error> {
    let authority = repository.authority(actor, now).await?;
    authority.require_write(fresh_mfa)?;
    let request_value = json!({
        "id": input.id,
        "expectedVersion": input.expected_version,
        "reason": input.reason,
    });
    let request_hash = crate::domain::position_gpo::sha256_hex(&request_value);
    if let Some(result) = receipt_result(
        repository.receipt(key).await?,
        actor,
        key,
        crate::domain::position_gpo::JOB_RETRY_OPERATION,
        &input.id,
        &request_hash,
    )? {
        return Ok(result);
    }
    let row = repository
        .job_state(actor.company_id.as_str(), &input.id)
        .await?;
    let Some(row) = row else {
        return Err(job_changed().into());
    };
    if row.version != input.expected_version || !matches!(row.status.as_str(), "blocked" | "retry")
    {
        return Err(job_changed().into());
    }
    let result = json!({
        "id": input.id,
        "status": "queued",
        "version": row.version + 1,
    });
    repository
        .commit_job_retry(actor, key, input, &request_hash, now)
        .await?;
    Ok(result)
}

pub async fn retry_position_gpo_config<R: PositionGpoRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    fresh_mfa: FreshMfa,
    key: &str,
    input: &ConfigRetryInput,
    now: i64,
) -> Result<Value, R::Error> {
    let authority = repository.authority(actor, now).await?;
    authority.require_write(fresh_mfa)?;
    let request_value = json!({
        "positionId": input.position_id,
        "expectedVersion": input.expected_version,
        "reason": input.reason,
    });
    let request_hash = crate::domain::position_gpo::sha256_hex(&request_value);
    if let Some(result) = receipt_result(
        repository.receipt(key).await?,
        actor,
        key,
        CONFIG_RETRY_OPERATION,
        &input.position_id,
        &request_hash,
    )? {
        return Ok(result);
    }
    let row = repository
        .config(actor.company_id.as_str(), &input.position_id)
        .await?;
    let Some(row) = row else {
        return Err(config_changed().into());
    };
    if row.version != input.expected_version || row.directory_config_status != "blocked" {
        return Err(config_changed().into());
    }
    let result = json!({
        "positionId": input.position_id,
        "version": row.version,
        "status": "pending",
    });
    repository
        .commit_config_retry(actor, key, input, &request_hash, now)
        .await?;
    Ok(result)
}

async fn queue_job<R: PositionGpoRepository>(
    repository: &mut R,
    row: &DesiredRow,
    desired_action: &str,
    now: i64,
) -> Result<(), R::Error> {
    let source_id = row.source_id();
    if row.account_id.is_some() {
        let prior = repository.latest_delivery(row, desired_action).await?;
        if let Some(status) = prior.as_deref() {
            if status != "directory_applied" {
                return Ok(());
            }
            if desired_action == "add" && repository.owned_membership(row, &source_id).await? {
                return Ok(());
            }
        }
    }
    let mut parts = vec![
        desired_action.to_owned(),
        row.connection_id.clone(),
        row.filter_group_id.clone(),
        row.account_id.clone().unwrap_or_else(|| "awaiting".into()),
        source_id.clone(),
        row.rule_id.clone(),
        row.engagement_version.to_string(),
        row.revision.to_string(),
    ];
    parts.push(if row.account_id.is_some() {
        repository.identifier()
    } else {
        String::new()
    });
    let id = delivery_id(&parts);
    let status = if row.account_id.is_some() {
        "queued"
    } else {
        "awaiting_identity"
    };
    repository
        .enqueue(row, &id, desired_action, status, &source_id, now)
        .await
}

/// Reconcile desired Position memberships into queued delivery jobs.
pub async fn reconcile_position_gpo<R: PositionGpoRepository>(
    repository: &mut R,
    now: i64,
) -> Result<BTreeMap<String, DesiredRow>, R::Error> {
    let rows = repository.desired_facts().await?;
    let mut desired: BTreeMap<String, DesiredRow> = BTreeMap::new();
    let mut waiting: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        if row.account_id.is_none() {
            waiting.insert(waiting_key(
                &row.company_id,
                &row.engagement_id,
                &row.position_id,
                &row.rule_id,
                row.revision,
            ));
            queue_job(repository, &row, "add", now).await?;
            continue;
        }
        desired.insert(
            membership_key(
                &row.connection_id,
                &row.filter_group_id,
                row.account_id.as_deref().unwrap_or_default(),
                &row.source_id(),
            ),
            row,
        );
    }
    for row in desired.values() {
        queue_job(repository, row, "add", now).await?;
    }
    for job in repository.pending_adds().await? {
        let current = match (job.account_id.as_deref(), job.source_id.as_deref()) {
            (Some(account), Some(source)) => desired.get(&membership_key(
                &job.connection_id,
                &job.filter_group_id,
                account,
                source,
            )),
            _ => None,
        };
        if !current.is_some_and(|row| {
            row.revision == job.revision
                && row.rule_id == job.rule_id.as_deref().unwrap_or_default()
                && row.position_id == job.position_id
        }) {
            repository.supersede(&job.id, "add_pending", now).await?;
        }
    }
    for claim in repository.claims().await? {
        if desired.contains_key(&membership_key(
            &claim.connection_id,
            &claim.group_id,
            &claim.member_id,
            &claim.source_id,
        )) {
            continue;
        }
        let row = DesiredRow {
            engagement_id: claim.engagement_id,
            company_id: claim.company_id,
            position_id: claim.position_id,
            person_id: claim.person_id,
            engagement_version: claim.engagement_version,
            rule_id: claim.rule_id,
            legacy_position_id: None,
            revision: claim.revision,
            connection_id: claim.connection_id,
            gpo_id: claim.gpo_id,
            filter_group_id: claim.group_id,
            computer_target_ou_id: claim.computer_target_ou_id,
            account_id: Some(claim.member_id),
        };
        queue_job(repository, &row, "remove", now).await?;
    }
    for job in repository.waiting_identities().await? {
        if !waiting.contains(&waiting_key(
            &job.company_id,
            job.engagement_id.as_deref().unwrap_or_default(),
            &job.position_id,
            job.rule_id.as_deref().unwrap_or_default(),
            job.revision,
        )) {
            repository
                .supersede(&job.id, "awaiting_identity", now)
                .await?;
        }
    }
    Ok(desired)
}

fn same_groups(left: &[String], right: &[String]) -> bool {
    same_ids(left, right)
}

/// Apply the current GPO security filter and OU links for every effective rule.
pub async fn configure_gpos<R: PositionGpoRepository, D: PositionGpoDirectory>(
    repository: &mut R,
    directory: &mut D,
    now: i64,
) -> Result<(), R::Error> {
    let rules = repository.effective_rules().await?;
    let ownership = repository.config_ownership().await?;
    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for rule in &rules {
        pairs.insert((rule.connection_id.clone(), rule.gpo_id.clone()));
    }
    for item in &ownership {
        pairs.insert((item.connection_id.clone(), item.gpo_id.clone()));
    }
    for (connection_id, gpo_id) in pairs {
        let target: Vec<&EffectiveRule> = rules
            .iter()
            .filter(|rule| rule.connection_id == connection_id && rule.gpo_id == gpo_id)
            .collect();
        let owned: Vec<&OwnershipRow> = ownership
            .iter()
            .filter(|item| item.connection_id == connection_id && item.gpo_id == gpo_id)
            .collect();
        let Some(company_id) = target
            .first()
            .map(|rule| rule.company_id.clone())
            .or(repository.connection_company(&connection_id).await?)
        else {
            continue;
        };
        let outcome = apply_connection(
            repository,
            directory,
            &company_id,
            &connection_id,
            &gpo_id,
            &target,
            &owned,
            now,
        )
        .await;
        let code = match &outcome {
            Ok(()) => None,
            Err(error) => Some(R::failure_code(error)),
        };
        for rule in &target {
            match code {
                None => {
                    repository
                        .mark_rule(
                            &rule.company_id,
                            &rule.id,
                            rule.revision,
                            "directory_applied",
                            None,
                        )
                        .await?;
                    if let Some(legacy) = rule.legacy_position_id.as_deref() {
                        repository
                            .mark_legacy(
                                &rule.company_id,
                                legacy,
                                rule.revision,
                                "directory_applied",
                                None,
                            )
                            .await?;
                    }
                }
                Some(code) => {
                    repository
                        .mark_rule(
                            &rule.company_id,
                            &rule.id,
                            rule.revision,
                            "blocked",
                            Some(code),
                        )
                        .await?;
                    if let Some(legacy) = rule.legacy_position_id.as_deref() {
                        repository
                            .mark_legacy(
                                &rule.company_id,
                                legacy,
                                rule.revision,
                                "blocked",
                                Some(code),
                            )
                            .await?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn apply_connection<R: PositionGpoRepository, D: PositionGpoDirectory>(
    repository: &mut R,
    directory: &mut D,
    company_id: &str,
    connection_id: &str,
    gpo_id: &str,
    target: &[&EffectiveRule],
    owned: &[&OwnershipRow],
    now: i64,
) -> Result<(), R::Error> {
    let connection = writable_connection(repository, company_id, connection_id).await?;
    let mut inventory = directory.inventory(&connection).await?;
    let desired_groups: BTreeSet<String> = target
        .iter()
        .map(|rule| rule.user_filter_group_id.clone())
        .collect();
    let owned_groups: BTreeSet<String> = owned
        .iter()
        .filter(|item| item.kind == "filter_group" && item.status != "pending")
        .map(|item| item.target_id.clone())
        .collect();
    let actual_groups = inventory_filters(&inventory, gpo_id);
    let mut next_groups: Vec<String> = actual_groups
        .iter()
        .filter(|group| !owned_groups.contains(*group) || desired_groups.contains(*group))
        .cloned()
        .collect();
    for group in &desired_groups {
        if !next_groups.contains(group) {
            next_groups.push(group.clone());
        }
    }
    if !same_groups(&actual_groups, &next_groups) {
        for group in &desired_groups {
            if !actual_groups.contains(group) {
                repository
                    .set_ownership(
                        "filter_group",
                        connection_id,
                        gpo_id,
                        group,
                        Some("pending"),
                        now,
                    )
                    .await?;
            }
        }
        let version = inventory_object(&inventory, gpo_id, "gpo")
            .map(|object| object.version.clone())
            .ok_or_else(target_invalid)?;
        let operation = delivery_id(&[
            "filter".into(),
            format!("{connection_id}:{gpo_id}"),
            json!(next_groups).to_string(),
            version,
        ]);
        directory
            .apply(
                &connection,
                &operation,
                &GpoAction::Filter {
                    object_id: gpo_id.to_owned(),
                    group_ids: next_groups.clone(),
                },
                &inventory,
            )
            .await?;
        for group in &desired_groups {
            if !actual_groups.contains(group) {
                repository
                    .set_ownership(
                        "filter_group",
                        connection_id,
                        gpo_id,
                        group,
                        Some("written"),
                        now,
                    )
                    .await?;
            }
        }
        inventory = directory.inventory(&connection).await?;
        if !same_groups(&inventory_filters(&inventory, gpo_id), &next_groups) {
            return Err(readback_required().into());
        }
        for group in &desired_groups {
            if !actual_groups.contains(group) {
                repository
                    .set_ownership(
                        "filter_group",
                        connection_id,
                        gpo_id,
                        group,
                        Some("confirmed"),
                        now,
                    )
                    .await?;
            }
        }
    }
    for item in owned
        .iter()
        .filter(|item| item.kind == "filter_group" && item.status == "written")
    {
        if inventory_filters(&inventory, gpo_id).contains(&item.target_id) {
            repository
                .set_ownership(
                    "filter_group",
                    connection_id,
                    gpo_id,
                    &item.target_id,
                    Some("confirmed"),
                    now,
                )
                .await?;
        }
    }
    for item in owned.iter().filter(|item| item.kind == "filter_group") {
        if !desired_groups.contains(&item.target_id) {
            repository
                .set_ownership(
                    "filter_group",
                    connection_id,
                    gpo_id,
                    &item.target_id,
                    None,
                    now,
                )
                .await?;
        }
    }
    let desired_ous: BTreeSet<String> = target
        .iter()
        .map(|rule| rule.computer_target_ou_id.clone())
        .collect();
    for ou_id in &desired_ous {
        if let Some((_, enabled)) = inventory_links(&inventory, ou_id)
            .into_iter()
            .find(|(id, _)| id == gpo_id)
        {
            if !enabled {
                return Err(crate::domain::position_gpo::link_disabled().into());
            }
            continue;
        }
        repository
            .set_ownership(
                "ou_link",
                connection_id,
                gpo_id,
                ou_id,
                Some("pending"),
                now,
            )
            .await?;
        let version = inventory_object(&inventory, ou_id, "ou")
            .map(|object| object.version.clone())
            .ok_or_else(target_invalid)?;
        let operation = delivery_id(&[
            "link".into(),
            format!("{connection_id}:{gpo_id}"),
            ou_id.clone(),
            version,
        ]);
        directory
            .apply(
                &connection,
                &operation,
                &GpoAction::Link {
                    object_id: gpo_id.to_owned(),
                    parent_id: ou_id.clone(),
                    enabled: true,
                    enforced: false,
                    remove: false,
                },
                &inventory,
            )
            .await?;
        repository
            .set_ownership(
                "ou_link",
                connection_id,
                gpo_id,
                ou_id,
                Some("written"),
                now,
            )
            .await?;
        inventory = directory.inventory(&connection).await?;
        if !inventory_links(&inventory, ou_id)
            .iter()
            .any(|(id, _)| id == gpo_id)
        {
            return Err(readback_required().into());
        }
        repository
            .set_ownership(
                "ou_link",
                connection_id,
                gpo_id,
                ou_id,
                Some("confirmed"),
                now,
            )
            .await?;
    }
    for item in owned
        .iter()
        .filter(|item| item.kind == "ou_link" && item.status == "written")
    {
        if inventory_links(&inventory, &item.target_id)
            .iter()
            .any(|(id, _)| id == gpo_id)
        {
            repository
                .set_ownership(
                    "ou_link",
                    connection_id,
                    gpo_id,
                    &item.target_id,
                    Some("confirmed"),
                    now,
                )
                .await?;
        }
    }
    for item in owned.iter().filter(|item| item.kind == "ou_link") {
        if desired_ous.contains(&item.target_id) {
            continue;
        }
        if item.status != "pending"
            && inventory_links(&inventory, &item.target_id)
                .iter()
                .any(|(id, _)| id == gpo_id)
        {
            let version = inventory_object(&inventory, &item.target_id, "ou")
                .map(|object| object.version.clone())
                .ok_or_else(target_invalid)?;
            let operation = delivery_id(&[
                "unlink".into(),
                format!("{connection_id}:{gpo_id}"),
                item.target_id.clone(),
                version,
            ]);
            directory
                .apply(
                    &connection,
                    &operation,
                    &GpoAction::Link {
                        object_id: gpo_id.to_owned(),
                        parent_id: item.target_id.clone(),
                        enabled: true,
                        enforced: false,
                        remove: true,
                    },
                    &inventory,
                )
                .await?;
            inventory = directory.inventory(&connection).await?;
            if inventory_links(&inventory, &item.target_id)
                .iter()
                .any(|(id, _)| id == gpo_id)
            {
                return Err(crate::domain::position_gpo::unlink_readback_required().into());
            }
        }
        repository
            .set_ownership("ou_link", connection_id, gpo_id, &item.target_id, None, now)
            .await?;
    }
    Ok(())
}

async fn desired_by_account<R: PositionGpoRepository>(
    repository: &mut R,
) -> Result<BTreeMap<String, DesiredRow>, R::Error> {
    let rows = repository.desired_facts().await?;
    Ok(rows
        .into_iter()
        .filter(|row| row.account_id.is_some())
        .map(|row| {
            (
                membership_key(
                    &row.connection_id,
                    &row.filter_group_id,
                    row.account_id.as_deref().unwrap_or_default(),
                    &row.source_id(),
                ),
                row,
            )
        })
        .collect())
}

/// Apply one drain tick. The caller owns the catalog lock and the per-connection
/// provider locks; no commit happens here.
pub async fn drain_position_gpo_locked<R: PositionGpoRepository, D: PositionGpoDirectory>(
    repository: &mut R,
    directory: &mut D,
    limit: i64,
    now: i64,
) -> Result<(), R::Error> {
    repository
        .recover_interrupted(now, now - INTERRUPTED_AFTER_MS)
        .await?;
    reconcile_position_gpo(repository, now).await?;
    configure_gpos(repository, directory, now).await?;
    let due = repository.due_deliveries(limit).await?;
    for job in due {
        if !repository.claim_delivery(&job.id, job.version, now).await? {
            continue;
        }
        let outcome = apply_delivery(repository, directory, &job, now).await;
        if let Err(error) = outcome {
            let code = R::failure_code(&error);
            repository.block_delivery(&job.id, code, now).await?;
        }
    }
    Ok(())
}

async fn apply_delivery<R: PositionGpoRepository, D: PositionGpoDirectory>(
    repository: &mut R,
    directory: &mut D,
    job: &JobRow,
    now: i64,
) -> Result<(), R::Error> {
    if !job.deliverable() {
        repository
            .finish_delivery(&job.id, "superseded", None, now)
            .await?;
        return Ok(());
    }
    let desired = desired_by_account(repository).await?;
    let key = membership_key(
        &job.connection_id,
        &job.filter_group_id,
        job.account_id.as_deref().unwrap_or_default(),
        job.source_id.as_deref().unwrap_or_default(),
    );
    if job.desired_action == "add"
        && !desired.get(&key).is_some_and(|row| {
            row.revision == job.revision
                && row.rule_id == job.rule_id.as_deref().unwrap_or_default()
                && row.position_id == job.position_id
        })
    {
        repository
            .finish_delivery(&job.id, "superseded", None, now)
            .await?;
        return Ok(());
    }
    let row = desired
        .get(&key)
        .cloned()
        .or_else(|| desired_row_from_job(job));
    let current_claim = match &row {
        Some(row) => {
            repository
                .membership(row, job.source_id.as_deref().unwrap_or_default())
                .await?
        }
        None => false,
    };
    if job.desired_action == "remove" && (!current_claim || desired.contains_key(&key)) {
        repository
            .finish_delivery(&job.id, "superseded", None, now)
            .await?;
        return Ok(());
    }
    let Some(row) = row else {
        repository
            .finish_delivery(&job.id, "superseded", None, now)
            .await?;
        return Ok(());
    };
    let connection = writable_connection(repository, &job.company_id, &job.connection_id).await?;
    let rule = repository
        .delivery_rule(
            &job.company_id,
            job.rule_id.as_deref().unwrap_or_default(),
            job.revision,
        )
        .await?;
    if job.desired_action == "add"
        && !rule.is_some_and(|rule| rule.directory_config_status == "directory_applied")
    {
        return Err(config_blocked().into());
    }
    let mut inventory = directory.inventory(&connection).await?;
    let group = inventory_object(&inventory, &job.filter_group_id, "group");
    if group.is_none() {
        if job.desired_action == "remove" {
            repository.delete_claim(job).await?;
            repository
                .finish_delivery(
                    &job.id,
                    "directory_applied",
                    Some(inventory.observed_at),
                    now,
                )
                .await?;
            return Ok(());
        }
        return Err(target_invalid().into());
    }
    let account = job.account_id.as_deref().unwrap_or_default();
    if job.desired_action == "add" {
        if inventory_object(&inventory, account, "account").is_none() {
            return Err(target_invalid().into());
        }
        let member = group
            .map(|group| group.member_ids.iter().any(|member| member == account))
            .unwrap_or(false);
        if member {
            if !repository
                .existing_membership(&job.connection_id, &job.filter_group_id, account)
                .await?
            {
                repository
                    .preserve_external(&job.connection_id, &job.filter_group_id, account, now)
                    .await?;
            }
        } else {
            directory
                .apply(
                    &connection,
                    &job.id,
                    &GpoAction::Members {
                        object_id: job.filter_group_id.clone(),
                        member_ids: vec![account.to_owned()],
                        add: true,
                    },
                    &inventory,
                )
                .await?;
            // A verified provider write owns its claim even if read-back fails.
            repository.record_claim(job, now).await?;
            inventory = directory.inventory(&connection).await?;
        }
        if !inventory_object(&inventory, &job.filter_group_id, "group")
            .is_some_and(|group| group.member_ids.iter().any(|member| member == account))
        {
            return Err(readback_required().into());
        }
        repository.record_claim(job, now).await?;
    } else {
        let others = repository
            .other_membership(&row, job.source_id.as_deref().unwrap_or_default())
            .await?;
        let other_desired = desired.values().any(|desired| {
            desired.connection_id == job.connection_id
                && desired.filter_group_id == job.filter_group_id
                && desired.account_id.as_deref() == Some(account)
        });
        if other_desired && !others {
            repository.defer_replacement(&job.id, now).await?;
            return Ok(());
        }
        if !others
            && !other_desired
            && group.is_some_and(|group| group.member_ids.iter().any(|member| member == account))
        {
            directory
                .apply(
                    &connection,
                    &job.id,
                    &GpoAction::Members {
                        object_id: job.filter_group_id.clone(),
                        member_ids: vec![account.to_owned()],
                        add: false,
                    },
                    &inventory,
                )
                .await?;
            inventory = directory.inventory(&connection).await?;
            if inventory_object(&inventory, &job.filter_group_id, "group")
                .is_some_and(|group| group.member_ids.iter().any(|member| member == account))
            {
                return Err(readback_required().into());
            }
        }
        repository.delete_claim(job).await?;
    }
    repository
        .finish_delivery(
            &job.id,
            "directory_applied",
            Some(inventory.observed_at),
            now,
        )
        .await?;
    Ok(())
}

fn desired_row_from_job(job: &JobRow) -> Option<DesiredRow> {
    Some(DesiredRow {
        engagement_id: job.engagement_id.clone()?,
        company_id: job.company_id.clone(),
        position_id: job.position_id.clone(),
        person_id: job.person_id.clone(),
        engagement_version: job.engagement_version,
        rule_id: job.rule_id.clone()?,
        legacy_position_id: None,
        revision: job.revision,
        connection_id: job.connection_id.clone(),
        gpo_id: job.gpo_id.clone(),
        filter_group_id: job.filter_group_id.clone(),
        computer_target_ou_id: job.computer_target_ou_id.clone(),
        account_id: job.account_id.clone(),
    })
}
