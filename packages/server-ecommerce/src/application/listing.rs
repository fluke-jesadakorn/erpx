//! Native Ecommerce listing draft/create/change use cases.
//!
//! The caller's serializable Holding transaction supplies the owner unit. Create
//! resolves the command receipt first, then requires a current active Company
//! channel, an active Commerce product, the configured functional currency and
//! one Finance unit-price Money snapshot before inserting the draft listing.
//! Change is an exact expected-version CAS on the draft title. Receipt, audit,
//! outbox and persisted readback are written in the same unit.
use crate::application::{AuditEntry, EcommerceFuture, OutboxEntry, ReceiptEntry};
use crate::domain::actor::EcommerceActor;
use crate::domain::command::{
    EcommerceReceipt, idempotency_conflict, not_found, readback_invalid, request_invalid,
    version_conflict,
};
use crate::domain::listing::{
    CHANGE_OPERATION, CREATE_OPERATION, RECORD_TYPE, ChangeListing, CreateListing,
    EcommerceFunctionalCurrency, EcommerceListingMoneyRequest, EcommerceListingMoneySnapshot,
    ListingDraft, ListingRecord, ValidatedListingChange, replay_change, replay_create,
    validate_hash,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

/// Owner-local ports for the listing command slice. The Commerce product /
/// functional-currency source and the Finance Money snapshot producer are
/// composed into the same transaction by the caller; the application never
/// reads another owner's table itself.
pub trait EcommerceListingRepository: Send {
    type Error: From<BusinessError> + Send;

    fn digest<'a>(&'a mut self, value: &'a str) -> EcommerceFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> EcommerceFuture<'a, Option<crate::domain::command::CommandReceipt>, Self::Error>;

    fn active_channel<'a>(
        &'a mut self,
        company: &'a CompanyId,
        channel_id: &'a str,
    ) -> EcommerceFuture<'a, bool, Self::Error>;

    fn active_product<'a>(
        &'a mut self,
        company: &'a CompanyId,
        product_id: &'a str,
    ) -> EcommerceFuture<'a, bool, Self::Error>;

    fn functional_currency<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<EcommerceFunctionalCurrency>, Self::Error>;

    fn capture_money<'a>(
        &'a mut self,
        actor: &'a EcommerceActor,
        request: &'a EcommerceListingMoneyRequest,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, EcommerceListingMoneySnapshot, Self::Error>;

    fn insert_listing<'a>(
        &'a mut self,
        actor: &'a EcommerceActor,
        listing: &'a ListingDraft,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error>;

    fn listing_source<'a>(
        &'a mut self,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ListingRecord>, Self::Error>;

    fn commit_listing_change<'a>(
        &'a mut self,
        change: &'a ValidatedListingChange,
        company: &'a CompanyId,
        actor: &'a EcommerceActor,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error>;

    fn listing_readback<'a>(
        &'a mut self,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ListingRecord>, Self::Error>;

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error>;

    fn audit<'a>(
        &'a mut self,
        actor: &'a EcommerceActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, (), Self::Error>;

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, (), Self::Error>;
}

pub async fn create_listing<R: EcommerceListingRepository>(
    actor: &EcommerceActor,
    command: &CreateListing,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<EcommerceReceipt, R::Error> {
    actor.require_write(now)?;
    let validated = command.validate()?;
    let draft = command.draft(validated);
    let identity = draft.identity(actor);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = replay_create(
        repository.receipt(command.key().as_str()).await?,
        &request_hash,
        &draft.id,
    )? {
        return Ok(prior);
    }
    if !repository
        .active_channel(&actor.company_id, &draft.channel_id)
        .await?
    {
        return Err(crate::domain::listing::channel_not_active().into());
    }
    if !repository
        .active_product(&actor.company_id, &draft.product_id)
        .await?
    {
        return Err(crate::domain::listing::product_not_active().into());
    }
    let functional = repository
        .functional_currency(&actor.company_id)
        .await?
        .ok_or_else(crate::domain::listing::currency_mapping_required)?;
    if functional.currency != draft.currency || functional.base_currency != draft.currency {
        return Err(crate::domain::listing::currency_mapping_required().into());
    }
    let money = repository
        .capture_money(actor, &draft.money_request(actor), now)
        .await?;
    money.validate(draft.price_minor, now)?;
    if !repository.insert_listing(actor, &draft, now).await? {
        return Err(idempotency_conflict().into());
    }
    let receipt = draft.receipt();
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: command.key().as_str(),
                operation: CREATE_OPERATION,
                actor,
                request_hash: &request_hash,
                record_id: &draft.id,
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
            actor,
            &AuditEntry {
                key: command.key().as_str(),
                operation: CREATE_OPERATION,
                record_type: RECORD_TYPE,
                record_id: &draft.id,
                version: 1,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &OutboxEntry {
                key: command.key().as_str(),
                operation: CREATE_OPERATION,
                record_type: RECORD_TYPE,
                record_id: &draft.id,
                version: 1,
                state: receipt.status.as_str(),
                authority_json: &authority,
            },
            now,
        )
        .await?;
    let readback = repository
        .listing_readback(&draft.id, &actor.company_id)
        .await?
        .ok_or_else(readback_invalid)?;
    draft.validate_readback(actor, &readback)?;
    Ok(receipt)
}

pub async fn change_listing<R: EcommerceListingRepository>(
    actor: &EcommerceActor,
    command: &ChangeListing,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<EcommerceReceipt, R::Error> {
    actor.require_write(now)?;
    let change = command.validate()?;
    let identity = change.identity(actor);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = replay_change(
        repository.receipt(command.key.as_str()).await?,
        &request_hash,
        &change,
    )? {
        return Ok(prior);
    }
    let source = repository
        .listing_source(&change.id, &actor.company_id)
        .await?
        .ok_or_else(not_found)?;
    change.validate_source(actor, &source)?;
    if !repository
        .commit_listing_change(&change, &actor.company_id, actor, now)
        .await?
    {
        return Err(version_conflict().into());
    }
    let receipt = change.receipt();
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: command.key.as_str(),
                operation: CHANGE_OPERATION,
                actor,
                request_hash: &request_hash,
                record_id: &change.id,
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
            actor,
            &AuditEntry {
                key: command.key.as_str(),
                operation: CHANGE_OPERATION,
                record_type: RECORD_TYPE,
                record_id: &change.id,
                version: receipt.version,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &OutboxEntry {
                key: command.key.as_str(),
                operation: CHANGE_OPERATION,
                record_type: RECORD_TYPE,
                record_id: &change.id,
                version: receipt.version,
                state: receipt.status.as_str(),
                authority_json: &authority,
            },
            now,
        )
        .await?;
    let readback = repository
        .listing_readback(&change.id, &actor.company_id)
        .await?
        .ok_or_else(readback_invalid)?;
    change.validate_readback(actor, &source, &readback)?;
    Ok(receipt)
}
