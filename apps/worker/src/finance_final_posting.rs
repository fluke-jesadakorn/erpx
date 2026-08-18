//! Finance-owned final posting coordinator.
//!
//! It accepts only an already human-approved packet. The port is implemented
//! by Finance composition and must create Invoice, AR, both balanced journals,
//! and the Revenue Schedule under one idempotency identity.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_finance::{PostingInstructions, PostingPacket, PostingReceipt, SalesPostingError};
use thiserror::Error;

#[async_trait]
pub trait FinancePostingOwnerPort: Send + Sync {
    async fn post_once(
        &self,
        idempotency_key: &str,
        instructions: &PostingInstructions,
    ) -> Result<PostingReceipt, FinalPostingError>;
}

#[derive(Debug)]
pub struct FinalPostingCoordinator<P> {
    finance: P,
}

impl<P: FinancePostingOwnerPort> FinalPostingCoordinator<P> {
    pub fn new(finance: P) -> Self {
        Self { finance }
    }

    pub async fn post_approved_packet(
        &self,
        packet: &mut PostingPacket,
        at: DateTime<Utc>,
    ) -> Result<PostingReceipt, FinalPostingError> {
        if let Some(receipt) = packet.receipt() {
            return Ok(receipt.clone());
        }
        let instructions = packet.posting_instructions()?;
        let idempotency_key = format!(
            "finance.sales-posting:{}:v{}",
            packet.id().as_uuid(),
            packet.version()
        );
        let receipt = self
            .finance
            .post_once(&idempotency_key, &instructions)
            .await?;
        packet.mark_posted(receipt.clone(), at)?;
        Ok(receipt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FinalPostingError {
    #[error("Finance posting is temporarily unavailable")]
    Retryable,
    #[error("Finance posting was permanently rejected")]
    Rejected,
    #[error("Finance posting idempotency identity conflicts")]
    IdempotencyConflict,
    #[error(transparent)]
    Domain(#[from] SalesPostingError),
}
