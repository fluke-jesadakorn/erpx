//! Native Ecommerce listing draft/create/change command facts.
//!
//! A listing draft is created against a current active Company channel and an
//! active Commerce product, with one Finance unit-price Money snapshot. The
//! source terms `(channel, product, currency, price, creator, created_at)` are
//! immutable after creation (the owner table guard enforces the same rule), so
//! the change command can only retitle a draft. Publication, rejection and
//! retirement stay in [`super::review`], which requires an independent Company
//! reviewer. No cart, checkout, return, Sales order, Inventory or Finance
//! posting is performed here.
use super::actor::EcommerceActor;
use super::command::{
    EcommerceReceipt, chars_len, command_identity, idempotency_conflict, js_trim,
    parse_receipt_result, readback_invalid, request_invalid, source_invalid, valid_currency,
    valid_hash, valid_identifier, valid_text, CommandReceipt,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId, TimestampMillis};
use serde::Serialize;

pub const CREATE_OPERATION: &str = "ecommerce.listing.create";
pub const CHANGE_OPERATION: &str = "ecommerce.listing.change";
pub const RECORD_TYPE: &str = "ecommerce_listing";

const TITLE_MIN: usize = 2;
const TITLE_MAX: usize = 200;
const MONEY_CURRENCY_SCALE: i64 = 2;
/// `decimalToMinor` in the retained Finance money helper rejects amounts above
/// `Number.MAX_SAFE_INTEGER`; the owner command keeps the same bound.
const MAX_PRICE_MINOR: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug)]
pub struct CreateListing {
    pub key: CommandKey,
    pub channel_id: RecordId,
    pub product_id: RecordId,
    pub title: String,
    pub currency: String,
    /// The retained wire form: `^\d+(\.\d{1,2})?$`.
    pub price: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedListing {
    pub channel_id: String,
    pub product_id: String,
    pub title: String,
    pub currency: String,
    pub price_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListingDraft {
    pub id: String,
    pub channel_id: String,
    pub product_id: String,
    pub title: String,
    pub currency: String,
    pub price_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeListing {
    pub key: CommandKey,
    pub id: RecordId,
    pub expected_version: i64,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedListingChange {
    pub id: String,
    pub expected_version: i64,
    pub title: String,
}

/// Owner row read for change CAS and persisted readback. `company_id` comes from
/// the listing's channel because `ecommerce_listings` has no Company column.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListingRecord {
    pub id: String,
    pub company_id: String,
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

/// Commerce/Finance source facts required before a listing row may exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcommerceFunctionalCurrency {
    pub currency: String,
    pub base_currency: String,
}

/// Finance `snapshotMoney` request retained for the listing unit price.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EcommerceListingMoneyRequest {
    pub company_id: String,
    pub listing_id: String,
    pub currency: String,
    pub amount_minor: i64,
}

/// Finance-owned money evidence for one listing unit price.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcommerceListingMoneySnapshot {
    pub currency_scale: i64,
    pub amount_units: i64,
    pub usd_units: i64,
    pub rate_id: Option<String>,
    pub used_latest_fallback: bool,
    pub captured_at: i64,
}

pub fn listing_exists() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ECOMMERCE_LISTING_EXISTS",
        "This channel already lists that product. Open the current listing instead.",
    )
}

pub fn channel_not_active() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ECOMMERCE_CHANNEL_NOT_ACTIVE",
        "Choose a current active channel in this Company before adding a listing.",
    )
}

pub fn product_not_active() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ECOMMERCE_PRODUCT_NOT_ACTIVE",
        "Choose a current active product in this Company before adding a listing.",
    )
}

pub fn currency_mapping_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "ECOMMERCE_CURRENCY_MAPPING_REQUIRED",
        "Use the configured Sales functional currency for this listing.",
    )
}

pub fn source_port_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_LISTING_SOURCE_UNAVAILABLE",
        "The current Commerce source for listing channels and products is not composed for this workspace.",
    )
}

pub fn money_source_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_MONEY_SOURCE_UNAVAILABLE",
        "The Finance unit-price Money snapshot producer is not composed for this workspace.",
    )
}

pub fn money_evidence_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_MONEY_EVIDENCE_INVALID",
        "The Finance unit-price snapshot did not match this listing command.",
    )
}

pub fn author_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ECOMMERCE_LISTING_AUTHOR_REQUIRED",
        "Only the draft author can change it. Send an independent review to publish or reject it.",
    )
}

/// Resolve the bounded title rule shared by create and change. The retained Zod
/// schema trims and limits to 200 code points; the catalog read additionally
/// rejects control characters and titles longer than 200 UTF-16 units, so both
/// rules are applied here to keep a written row readable.
fn valid_title(value: &str) -> Option<String> {
    let title = js_trim(value);
    let length = chars_len(title);
    ((TITLE_MIN..=TITLE_MAX).contains(&length) && valid_text(title, TITLE_MAX))
        .then(|| title.to_owned())
}

/// Retained `listingSchema` currency rule: exactly three UTF-16 units, then
/// upper-cased into the owner `[A-Z]{3}` currency form.
fn valid_listing_currency(value: &str) -> Option<String> {
    if value.encode_utf16().count() != 3 {
        return None;
    }
    let currency = value.to_ascii_uppercase();
    valid_currency(&currency).then_some(currency)
}

/// Exact retained price rule: `^\d+(\.\d{1,2})?$` into minor units, with the
/// retained safe-integer bound.
fn minor_price(value: &str) -> Option<i64> {
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = match parts.next() {
        Some(fraction)
            if (1..=2).contains(&fraction.len())
                && fraction.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            fraction
        }
        Some(_) => return None,
        None => "",
    };
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    if whole.len() > 16 {
        return None;
    }
    let fraction = if fraction.is_empty() {
        0
    } else {
        let digits = fraction.parse::<i64>().ok()?;
        match fraction.len() {
            2 => digits,
            1 => digits * 10,
            _ => return None,
        }
    };
    let minor = whole.parse::<i64>().ok()?.checked_mul(100)?.checked_add(fraction)?;
    (minor <= MAX_PRICE_MINOR).then_some(minor)
}

impl CreateListing {
    pub fn validate(&self) -> Result<ValidatedListing, BusinessError> {
        let title = valid_title(&self.title).ok_or_else(request_invalid)?;
        let currency = valid_listing_currency(&self.currency).ok_or_else(request_invalid)?;
        let price_minor = minor_price(&self.price).ok_or_else(request_invalid)?;
        Ok(ValidatedListing {
            channel_id: self.channel_id.as_str().to_owned(),
            product_id: self.product_id.as_str().to_owned(),
            title,
            currency,
            price_minor,
        })
    }

    pub fn draft(&self, listing: ValidatedListing) -> ListingDraft {
        ListingDraft {
            id: format!("ecommerce-listing-{}", self.key.as_str()),
            channel_id: listing.channel_id,
            product_id: listing.product_id,
            title: listing.title,
            currency: listing.currency,
            price_minor: listing.price_minor,
        }
    }

    pub fn key(&self) -> &CommandKey {
        &self.key
    }
}

#[derive(Serialize)]
struct CreateIdentity<'a> {
    id: &'a str,
    #[serde(rename = "channelId")]
    channel_id: &'a str,
    #[serde(rename = "productId")]
    product_id: &'a str,
    title: &'a str,
    currency: &'a str,
    #[serde(rename = "priceMinor")]
    price_minor: i64,
}

impl ListingDraft {
    pub fn identity(&self, actor: &EcommerceActor) -> String {
        command_identity(
            actor,
            CREATE_OPERATION,
            &CreateIdentity {
                id: &self.id,
                channel_id: &self.channel_id,
                product_id: &self.product_id,
                title: &self.title,
                currency: &self.currency,
                price_minor: self.price_minor,
            },
        )
    }

    /// The Finance unit-price request retained with the same transaction.
    pub fn money_request(&self, actor: &EcommerceActor) -> EcommerceListingMoneyRequest {
        EcommerceListingMoneyRequest {
            company_id: actor.company_id.as_str().to_owned(),
            listing_id: self.id.clone(),
            currency: self.currency.clone(),
            amount_minor: self.price_minor,
        }
    }

    pub fn validate_readback(
        &self,
        actor: &EcommerceActor,
        listing: &ListingRecord,
    ) -> Result<(), BusinessError> {
        if listing.id != self.id
            || listing.company_id != actor.company_id.as_str()
            || listing.channel_id != self.channel_id
            || listing.product_id != self.product_id
            || listing.title != self.title
            || listing.currency != self.currency
            || listing.price_minor != self.price_minor
            || listing.status != "draft"
            || listing.created_by != actor.user_id.as_str()
            || listing.reviewed_by.is_some()
            || listing.version != 1
            || listing.created_at < 0
            || listing.updated_at != listing.created_at
        {
            return Err(readback_invalid());
        }
        Ok(())
    }

    pub fn receipt(&self) -> EcommerceReceipt {
        EcommerceReceipt {
            id: self.id.clone(),
            status: "draft".into(),
            version: 1,
        }
    }
}

impl ChangeListing {
    pub fn validate(&self) -> Result<ValidatedListingChange, BusinessError> {
        if self.expected_version < 1 {
            return Err(request_invalid());
        }
        let title = valid_title(&self.title).ok_or_else(request_invalid)?;
        Ok(ValidatedListingChange {
            id: self.id.as_str().to_owned(),
            expected_version: self.expected_version,
            title,
        })
    }
}

#[derive(Serialize)]
struct ChangeIdentity<'a> {
    id: &'a str,
    #[serde(rename = "expectedVersion")]
    expected_version: i64,
    title: &'a str,
}

impl ValidatedListingChange {
    pub fn identity(&self, actor: &EcommerceActor) -> String {
        command_identity(
            actor,
            CHANGE_OPERATION,
            &ChangeIdentity {
                id: &self.id,
                expected_version: self.expected_version,
                title: &self.title,
            },
        )
    }

    pub fn next_version(&self) -> i64 {
        self.expected_version + 1
    }

    /// Draft authoring is not review. A reviewer must never be able to retitle a
    /// draft and then publish it as its independent decision, so only the
    /// creator can change a draft and only at the exact current version.
    pub fn validate_source(
        &self,
        actor: &EcommerceActor,
        source: &ListingRecord,
    ) -> Result<(), BusinessError> {
        if source.id != self.id
            || source.company_id != actor.company_id.as_str()
            || !valid_identifier(&source.id)
        {
            return Err(super::command::not_found());
        }
        if source.version != self.expected_version || source.status != "draft" {
            return Err(super::command::version_conflict());
        }
        if source.created_by != actor.user_id.as_str() {
            return Err(author_required());
        }
        Ok(())
    }

    /// Proves the change touched only the title/version/timestamp: every source
    /// term of the listing is unchanged and no review marker appeared.
    pub fn validate_readback(
        &self,
        actor: &EcommerceActor,
        source: &ListingRecord,
        listing: &ListingRecord,
    ) -> Result<(), BusinessError> {
        if listing.id != self.id
            || listing.company_id != actor.company_id.as_str()
            || listing.channel_id != source.channel_id
            || listing.product_id != source.product_id
            || listing.currency != source.currency
            || listing.price_minor != source.price_minor
            || listing.created_by != source.created_by
            || listing.reviewed_by != source.reviewed_by
            || listing.created_at != source.created_at
            || listing.status != "draft"
            || listing.title != self.title
            || listing.version != self.next_version()
            || listing.updated_at < source.updated_at
        {
            return Err(readback_invalid());
        }
        Ok(())
    }

    pub fn receipt(&self) -> EcommerceReceipt {
        EcommerceReceipt {
            id: self.id.clone(),
            status: "draft".into(),
            version: self.next_version(),
        }
    }
}

impl EcommerceListingMoneySnapshot {
    /// The producer must retain the same currency scale, amount and capture time
    /// as this command; a mismatched snapshot is a source failure, not a
    /// successful listing write.
    pub fn validate(
        &self,
        amount_minor: i64,
        now: TimestampMillis,
    ) -> Result<(), BusinessError> {
        if self.currency_scale != MONEY_CURRENCY_SCALE
            || self.amount_units != amount_minor
            || self.usd_units < 0
            || self.rate_id.as_deref().is_some_and(str::is_empty)
            || self.captured_at < 0
            || self.captured_at > now.get()
        {
            return Err(money_evidence_invalid());
        }
        Ok(())
    }
}

pub fn replay_create(
    receipt: Option<CommandReceipt>,
    request_hash: &str,
    id: &str,
) -> Result<Option<EcommerceReceipt>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash {
        return Err(idempotency_conflict());
    }
    let parsed = parse_receipt_result(&receipt.result_json)?;
    if parsed.id != id || parsed.status != "draft" || parsed.version != 1 {
        return Err(source_invalid());
    }
    Ok(Some(parsed))
}

pub fn replay_change(
    receipt: Option<CommandReceipt>,
    request_hash: &str,
    change: &ValidatedListingChange,
) -> Result<Option<EcommerceReceipt>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash {
        return Err(idempotency_conflict());
    }
    let parsed = parse_receipt_result(&receipt.result_json)?;
    if parsed.id != change.id
        || parsed.status != "draft"
        || parsed.version != change.next_version()
    {
        return Err(source_invalid());
    }
    Ok(Some(parsed))
}

pub fn validate_hash(value: &str) -> Result<(), BusinessError> {
    if !valid_hash(value) {
        return Err(source_invalid());
    }
    Ok(())
}
