//! Native Ecommerce storefront catalog read use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. This owner reads only `ecommerce_channels` and
//! `ecommerce_listings`; candidate products, customers, warehouses and Sales
//! order facts remain on their existing owner reads.
use crate::application::EcommerceCatalogRepository;
use crate::domain::actor::EcommerceActor;
use crate::domain::catalog::{StorefrontCatalog, validate_catalog};
use erpx_server_shared::public::TimestampMillis;

pub async fn storefront_catalog<R: EcommerceCatalogRepository>(
    actor: &EcommerceActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<StorefrontCatalog, R::Error> {
    actor.require_read(now)?;
    let channels = repository.channels(&actor.company_id).await?;
    let listings = repository.listings(&actor.company_id).await?;
    validate_catalog(actor, channels, listings, now).map_err(Into::into)
}
