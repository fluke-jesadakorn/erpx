//! Native Ecommerce application ports and use cases.
use crate::domain::actor::EcommerceActor;
use crate::domain::catalog::{EcommerceChannelRow, EcommerceListingRow};
use crate::domain::channel::{ChannelDraft, ChannelReadback};
use crate::domain::review::{ReviewKind, ReviewReadback, ReviewSource};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;

pub type EcommerceFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait EcommerceCatalogRepository: Send {
    type Error: From<BusinessError> + Send;

    fn channels<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Vec<EcommerceChannelRow>, Self::Error>;

    fn listings<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Vec<EcommerceListingRow>, Self::Error>;
}

pub struct AuditEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub authority_json: &'a str,
}

pub struct OutboxEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub state: &'a str,
    pub authority_json: &'a str,
}

pub struct ReceiptEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub actor: &'a EcommerceActor,
    pub request_hash: &'a str,
    pub record_id: &'a str,
    pub result_json: &'a str,
}

pub trait EcommerceCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn digest<'a>(&'a mut self, value: &'a str) -> EcommerceFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> EcommerceFuture<'a, Option<crate::domain::command::CommandReceipt>, Self::Error>;

    fn insert_channel<'a>(
        &'a mut self,
        actor: &'a EcommerceActor,
        channel: &'a ChannelDraft,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error>;

    fn channel_readback<'a>(
        &'a mut self,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ChannelReadback>, Self::Error>;

    fn review_source<'a>(
        &'a mut self,
        kind: ReviewKind,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ReviewSource>, Self::Error>;

    fn commit_review<'a>(
        &'a mut self,
        review: &'a crate::domain::review::ValidatedReview,
        company: &'a CompanyId,
        actor: &'a EcommerceActor,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error>;

    fn review_readback<'a>(
        &'a mut self,
        kind: ReviewKind,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ReviewReadback>, Self::Error>;

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

pub mod catalog;
pub mod channel;
pub mod review;
