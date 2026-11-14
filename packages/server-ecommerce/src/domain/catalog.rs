//! Native Ecommerce storefront catalog read facts.
//!
//! The qualified read returns the current Company's channel drafts/active
//! records and listing drafts/published records. It never reads product,
//! customer, warehouse, Sales-order or package rows from another owner. The
//! retained workspace read remains authoritative for cart/checkout rows and
//! for the candidate options used by create forms.
use super::actor::EcommerceActor;
use super::command::{
    js_trim, source_invalid, valid_configuration, valid_cp_text, valid_currency, valid_identifier, valid_text,
};
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use serde::Serialize;

pub const CATALOG_LIMIT: usize = 500;
const NAME_LIMIT: usize = 160;
const TITLE_LIMIT: usize = 200;
const REVIEWER_LIMIT: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EcommerceChannelRow {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub channel_type: String,
    pub configuration_json: String,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EcommerceListingRow {
    pub id: String,
    pub channel_id: String,
    pub product_id: String,
    pub title: String,
    pub currency: String,
    pub price_minor: i64,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StorefrontCatalog {
    pub channels: Vec<EcommerceChannelRow>,
    pub listings: Vec<EcommerceListingRow>,
}

pub(crate) fn too_large() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_CATALOG_TOO_LARGE",
        "The Ecommerce storefront catalog is too large to load at once.",
    )
}

fn valid_channel_type(value: &str) -> bool {
    matches!(value, "web" | "marketplace" | "social")
}

fn valid_channel_status(value: &str) -> bool {
    matches!(value, "draft" | "active" | "rejected" | "retired")
}

fn valid_listing_status(value: &str) -> bool {
    matches!(value, "draft" | "published" | "rejected" | "retired")
}

fn valid_owner(value: &str) -> bool {
    valid_identifier(value)
}

fn valid_timestamp_pair(created_at: i64, updated_at: i64) -> bool {
    created_at >= 0 && updated_at >= created_at
}

impl EcommerceChannelRow {
    fn validate(&self, actor: &EcommerceActor) -> Result<(), BusinessError> {
        if self.company_id != actor.company_id.as_str()
            || !valid_identifier(&self.id)
            || !valid_cp_text(&self.name, NAME_LIMIT)
            || js_trim(&self.name).is_empty()
            || !valid_channel_type(&self.channel_type)
            || !valid_configuration(&self.configuration_json)
            || !valid_channel_status(&self.status)
            || !valid_owner(&self.created_by)
            || self
                .reviewed_by
                .as_deref()
                .is_some_and(|value| !valid_text(value, REVIEWER_LIMIT))
            || self.version < 1
            || !valid_timestamp_pair(self.created_at, self.updated_at)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

impl EcommerceListingRow {
    fn validate(&self, channel: &EcommerceChannelRow) -> Result<(), BusinessError> {
        if !valid_identifier(&self.id)
            || self.channel_id != channel.id
            || !valid_identifier(&self.product_id)
            || !valid_text(&self.title, TITLE_LIMIT)
            || js_trim(&self.title).is_empty()
            || !valid_currency(&self.currency)
            || self.price_minor < 0
            || !valid_listing_status(&self.status)
            || !valid_owner(&self.created_by)
            || self
                .reviewed_by
                .as_deref()
                .is_some_and(|value| !valid_text(value, REVIEWER_LIMIT))
            || self.version < 1
            || !valid_timestamp_pair(self.created_at, self.updated_at)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

fn ordered<T>(rows: &[T], key: impl Fn(&T) -> (i64, &str)) -> bool {
    rows.windows(2).all(|pair| key(&pair[0]) > key(&pair[1]))
}

/// Validate the bounded owner page and Company isolation before presentation.
/// Listings are bound to a channel in the same page so a foreign or missing
/// Company channel cannot be presented as a current listing.
pub fn validate_catalog(
    actor: &EcommerceActor,
    channels: Vec<EcommerceChannelRow>,
    listings: Vec<EcommerceListingRow>,
    now: TimestampMillis,
) -> Result<StorefrontCatalog, BusinessError> {
    actor.require_read(now)?;
    if channels.len() > CATALOG_LIMIT || listings.len() > CATALOG_LIMIT {
        return Err(too_large());
    }
    if !ordered(&channels, |row| (row.updated_at, row.id.as_str()))
        || !ordered(&listings, |row| (row.updated_at, row.id.as_str()))
    {
        return Err(source_invalid());
    }
    let mut seen_channels = std::collections::BTreeSet::new();
    for channel in &channels {
        channel.validate(actor)?;
        if !seen_channels.insert(channel.id.as_str()) {
            return Err(source_invalid());
        }
    }
    let mut seen_listings = std::collections::BTreeSet::new();
    let mut seen_pairs = std::collections::BTreeSet::new();
    for listing in &listings {
        let channel = channels
            .iter()
            .find(|channel| channel.id == listing.channel_id)
            .ok_or_else(source_invalid)?;
        listing.validate(channel)?;
        if !seen_listings.insert(listing.id.as_str())
            || !seen_pairs.insert((listing.channel_id.as_str(), listing.product_id.as_str()))
        {
            return Err(source_invalid());
        }
    }
    Ok(StorefrontCatalog { channels, listings })
}
