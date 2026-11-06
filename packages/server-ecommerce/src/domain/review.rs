//! Native Ecommerce channel/listing review command facts.
//!
//! Review is independent of the creator for activation, publication and
//! rejection. Retirement may be performed by the creator so an abandoned draft
//! can be closed. The transition is an exact expected-version CAS on the
//! Company-scoped owner row.
use super::actor::EcommerceActor;
use super::command::{
    EcommerceReceipt, chars_len, command_identity, cp_text, idempotency_conflict, js_trim,
    parse_receipt_result, readback_invalid, request_invalid, source_invalid, valid_hash,
    valid_identifier,
};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};
use serde::Serialize;

const REASON_MIN: usize = 3;
const REASON_MAX: usize = 2_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewKind {
    Channel,
    Listing,
}

impl ReviewKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "channel" => Some(Self::Channel),
            "listing" => Some(Self::Listing),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Listing => "listing",
        }
    }

    pub fn record_type(self) -> &'static str {
        match self {
            Self::Channel => "ecommerce_channel",
            Self::Listing => "ecommerce_listing",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewAction {
    Activate,
    Reject,
    Retire,
    Publish,
}

impl ReviewAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "activate" => Some(Self::Activate),
            "reject" => Some(Self::Reject),
            "retire" => Some(Self::Retire),
            "publish" => Some(Self::Publish),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Activate => "activate",
            Self::Reject => "reject",
            Self::Retire => "retire",
            Self::Publish => "publish",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReviewCommand {
    pub key: CommandKey,
    pub kind: ReviewKind,
    pub id: RecordId,
    pub action: ReviewAction,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedReview {
    pub kind: ReviewKind,
    pub id: String,
    pub action: ReviewAction,
    pub expected_version: i64,
    pub reason: String,
    pub expected_status: &'static str,
    pub next_status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewSource {
    pub id: String,
    pub company_id: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewReadback {
    pub id: String,
    pub company_id: String,
    pub status: String,
    pub version: i64,
    pub reviewed_by: Option<String>,
}

impl ReviewKind {
    fn allows(self, action: ReviewAction) -> bool {
        match self {
            Self::Channel => matches!(
                action,
                ReviewAction::Activate | ReviewAction::Reject | ReviewAction::Retire
            ),
            Self::Listing => matches!(
                action,
                ReviewAction::Publish | ReviewAction::Reject | ReviewAction::Retire
            ),
        }
    }

    fn active_status(self) -> &'static str {
        match self {
            Self::Channel => "active",
            Self::Listing => "published",
        }
    }

    fn draft_status(self) -> &'static str {
        "draft"
    }
}

impl ReviewCommand {
    pub fn operation(&self) -> String {
        format!("ecommerce.{}.{}", self.kind.as_str(), self.action.as_str())
    }

    pub fn validate(&self) -> Result<ValidatedReview, BusinessError> {
        if !self.kind.allows(self.action) || self.expected_version < 1 {
            return Err(request_invalid());
        }
        let reason = js_trim(&self.reason);
        let reason_len = chars_len(reason);
        if !(REASON_MIN..=REASON_MAX).contains(&reason_len) || !cp_text(reason, REASON_MAX) {
            return Err(request_invalid());
        }
        let (expected_status, next_status) = match self.action {
            ReviewAction::Activate => (self.kind.draft_status(), self.kind.active_status()),
            ReviewAction::Publish => (self.kind.draft_status(), self.kind.active_status()),
            ReviewAction::Reject => (self.kind.draft_status(), "rejected"),
            ReviewAction::Retire => (self.kind.active_status(), "retired"),
        };
        Ok(ValidatedReview {
            kind: self.kind,
            id: self.id.as_str().to_owned(),
            action: self.action,
            expected_version: self.expected_version,
            reason: reason.to_owned(),
            expected_status,
            next_status,
        })
    }
}

#[derive(Serialize)]
struct IdentityInput<'a> {
    id: &'a str,
    action: &'a str,
    #[serde(rename = "expectedVersion")]
    expected_version: i64,
    reason: &'a str,
}

impl ValidatedReview {
    pub fn operation(&self) -> String {
        format!("ecommerce.{}.{}", self.kind.as_str(), self.action.as_str())
    }

    pub fn identity(&self, actor: &EcommerceActor) -> String {
        command_identity(
            actor,
            &self.operation(),
            &IdentityInput {
                id: &self.id,
                action: self.action.as_str(),
                expected_version: self.expected_version,
                reason: &self.reason,
            },
        )
    }

    pub fn validate_source(
        &self,
        actor: &EcommerceActor,
        source: &ReviewSource,
    ) -> Result<(), BusinessError> {
        if source.id != self.id
            || source.company_id != actor.company_id.as_str()
            || !valid_identifier(&source.id)
        {
            return Err(super::command::not_found());
        }
        if source.version != self.expected_version || source.status != self.expected_status {
            return Err(super::command::version_conflict());
        }
        if self.action != ReviewAction::Retire && source.created_by == actor.user_id.as_str() {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "ECOMMERCE_INDEPENDENT_REVIEW_REQUIRED",
                "A different reviewer must activate, publish or reject this record.",
            ));
        }
        Ok(())
    }

    pub fn validate_readback(
        &self,
        actor: &EcommerceActor,
        readback: &ReviewReadback,
    ) -> Result<(), BusinessError> {
        if readback.id != self.id
            || readback.company_id != actor.company_id.as_str()
            || readback.status != self.next_status
            || readback.version != self.expected_version + 1
            || readback.reviewed_by.as_deref() != Some(actor.user_id.as_str())
        {
            return Err(readback_invalid());
        }
        Ok(())
    }

    pub fn receipt(&self) -> EcommerceReceipt {
        EcommerceReceipt {
            id: self.id.clone(),
            status: self.next_status.into(),
            version: self.expected_version + 1,
        }
    }
}

pub fn replay(
    receipt: Option<super::command::CommandReceipt>,
    request_hash: &str,
    review: &ValidatedReview,
) -> Result<Option<EcommerceReceipt>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash {
        return Err(idempotency_conflict());
    }
    let parsed = parse_receipt_result(&receipt.result_json)?;
    if parsed.id != review.id
        || parsed.status != review.next_status
        || parsed.version != review.expected_version + 1
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
