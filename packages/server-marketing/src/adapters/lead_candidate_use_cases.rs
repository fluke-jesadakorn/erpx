//! Native Marketing campaign, lead-candidate, engagement and handoff use
//! cases.
//!
//! The caller owns one serializable Holding transaction. Marketing locks its
//! command key, replays the exact command receipt, re-checks current IAM,
//! writes only its own tables and hands the CRM child to the public CRM port in
//! the same transaction. Files/CRM/Commerce SQL is never reached here.
use crate::domain::lead_candidate::{
    MarketingAssetAction, MarketingAssetProfile, MarketingAssetProfiles, MarketingAssetReviewInput,
    MarketingAssetReviewRow, MarketingCampaignCreateInput, MarketingCampaignReadback,
    MarketingCandidateReadback, MarketingCommandReceipt, MarketingCrmLeadRequest,
    MarketingCrmLeadResult, MarketingEngagementRow, MarketingFormAssetFacts, MarketingLeadAction,
    MarketingLeadActionInput, MarketingLeadActor, MarketingLeadCandidateInput,
    MarketingLeadCandidateRow, MarketingLeadDetail, asset_changed, asset_not_found,
    campaign_code_taken, candidate_changed, candidate_not_found, crm_request,
    form_configuration_allows_capture, form_not_ready, form_readiness_port_required,
    form_requires_files_readiness, idempotency_conflict, independent_asset_review_required,
    independent_review_required, js_trim, permission_required, readback_invalid, request_invalid,
    source_invalid,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use sqlx::PgConnection;

pub struct MarketingStoredReceipt {
    pub request_hash: String,
    pub result_json: String,
    pub operation: Option<String>,
    pub record_id: Option<String>,
}

pub struct MarketingReceiptEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub actor: &'a MarketingLeadActor,
    pub request_hash: &'a str,
    pub record_id: &'a str,
    pub result_json: &'a str,
}

pub struct MarketingAuditEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub authority_json: &'a str,
}

pub struct MarketingOutboxEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub state: &'a str,
    pub authority_json: &'a str,
}

pub trait MarketingLeadRepository {
    fn ready<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError>;

    fn lock_command<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        key: &'a str,
    ) -> PgWorkFuture<'a, (), BusinessError>;

    fn digest<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        value: &'a str,
    ) -> PgWorkFuture<'a, String, BusinessError>;

    fn receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        key: &'a str,
    ) -> PgWorkFuture<'a, Option<MarketingStoredReceipt>, BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn form_asset<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        form_id: &'a str,
        form_version: i64,
    ) -> PgWorkFuture<'a, Option<MarketingFormAssetFacts>, BusinessError>;

    fn campaign_create<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        id: &'a str,
        actor: &'a MarketingLeadActor,
        input: &'a MarketingCampaignCreateInput,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn asset_create<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        id: &'a str,
        actor: &'a MarketingLeadActor,
        input: &'a crate::domain::lead_candidate::MarketingAssetCreateInput,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn asset_readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        id: &'a str,
    ) -> PgWorkFuture<'a, Option<MarketingAssetProfile>, BusinessError>;

    fn asset_review_row<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        id: &'a str,
        lock: bool,
    ) -> PgWorkFuture<'a, Option<MarketingAssetReviewRow>, BusinessError>;

    #[allow(clippy::too_many_arguments)]
    fn asset_review<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        id: &'a str,
        expected_version: i64,
        expected_status: &'a str,
        status: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    #[allow(clippy::too_many_arguments)]
    fn insert_asset_event<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        id: &'a str,
        asset_id: &'a str,
        version: i64,
        action: &'a str,
        actor_id: &'a str,
        reason: &'a str,
        status: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn campaign_readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        id: &'a str,
    ) -> PgWorkFuture<'a, Option<MarketingCampaignReadback>, BusinessError>;

    fn candidate_create<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        id: &'a str,
        actor: &'a MarketingLeadActor,
        input: &'a MarketingLeadCandidateInput,
        attribution_json: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn candidate<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        id: &'a str,
        lock: bool,
    ) -> PgWorkFuture<'a, Option<MarketingLeadCandidateRow>, BusinessError>;

    fn candidate_readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        id: &'a str,
    ) -> PgWorkFuture<'a, Option<MarketingCandidateReadback>, BusinessError>;

    fn asset_profiles<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        limit: usize,
    ) -> PgWorkFuture<'a, Vec<MarketingAssetProfile>, BusinessError>;

    fn candidates<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        limit: usize,
    ) -> PgWorkFuture<'a, Vec<MarketingLeadCandidateRow>, BusinessError>;

    fn history<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        candidate_id: &'a str,
        limit: usize,
    ) -> PgWorkFuture<'a, Vec<MarketingEngagementRow>, BusinessError>;

    fn update_candidate_engagement<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        id: &'a str,
        expected_version: i64,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn update_candidate_handoff<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        id: &'a str,
        expected_version: i64,
        crm_lead_id: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    #[allow(clippy::too_many_arguments)]
    fn insert_engagement<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        id: &'a str,
        candidate_id: &'a str,
        version: i64,
        event_type: &'a str,
        channel: &'a str,
        evidence: &'a str,
        actor_id: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn insert_receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a MarketingReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn audit<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        entry: &'a MarketingAuditEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError>;

    fn outbox<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a MarketingOutboxEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

/// CRM-owned lead handoff. Composition must implement this with the existing
/// CRM public port inside the same transaction.
pub trait MarketingCrmHandoffPort {
    fn create_lead<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: &'a MarketingCrmLeadRequest,
        command_key: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, MarketingCrmLeadResult, BusinessError>;
}

fn hash_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn replay<R: MarketingLeadRepository + ?Sized>(
    repository: &mut R,
    c: &mut PgConnection,
    key: &str,
    request_hash: &str,
    operation: &str,
    record_id: &str,
) -> Result<Option<MarketingCommandReceipt>, PgTransactionError<BusinessError>> {
    let Some(prior) = repository.receipt(c, key).await? else {
        return Ok(None);
    };
    if prior.request_hash != request_hash
        || prior
            .operation
            .as_deref()
            .is_some_and(|value| value != operation)
        || prior
            .record_id
            .as_deref()
            .is_some_and(|value| value != record_id)
    {
        return Err(idempotency_conflict().into());
    }
    MarketingCommandReceipt::from_json(&prior.result_json)
        .map(Some)
        .map_err(Into::into)
}

struct CommandStart {
    prior: Option<MarketingCommandReceipt>,
    request_hash: String,
}

#[allow(clippy::too_many_arguments)]
async fn begin<R: MarketingLeadRepository + ?Sized>(
    repository: &mut R,
    c: &mut PgConnection,
    actor: &MarketingLeadActor,
    key: &str,
    identity: &str,
    operation: &str,
    record_id: &str,
    now: TimestampMillis,
) -> Result<CommandStart, PgTransactionError<BusinessError>> {
    repository.ready(c).await?;
    repository.lock_command(c, key).await?;
    let request_hash = repository.digest(c, identity).await?;
    if !hash_valid(&request_hash) {
        return Err(source_invalid().into());
    }
    if let Some(prior) = replay(repository, c, key, &request_hash, operation, record_id).await? {
        return Ok(CommandStart {
            prior: Some(prior),
            request_hash,
        });
    }
    if !repository.current_authority(c, actor, now).await? {
        return Err(permission_required().into());
    }
    Ok(CommandStart {
        prior: None,
        request_hash,
    })
}

/// Insert the command receipt only after every owner effect and readback in the
/// same transaction. `authority_json` is the current selected projection.
#[allow(clippy::too_many_arguments)]
async fn finish<R: MarketingLeadRepository + ?Sized>(
    repository: &mut R,
    c: &mut PgConnection,
    actor: &MarketingLeadActor,
    key: &str,
    operation: &str,
    record_type: &str,
    record_id: &str,
    state: &str,
    version: i64,
    request_hash: &str,
    result: &MarketingCommandReceipt,
    now: TimestampMillis,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !repository.current_authority(c, actor, now).await? {
        return Err(permission_required().into());
    }
    let result_json = result.to_json()?;
    if !repository
        .insert_receipt(
            c,
            &MarketingReceiptEntry {
                key,
                operation,
                actor,
                request_hash,
                record_id,
                result_json: &result_json,
            },
            now,
        )
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    let authority = actor.authority_json();
    repository
        .audit(
            c,
            actor,
            &MarketingAuditEntry {
                key,
                operation,
                record_type,
                record_id,
                version,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            c,
            &MarketingOutboxEntry {
                key,
                operation,
                record_type,
                record_id,
                version,
                state,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    Ok(())
}

pub async fn create_campaign<R: MarketingLeadRepository + ?Sized>(
    actor: &MarketingLeadActor,
    input: &MarketingCampaignCreateInput,
    key: &str,
    repository: &mut R,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<MarketingCommandReceipt, PgTransactionError<BusinessError>> {
    actor.require_write(now)?;
    input.validate()?;
    let id = format!("campaign-{key}");
    let identity = serde_json::json!({
        "operation": "marketing.campaign.create",
        "actor": actor.user_id,
        "company": actor.company_id,
        "code": input.code(),
        "name": input.name(),
    })
    .to_string();
    let start = begin(
        repository,
        c,
        actor,
        key,
        &identity,
        "marketing.campaign.create",
        &id,
        now,
    )
    .await?;
    if let Some(prior) = start.prior {
        return Ok(prior);
    }
    if !repository
        .campaign_create(c, &id, actor, input, now.get())
        .await?
    {
        return Err(campaign_code_taken().into());
    }
    let readback = repository
        .campaign_readback(c, &actor.company_id, &id)
        .await?
        .ok_or_else(readback_invalid)?;
    if readback.code != input.code() || readback.name != input.name() || readback.version != 1 {
        return Err(readback_invalid().into());
    }
    let receipt = MarketingCommandReceipt {
        id: id.clone(),
        status: String::new(),
        version: 1,
        crm_lead_id: None,
        code: Some(readback.code),
        name: Some(readback.name),
    };
    finish(
        repository,
        c,
        actor,
        key,
        "marketing.campaign.create",
        "marketing_campaign",
        &id,
        "created",
        1,
        &start.request_hash,
        &receipt,
        now,
    )
    .await?;
    Ok(receipt)
}

pub async fn create_candidate<R: MarketingLeadRepository + ?Sized>(
    actor: &MarketingLeadActor,
    input: &MarketingLeadCandidateInput,
    key: &str,
    repository: &mut R,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<MarketingCommandReceipt, PgTransactionError<BusinessError>> {
    actor.require_write(now)?;
    input.validate(actor)?;
    let id = format!("marketing-candidate-{key}");
    let identity = serde_json::json!({
        "operation": "marketing.candidate.create",
        "actor": actor.user_id,
        "company": actor.company_id,
        "branch": actor.branch_id.as_ref().map(|value| value.as_str()),
        "formId": input.form_id,
        "formVersion": input.form_version,
        "contactName": input.contact_name,
        "contactDetail": input.contact_detail,
        "consentEvidence": input.consent_evidence,
        "attribution": input.attribution,
        "brief": input.brief,
    })
    .to_string();
    let start = begin(
        repository,
        c,
        actor,
        key,
        &identity,
        "marketing.candidate.create",
        &id,
        now,
    )
    .await?;
    if let Some(prior) = start.prior {
        return Ok(prior);
    }
    let form = repository
        .form_asset(c, &actor.company_id, &input.form_id, input.form_version)
        .await?
        .ok_or_else(form_not_ready)?;
    if !form_configuration_allows_capture(&form.configuration_json)? {
        return Err(form_not_ready().into());
    }
    if form_requires_files_readiness(&form.configuration_json)? {
        return Err(form_readiness_port_required().into());
    }
    let attribution_json = input.attribution_json()?;
    if !repository
        .candidate_create(c, &id, actor, input, &attribution_json, now.get())
        .await?
    {
        return Err(source_invalid().into());
    }
    let engagement_id = format!("marketing-engagement-{key}");
    if !repository
        .insert_engagement(
            c,
            &engagement_id,
            &id,
            1,
            "form_submit",
            "web",
            &input.consent_evidence,
            &actor.user_id,
            now.get(),
        )
        .await?
    {
        return Err(source_invalid().into());
    }
    let readback = repository
        .candidate_readback(c, actor, &id)
        .await?
        .ok_or_else(readback_invalid)?;
    if readback.status != "pending"
        || readback.version != 1
        || readback.form_id != input.form_id
        || readback.form_version != input.form_version
        || readback.branch_id != input.branch_id
        || readback.contact_name != js_trim(&input.contact_name)
        || readback.contact_detail != js_trim(&input.contact_detail)
        || readback.consent_evidence != js_trim(&input.consent_evidence)
        || readback.attribution_json != attribution_json
        || readback.brief != js_trim(&input.brief)
        || readback.created_by != actor.user_id
    {
        return Err(readback_invalid().into());
    }
    let receipt = MarketingCommandReceipt {
        id: id.clone(),
        status: "pending".into(),
        version: 1,
        crm_lead_id: None,
        code: None,
        name: None,
    };
    finish(
        repository,
        c,
        actor,
        key,
        "marketing.candidate.create",
        "marketing_candidate",
        &id,
        "pending",
        1,
        &start.request_hash,
        &receipt,
        now,
    )
    .await?;
    Ok(receipt)
}

pub async fn create_asset<R: MarketingLeadRepository + ?Sized>(
    actor: &MarketingLeadActor,
    input: &crate::domain::lead_candidate::MarketingAssetCreateInput,
    key: &str,
    repository: &mut R,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<MarketingCommandReceipt, PgTransactionError<BusinessError>> {
    actor.require_write(now)?;
    input.validate()?;
    let id = format!("marketing-{}-{key}", input.kind);
    let identity = serde_json::json!({
        "operation": "marketing.asset.create",
        "actor": actor.user_id,
        "company": actor.company_id,
        "kind": input.kind,
        "name": input.name(),
        "configuration": input.configuration_json,
        "preview": input.preview_json,
    })
    .to_string();
    let start = begin(
        repository,
        c,
        actor,
        key,
        &identity,
        "marketing.asset.create",
        &id,
        now,
    )
    .await?;
    if let Some(prior) = start.prior {
        return Ok(prior);
    }
    if !repository
        .asset_create(c, &id, actor, input, now.get())
        .await?
    {
        return Err(source_invalid().into());
    }
    let event_id = format!("marketing-asset-event-{key}");
    if !repository
        .insert_asset_event(
            c,
            &event_id,
            &id,
            1,
            "create",
            &actor.user_id,
            "Asset drafted for independent review",
            "draft",
            now.get(),
        )
        .await?
    {
        return Err(source_invalid().into());
    }
    let readback = repository
        .asset_readback(c, &actor.company_id, &id)
        .await?
        .ok_or_else(readback_invalid)?;
    readback.validate(actor)?;
    if readback.kind != input.kind
        || readback.name != input.name()
        || readback.status != "draft"
        || readback.version != 1
        || readback.configuration_json != input.configuration_json
        || readback.preview_json != input.preview_json
    {
        return Err(readback_invalid().into());
    }
    let receipt = MarketingCommandReceipt {
        id: id.clone(),
        status: "draft".into(),
        version: 1,
        crm_lead_id: None,
        code: None,
        name: Some(readback.name.clone()),
    };
    finish(
        repository,
        c,
        actor,
        key,
        "marketing.asset.create",
        "marketing_asset",
        &id,
        "draft",
        1,
        &start.request_hash,
        &receipt,
        now,
    )
    .await?;
    Ok(receipt)
}

/// Review one draft/active Marketing asset. Activation and rejection require a
/// different reviewer; retirement may be performed by the creator. Form
/// activation keeps the retained consent and Files-readiness gates and fails
/// closed when the Files-owned readiness port is not bound.
pub async fn review_asset<R: MarketingLeadRepository + ?Sized>(
    actor: &MarketingLeadActor,
    asset_id: &str,
    input: &MarketingAssetReviewInput,
    key: &str,
    repository: &mut R,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<MarketingCommandReceipt, PgTransactionError<BusinessError>> {
    actor.require_write(now)?;
    let action = input.validate()?;
    let operation = action.operation();
    let identity = serde_json::json!({
        "operation": operation,
        "actor": actor.user_id,
        "company": actor.company_id,
        "asset": asset_id,
        "action": input.action,
        "expectedVersion": input.expected_version,
        "reason": input.reason,
    })
    .to_string();
    let start = begin(
        repository, c, actor, key, &identity, operation, asset_id, now,
    )
    .await?;
    if let Some(prior) = start.prior {
        return Ok(prior);
    }
    let row = repository
        .asset_review_row(c, &actor.company_id, asset_id, true)
        .await?
        .ok_or_else(asset_not_found)?;
    row.validate(actor)?;
    if action.requires_independent_review() && row.created_by == actor.user_id {
        return Err(independent_asset_review_required().into());
    }
    if row.status != action.expected_status() || row.version != input.expected_version {
        return Err(asset_changed().into());
    }
    if action == MarketingAssetAction::Activate && row.kind == "form" {
        if !form_configuration_allows_capture(&row.configuration_json)? {
            return Err(crate::domain::lead_candidate::invalid(
                "MARKETING_CONSENT_REQUIRED",
                "Lead capture forms must require consent before activation.",
            )
            .into());
        }
        if form_requires_files_readiness(&row.configuration_json)? {
            return Err(form_readiness_port_required().into());
        }
    }
    let status = action.status();
    if !repository
        .asset_review(
            c,
            actor,
            asset_id,
            input.expected_version,
            action.expected_status(),
            status,
            now.get(),
        )
        .await?
    {
        return Err(asset_changed().into());
    }
    let version = input.expected_version + 1;
    let reason = input.reason();
    let event_id = format!("marketing-asset-event-{key}");
    if !repository
        .insert_asset_event(
            c,
            &event_id,
            asset_id,
            version,
            &input.action,
            &actor.user_id,
            &reason,
            status,
            now.get(),
        )
        .await?
    {
        return Err(source_invalid().into());
    }
    let readback = repository
        .asset_readback(c, &actor.company_id, asset_id)
        .await?
        .ok_or_else(readback_invalid)?;
    readback.validate(actor)?;
    if readback.status != status || readback.version != version {
        return Err(readback_invalid().into());
    }
    let receipt = MarketingCommandReceipt {
        id: asset_id.into(),
        status: status.into(),
        version,
        crm_lead_id: None,
        code: None,
        name: Some(readback.name),
    };
    finish(
        repository,
        c,
        actor,
        key,
        operation,
        "marketing_asset",
        asset_id,
        status,
        version,
        &start.request_hash,
        &receipt,
        now,
    )
    .await?;
    Ok(receipt)
}

pub async fn record_engagement<R: MarketingLeadRepository + ?Sized>(
    actor: &MarketingLeadActor,
    candidate_id: &str,
    input: &MarketingLeadActionInput,
    key: &str,
    repository: &mut R,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<MarketingCommandReceipt, PgTransactionError<BusinessError>> {
    actor.require_write(now)?;
    if input.validate(now.get())? != MarketingLeadAction::RecordEngagement {
        return Err(request_invalid().into());
    }
    let identity = serde_json::json!({
        "operation": "marketing.candidate.engagement",
        "actor": actor.user_id,
        "candidate": candidate_id,
        "expectedVersion": input.expected_version,
        "eventType": input.event_type,
        "channel": input.channel,
        "reason": input.reason,
    })
    .to_string();
    let start = begin(
        repository,
        c,
        actor,
        key,
        &identity,
        "marketing.candidate.engagement",
        candidate_id,
        now,
    )
    .await?;
    if let Some(prior) = start.prior {
        return Ok(prior);
    }
    let row = repository
        .candidate(c, actor, candidate_id, true)
        .await?
        .ok_or_else(candidate_not_found)?;
    row.validate(actor)?;
    if row.version != input.expected_version || row.status != "pending" {
        return Err(candidate_changed().into());
    }
    if !repository
        .update_candidate_engagement(c, actor, candidate_id, input.expected_version, now.get())
        .await?
    {
        return Err(candidate_changed().into());
    }
    let version = input.expected_version + 1;
    let engagement_id = format!("marketing-engagement-{key}");
    if !repository
        .insert_engagement(
            c,
            &engagement_id,
            candidate_id,
            version,
            input.event_type.as_deref().unwrap_or_default(),
            input.channel.as_deref().unwrap_or_default(),
            js_trim(&input.reason),
            &actor.user_id,
            now.get(),
        )
        .await?
    {
        return Err(source_invalid().into());
    }
    let readback = repository
        .candidate_readback(c, actor, candidate_id)
        .await?
        .ok_or_else(readback_invalid)?;
    let history = repository.history(c, candidate_id, 500).await?;
    if readback.version != version
        || readback.status != "pending"
        || !history.iter().any(|event| {
            event.id == engagement_id
                && event.version == version
                && event.event_type == input.event_type.as_deref().unwrap_or_default()
                && event.channel == input.channel.as_deref().unwrap_or_default()
                && event.evidence == js_trim(&input.reason)
        })
    {
        return Err(readback_invalid().into());
    }
    let receipt = MarketingCommandReceipt {
        id: candidate_id.into(),
        status: readback.status,
        version,
        crm_lead_id: None,
        code: None,
        name: None,
    };
    finish(
        repository,
        c,
        actor,
        key,
        "marketing.candidate.engagement",
        "marketing_candidate",
        candidate_id,
        "pending",
        version,
        &start.request_hash,
        &receipt,
        now,
    )
    .await?;
    Ok(receipt)
}

#[allow(clippy::too_many_arguments)]
pub async fn handoff_candidate<
    R: MarketingLeadRepository + ?Sized,
    C: MarketingCrmHandoffPort + ?Sized,
>(
    actor: &MarketingLeadActor,
    candidate_id: &str,
    input: &MarketingLeadActionInput,
    key: &str,
    repository: &mut R,
    crm: &mut C,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<MarketingCommandReceipt, PgTransactionError<BusinessError>> {
    actor.require_write(now)?;
    if input.validate(now.get())? != MarketingLeadAction::Handoff {
        return Err(request_invalid().into());
    }
    let identity = serde_json::json!({
        "operation": "marketing.candidate.handoff",
        "actor": actor.user_id,
        "candidate": candidate_id,
        "expectedVersion": input.expected_version,
        "reason": input.reason,
        "dueDate": input.due_date,
    })
    .to_string();
    let start = begin(
        repository,
        c,
        actor,
        key,
        &identity,
        "marketing.candidate.handoff",
        candidate_id,
        now,
    )
    .await?;
    if let Some(prior) = start.prior {
        return Ok(prior);
    }
    let row = repository
        .candidate(c, actor, candidate_id, true)
        .await?
        .ok_or_else(candidate_not_found)?;
    row.validate(actor)?;
    if row.version != input.expected_version || row.status != "pending" {
        return Err(candidate_changed().into());
    }
    if row.created_by == actor.user_id {
        return Err(independent_review_required().into());
    }
    let request = crm_request(&row, input.due_date.as_deref(), now.get())?;
    let crm_key = format!("{key}-crm");
    let crm_lead = crm.create_lead(c, &request, &crm_key, now.get()).await?;
    if crm_lead.lead_id.is_empty() || crm_lead.lead_id.len() > 200 {
        return Err(source_invalid().into());
    }
    let version = input.expected_version + 1;
    if !repository
        .update_candidate_handoff(
            c,
            actor,
            candidate_id,
            input.expected_version,
            &crm_lead.lead_id,
            now.get(),
        )
        .await?
    {
        return Err(candidate_changed().into());
    }
    let engagement_id = format!("marketing-engagement-{key}");
    if !repository
        .insert_engagement(
            c,
            &engagement_id,
            candidate_id,
            version,
            "crm_handoff",
            "internal",
            js_trim(&input.reason),
            &actor.user_id,
            now.get(),
        )
        .await?
    {
        return Err(source_invalid().into());
    }
    let readback = repository
        .candidate_readback(c, actor, candidate_id)
        .await?
        .ok_or_else(readback_invalid)?;
    if readback.status != "handed_off"
        || readback.version != version
        || readback.crm_lead_id.as_deref() != Some(crm_lead.lead_id.as_str())
    {
        return Err(readback_invalid().into());
    }
    let receipt = MarketingCommandReceipt {
        id: candidate_id.into(),
        status: "handed_off".into(),
        version,
        crm_lead_id: Some(crm_lead.lead_id),
        code: None,
        name: None,
    };
    finish(
        repository,
        c,
        actor,
        key,
        "marketing.candidate.handoff",
        "marketing_candidate",
        candidate_id,
        "handed_off",
        version,
        &start.request_hash,
        &receipt,
        now,
    )
    .await?;
    Ok(receipt)
}

pub async fn asset_profiles<R: MarketingLeadRepository + ?Sized>(
    actor: &MarketingLeadActor,
    repository: &mut R,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<MarketingAssetProfiles, PgTransactionError<BusinessError>> {
    actor.require_read(now)?;
    repository.ready(c).await?;
    let assets = repository
        .asset_profiles(
            c,
            &actor.company_id,
            crate::domain::lead_candidate::ASSET_LIMIT + 1,
        )
        .await?;
    if assets.len() > crate::domain::lead_candidate::ASSET_LIMIT {
        return Err(source_invalid().into());
    }
    for asset in &assets {
        asset.validate(actor)?;
    }
    Ok(MarketingAssetProfiles { assets })
}

pub async fn lead_candidates<R: MarketingLeadRepository + ?Sized>(
    actor: &MarketingLeadActor,
    repository: &mut R,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<Vec<MarketingLeadCandidateRow>, PgTransactionError<BusinessError>> {
    actor.require_read(now)?;
    repository.ready(c).await?;
    let candidates = repository
        .candidates(c, actor, crate::domain::lead_candidate::CANDIDATE_LIMIT + 1)
        .await?;
    if candidates.len() > crate::domain::lead_candidate::CANDIDATE_LIMIT {
        return Err(source_invalid().into());
    }
    for candidate in &candidates {
        candidate.validate(actor)?;
    }
    Ok(candidates)
}

pub async fn lead_detail<R: MarketingLeadRepository + ?Sized>(
    actor: &MarketingLeadActor,
    candidate_id: &str,
    repository: &mut R,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<MarketingLeadDetail, PgTransactionError<BusinessError>> {
    actor.require_read(now)?;
    repository.ready(c).await?;
    let candidate = repository
        .candidate(c, actor, candidate_id, false)
        .await?
        .ok_or_else(candidate_not_found)?;
    candidate.validate(actor)?;
    let history = repository
        .history(
            c,
            candidate_id,
            crate::domain::lead_candidate::HISTORY_LIMIT + 1,
        )
        .await?;
    if history.len() > crate::domain::lead_candidate::HISTORY_LIMIT {
        return Err(source_invalid().into());
    }
    for event in &history {
        event.validate(candidate_id)?;
    }
    Ok(MarketingLeadDetail { candidate, history })
}
