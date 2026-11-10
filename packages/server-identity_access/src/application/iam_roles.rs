//! Identity Access role, independent-review and access-review use cases.
//!
//! Ports supply current authority, the serializable owner transaction, replay,
//! catalogue facts and persisted readback. This layer never opens a transaction
//! or reads an HTTP request.
use crate::domain::iam_roles::{
    ACCESS_REVIEW_CANCEL_OPERATION, ACCESS_REVIEW_DECIDE_OPERATION, ACCESS_REVIEW_START_OPERATION,
    AccessReviewCampaignInput, AccessReviewCancelInput, AccessReviewCancelPlan,
    AccessReviewDecisionInput, AccessReviewDecisionPlan, AccessReviewStartPlan, AccessRevocation,
    CampaignRow, IamReceiptSnapshot, IamRoleActor, MAX_CAMPAIGN_ASSIGNMENTS,
    REVIEW_DECIDE_OPERATION, REVIEW_KIND_ROLE_PUBLISH, ROLE_SAVE_OPERATION, ReviewAssignment,
    ReviewDecisionInput, ReviewDecisionPlan, ReviewItemRow, ReviewRow, RevocationGuardFacts,
    RolePublishPayload, RoleRow, RoleSaveInput, RoleSavePlan, access_changed,
    access_review_campaign_changed, access_review_campaign_not_found, access_review_item_changed,
    access_review_item_not_found, campaign_cancel_identity, campaign_cancelled_event_id,
    campaign_completed_event_id, campaign_decision_event_id, campaign_decision_identity,
    campaign_decisions_exist, campaign_id, campaign_owner_required, campaign_start_identity,
    campaign_started_event_id, ensure_campaign_independent, ensure_campaign_source,
    ensure_independent_review, ensure_publication_delegation, ensure_revocation_allowed,
    idempotency_conflict, no_active_assignments, proposer_authority_revoked, readback_invalid,
    required_permission, review_changed, review_decision_identity, review_not_found,
    review_outside_scope, review_scope_covered, review_source_changed, review_unsupported,
    role_changed, role_review_id, role_save_identity, role_scope_invalid, source_invalid,
};
use crate::domain::permission_catalog::PermissionCatalog;
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub type IamRolesFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait IamRolesReviewRepository {
    type Error: From<BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        permission: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, bool, Self::Error>;
    fn digest<'a>(&'a mut self, value: &'a str) -> IamRolesFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> IamRolesFuture<'a, Option<IamReceiptSnapshot>, Self::Error>;
    fn role_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Option<RoleRow>, Self::Error>;
    fn next_revision<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, i64, Self::Error>;
    fn affected_role_users<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Vec<String>, Self::Error>;
    fn commit_role_save<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a RoleSavePlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error>;
    fn review_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Option<ReviewRow>, Self::Error>;
    fn same_person<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        user_id: &'a str,
    ) -> IamRolesFuture<'a, bool, Self::Error>;
    fn branch_covers<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        node_id: &'a str,
    ) -> IamRolesFuture<'a, bool, Self::Error>;
    fn proposer_authority<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        assignment_id: &'a str,
        proposer_user_id: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, Option<crate::domain::iam_roles::ProposerAuthority>, Self::Error>;
    fn role_allows<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        role: &'a str,
        permission: &'a str,
    ) -> IamRolesFuture<'a, bool, Self::Error>;
    fn reviewer_affected_by_role<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        role_id: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, bool, Self::Error>;
    fn commit_review_decision<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a ReviewDecisionPlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error>;
    fn active_review_assignments<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        now: i64,
    ) -> IamRolesFuture<'a, Vec<ReviewAssignment>, Self::Error>;
    fn commit_campaign_start<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a AccessReviewStartPlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error>;
    fn review_item_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        item_id: &'a str,
    ) -> IamRolesFuture<'a, Option<ReviewItemRow>, Self::Error>;
    fn assignment_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Option<ReviewAssignment>, Self::Error>;
    fn assignment_active<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, bool, Self::Error>;
    fn revocation_guard<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        user_id: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, RevocationGuardFacts, Self::Error>;
    fn commit_campaign_decision<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a AccessReviewDecisionPlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error>;
    fn campaign_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Option<CampaignRow>, Self::Error>;
    fn decided_items<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        campaign_id: &'a str,
    ) -> IamRolesFuture<'a, i64, Self::Error>;
    fn commit_campaign_cancel<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a AccessReviewCancelPlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error>;
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn receipt_value(
    actor: &IamRoleActor,
    record_id: &str,
    operation: &str,
    request_hash: &str,
    receipt: Option<IamReceiptSnapshot>,
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
    let parsed: Value =
        serde_json::from_str(&receipt.result_json).map_err(|_| readback_invalid())?;
    if parsed["id"].as_str() != Some(record_id) {
        return Err(readback_invalid());
    }
    let valid = match operation {
        ROLE_SAVE_OPERATION => parsed["status"].as_str() == Some("pending"),
        REVIEW_DECIDE_OPERATION => {
            matches!(parsed["status"].as_str(), Some("approved" | "rejected"))
        }
        ACCESS_REVIEW_START_OPERATION => parsed["status"].as_str() == Some("open"),
        ACCESS_REVIEW_DECIDE_OPERATION => {
            matches!(parsed["decision"].as_str(), Some("keep" | "revoke"))
                && matches!(
                    parsed["campaignStatus"].as_str(),
                    Some("open" | "completed")
                )
        }
        ACCESS_REVIEW_CANCEL_OPERATION => parsed["status"].as_str() == Some("cancelled"),
        _ => false,
    };
    if !valid {
        return Err(readback_invalid());
    }
    Ok(Some(parsed))
}

async fn require_current<R: IamRolesReviewRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    permission: &str,
    now: i64,
) -> Result<(), R::Error> {
    if !repository.current_authority(actor, permission, now).await? {
        return Err(access_changed().into());
    }
    Ok(())
}

fn new_role_id(actor: &IamRoleActor, hash: &str) -> String {
    let prefix = if actor.platform {
        "platform_custom_"
    } else {
        "custom_"
    };
    format!("{prefix}{}", &hash[..32])
}

/// Append an immutable role revision and open an independent publication
/// review. The role is not published until [`decide_review`] approves it.
pub async fn save_role<R: IamRolesReviewRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    catalog: &PermissionCatalog,
    key: &CommandKey,
    input: &RoleSaveInput,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require("iam.roles.manage", now)?;
    actor.require_management_scope()?;
    let input = crate::domain::iam_roles::validate_role_save(input.clone(), actor, catalog)?;
    require_current(repository, actor, "iam.roles.manage", now).await?;
    let identity = role_save_identity(actor, key.as_str(), &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    let provisional_id = input.id.clone().unwrap_or_default();
    let provisional_review = role_review_id(key.as_str());
    if let Some(prior) = receipt_value(
        actor,
        &provisional_review,
        ROLE_SAVE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let existing = if provisional_id.is_empty() {
        None
    } else {
        repository.role_for_update(actor, &provisional_id).await?
    };
    let (id, created) = match existing {
        Some(row) => {
            row.validate_source()?;
            if row.built_in {
                return Err(crate::domain::iam_roles::protected_role().into());
            }
            if row.published_revision != input.expected_version {
                return Err(role_changed().into());
            }
            (row.id, false)
        }
        None => {
            if input.expected_version != 0 {
                return Err(role_changed().into());
            }
            (
                input
                    .id
                    .clone()
                    .unwrap_or_else(|| new_role_id(actor, &hash)),
                true,
            )
        }
    };
    if id.encode_utf16().count() > crate::domain::iam_roles::MAX_ROLE_ID {
        return Err(role_scope_invalid().into());
    }
    let revision = repository.next_revision(actor, &id).await?;
    if !(1..=crate::domain::iam_roles::MAX_REVISION).contains(&revision) {
        return Err(readback_invalid().into());
    }
    let affected = repository.affected_role_users(actor, &id).await?;
    let review_id = provisional_review;
    let publication = RolePublishPayload {
        id: id.clone(),
        name: input.name.clone(),
        revision,
        expected_version: input.expected_version,
        permissions: input.permissions.clone(),
        grantable_permissions: input.grantable_permissions.clone(),
        affected: affected.clone(),
        scope_kind: actor.scope_kind.clone(),
        scope_node_id: actor
            .scope_node_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        authorizing_assignment_id: actor.assignment_id.as_str().to_owned(),
    };
    let plan = RoleSavePlan {
        id,
        name: input.name,
        expected_version: input.expected_version,
        revision,
        permissions: input.permissions,
        grantable_permissions: input.grantable_permissions,
        scopes: input.scopes,
        reason: input.reason,
        created,
        affected,
        review_id: review_id.clone(),
        review_payload: publication.to_payload_value(),
        now,
    };
    if let Some(prior) = receipt_value(
        actor,
        &review_id,
        ROLE_SAVE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    repository
        .commit_role_save(actor, key.as_str(), &plan, &hash)
        .await
}

/// Approve or reject one independent review. Only `role.publish` has a native
/// approval handler in this slice; every other kind may still be rejected.
pub async fn decide_review<R: IamRolesReviewRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    catalog: &PermissionCatalog,
    key: &CommandKey,
    review_id: &str,
    input: &ReviewDecisionInput,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require("iam.review", now)?;
    let input = input.clone().validate()?;
    if RecordId::new(review_id.to_owned()).is_err() {
        return Err(review_not_found().into());
    }
    require_current(repository, actor, "iam.review", now).await?;
    let identity = review_decision_identity(actor, key.as_str(), review_id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        review_id,
        REVIEW_DECIDE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let review = repository
        .review_for_update(actor, review_id)
        .await?
        .ok_or_else(review_not_found)?;
    review.validate_source()?;
    if review.status != "pending" || review.version != input.expected_version {
        return Err(review_changed().into());
    }
    let same_person = repository.same_person(actor, &review.created_by).await?;
    ensure_independent_review(actor, &review, same_person)?;
    let mut covered = review_scope_covered(actor, &review.kind, &review.payload);
    if !covered
        && actor.scope_kind == "branch"
        && let Some(node) = review.payload["scopeNodeId"].as_str()
    {
        covered = repository.branch_covers(actor, node).await?;
    }
    if !covered {
        return Err(review_outside_scope().into());
    }
    let publication = if input.decision == "approve" {
        let assignment_id = review.payload["authorizingAssignmentId"]
            .as_str()
            .ok_or_else(source_invalid)?;
        let proposer = repository
            .proposer_authority(actor, assignment_id, &review.created_by, now)
            .await?
            .ok_or_else(proposer_authority_revoked)?;
        let permission = required_permission(&review.kind);
        if !repository
            .role_allows(actor, &proposer.role, permission)
            .await?
        {
            return Err(proposer_authority_revoked().into());
        }
        if review.kind == REVIEW_KIND_ROLE_PUBLISH {
            let payload = RolePublishPayload::parse(&review.payload, catalog, actor.platform)?;
            ensure_publication_delegation(actor, &payload, &proposer)?;
            if repository
                .reviewer_affected_by_role(actor, &payload.id, now)
                .await?
            {
                return Err(crate::domain::iam_roles::independent_review_required().into());
            }
            Some(crate::domain::iam_roles::RolePublication {
                id: payload.id,
                name: payload.name,
                revision: payload.revision,
                expected_version: payload.expected_version,
                permissions: payload.permissions,
                grantable_permissions: payload.grantable_permissions,
            })
        } else {
            return Err(review_unsupported().into());
        }
    } else {
        None
    };
    let plan = ReviewDecisionPlan {
        review_id: review.id,
        expected_version: input.expected_version,
        decision: input.decision,
        reason: input.reason,
        publication,
        now,
    };
    if let Some(prior) = receipt_value(
        actor,
        review_id,
        REVIEW_DECIDE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    repository
        .commit_review_decision(actor, key.as_str(), &plan, &hash)
        .await
}

/// Open an access-review campaign over the exact current active assignment
/// snapshot. Item decisions are separate CAS commands.
pub async fn start_access_review<R: IamRolesReviewRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    key: &CommandKey,
    input: &AccessReviewCampaignInput,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_holding_review(now)?;
    let input = input.clone().validate(now)?;
    require_current(repository, actor, "iam.review", now).await?;
    let identity = campaign_start_identity(actor, key.as_str(), &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    let id = campaign_id(key.as_str());
    if let Some(prior) = receipt_value(
        actor,
        &id,
        ACCESS_REVIEW_START_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let assignments = repository.active_review_assignments(actor, now).await?;
    if assignments.is_empty() {
        return Err(no_active_assignments().into());
    }
    if assignments.len() > MAX_CAMPAIGN_ASSIGNMENTS {
        return Err(source_invalid().into());
    }
    for assignment in &assignments {
        assignment.validate_source()?;
    }
    let plan = AccessReviewStartPlan {
        campaign_id: id.clone(),
        name: input.name,
        due_at: input.due_at,
        reason: input.reason,
        assignments,
        started_event_id: campaign_started_event_id(key.as_str()),
        now,
    };
    if let Some(prior) = receipt_value(
        actor,
        &id,
        ACCESS_REVIEW_START_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    repository
        .commit_campaign_start(actor, key.as_str(), &plan, &hash)
        .await
}

/// Certify one campaign item. `revoke` applies the same self/last-admin guards
/// as the assignment withdrawal slice; `keep` records the certification only.
pub async fn decide_access_review_item<R: IamRolesReviewRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    key: &CommandKey,
    item_id: &str,
    input: &AccessReviewDecisionInput,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_holding_review(now)?;
    let input = input.clone().validate()?;
    if RecordId::new(item_id.to_owned()).is_err() {
        return Err(access_review_item_not_found().into());
    }
    require_current(repository, actor, "iam.review", now).await?;
    let identity = campaign_decision_identity(actor, key.as_str(), item_id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        item_id,
        ACCESS_REVIEW_DECIDE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let item = repository
        .review_item_for_update(actor, item_id)
        .await?
        .ok_or_else(access_review_item_not_found)?;
    item.validate_source()?;
    if item.campaign_status != "open" || item.decision != "pending" {
        return Err(access_review_item_changed().into());
    }
    if item.campaign_version != input.expected_campaign_version {
        return Err(access_review_campaign_changed().into());
    }
    let same_person = repository.same_person(actor, &item.user_id).await?;
    ensure_campaign_independent(actor, &item, same_person)?;
    let assignment = repository
        .assignment_for_update(actor, &item.assignment_id)
        .await?
        .ok_or_else(review_source_changed)?;
    assignment.validate_source()?;
    if assignment.id != item.assignment_id
        || assignment.user_id != item.user_id
        || assignment.role != item.role
        || assignment.scope_kind != item.scope_kind
        || assignment.scope_node_id != item.scope_node_id
        || assignment.source != item.source
        || assignment.valid_until != item.valid_until
    {
        return Err(review_source_changed().into());
    }
    let active = repository
        .assignment_active(actor, &assignment.id, now)
        .await?;
    ensure_campaign_source(&item, &assignment, active)?;
    let revocation = if input.decision == "revoke" {
        let facts = repository
            .revocation_guard(actor, &item.user_id, now)
            .await?;
        if facts.recovery_member {
            actor.require("iam.recovery", now)?;
        }
        ensure_revocation_allowed(actor, &assignment, &facts)?;
        Some(AccessRevocation {
            assignment_id: assignment.id.clone(),
            assignment_version: assignment.version,
            user_id: assignment.user_id,
            role: assignment.role,
            company_id: actor.company_id.as_str().to_owned(),
            scope_kind: assignment.scope_kind,
            scope_node_id: assignment.scope_node_id,
            reason: input.reason.clone(),
        })
    } else {
        None
    };
    let plan = AccessReviewDecisionPlan {
        item_id: item.id,
        campaign_id: item.campaign_id,
        decision: input.decision,
        reason: input.reason,
        expected_campaign_version: item.campaign_version,
        resulting_campaign_version: item.campaign_version + 1,
        revocation,
        decision_event_id: campaign_decision_event_id(key.as_str()),
        completed_event_id: Some(campaign_completed_event_id(key.as_str())),
        now,
    };
    if let Some(prior) = receipt_value(
        actor,
        item_id,
        ACCESS_REVIEW_DECIDE_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    repository
        .commit_campaign_decision(actor, key.as_str(), &plan, &hash)
        .await
}

/// Cancel an undecided campaign. Only its creator may cancel it.
pub async fn cancel_access_review_campaign<R: IamRolesReviewRepository>(
    repository: &mut R,
    actor: &IamRoleActor,
    key: &CommandKey,
    campaign_id: &str,
    input: &AccessReviewCancelInput,
    now: i64,
) -> Result<Value, R::Error> {
    actor.require_holding_review(now)?;
    let input = input.clone().validate()?;
    if RecordId::new(campaign_id.to_owned()).is_err() {
        return Err(access_review_campaign_not_found().into());
    }
    require_current(repository, actor, "iam.review", now).await?;
    let identity = campaign_cancel_identity(actor, key.as_str(), campaign_id, &input);
    let hash = repository.digest(&identity).await?;
    if !valid_hash(&hash) {
        return Err(readback_invalid().into());
    }
    if let Some(prior) = receipt_value(
        actor,
        campaign_id,
        ACCESS_REVIEW_CANCEL_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    let campaign = repository
        .campaign_for_update(actor, campaign_id)
        .await?
        .ok_or_else(access_review_campaign_not_found)?;
    campaign.validate_source()?;
    if campaign.created_by != actor.user_id.as_str() {
        return Err(campaign_owner_required().into());
    }
    if campaign.status != "open" || campaign.version != input.expected_version {
        return Err(access_review_campaign_changed().into());
    }
    if repository.decided_items(actor, campaign_id).await? != 0 {
        return Err(campaign_decisions_exist().into());
    }
    let plan = AccessReviewCancelPlan {
        campaign_id: campaign.id,
        expected_version: input.expected_version,
        resulting_version: input.expected_version + 1,
        reason: input.reason,
        cancelled_event_id: campaign_cancelled_event_id(key.as_str()),
        now,
    };
    if let Some(prior) = receipt_value(
        actor,
        campaign_id,
        ACCESS_REVIEW_CANCEL_OPERATION,
        &hash,
        repository.receipt(key.as_str()).await?,
    )? {
        return Ok(prior);
    }
    repository
        .commit_campaign_cancel(actor, key.as_str(), &plan, &hash)
        .await
}
