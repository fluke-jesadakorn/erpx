//! Brand identity read use cases. The workspace is assembled from one retained
//! authorization unit; no deserialized actor or cached authority is accepted.
use crate::application::identity::BrandActor;
use crate::domain::identity::{
    ActiveBrandIdentity, ActiveBrandIdentityRow, BrandCategoryLock, BrandDraft,
    BrandIdentityEvent, BrandIdentityWorkspace, BrandMediaOption, BrandPublished, LegacyBrand,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub trait PublishedBrandIdentityPorts {
    type Error: From<BusinessError> + Send;

    fn available(&mut self) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn active_row(
        &mut self,
    ) -> impl Future<Output = Result<Option<ActiveBrandIdentityRow>, Self::Error>> + Send;
}

/// Read the active identity only where the version table exists. An absent
/// relation means this Holding has no Brand identity lifecycle yet.
pub async fn read_published_brand_identity<P: PublishedBrandIdentityPorts>(
    ports: &mut P,
) -> Result<Option<ActiveBrandIdentity>, P::Error> {
    if !ports.available().await? {
        return Ok(None);
    }
    let row = ports.active_row().await?;
    match row {
        Some(row) => Ok(Some(row.into_identity()?)),
        None => Ok(None),
    }
}

pub trait BrandIdentityReadPorts {
    type Error: From<BusinessError> + Send;

    fn actor(&self) -> &BrandActor;
    fn authorize(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn drafts(
        &mut self,
        include_others: bool,
    ) -> impl Future<Output = Result<Vec<BrandDraft>, Self::Error>> + Send;
    fn versions(&mut self) -> impl Future<Output = Result<Vec<BrandPublished>, Self::Error>> + Send;
    fn locks(&mut self) -> impl Future<Output = Result<Vec<BrandCategoryLock>, Self::Error>> + Send;
    fn history(&mut self) -> impl Future<Output = Result<Vec<BrandIdentityEvent>, Self::Error>> + Send;
    fn active(
        &mut self,
    ) -> impl Future<Output = Result<Option<ActiveBrandIdentity>, Self::Error>> + Send;
    fn legacy(&mut self) -> impl Future<Output = Result<LegacyBrand, Self::Error>> + Send;
    fn media(
        &mut self,
    ) -> impl Future<Output = Result<Vec<BrandMediaOption>, Self::Error>> + Send;
}

/// Read the Brand workspace exactly as the selected role may see it. History is
/// retained Company evidence but is only disclosed to identity managers.
pub async fn read_brand_identity_workspace<P: BrandIdentityReadPorts>(
    ports: &mut P,
) -> Result<BrandIdentityWorkspace, P::Error> {
    ports.authorize().await?;
    let actor = ports.actor();
    let actor_id = actor.user_id().to_owned();
    let can_manage = actor.can_manage();
    let can_lock = actor.can_lock();
    let drafts = ports.drafts(can_manage).await?;
    let versions = ports.versions().await?;
    let locks = ports.locks().await?;
    let history = ports.history().await?;
    let active = ports.active().await?;
    let legacy = ports.legacy().await?;
    let media = ports.media().await?;
    let initial = match &active {
        Some(active) => active.identity.clone(),
        None => crate::domain::identity::BrandIdentity {
            identity: crate::domain::identity::IdentitySection {
                short_name: legacy.short_name,
                tagline: String::new(),
            },
            visual: crate::domain::identity::VisualSection {
                accent_color: legacy.accent_color,
                logo_publication_id: None,
            },
        },
    };
    Ok(BrandIdentityWorkspace {
        drafts,
        versions,
        locks,
        history: if can_manage { history } else { Vec::new() },
        active,
        initial,
        media,
        actor_id,
        can_manage,
        can_lock,
    })
}
