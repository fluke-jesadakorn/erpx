//! Branding owner use cases. Ports own current authority, replay, snapshots and
//! the transaction; this layer never opens a transaction or reads HTTP.
use crate::adapters::identity_use_cases::BrandActor;
use crate::domain::branding::{
    BrandMediaInput, BrandPresentationContext, BrandProfileEvidence, BrandScopeKind, BrandVariant,
    BrandingAttachment, BrandingBranch, BrandingProfile, BrandingProfiles, BrandingPublication,
    BrandingStorefront, BrandingWorkspace, DocumentBrand, DocumentMode, StorefrontAppBrand,
    UpdateProfileInput, UpdateStorefrontInput, WorkspaceBrand, brand_presentation,
    storefront_presentation,
};
use crate::domain::identity::ActiveBrandIdentity;
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{CommandField, CommandJson};
use serde::Serialize;
use serde_json::{Value, json};
use std::future::Future;

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn input_json<T: Serialize>(input: &T) -> Result<CommandJson, BusinessError> {
    CommandJson::from_serializable(input)
}

#[derive(Clone, Debug)]
pub struct BrandingMutationCommon {
    pub key: String,
    pub operation: &'static str,
    pub record_id: String,
    pub version: i64,
    pub input: CommandJson,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub enum BrandingMutationAction {
    Profile {
        input: UpdateProfileInput,
        current: Option<BrandingProfile>,
        previous_attachment_id: Option<String>,
        previous_publication_id: Option<String>,
        result: Value,
    },
    Storefront {
        input: UpdateStorefrontInput,
        current: BrandingStorefront,
        previous_attachment_id: Option<String>,
        previous_publication_id: Option<String>,
        result: Value,
    },
    Media {
        input: BrandMediaInput,
        current_profile: Option<BrandingProfile>,
        current_storefront: Option<BrandingStorefront>,
        previous_attachment_id: Option<String>,
        previous_publication_id: Option<String>,
        result: Value,
    },
}

#[derive(Clone, Debug)]
pub struct BrandingMutation {
    pub common: BrandingMutationCommon,
    pub action: BrandingMutationAction,
}

pub trait BrandingPorts {
    type Error: From<BusinessError> + Send;

    fn actor(&self) -> &BrandActor;
    fn now(&self) -> i64;
    fn assert_media_writable(&self) -> Result<(), BusinessError>;
    fn authorize(
        &mut self,
        write: bool,
        owner: bool,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn replay(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &CommandJson,
    ) -> impl Future<Output = Result<Option<Value>, Self::Error>> + Send;
    fn profile(
        &mut self,
        scope_kind: BrandScopeKind,
        scope_id: &str,
    ) -> impl Future<Output = Result<Option<BrandingProfile>, Self::Error>> + Send;
    fn branch(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<BrandingBranch>, Self::Error>> + Send;
    fn branches(&mut self)
    -> impl Future<Output = Result<Vec<BrandingBranch>, Self::Error>> + Send;
    fn storefront(
        &mut self,
    ) -> impl Future<Output = Result<Option<BrandingStorefront>, Self::Error>> + Send;
    fn published(&mut self) -> impl Future<Output = Result<Option<Value>, Self::Error>> + Send;
    fn attachment(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<BrandingAttachment>, Self::Error>> + Send;
    fn publication(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<BrandingPublication>, Self::Error>> + Send;
    fn commit(
        &mut self,
        mutation: &BrandingMutation,
        result: &Value,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

fn now_or_expected(current: Option<i64>, expected: Option<i64>, now: i64) -> i64 {
    expected
        .map(|value| value.saturating_add(1))
        .or_else(|| current.map(|value| value.saturating_add(1)))
        .unwrap_or(now)
        .min(9_007_199_254_740_991)
}

fn scope_label(scope: BrandScopeKind, id: &str) -> String {
    match scope {
        BrandScopeKind::Holding => format!("holding:{id}"),
        BrandScopeKind::Branch => format!("branch:{id}"),
    }
}

fn expected_owner(scope: BrandScopeKind) -> &'static str {
    match scope {
        BrandScopeKind::Holding => "holding_brand",
        BrandScopeKind::Branch => "branch_brand",
    }
}

fn validate_attachment(
    scope: BrandScopeKind,
    scope_id: &str,
    company_id: &str,
    actor_holding_id: &str,
    variant: Option<BrandVariant>,
    attachment: Option<&BrandingAttachment>,
) -> Result<(), BusinessError> {
    let Some(attachment) = attachment else {
        return Err(conflict(
            "BRAND_MEDIA_NOT_FOUND",
            "Choose an existing approved Brand media source.",
        ));
    };
    let owner_id = match scope {
        BrandScopeKind::Holding => actor_holding_id,
        BrandScopeKind::Branch => scope_id,
    };
    let purpose = variant.map(BrandVariant::as_str).unwrap_or("logo");
    if attachment.owner_type != expected_owner(scope)
        || attachment.owner_id != owner_id
        || attachment.purpose != purpose
        || attachment.revoked_at.is_some()
        || attachment
            .company_id
            .as_deref()
            .is_some_and(|value| value != company_id)
    {
        return Err(conflict(
            "BRAND_MEDIA_NOT_AVAILABLE",
            "The selected media is not available for this Brand owner.",
        ));
    }
    Ok(())
}

fn validate_publication(
    publication: Option<&BrandingPublication>,
    attachment_id: Option<&str>,
    expected_target_type: &str,
    target_id: &str,
    company_id: &str,
) -> Result<(), BusinessError> {
    let Some(publication) = publication else {
        return Err(conflict(
            "BRAND_MEDIA_NOT_APPROVED",
            "Choose an independently approved Brand media publication.",
        ));
    };
    if publication.state != "approved"
        || publication.company_id != company_id
        || publication.target_type != expected_target_type
        || publication.target_id != target_id
        || attachment_id.is_some_and(|id| publication.source_attachment_id.as_deref() != Some(id))
    {
        return Err(conflict(
            "BRAND_MEDIA_NOT_APPROVED",
            "Choose an independently approved Brand media publication.",
        ));
    }
    Ok(())
}

/// Update a Holding or Branch branding profile or remove one of its media
/// references. The command key and request hash are retained by the adapter.
pub async fn update_brand_profile<P: BrandingPorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    let input = UpdateProfileInput::parse(raw)?;
    let actor = ports.actor().clone();
    if input.remove.is_some() {
        ports.assert_media_writable()?;
    }
    if input.scope_kind == BrandScopeKind::Holding && input.scope_id != actor.holding_id {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "BRAND_MANAGEMENT_FORBIDDEN",
            "A Holding owner or authorized organization administrator is required.",
        )
        .into());
    }
    ports
        .authorize(true, input.scope_kind == BrandScopeKind::Holding)
        .await?;
    if input.scope_kind == BrandScopeKind::Branch && ports.branch(&input.scope_id).await?.is_none()
    {
        return Err(conflict("BRANCH_NOT_FOUND", "The document Branch is unavailable.").into());
    }
    let operation = "organization.brand.update";
    let record_id = scope_label(input.scope_kind, &input.scope_id);
    let command_input = input_json(&input)?;
    if let Some(prior) = ports
        .replay(key, operation, &record_id, &command_input)
        .await?
    {
        return Ok(prior);
    }
    let current = ports.profile(input.scope_kind, &input.scope_id).await?;
    let previous_attachment_id = match input.remove {
        Some(BrandVariant::Primary) => current
            .as_ref()
            .and_then(|row| row.primary_attachment_id.clone()),
        Some(BrandVariant::Inverse) => current
            .as_ref()
            .and_then(|row| row.inverse_attachment_id.clone()),
        None => None,
    };
    let previous_publication_id = match input.remove {
        Some(BrandVariant::Primary) => current
            .as_ref()
            .and_then(|row| row.primary_publication_id.clone()),
        Some(BrandVariant::Inverse) => current
            .as_ref()
            .and_then(|row| row.inverse_publication_id.clone()),
        None => None,
    };
    let document_mode = match input.scope_kind {
        BrandScopeKind::Holding => DocumentMode::Holding,
        BrandScopeKind::Branch => input
            .document_mode
            .or_else(|| {
                current
                    .as_ref()
                    .and_then(|row| DocumentMode::parse(&row.document_mode))
            })
            .unwrap_or(DocumentMode::Branch),
    };
    let result = json!({
        "scopeKind": input.scope_kind.as_str(),
        "scopeId": input.scope_id.clone(),
        "documentMode": document_mode.as_str(),
        "removedVariant": input.remove.map(BrandVariant::as_str),
    });
    let now = ports.now();
    let version = now_or_expected(
        current.as_ref().map(|row| row.updated_at),
        input.expected_updated_at,
        now,
    );
    ports
        .commit(
            &BrandingMutation {
                common: BrandingMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version,
                    input: command_input,
                    now,
                },
                action: BrandingMutationAction::Profile {
                    input,
                    current,
                    previous_attachment_id,
                    previous_publication_id,
                    result: result.clone(),
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Update the storefront name, accent or logo. Published identity is owned by
/// the native identity command and remains authoritative while active.
pub async fn update_storefront<P: BrandingPorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    let input = UpdateStorefrontInput::parse(raw)?;
    if input.remove_logo.unwrap_or(false) {
        ports.assert_media_writable()?;
    }
    ports.authorize(true, false).await?;
    if ports.published().await?.is_some() {
        return Err(conflict(
            "BRAND_PUBLISHED_IDENTITY_CONTROLLED",
            "Save and independently publish a Brand identity draft to change the current storefront identity.",
        )
        .into());
    }
    let current = ports
        .storefront()
        .await?
        .ok_or_else(|| conflict("COMPANY_REQUIRED", "The Company storefront is unavailable."))?;
    let operation = "organization.storefront-brand.update";
    let record_id = current.company_id.clone();
    let command_input = input_json(&input)?;
    if let Some(prior) = ports
        .replay(key, operation, &record_id, &command_input)
        .await?
    {
        return Ok(prior);
    }
    let result = json!({
        "companyId": current.company_id.clone(),
        "shortName": input.short_name.clone().or_else(|| current.short_name.clone()).unwrap_or_else(|| current.company_name.chars().take(40).collect()),
        "accentColor": input.accent_color.clone().or_else(|| current.accent_color.clone()).unwrap_or_else(|| "#3659c9".into()),
        "logoRemoved": input.remove_logo.unwrap_or(false),
    });
    let previous_attachment_id = if input.remove_logo.unwrap_or(false) {
        current.logo_attachment_id.clone()
    } else {
        None
    };
    let previous_publication_id = if input.remove_logo.unwrap_or(false) {
        current.logo_publication_id.clone()
    } else {
        None
    };
    let now = ports.now();
    let version = now_or_expected(Some(current.updated_at), input.expected_updated_at, now);
    ports
        .commit(
            &BrandingMutation {
                common: BrandingMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version,
                    input: command_input,
                    now,
                },
                action: BrandingMutationAction::Storefront {
                    input,
                    current,
                    previous_attachment_id,
                    previous_publication_id,
                    result: result.clone(),
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Attach an existing approved attachment/publication to a Brand profile or the
/// storefront. Raw byte ingestion and preview generation belong to the Files
/// owner; this command owns the Brand pointer and previous-media revocation.
pub async fn attach_brand_media<P: BrandingPorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    let input = BrandMediaInput::parse(raw)?;
    ports.assert_media_writable()?;
    let actor = ports.actor().clone();
    if input.scope_kind == BrandScopeKind::Holding && input.scope_id != actor.holding_id {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "BRAND_MANAGEMENT_FORBIDDEN",
            "A Holding owner or authorized organization administrator is required.",
        )
        .into());
    }
    ports
        .authorize(true, input.scope_kind == BrandScopeKind::Holding)
        .await?;
    if input.scope_kind == BrandScopeKind::Branch && ports.branch(&input.scope_id).await?.is_none()
    {
        return Err(conflict("BRANCH_NOT_FOUND", "The document Branch is unavailable.").into());
    }
    let attachment = match input.attachment_id.as_deref() {
        Some(id) => ports.attachment(id).await?,
        None => None,
    };
    validate_attachment(
        input.scope_kind,
        &input.scope_id,
        actor.company_id(),
        &actor.holding_id,
        input.variant,
        attachment.as_ref(),
    )?;
    let publication = match input.publication_id.as_deref() {
        Some(id) => ports.publication(id).await?,
        None => None,
    };
    let target_type = match input.variant {
        Some(_) => expected_owner(input.scope_kind),
        None => "company_storefront_brand",
    };
    let target_id: &str = if input.variant.is_some() {
        input.scope_id.as_str()
    } else {
        actor.company_id()
    };
    if input.publication_id.is_some() {
        validate_publication(
            publication.as_ref(),
            input.attachment_id.as_deref(),
            target_type,
            target_id,
            actor.company_id(),
        )?;
    }
    let operation = if input.variant.is_some() {
        "organization.brand.media"
    } else {
        "organization.storefront-brand.media"
    };
    let record_id = if input.variant.is_some() {
        scope_label(input.scope_kind, &input.scope_id)
    } else {
        actor.company_id().to_owned()
    };
    let command_input = input_json(&input)?;
    if let Some(prior) = ports
        .replay(key, operation, &record_id, &command_input)
        .await?
    {
        return Ok(prior);
    }
    let current_profile = if input.variant.is_some() {
        ports.profile(input.scope_kind, &input.scope_id).await?
    } else {
        None
    };
    let current_storefront = if input.variant.is_none() {
        ports.storefront().await?
    } else {
        None
    };
    let previous_attachment_id = match input.variant {
        Some(BrandVariant::Primary) => current_profile
            .as_ref()
            .and_then(|row| row.primary_attachment_id.clone()),
        Some(BrandVariant::Inverse) => current_profile
            .as_ref()
            .and_then(|row| row.inverse_attachment_id.clone()),
        None => current_storefront
            .as_ref()
            .and_then(|row| row.logo_attachment_id.clone()),
    };
    let previous_publication_id = match input.variant {
        Some(BrandVariant::Primary) => current_profile
            .as_ref()
            .and_then(|row| row.primary_publication_id.clone()),
        Some(BrandVariant::Inverse) => current_profile
            .as_ref()
            .and_then(|row| row.inverse_publication_id.clone()),
        None => current_storefront
            .as_ref()
            .and_then(|row| row.logo_publication_id.clone()),
    };
    let result = json!({
        "scopeKind": input.scope_kind.as_str(),
        "scopeId": input.scope_id.clone(),
        "variant": input.variant.map(BrandVariant::as_str),
        "attachmentId": input.attachment_id.clone(),
        "publicationId": input.publication_id.clone(),
        "previousAttachmentId": previous_attachment_id.clone(),
        "previousPublicationId": previous_publication_id.clone(),
    });
    let current_updated = if let Some(profile) = &current_profile {
        Some(profile.updated_at)
    } else {
        current_storefront.as_ref().map(|row| row.updated_at)
    };
    let now = ports.now();
    let version = now_or_expected(current_updated, input.expected_updated_at, now);
    ports
        .commit(
            &BrandingMutation {
                common: BrandingMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version,
                    input: command_input,
                    now,
                },
                action: BrandingMutationAction::Media {
                    input,
                    current_profile,
                    current_storefront,
                    previous_attachment_id,
                    previous_publication_id,
                    result: result.clone(),
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Read the Branding administration workspace. The caller retains current
/// records/read authority; the response keeps stable IDs and the published
/// identity overlay separately so the HTTP client can build the retained URLs.
pub async fn read_branding_workspace<P: BrandingPorts>(
    ports: &mut P,
) -> Result<BrandingWorkspace, P::Error> {
    ports.authorize(false, false).await?;
    let actor = ports.actor().clone();
    let storefront = ports
        .storefront()
        .await?
        .ok_or_else(|| conflict("COMPANY_REQUIRED", "The Company storefront is unavailable."))?;
    let holding_profile = ports
        .profile(BrandScopeKind::Holding, &actor.holding_id)
        .await?;
    let branches = ports.branches().await?;
    let mut branch_rows = Vec::with_capacity(branches.len());
    for branch in branches {
        let profile = ports.profile(BrandScopeKind::Branch, &branch.id).await?;
        branch_rows.push((branch, profile));
    }
    let published = ports.published().await?;
    Ok(BrandingWorkspace {
        holding_id: actor.holding_id.clone(),
        holding_name: actor.holding_id.clone(),
        company_id: storefront.company_id.clone(),
        company_slug: storefront.company_slug.clone(),
        company_name: storefront.company_name.clone(),
        holding_profile,
        branches: branch_rows,
        storefront,
        published_identity: published,
    })
}

pub fn branding_version(now: i64) -> Result<i64, BusinessError> {
    if (0..=9_007_199_254_740_991).contains(&now) {
        Ok(now)
    } else {
        Err(unavailable(
            "BRAND_VERSION_INVALID",
            "The Branding version is unavailable.",
        ))
    }
}

fn company_required() -> BusinessError {
    conflict("COMPANY_REQUIRED", "The Company storefront is unavailable.")
}

fn branch_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "BRANCH_NOT_FOUND",
        "The document Branch is unavailable.",
    )
}

fn selected_branch(actor: &BrandActor) -> Option<&str> {
    match &actor.command.branch_id {
        CommandField::Value(branch) if !branch.is_empty() => Some(branch.as_str()),
        _ => None,
    }
}

/// Native presentation reads. The caller retains the current Holding transaction
/// and WorkContext; every query is Company-scoped inside the adapter.
pub trait BrandingPresentationPorts {
    type Error: From<BusinessError> + Send;

    fn actor(&self) -> &BrandActor;
    fn authorize(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn profile(
        &mut self,
        scope_kind: BrandScopeKind,
        scope_id: &str,
    ) -> impl Future<Output = Result<Option<BrandingProfile>, Self::Error>> + Send;
    fn profiles(
        &mut self,
    ) -> impl Future<Output = Result<Vec<BrandingProfile>, Self::Error>> + Send;
    fn branch(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<BrandingBranch>, Self::Error>> + Send;
    fn branches(&mut self)
    -> impl Future<Output = Result<Vec<BrandingBranch>, Self::Error>> + Send;
    fn storefront(
        &mut self,
    ) -> impl Future<Output = Result<Option<BrandingStorefront>, Self::Error>> + Send;
    fn active_identity(
        &mut self,
    ) -> impl Future<Output = Result<Option<ActiveBrandIdentity>, Self::Error>> + Send;
    fn evidence(
        &mut self,
        branch_id: &str,
    ) -> impl Future<Output = Result<Option<BrandProfileEvidence>, Self::Error>> + Send;
}

/// The workspace brand for the selected scope. A selected Branch that is not an
/// active Branch of this Company falls back to the Holding brand exactly as the
/// retained reader did.
pub async fn resolve_workspace_brand<P: BrandingPresentationPorts>(
    ports: &mut P,
    context: &BrandPresentationContext,
) -> Result<WorkspaceBrand, P::Error> {
    ports.authorize().await?;
    let actor = ports.actor().clone();
    let branch_id = selected_branch(ports.actor()).map(str::to_owned);
    if let Some(branch_id) = branch_id
        && let Some(branch) = ports.branch(&branch_id).await?
    {
        let profile = ports
            .profile(BrandScopeKind::Branch, &branch.id)
            .await?;
        return Ok(brand_presentation(
            BrandScopeKind::Branch,
            &branch.id,
            &branch.display_name,
            profile.as_ref(),
            context.media(),
            context.legacy_media_readable(),
        ));
    }
    let profile = ports
        .profile(BrandScopeKind::Holding, &actor.holding_id)
        .await?;
    Ok(brand_presentation(
        BrandScopeKind::Holding,
        &actor.holding_id,
        context.holding_name(),
        profile.as_ref(),
        context.media(),
        context.legacy_media_readable(),
    ))
}

/// The document brand choice. A document without a Branch uses Holding mode and
/// ignores a requested Branch mode; a Branch document defaults to its stored
/// document mode.
pub async fn resolve_document_brand<P: BrandingPresentationPorts>(
    ports: &mut P,
    context: &BrandPresentationContext,
    branch_id: Option<&str>,
    requested_mode: Option<&str>,
) -> Result<DocumentBrand, P::Error> {
    ports.authorize().await?;
    let actor = ports.actor().clone();
    let storefront = ports.storefront().await?.ok_or_else(company_required)?;
    let holding_profile = ports
        .profile(BrandScopeKind::Holding, &actor.holding_id)
        .await?;
    let holding = brand_presentation(
        BrandScopeKind::Holding,
        &actor.holding_id,
        context.holding_name(),
        holding_profile.as_ref(),
        context.media(),
        context.legacy_media_readable(),
    );
    let Some(branch_id) = branch_id else {
        return Ok(DocumentBrand {
            mode: DocumentMode::Holding.as_str().into(),
            company_name: storefront.company_name,
            branch_name: None,
            holding,
            branch: None,
        });
    };
    let branch = ports.branch(branch_id).await?.ok_or_else(branch_not_found)?;
    let profile = ports.profile(BrandScopeKind::Branch, &branch.id).await?;
    let mode = requested_mode
        .and_then(DocumentMode::parse)
        .or_else(|| {
            profile
                .as_ref()
                .and_then(|row| DocumentMode::parse(&row.document_mode))
        })
        .unwrap_or(DocumentMode::Branch);
    let presentation = brand_presentation(
        BrandScopeKind::Branch,
        &branch.id,
        &branch.display_name,
        profile.as_ref(),
        context.media(),
        context.legacy_media_readable(),
    );
    Ok(DocumentBrand {
        mode: mode.as_str().into(),
        company_name: storefront.company_name,
        branch_name: Some(branch.display_name),
        holding,
        branch: Some(presentation),
    })
}

/// The Branding administration projection. Holding and Branch profiles are
/// restricted to this Holding/Company so another tenant's values cannot win.
pub async fn list_brand_profiles<P: BrandingPresentationPorts>(
    ports: &mut P,
    context: &BrandPresentationContext,
) -> Result<BrandingProfiles, P::Error> {
    ports.authorize().await?;
    let actor = ports.actor().clone();
    let branches = ports.branches().await?;
    let profiles = ports.profiles().await?;
    let storefront = ports.storefront().await?.ok_or_else(company_required)?;
    let published = ports.active_identity().await?;
    let find = |kind: BrandScopeKind, id: &str| {
        profiles
            .iter()
            .find(|row| row.scope_kind == kind.as_str() && row.scope_id == id)
    };
    let holding = brand_presentation(
        BrandScopeKind::Holding,
        &actor.holding_id,
        context.holding_name(),
        find(BrandScopeKind::Holding, &actor.holding_id),
        context.media(),
        context.legacy_media_readable(),
    );
    let branches = branches
        .iter()
        .map(|branch| {
            brand_presentation(
                BrandScopeKind::Branch,
                &branch.id,
                &branch.display_name,
                find(BrandScopeKind::Branch, &branch.id),
                context.media(),
                context.legacy_media_readable(),
            )
        })
        .collect();
    Ok(BrandingProfiles {
        holding,
        branches,
        storefront: storefront_presentation(
            &storefront.company_id,
            context,
            &storefront,
            published.as_ref(),
            context.media(),
        ),
    })
}

/// The Company storefront brand for the front-app shell.
pub async fn read_storefront_app_brand<P: BrandingPresentationPorts>(
    ports: &mut P,
    context: &BrandPresentationContext,
) -> Result<StorefrontAppBrand, P::Error> {
    ports.authorize().await?;
    let company_id = ports.actor().company_id().to_owned();
    let storefront = ports.storefront().await?.ok_or_else(company_required)?;
    let published = ports.active_identity().await?;
    Ok(storefront_presentation(
        &company_id,
        context,
        &storefront,
        published.as_ref(),
        context.media(),
    ))
}

/// Retained executive Brand evidence: the active Branch profile joined to its
/// Company Branch record. `None` keeps the retained reader's null row.
pub async fn read_brand_profile_evidence<P: BrandingPresentationPorts>(
    ports: &mut P,
    branch_id: &str,
) -> Result<Option<BrandProfileEvidence>, P::Error> {
    ports.authorize().await?;
    ports.evidence(branch_id).await
}
