//! Current Billing actor policy.
//!
//! Billing is a Holding-level account owned by ERPX. Composition supplies the
//! normal Account session, Platform assignment or Holding-owner assignment and
//! the target Holding. This owner policy only checks the retained:
//!
//! * Platform read roles (`platform_admin`, `platform_auditor`);
//! * Platform write role (`platform_admin`);
//! * Holding owner read/write boundary on the exact target Holding.
//!
//! The billing account has no Company dimension. A Company assignment must not
//! be used to widen a Billing read to another Holding.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, FailureKind, HoldingId, RecordId, TimestampMillis, UserId,
};
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct BillingActor {
    pub user_id: UserId,
    pub assignment_id: AssignmentId,
    pub holding_id: Option<HoldingId>,
    pub platform: bool,
    pub role: String,
    pub expires_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingContext {
    pub user_id: String,
    pub holding_id: String,
    pub platform: bool,
    pub role: String,
    pub assignment_id: String,
    pub expires_at: Option<i64>,
}

fn context_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your selected work context changed. Review the current workspace and role.",
    )
}

fn platform_read_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FORBIDDEN",
        "Platform billing permission required.",
    )
}

fn platform_write_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FORBIDDEN",
        "Platform billing write permission required.",
    )
}

fn not_customer() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ERPX_NOT_CUSTOMER",
        "ERPX is not a customer billing account.",
    )
}

fn holding_owner_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FORBIDDEN",
        "Only the Holding owner can access billing.",
    )
}

impl BillingActor {
    fn active(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if self
            .expires_at
            .is_some_and(|expires_at| now.get() >= expires_at)
        {
            return Err(context_changed());
        }
        Ok(())
    }

    fn platform_read_role(&self) -> bool {
        matches!(self.role.as_str(), "platform_admin" | "platform_auditor")
    }

    /// Platform auditors may read the same Holding projection as administrators.
    pub fn require_read(
        &self,
        target: &HoldingId,
        now: TimestampMillis,
    ) -> Result<(), BusinessError> {
        self.active(now)?;
        if target.as_str() == "h003" {
            return Err(not_customer());
        }
        if self.platform {
            return if self.platform_read_role() {
                Ok(())
            } else {
                Err(platform_read_required())
            };
        }
        if self.role != "holding_owner"
            || self.holding_id.as_ref() != Some(target)
            || RecordId::new(target.as_str()).is_err()
        {
            return Err(holding_owner_required());
        }
        Ok(())
    }

    /// All cost mutations are Platform-administrator commands. A Holding owner
    /// cannot mark actual costs complete or edit provider evidence.
    pub fn require_cost_command(
        &self,
        target: &HoldingId,
        now: TimestampMillis,
    ) -> Result<(), BusinessError> {
        self.active(now)?;
        if !self.platform || self.role != "platform_admin" {
            return Err(platform_write_required());
        }
        if target.as_str() == "h003" {
            return Err(not_customer());
        }
        if RecordId::new(target.as_str()).is_err() {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Holding not found.",
            ));
        }
        Ok(())
    }

    /// Target price assignment is a Holding-owner command. Platform
    /// administrators cannot assign a target offering to a customer Holding.
    pub fn require_target_command(
        &self,
        target: &HoldingId,
        now: TimestampMillis,
    ) -> Result<(), BusinessError> {
        self.active(now)?;
        if target.as_str() == "h003" {
            return Err(not_customer());
        }
        if self.role != "holding_owner"
            || self.holding_id.as_ref() != Some(target)
            || RecordId::new(target.as_str()).is_err()
        {
            return Err(holding_owner_required());
        }
        Ok(())
    }

    /// Retention-compatible authority projection. It records the selected
    /// assignment and scope for audit readers; it grants no authority.
    pub fn authority_json(&self, target: &HoldingId) -> String {
        serde_json::to_string(&serde_json::json!({
            "userId": self.user_id.as_str(),
            "assignmentId": self.assignment_id.as_str(),
            "holdingId": target.as_str(),
            "platform": self.platform,
            "role": self.role,
            "expiresAt": self.expires_at,
        }))
        .expect("the Billing authority projection always serializes")
    }

    pub fn context(&self, target: &HoldingId) -> BillingContext {
        BillingContext {
            user_id: self.user_id.as_str().to_owned(),
            holding_id: target.as_str().to_owned(),
            platform: self.platform,
            role: self.role.clone(),
            assignment_id: self.assignment_id.as_str().to_owned(),
            expires_at: self.expires_at,
        }
    }
}
