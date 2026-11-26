//! Pure domain for the native POS room-charge reversal queue read.
//!
//! The axum owner owns the SQL, the projection and the response shape. This
//! module contains no I/O: it validates the bounded request and the
//! composition-supplied authority/package decisions, pins the exact retired
//! wire field set and derives every per-row review flag. TypeScript is a
//! transport only; no SQL or business rule remains there for this operation.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};

pub const MAX_ID_LEN: usize = 200;

/// Exact field set of the retired `roomChargeReversalQueue` response row: the
/// 19 projected SQL columns plus the four derived review fields. A row has
/// exactly these keys — no additions and no omissions.
pub const ROW_KEYS: [&str; 23] = [
    "id",
    "company_id",
    "branch_id",
    "sale_id",
    "property_id",
    "sale_version",
    "property_version",
    "restock_line_ids_json",
    "number",
    "state",
    "version",
    "reason",
    "amount_minor",
    "requested_by",
    "pos_approved_by",
    "created_by",
    "charged_by",
    "updated_at",
    "source_current",
    "stage",
    "canAct",
    "canApprove",
    "reasonDisabled",
];

/// Authority facts composition supplies for the caller. `self_ids` are the
/// user identities that the IAM `samePerson` predicate treats as the actor;
/// the owner compares reviewer ids without importing Identity or PMS.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActorDecision {
    pub pos_approve: bool,
    pub pms_approve: bool,
    pub department: bool,
    pub self_ids: Vec<String>,
}

/// Package decision composition supplies for the caller. `enforced = false`
/// is the retained "disabled" mode, where no purchase history gate applies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageDecision {
    pub enforced: bool,
    pub history: bool,
    pub settle_existing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomChargeQuery {
    pub company: String,
    pub branch: Option<String>,
    pub property: Option<String>,
    pub actor: ActorDecision,
    pub package: PackageDecision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomChargeRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub sale_id: String,
    pub property_id: String,
    pub sale_version: i64,
    pub property_version: i64,
    pub restock_line_ids_json: Option<String>,
    pub number: String,
    pub state: String,
    pub version: i64,
    pub reason: String,
    pub amount_minor: i64,
    pub requested_by: String,
    pub pos_approved_by: Option<String>,
    pub created_by: String,
    pub charged_by: Option<String>,
    pub updated_at: i64,
    pub source_current: bool,
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid("POS_REQUEST_INVALID", "Use the bounded POS room-charge filters.")
}

pub fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

fn bounded(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_LEN
}

fn safe_id(value: &str) -> bool {
    bounded(value)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// All request invariants are owned here. The pool and HTTP layers never
/// derive a Company, Branch or Property scope from an untrusted value without
/// these checks; the authority/package decisions are validated before any SQL.
pub fn validate(query: &RoomChargeQuery) -> Result<(), BusinessError> {
    if !safe_id(&query.company)
        || query.branch.as_deref().is_some_and(|value| !safe_id(value))
        || query
            .property
            .as_deref()
            .filter(|value| !value.is_empty())
            .is_some_and(|value| !safe_id(value))
    {
        return Err(invalid());
    }
    for id in &query.actor.self_ids {
        if !safe_id(id) {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Fail closed on a malformed native row: identity scope must be present and
/// the lifecycle discriminator must be one of the two retired queue states.
pub fn valid_row(row: &RoomChargeRow) -> bool {
    [&row.id, &row.company_id, &row.branch_id, &row.sale_id, &row.property_id]
        .iter()
        .all(|value| bounded(value))
        && matches!(row.state.as_str(), "requested" | "pos_approved")
        && row.sale_version >= 0
        && row.property_version >= 0
        && row.version >= 0
        && row.updated_at >= 0
}

impl RoomChargeRow {
    /// Build the exact retired payload. `stage`, `canAct`, `canApprove` and
    /// `reasonDisabled` mirror the retained derivation byte for byte, using
    /// composition's `samePerson` and package facts as inputs.
    pub fn value(
        self,
        pos_approve: bool,
        pms_approve: bool,
        self_ids: &[String],
        purchased_settlement: bool,
    ) -> Value {
        let reviewers = [
            Some(&self.requested_by),
            self.pos_approved_by.as_ref(),
            Some(&self.created_by),
            self.charged_by.as_ref(),
        ];
        let independent = !reviewers
            .into_iter()
            .flatten()
            .any(|user| self_ids.iter().any(|id| id == user));
        let stage = if self.state == "requested" {
            "pos"
        } else {
            "pms"
        };
        let allowed = purchased_settlement && (if stage == "pos" { pos_approve } else { pms_approve });
        let can_act = allowed && independent;
        let can_approve = can_act && self.source_current;
        let reason_disabled = if !allowed {
            Some(format!("The selected assignment requires {stage}.approve."))
        } else if !independent {
            Some("A different person must review this request.".to_owned())
        } else if !self.source_current {
            Some("The source changed. Reject this request and request again.".to_owned())
        } else {
            None
        };
        json!({
            "id": self.id,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "sale_id": self.sale_id,
            "property_id": self.property_id,
            "sale_version": self.sale_version,
            "property_version": self.property_version,
            "restock_line_ids_json": self.restock_line_ids_json,
            "number": self.number,
            "state": self.state,
            "version": self.version,
            "reason": self.reason,
            "amount_minor": self.amount_minor,
            "requested_by": self.requested_by,
            "pos_approved_by": self.pos_approved_by,
            "created_by": self.created_by,
            "charged_by": self.charged_by,
            "updated_at": self.updated_at,
            "source_current": self.source_current,
            "stage": stage,
            "canAct": can_act,
            "canApprove": can_approve,
            "reasonDisabled": reason_disabled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> RoomChargeRow {
        RoomChargeRow {
            id: "rc1".into(),
            company_id: "co".into(),
            branch_id: "br".into(),
            sale_id: "sale".into(),
            property_id: "prop".into(),
            sale_version: 2,
            property_version: 3,
            restock_line_ids_json: Some("[\"l1\"]".into()),
            number: "POS-1".into(),
            state: "requested".into(),
            version: 1,
            reason: "Wrong room".into(),
            amount_minor: 10_000,
            requested_by: "requester".into(),
            pos_approved_by: None,
            created_by: "cashier".into(),
            charged_by: None,
            updated_at: 1_700_000_000_000,
            source_current: true,
        }
    }

    #[test]
    fn pins_exact_payload_field_set_and_types() {
        let value = row().value(true, true, &[], true);
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), ROW_KEYS.len());
        for key in ROW_KEYS {
            assert!(object.contains_key(key), "missing payload field: {key}");
        }
        assert_eq!(value["sale_version"], 2);
        assert_eq!(value["amount_minor"], 10_000);
        assert_eq!(value["source_current"], true);
        assert_eq!(value["pos_approved_by"], Value::Null);
        assert_eq!(value["stage"], "pos");
        assert_eq!(value["canAct"], true);
        assert_eq!(value["canApprove"], true);
        assert_eq!(value["reasonDisabled"], Value::Null);
    }

    #[test]
    fn derives_review_flags_with_retained_precedence() {
        // Same-person reviewer blocks independent review even when sourced
        // from the audit subquery rather than a direct column.
        let mut value = row();
        value.charged_by = Some("cashier".into());
        let payload = value.clone().value(true, true, &["cashier".into()], true);
        assert_eq!(payload["canAct"], false);
        assert_eq!(payload["reasonDisabled"], "A different person must review this request.");

        // A stale source keeps the row actionable but blocks approval.
        let mut stale = value;
        stale.source_current = false;
        let payload = stale.value(true, true, &[], true);
        assert_eq!(payload["canAct"], true);
        assert_eq!(payload["canApprove"], false);
        assert_eq!(
            payload["reasonDisabled"],
            "The source changed. Reject this request and request again."
        );

        // Missing purchase/settlement entitlement blocks the PMS stage with
        // the exact retained message.
        let mut pms = row();
        pms.state = "pos_approved".into();
        let payload = pms.value(true, true, &[], false);
        assert_eq!(payload["stage"], "pms");
        assert_eq!(payload["canAct"], false);
        assert_eq!(
            payload["reasonDisabled"],
            "The selected assignment requires pms.approve."
        );
    }

    #[test]
    fn validates_scope_and_rejects_untrusted_ids() {
        let query = RoomChargeQuery {
            company: "co".into(),
            branch: Some("br".into()),
            property: None,
            actor: ActorDecision {
                pos_approve: true,
                pms_approve: false,
                department: false,
                self_ids: vec!["cashier".into()],
            },
            package: PackageDecision {
                enforced: false,
                history: false,
                settle_existing: false,
            },
        };
        assert!(validate(&query).is_ok());
        let mut bad = query.clone();
        bad.company.clear();
        assert!(validate(&bad).is_err());
        bad = RoomChargeQuery {
            property: Some("bad property".into()),
            ..query
        };
        assert!(validate(&bad).is_err());
        assert!(valid_row(&row()));
    }
}
