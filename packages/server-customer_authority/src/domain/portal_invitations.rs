//! Native Customer Portal invitation issue/resend policy.
//!
//! One approved customer binding owns at most one current invitation. An
//! issuance replaces the previous live invitation with immutable supersession
//! evidence, and only the token digest is persisted. No database, clock or HTTP
//! effect is reachable from this module.
use crate::adapters::portal_invitation_token::{invitation_token, invitation_token_hash};
use crate::domain::access::AccessActor;
use crate::domain::portal_invitation_delivery::{INVITABLE_ORDER_STATES, InvitationClaims};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const INVITATION_LIFETIME_MILLIS: i64 = 7 * 86_400_000;
pub const MAX_REASON: usize = 2_000;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_ACCESS_REQUEST_INVALID",
        "Review the customer access invitation command.",
    )
}
pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_ACCESS_NOT_FOUND",
        "Customer access request not found in this scope.",
    )
}
pub fn version_conflict() -> BusinessError {
    conflict(
        "CUSTOMER_ACCESS_CHANGED",
        "Customer access changed before the invitation was issued.",
    )
}
pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_ACCESS_PERMISSION_REQUIRED",
        "Only the original requester or an authorized sales manager may invite this customer.",
    )
}
pub fn identity_bound() -> BusinessError {
    conflict(
        "CUSTOMER_PORTAL_IDENTITY_ALREADY_BOUND",
        "This approved binding already belongs to a verified Portal account.",
    )
}
pub fn binding_unavailable() -> BusinessError {
    conflict(
        "CUSTOMER_PORTAL_BINDING_UNAVAILABLE",
        "Only a current approved customer binding can be invited.",
    )
}
pub fn invitation_active() -> BusinessError {
    conflict(
        "CUSTOMER_PORTAL_INVITATION_ACTIVE",
        "A current invitation already exists. Resend it to replace the previous link.",
    )
}
pub fn invitation_not_active() -> BusinessError {
    conflict(
        "CUSTOMER_PORTAL_INVITATION_NOT_ACTIVE",
        "There is no current invitation to resend. Issue a new invitation instead.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "CUSTOMER_ACCESS_IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different customer access command.",
    )
}
pub fn source_changed() -> BusinessError {
    conflict(
        "CUSTOMER_ACCESS_SOURCE_CHANGED",
        "The original customer access source changed.",
    )
}
pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_ACCESS_READBACK_INVALID",
        "The customer invitation was not persisted as submitted.",
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationAction {
    Invite,
    Resend,
}

impl InvitationAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invite => "invite",
            Self::Resend => "resend",
        }
    }

    pub fn operation(self) -> String {
        format!("customer.access.{}", self.as_str())
    }
}

/// Strict wire command; field order is the retained request digest order.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvitationCommand {
    pub action: InvitationAction,
    pub expected_version: i64,
    pub reason: String,
}

impl InvitationCommand {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let command: Self = serde_json::from_value(value.clone()).map_err(|_| request_invalid())?;
        if command.expected_version < 1
            || command.reason.trim() != command.reason
            || command.reason.chars().count() < 3
            || command.reason.chars().count() > MAX_REASON
            || command.reason.chars().any(|value| value.is_control())
        {
            return Err(request_invalid());
        }
        Ok(command)
    }
}

#[derive(Clone, Debug)]
pub struct InvitationBinding {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub email: String,
    pub status: String,
    pub order_id: String,
    pub order_state: String,
    pub expires_at: i64,
    pub version: i64,
    pub created_by: String,
    pub identity_exists: bool,
}

impl InvitationBinding {
    pub fn invitable(&self, now: i64) -> bool {
        self.status == "approved"
            && self.expires_at > now
            && INVITABLE_ORDER_STATES.contains(&self.order_state.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct ActiveInvitation {
    pub id: String,
    pub generation: i64,
    pub state: String,
    pub token_expires_at: i64,
}

/// The exact response body returned to the staff route/client.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationReceipt {
    pub id: String,
    pub binding_id: String,
    pub invitation_id: String,
    pub generation: i64,
    pub version: i64,
    pub state: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug)]
pub struct InvitationPlan {
    pub invitation_id: String,
    pub holding_slug: String,
    pub email: String,
    pub generation: i64,
    pub token_hash: String,
    pub token_expires_at: i64,
    pub now: i64,
    pub previous: Option<ActiveInvitation>,
    pub terminal: Option<&'static str>,
    pub reason: String,
    pub action: InvitationAction,
}

/// Decide one issuance from the current binding, identity and invitation facts.
pub fn issue_plan(
    actor: &AccessActor,
    binding: &InvitationBinding,
    active: Option<&ActiveInvitation>,
    command: &InvitationCommand,
    secret: &str,
    holding_slug: &str,
    now: i64,
) -> Result<InvitationPlan, BusinessError> {
    if binding.version != command.expected_version {
        return Err(version_conflict());
    }
    let can_manage = binding.created_by == actor.user_id.as_str() || actor.has("commerce.approve");
    if !can_manage {
        return Err(permission_required());
    }
    if binding.identity_exists {
        return Err(identity_bound());
    }
    if !binding.invitable(now) {
        return Err(binding_unavailable());
    }
    if active.is_some_and(|value| value.state == "accepted") {
        return Err(identity_bound());
    }
    let active_expired = active.is_some_and(|value| value.token_expires_at <= now);
    match command.action {
        InvitationAction::Invite => {
            if active.is_some() && !active_expired {
                return Err(invitation_active());
            }
        }
        InvitationAction::Resend => {
            if active.is_none() || active_expired {
                return Err(invitation_not_active());
            }
        }
    }
    let token_expires_at = binding
        .expires_at
        .min(now.saturating_add(INVITATION_LIFETIME_MILLIS));
    if token_expires_at <= now {
        return Err(binding_unavailable());
    }
    let generation = active.map_or(1, |value| value.generation + 1);
    let invitation_id = format!("customer-portal-invitation-{}", uuid::Uuid::new_v4());
    let claims = InvitationClaims {
        invitation_id: invitation_id.clone(),
        binding_id: binding.id.clone(),
        holding_slug: holding_slug.to_owned(),
        generation: generation as i32,
        expires_at: token_expires_at,
        key_version: 1,
    };
    let token = invitation_token(secret, &claims);
    let previous = active.cloned();
    let terminal = previous.as_ref().map(|value| {
        if value.token_expires_at <= now {
            "expired"
        } else {
            "superseded"
        }
    });
    Ok(InvitationPlan {
        invitation_id,
        holding_slug: holding_slug.to_owned(),
        email: binding.email.to_lowercase(),
        generation,
        token_hash: invitation_token_hash(&token),
        token_expires_at,
        now,
        previous,
        terminal,
        reason: command.reason.clone(),
        action: command.action,
    })
}

pub fn receipt(
    plan: &InvitationPlan,
    binding_id: &str,
    expected_version: i64,
) -> InvitationReceipt {
    InvitationReceipt {
        id: binding_id.to_owned(),
        binding_id: binding_id.to_owned(),
        invitation_id: plan.invitation_id.clone(),
        generation: plan.generation,
        version: expected_version,
        state: "queued".to_owned(),
        expires_at: plan.token_expires_at,
    }
}

/// History rows returned by the owner read; ordering is generation DESC and
/// event creation order.
#[derive(Clone, Debug, Default)]
pub struct InvitationHistory {
    pub invitations: Vec<Value>,
    pub invitation_events: Vec<Value>,
}

impl InvitationHistory {
    pub fn to_wire(&self) -> Value {
        json!({
            "invitations": self.invitations,
            "invitationEvents": self.invitation_events,
        })
    }
}

pub fn supersede_event_id() -> String {
    format!("customer-portal-invite-event-{}", uuid::Uuid::new_v4())
}

pub fn issued_event_id(key: &str) -> String {
    format!("customer-portal-invite-event-{key}")
}
