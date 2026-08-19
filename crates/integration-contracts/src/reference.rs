//! Stable record identity and disposable Item Directory descriptor contracts.

use std::{error::Error, fmt};

use chrono::{DateTime, Utc};
use erpx_kernel::{
    AuthorityScope, DataClassification, HoldingId, PolicyId, PrincipalId, RecordId, ScopeError,
    ValidationError, validate_bounded_text,
};
use serde::{Deserialize, Serialize};

use crate::catalog::{ContextId, RecordTypeDescriptor, record_type};

pub const MAX_LIFECYCLE_STATE_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordPolicyPrincipalKind {
    Human,
    Service,
    Agent,
    HumanGroup,
    AgentPool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordPolicyRealm {
    InternalWorkforce,
    PlatformWorkforce,
    CustomerPortal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordPolicyScopeRule {
    /// Same scope or an explicitly authorized ancestor. Sibling branches need
    /// a current two-sided Organization `OpenRecord` sharing agreement.
    HierarchyOrExplicitSharedBranch,
}

/// One explicit permission/subject/realm tuple approved by the record owner.
/// Multiple tuples are alternatives, never independently combinable parts;
/// this prevents a Customer Portal permission from being paired with an
/// Internal Workforce realm (or the reverse).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordPolicyAccessRule {
    pub permission: &'static str,
    pub principal_kinds: &'static [RecordPolicyPrincipalKind],
    pub realms: &'static [RecordPolicyRealm],
}

/// Owner-approved authorization policy for one live record family. This is a
/// closed registry: policy IDs are opaque, permissions are explicit, and a
/// family never inherits authority from a wildcard or named user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordPolicyDefinition {
    pub record_type: &'static str,
    pub owner: ContextId,
    policy_id: u128,
    pub version: u64,
    pub permission: &'static str,
    pub principal_kinds: &'static [RecordPolicyPrincipalKind],
    pub realms: &'static [RecordPolicyRealm],
    pub additional_access_rules: &'static [RecordPolicyAccessRule],
    pub classification_ceiling: DataClassification,
    pub scope_rule: RecordPolicyScopeRule,
    pub published_to_descendants: bool,
    pub effective_from_epoch_seconds: i64,
    pub revoked_at_epoch_seconds: Option<i64>,
    pub authority_evidence: &'static str,
}

impl RecordPolicyDefinition {
    pub fn policy_id(self) -> PolicyId {
        PolicyId::try_from_uuid(uuid::Uuid::from_u128(self.policy_id))
            .expect("static record policy IDs are non-nil")
    }

    pub const fn is_active_at(self, epoch_seconds: i64) -> bool {
        epoch_seconds >= self.effective_from_epoch_seconds
            && match self.revoked_at_epoch_seconds {
                Some(revoked_at) => epoch_seconds < revoked_at,
                None => true,
            }
    }

    pub fn permits_access(
        self,
        permission: &str,
        principal_kind: RecordPolicyPrincipalKind,
        realm: RecordPolicyRealm,
    ) -> bool {
        (self.permission == permission
            && self.principal_kinds.contains(&principal_kind)
            && self.realms.contains(&realm))
            || self.additional_access_rules.iter().any(|rule| {
                rule.permission == permission
                    && rule.principal_kinds.contains(&principal_kind)
                    && rule.realms.contains(&realm)
            })
    }

    pub fn accepts_permission(self, permission: &str) -> bool {
        self.permission == permission
            || self
                .additional_access_rules
                .iter()
                .any(|rule| rule.permission == permission)
    }
}

const INTERNAL_SUBJECTS: &[RecordPolicyPrincipalKind] = &[
    RecordPolicyPrincipalKind::Human,
    RecordPolicyPrincipalKind::Service,
];
const EXTERNAL_CUSTOMER_SUBJECTS: &[RecordPolicyPrincipalKind] = &[
    RecordPolicyPrincipalKind::Human,
    RecordPolicyPrincipalKind::Service,
];
const INTERNAL_REALMS: &[RecordPolicyRealm] = &[RecordPolicyRealm::InternalWorkforce];
const HUMAN_SUBJECTS: &[RecordPolicyPrincipalKind] = &[RecordPolicyPrincipalKind::Human];
const CUSTOMER_PORTAL_REALMS: &[RecordPolicyRealm] = &[RecordPolicyRealm::CustomerPortal];
const NO_ADDITIONAL_ACCESS: &[RecordPolicyAccessRule] = &[];
const PLATFORM_REALMS: &[RecordPolicyRealm] = &[
    RecordPolicyRealm::InternalWorkforce,
    RecordPolicyRealm::PlatformWorkforce,
];
const SUPPORT_REALMS: &[RecordPolicyRealm] = &[
    RecordPolicyRealm::InternalWorkforce,
    RecordPolicyRealm::PlatformWorkforce,
    RecordPolicyRealm::CustomerPortal,
];

const fn policy(
    record_type: &'static str,
    owner: ContextId,
    ordinal: u128,
    permission: &'static str,
    realms: &'static [RecordPolicyRealm],
    classification_ceiling: DataClassification,
) -> RecordPolicyDefinition {
    RecordPolicyDefinition {
        record_type,
        owner,
        policy_id: 0x4552_5058_0000_0000_0000_0000_0000_0000 | ordinal,
        version: 1,
        permission,
        principal_kinds: INTERNAL_SUBJECTS,
        realms,
        additional_access_rules: NO_ADDITIONAL_ACCESS,
        classification_ceiling,
        scope_rule: RecordPolicyScopeRule::HierarchyOrExplicitSharedBranch,
        published_to_descendants: false,
        // 2026-01-01T00:00:00Z. The registry is an accepted baseline, so a
        // runtime clock near the source snapshot must never make every policy
        // look future-dated and silently revoke otherwise valid authority.
        effective_from_epoch_seconds: 1_767_225_600,
        revoked_at_epoch_seconds: None,
        authority_evidence: "record-policy-baseline-v1",
    }
}

#[allow(clippy::too_many_arguments)]
const fn future_policy(
    record_type: &'static str,
    owner: ContextId,
    ordinal: u128,
    permission: &'static str,
    principal_kinds: &'static [RecordPolicyPrincipalKind],
    realms: &'static [RecordPolicyRealm],
    additional_access_rules: &'static [RecordPolicyAccessRule],
    classification_ceiling: DataClassification,
) -> RecordPolicyDefinition {
    RecordPolicyDefinition {
        record_type,
        owner,
        policy_id: 0x4552_5058_0000_0000_0000_0000_0000_0000 | ordinal,
        version: 1,
        permission,
        principal_kinds,
        realms,
        additional_access_rules,
        classification_ceiling,
        scope_rule: RecordPolicyScopeRule::HierarchyOrExplicitSharedBranch,
        published_to_descendants: false,
        effective_from_epoch_seconds: 1_767_225_600,
        revoked_at_epoch_seconds: None,
        authority_evidence: "future-record-policy-fsd006-v1",
    }
}

const fn published_policy(
    record_type: &'static str,
    owner: ContextId,
    ordinal: u128,
    permission: &'static str,
    realms: &'static [RecordPolicyRealm],
    classification_ceiling: DataClassification,
) -> RecordPolicyDefinition {
    let mut definition = policy(
        record_type,
        owner,
        ordinal,
        permission,
        realms,
        classification_ceiling,
    );
    definition.published_to_descendants = true;
    definition
}

pub const CURRENT_RECORD_POLICIES: [RecordPolicyDefinition; 92] = [
    policy(
        "identity.principal",
        ContextId::IdentityAccess,
        1,
        "identity_access.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "identity.role",
        ContextId::IdentityAccess,
        2,
        "role.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    published_policy(
        "organization.holding",
        ContextId::Organization,
        3,
        "organization.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    published_policy(
        "organization.company",
        ContextId::Organization,
        4,
        "organization.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "organization.branch",
        ContextId::Organization,
        5,
        "organization.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "commercial.subscription",
        ContextId::Commercial,
        6,
        "commercial.billing.read",
        PLATFORM_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "commercial.entitlement",
        ContextId::Commercial,
        7,
        "commercial.billing.read",
        PLATFORM_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "brand.profile",
        ContextId::Brand,
        8,
        "brand.read",
        INTERNAL_REALMS,
        DataClassification::Internal,
    ),
    policy(
        "customer.account",
        ContextId::CustomerAuthority,
        9,
        "customer_authority.read",
        PLATFORM_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "support.customer-case",
        ContextId::Support,
        10,
        "support.ticket.read",
        SUPPORT_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "support.incident",
        ContextId::Support,
        11,
        "support.incident.read",
        PLATFORM_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "support.access-session",
        ContextId::Support,
        12,
        "support.diagnostics.read",
        PLATFORM_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "finance.journal",
        ContextId::Finance,
        13,
        "finance.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "finance.invoice",
        ContextId::Finance,
        14,
        "finance.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "finance.close-cycle",
        ContextId::Finance,
        15,
        "finance.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "sales.quote",
        ContextId::Sales,
        16,
        "sales.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "sales.order",
        ContextId::Sales,
        17,
        "sales.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "pos.sale",
        ContextId::PointOfSale,
        18,
        "point-of-sale.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "pos.refund",
        ContextId::PointOfSale,
        19,
        "point-of-sale.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "procurement.purchase-order",
        ContextId::Procurement,
        20,
        "procurement.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "asset.asset",
        ContextId::Assets,
        21,
        "assets.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "maintenance.work-order",
        ContextId::Maintenance,
        22,
        "maintenance.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "quality.inspection",
        ContextId::Quality,
        23,
        "quality.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "operations.exception",
        ContextId::Operations,
        24,
        "operations.exception.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "people.employee",
        ContextId::PeoplePayroll,
        25,
        "hr-payroll.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "people.leave",
        ContextId::PeoplePayroll,
        26,
        "hr-payroll.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "payroll.run",
        ContextId::PeoplePayroll,
        27,
        "hr-payroll.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "legal.contract",
        ContextId::Legal,
        28,
        "legal.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "legal.matter",
        ContextId::Legal,
        29,
        "legal.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "crm.lead",
        ContextId::Crm,
        30,
        "marketing-crm.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "crm.opportunity",
        ContextId::Crm,
        31,
        "marketing-crm.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "identity.grant",
        ContextId::IdentityAccess,
        32,
        "identity_authority.grant.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "identity.access-review",
        ContextId::IdentityAccess,
        33,
        "identity_authority.access_review.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "identity.break-glass-session",
        ContextId::IdentityAccess,
        34,
        "identity_authority.break_glass.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "platform.holding",
        ContextId::PlatformAdministration,
        35,
        "platform.holding.read",
        PLATFORM_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "platform.data-plane-operation",
        ContextId::PlatformAdministration,
        36,
        "platform.data_plane.read",
        PLATFORM_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "platform.migration",
        ContextId::PlatformAdministration,
        37,
        "platform.migration.read",
        PLATFORM_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "platform.provider",
        ContextId::PlatformAdministration,
        38,
        "platform.provider.read",
        PLATFORM_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "platform.release",
        ContextId::PlatformAdministration,
        39,
        "platform.release.read",
        PLATFORM_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "organization.sharing-agreement",
        ContextId::Organization,
        40,
        "organization.sharing.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "commercial.account",
        ContextId::Commercial,
        41,
        "commercial.account.read",
        PLATFORM_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "commercial.adjustment",
        ContextId::Commercial,
        42,
        "commercial.adjustment.read",
        PLATFORM_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "brand.asset",
        ContextId::Brand,
        43,
        "brand.asset.read",
        INTERNAL_REALMS,
        DataClassification::Internal,
    ),
    policy(
        "brand.publication",
        ContextId::Brand,
        44,
        "brand.publication.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "customer.authority-grant",
        ContextId::CustomerAuthority,
        45,
        "customer_authority.grant.read",
        PLATFORM_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "customer.support-access",
        ContextId::CustomerAuthority,
        46,
        "customer_authority.support_access.read",
        PLATFORM_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "customer-360.profile",
        ContextId::Customer360,
        47,
        "customer_360.profile.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "customer-360.note",
        ContextId::Customer360,
        48,
        "customer_360.note.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "customer-360.merge",
        ContextId::Customer360,
        49,
        "customer_360.merge.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "support.platform-case",
        ContextId::Support,
        50,
        "support.platform_case.read",
        PLATFORM_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "finance.credit-note",
        ContextId::Finance,
        51,
        "finance.credit_note.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "finance.debit-note",
        ContextId::Finance,
        52,
        "finance.debit_note.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "finance.payment",
        ContextId::Finance,
        53,
        "finance.payment.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "finance.reconciliation",
        ContextId::Finance,
        54,
        "finance.reconciliation.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "sales.return",
        ContextId::Sales,
        55,
        "sales.return.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "sales.fulfillment-handoff",
        ContextId::Sales,
        56,
        "sales.fulfillment_handoff.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "pos.register-session",
        ContextId::PointOfSale,
        57,
        "point-of-sale.register_session.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "pos.cash-movement",
        ContextId::PointOfSale,
        58,
        "point-of-sale.cash_movement.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "procurement.requisition",
        ContextId::Procurement,
        59,
        "procurement.requisition.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "procurement.rfq",
        ContextId::Procurement,
        60,
        "procurement.rfq.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "procurement.receipt",
        ContextId::Procurement,
        61,
        "procurement.receipt.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "procurement.supplier-return",
        ContextId::Procurement,
        62,
        "procurement.supplier_return.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "inventory.document",
        ContextId::Inventory,
        63,
        "inventory.document.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "inventory.transfer",
        ContextId::Inventory,
        64,
        "inventory.transfer.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "inventory.count",
        ContextId::Inventory,
        65,
        "inventory.count.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "inventory.reservation",
        ContextId::Inventory,
        66,
        "inventory.reservation.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "inventory.lot",
        ContextId::Inventory,
        67,
        "inventory.lot.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "inventory.serial",
        ContextId::Inventory,
        68,
        "inventory.serial.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "asset.transfer",
        ContextId::Assets,
        69,
        "assets.transfer.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "asset.depreciation-run",
        ContextId::Assets,
        70,
        "assets.depreciation.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "asset.disposal",
        ContextId::Assets,
        71,
        "assets.disposal.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "maintenance.plan",
        ContextId::Maintenance,
        72,
        "maintenance.plan.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "maintenance.inspection",
        ContextId::Maintenance,
        73,
        "maintenance.inspection.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "maintenance.request",
        ContextId::Maintenance,
        74,
        "maintenance.request.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "quality.plan",
        ContextId::Quality,
        75,
        "quality.plan.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "quality.nonconformance",
        ContextId::Quality,
        76,
        "quality.nonconformance.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "quality.corrective-action",
        ContextId::Quality,
        77,
        "quality.corrective_action.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "operations.operation",
        ContextId::Operations,
        78,
        "operations.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "operations.case",
        ContextId::Operations,
        79,
        "operations.case.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "operations.approval-work",
        ContextId::Operations,
        80,
        "operations.approval_work.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "people.employment",
        ContextId::PeoplePayroll,
        81,
        "hr-payroll.employment.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "people.time-record",
        ContextId::PeoplePayroll,
        82,
        "hr-payroll.time_record.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "payroll.payslip",
        ContextId::PeoplePayroll,
        83,
        "hr-payroll.payslip.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "legal.case",
        ContextId::Legal,
        84,
        "legal.case.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "legal.hold",
        ContextId::Legal,
        85,
        "legal.hold.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "legal.signature-envelope",
        ContextId::Legal,
        86,
        "legal.signature_envelope.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "marketing.campaign",
        ContextId::Marketing,
        87,
        "marketing.campaign.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "marketing.content",
        ContextId::Marketing,
        88,
        "marketing.content.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "marketing.audience-activation",
        ContextId::Marketing,
        89,
        "marketing.audience_activation.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "marketing.consent-purpose",
        ContextId::Marketing,
        90,
        "marketing.consent_purpose.read",
        INTERNAL_REALMS,
        DataClassification::Restricted,
    ),
    policy(
        "crm.activity",
        ContextId::Crm,
        91,
        "marketing-crm.activity.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
    policy(
        "crm.forecast",
        ContextId::Crm,
        92,
        "marketing-crm.forecast.read",
        INTERNAL_REALMS,
        DataClassification::Confidential,
    ),
];

const COMMERCE_CATALOG_EXTERNAL: &[RecordPolicyAccessRule] = &[
    RecordPolicyAccessRule {
        permission: "commerce.customer.catalog.read",
        principal_kinds: HUMAN_SUBJECTS,
        realms: CUSTOMER_PORTAL_REALMS,
    },
    RecordPolicyAccessRule {
        permission: "commerce.guest.catalog.read",
        principal_kinds: HUMAN_SUBJECTS,
        realms: CUSTOMER_PORTAL_REALMS,
    },
];
const COMMERCE_CART_EXTERNAL: &[RecordPolicyAccessRule] = &[
    RecordPolicyAccessRule {
        permission: "commerce.customer.cart.read",
        principal_kinds: HUMAN_SUBJECTS,
        realms: CUSTOMER_PORTAL_REALMS,
    },
    RecordPolicyAccessRule {
        permission: "commerce.guest.cart.read",
        principal_kinds: HUMAN_SUBJECTS,
        realms: CUSTOMER_PORTAL_REALMS,
    },
];
const COMMERCE_ORDER_EXTERNAL: &[RecordPolicyAccessRule] = &[
    RecordPolicyAccessRule {
        permission: "commerce.customer.order.read",
        principal_kinds: HUMAN_SUBJECTS,
        realms: CUSTOMER_PORTAL_REALMS,
    },
    RecordPolicyAccessRule {
        permission: "commerce.guest.order.read",
        principal_kinds: HUMAN_SUBJECTS,
        realms: CUSTOMER_PORTAL_REALMS,
    },
];
const COMMERCE_RETURN_EXTERNAL: &[RecordPolicyAccessRule] = &[
    RecordPolicyAccessRule {
        permission: "commerce.customer.return.read",
        principal_kinds: HUMAN_SUBJECTS,
        realms: CUSTOMER_PORTAL_REALMS,
    },
    RecordPolicyAccessRule {
        permission: "commerce.guest.return.read",
        principal_kinds: HUMAN_SUBJECTS,
        realms: CUSTOMER_PORTAL_REALMS,
    },
];
const TALENT_CANDIDATE_SELF: &[RecordPolicyAccessRule] = &[RecordPolicyAccessRule {
    permission: "talent.candidate.self.read",
    principal_kinds: HUMAN_SUBJECTS,
    realms: CUSTOMER_PORTAL_REALMS,
}];
const FIELD_SERVICE_CUSTOMER: &[RecordPolicyAccessRule] = &[RecordPolicyAccessRule {
    permission: "field_service.customer.read",
    principal_kinds: EXTERNAL_CUSTOMER_SUBJECTS,
    realms: CUSTOMER_PORTAL_REALMS,
}];
const COLLABORATION_PORTAL: &[RecordPolicyAccessRule] = &[RecordPolicyAccessRule {
    permission: "collaboration.portal.read",
    principal_kinds: HUMAN_SUBJECTS,
    realms: CUSTOMER_PORTAL_REALMS,
}];

/// Accepted future-context policies. The 49 entries exactly match the live
/// future owner publishers; no catalog-only record family receives authority.
pub const FUTURE_RECORD_POLICIES: [RecordPolicyDefinition; 49] = [
    future_policy(
        "manufacturing.specification",
        ContextId::Manufacturing,
        101,
        "manufacturing.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "manufacturing.routing",
        ContextId::Manufacturing,
        102,
        "manufacturing.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "manufacturing.order",
        ContextId::Manufacturing,
        103,
        "manufacturing.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "manufacturing.batch",
        ContextId::Manufacturing,
        104,
        "manufacturing.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "manufacturing.result",
        ContextId::Manufacturing,
        105,
        "manufacturing.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "planning.scenario",
        ContextId::Planning,
        106,
        "planning.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "planning.demand-plan",
        ContextId::Planning,
        107,
        "planning.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "planning.supply-plan",
        ContextId::Planning,
        108,
        "planning.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "planning.capacity-plan",
        ContextId::Planning,
        109,
        "planning.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "planning.recommendation",
        ContextId::Planning,
        110,
        "planning.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "project.project",
        ContextId::Projects,
        111,
        "projects.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "project.phase",
        ContextId::Projects,
        112,
        "projects.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "project.item",
        ContextId::Projects,
        113,
        "projects.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "project.template",
        ContextId::Projects,
        114,
        "projects.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Internal,
    ),
    future_policy(
        "project.handoff",
        ContextId::Projects,
        115,
        "projects.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "commerce.catalog-publication",
        ContextId::Ecommerce,
        116,
        "commerce.catalog.manage.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        COMMERCE_CATALOG_EXTERNAL,
        DataClassification::Internal,
    ),
    future_policy(
        "commerce.cart",
        ContextId::Ecommerce,
        117,
        "commerce.cart.manage.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        COMMERCE_CART_EXTERNAL,
        DataClassification::Confidential,
    ),
    future_policy(
        "commerce.order",
        ContextId::Ecommerce,
        118,
        "commerce.order.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        COMMERCE_ORDER_EXTERNAL,
        DataClassification::Confidential,
    ),
    future_policy(
        "commerce.fulfillment-request",
        ContextId::Ecommerce,
        119,
        "commerce.fulfillment.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "commerce.return",
        ContextId::Ecommerce,
        120,
        "commerce.return.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        COMMERCE_RETURN_EXTERNAL,
        DataClassification::Confidential,
    ),
    future_policy(
        "talent.requisition",
        ContextId::RecruitingTalent,
        121,
        "talent.workforce.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "talent.candidate",
        ContextId::RecruitingTalent,
        122,
        "talent.candidate.manage.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        TALENT_CANDIDATE_SELF,
        DataClassification::Restricted,
    ),
    future_policy(
        "talent.application",
        ContextId::RecruitingTalent,
        123,
        "talent.candidate.manage.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        TALENT_CANDIDATE_SELF,
        DataClassification::Restricted,
    ),
    future_policy(
        "talent.interview",
        ContextId::RecruitingTalent,
        124,
        "talent.candidate.manage.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        TALENT_CANDIDATE_SELF,
        DataClassification::Restricted,
    ),
    future_policy(
        "talent.offer",
        ContextId::RecruitingTalent,
        125,
        "talent.candidate.manage.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        TALENT_CANDIDATE_SELF,
        DataClassification::Restricted,
    ),
    future_policy(
        "talent.review-cycle",
        ContextId::RecruitingTalent,
        126,
        "talent.workforce.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "talent.proposal",
        ContextId::RecruitingTalent,
        127,
        "talent.workforce.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "learning.course",
        ContextId::Learning,
        128,
        "learning.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "learning.assignment",
        ContextId::Learning,
        129,
        "learning.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "learning.enrollment",
        ContextId::Learning,
        130,
        "learning.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "learning.completion",
        ContextId::Learning,
        131,
        "learning.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "benefits.plan",
        ContextId::Benefits,
        132,
        "benefits.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "benefits.window",
        ContextId::Benefits,
        133,
        "benefits.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "benefits.election",
        ContextId::Benefits,
        134,
        "benefits.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "benefits.handoff",
        ContextId::Benefits,
        135,
        "benefits.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "field-service.agreement",
        ContextId::FieldService,
        136,
        "field_service.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        FIELD_SERVICE_CUSTOMER,
        DataClassification::Confidential,
    ),
    future_policy(
        "field-service.request",
        ContextId::FieldService,
        137,
        "field_service.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        FIELD_SERVICE_CUSTOMER,
        DataClassification::Confidential,
    ),
    future_policy(
        "field-service.dispatch",
        ContextId::FieldService,
        138,
        "field_service.dispatch.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "field-service.visit",
        ContextId::FieldService,
        139,
        "field_service.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        FIELD_SERVICE_CUSTOMER,
        DataClassification::Confidential,
    ),
    future_policy(
        "field-service.handoff",
        ContextId::FieldService,
        140,
        "field_service.handoff.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Confidential,
    ),
    future_policy(
        "collaboration.thread",
        ContextId::Collaboration,
        141,
        "collaboration.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        COLLABORATION_PORTAL,
        DataClassification::Restricted,
    ),
    future_policy(
        "collaboration.moderation-case",
        ContextId::Collaboration,
        142,
        "collaboration.moderation.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "collaboration.promotion",
        ContextId::Collaboration,
        143,
        "collaboration.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        COLLABORATION_PORTAL,
        DataClassification::Restricted,
    ),
    future_policy(
        "agent.definition",
        ContextId::AgentOrchestration,
        144,
        "agent.registry.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "agent.version",
        ContextId::AgentOrchestration,
        145,
        "agent.registry.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "agent.assignment",
        ContextId::AgentOrchestration,
        146,
        "agent.assignment.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "agent.delegation-tree",
        ContextId::AgentOrchestration,
        147,
        "agent.assignment.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "agent.session",
        ContextId::AgentOrchestration,
        148,
        "agent.session.read",
        HUMAN_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
    future_policy(
        "agent.budget-decision",
        ContextId::AgentOrchestration,
        149,
        "agent.budget.read",
        INTERNAL_SUBJECTS,
        INTERNAL_REALMS,
        NO_ADDITIONAL_ACCESS,
        DataClassification::Restricted,
    ),
];

pub fn record_policies() -> impl Iterator<Item = &'static RecordPolicyDefinition> {
    CURRENT_RECORD_POLICIES
        .iter()
        .chain(FUTURE_RECORD_POLICIES.iter())
}

pub fn record_policy(record_type: &str) -> Option<&'static RecordPolicyDefinition> {
    record_policies().find(|definition| definition.record_type == record_type)
}

pub fn record_policy_id(record: &RecordRef) -> Result<PolicyId, ReferenceValidationError> {
    let definition = record_policy(record.record_type().as_str())
        .ok_or(ReferenceValidationError::UnknownRecordPolicy)?;
    if definition.owner != record.owner_context() {
        return Err(ReferenceValidationError::OwnerMismatch);
    }
    Ok(definition.policy_id())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecordTypeId(String);

impl RecordTypeId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, ReferenceValidationError> {
        let value = value.into();
        if record_type(&value).is_none() {
            return Err(ReferenceValidationError::UnknownRecordType);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn descriptor(&self) -> &'static RecordTypeDescriptor {
        record_type(&self.0).expect("RecordTypeId is validated against the static catalog")
    }
}

/// Stable record identity. Aggregate version and route are deliberately not
/// part of identity, so copied links survive lifecycle and route changes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RecordRef {
    id: RecordId,
    owner_context: ContextId,
    record_type: RecordTypeId,
}

impl RecordRef {
    pub fn try_new(
        id: RecordId,
        owner_context: ContextId,
        record_type: RecordTypeId,
    ) -> Result<Self, ReferenceValidationError> {
        let value = Self {
            id,
            owner_context,
            record_type,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        if self.record_type.descriptor().owner != self.owner_context {
            return Err(ReferenceValidationError::OwnerMismatch);
        }
        Ok(())
    }

    pub const fn id(&self) -> RecordId {
        self.id
    }

    pub const fn owner_context(&self) -> ContextId {
        self.owner_context
    }

    pub const fn record_type(&self) -> &RecordTypeId {
        &self.record_type
    }

    pub fn canonical_collection_route(&self) -> String {
        self.record_type.descriptor().collection_route()
    }

    pub fn canonical_detail_route(&self) -> String {
        let descriptor = self.record_type.descriptor();
        format!("/{}/{}/{}", descriptor.module, descriptor.record, self.id)
    }

    pub fn copied_reference_route(&self) -> String {
        format!("/reference/item/{}", self.id)
    }
}

/// A reference to an exact aggregate state, for commands, approvals, anchors,
/// and immutable evidence. It is not used as the stable copied-link identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordVersionRef {
    record: RecordRef,
    version: u64,
}

impl RecordVersionRef {
    pub fn try_new(record: RecordRef, version: u64) -> Result<Self, ReferenceValidationError> {
        record.validate()?;
        if version == 0 {
            return Err(ReferenceValidationError::ZeroVersion("record_version"));
        }
        Ok(Self { record, version })
    }

    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        self.record.validate()?;
        if self.version == 0 {
            return Err(ReferenceValidationError::ZeroVersion("record_version"));
        }
        Ok(())
    }

    pub const fn record(&self) -> &RecordRef {
        &self.record
    }

    pub const fn version(&self) -> u64 {
        self.version
    }
}

/// Rebuildable authorized descriptor published by the record owner. This is a
/// projection contract, never an alternate write authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDescriptor {
    record: RecordRef,
    scope: AuthorityScope,
    canonical_route: String,
    lifecycle_state: String,
    authorization_policy: PolicyId,
    authorization_policy_version: u64,
    classification: DataClassification,
    descriptor_version: u64,
    source_event_position: u64,
}

impl ItemDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        record: RecordRef,
        scope: AuthorityScope,
        lifecycle_state: impl Into<String>,
        authorization_policy: PolicyId,
        classification: DataClassification,
        descriptor_version: u64,
        source_event_position: u64,
    ) -> Result<Self, ReferenceValidationError> {
        let canonical_route = record.canonical_detail_route();
        let authorization_policy_version =
            record_policy(record.record_type().as_str()).map_or(1, |definition| definition.version);
        let value = Self {
            record,
            scope,
            canonical_route,
            lifecycle_state: lifecycle_state.into(),
            authorization_policy,
            authorization_policy_version,
            classification,
            descriptor_version,
            source_event_position,
        };
        value.validate()?;
        Ok(value)
    }

    /// Constructs a descriptor after an approved canonical route move. The
    /// stable record identity remains unchanged and copied reference routes do
    /// not depend on this mutable route.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new_with_route(
        record: RecordRef,
        scope: AuthorityScope,
        canonical_route: impl Into<String>,
        lifecycle_state: impl Into<String>,
        authorization_policy: PolicyId,
        classification: DataClassification,
        descriptor_version: u64,
        source_event_position: u64,
    ) -> Result<Self, ReferenceValidationError> {
        let authorization_policy_version =
            record_policy(record.record_type().as_str()).map_or(1, |definition| definition.version);
        let value = Self {
            record,
            scope,
            canonical_route: canonical_route.into(),
            lifecycle_state: lifecycle_state.into(),
            authorization_policy,
            authorization_policy_version,
            classification,
            descriptor_version,
            source_event_position,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        self.record.validate()?;
        self.scope.validate()?;
        validate_bounded_text(
            &self.lifecycle_state,
            "lifecycle_state",
            MAX_LIFECYCLE_STATE_BYTES,
        )?;
        if !self.lifecycle_state.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        }) {
            return Err(ReferenceValidationError::InvalidLifecycleState);
        }
        if !canonical_route_matches_record(&self.canonical_route, self.record.id()) {
            return Err(ReferenceValidationError::CanonicalRouteMismatch);
        }
        if let Some(definition) = record_policy(self.record.record_type().as_str()) {
            if definition.owner != self.record.owner_context()
                || definition.policy_id() != self.authorization_policy
                || definition.version != self.authorization_policy_version
            {
                return Err(ReferenceValidationError::RecordPolicyMismatch);
            }
            if !self
                .classification
                .is_permitted_by(definition.classification_ceiling)
            {
                return Err(ReferenceValidationError::ClassificationExceedsPolicy);
            }
        }
        if self.authorization_policy_version == 0 {
            return Err(ReferenceValidationError::ZeroVersion(
                "authorization_policy_version",
            ));
        }
        if self.descriptor_version == 0 {
            return Err(ReferenceValidationError::ZeroVersion("descriptor_version"));
        }
        if self.source_event_position == 0 {
            return Err(ReferenceValidationError::ZeroVersion(
                "source_event_position",
            ));
        }
        Ok(())
    }

    pub const fn record(&self) -> &RecordRef {
        &self.record
    }

    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }

    pub fn canonical_route(&self) -> &str {
        &self.canonical_route
    }

    pub fn lifecycle_state(&self) -> &str {
        &self.lifecycle_state
    }

    pub const fn authorization_policy(&self) -> PolicyId {
        self.authorization_policy
    }

    pub const fn authorization_policy_version(&self) -> u64 {
        self.authorization_policy_version
    }

    pub const fn classification(&self) -> DataClassification {
        self.classification
    }

    pub const fn descriptor_version(&self) -> u64 {
        self.descriptor_version
    }

    pub const fn source_event_position(&self) -> u64 {
        self.source_event_position
    }
}

fn canonical_route_matches_record(route: &str, record_id: RecordId) -> bool {
    let Some(route) = route.strip_prefix('/') else {
        return false;
    };
    let mut segments = route.split('/');
    let Some(module) = segments.next() else {
        return false;
    };
    let Some(record) = segments.next() else {
        return false;
    };
    let Some(id) = segments.next() else {
        return false;
    };
    segments.next().is_none()
        && !module.is_empty()
        && !record.is_empty()
        && module
            .bytes()
            .chain(record.bytes())
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && id == record_id.to_string()
}

pub const ITEM_DIRECTORY_FACT_VERSION: u32 = 1;
pub const ITEM_DIRECTORY_OWNER_EVENT_TYPE: &str = "erpx.item-directory.owner-fact";
/// Fully typed grant, revocation, relation, route, and tombstone facts. These
/// facts already carry their immutable owner source position; the worker
/// rejects a fact that claims a position after its durable delivery envelope.
pub const ITEM_DIRECTORY_FACT_EVENT_TYPE: &str = "erpx.item-directory.fact";
pub const ITEM_DIRECTORY_PROJECTION_NAME: &str = "item_directory";
pub const MAX_AUTHORIZED_SEARCH_TEXT_BYTES: usize = 4_096;

/// Domain-owned descriptor material before the write-side outbox assigns its
/// committed global sequence. The worker is the only component allowed to
/// materialize this draft, using the immutable sequence read from the outbox;
/// it must never derive descriptor fields from an arbitrary aggregate JSON
/// snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDescriptorDraft {
    pub record: RecordRef,
    pub scope: AuthorityScope,
    pub lifecycle_state: String,
    pub authorization_policy: PolicyId,
    pub classification: DataClassification,
    pub descriptor_version: u64,
    pub authorized_search_text: String,
}

impl ItemDescriptorDraft {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        self.materialize(1).map(|_| ())
    }

    pub fn materialize(
        &self,
        committed_source_position: u64,
    ) -> Result<ItemDirectoryFact, ReferenceValidationError> {
        let descriptor = ItemDescriptor::try_new(
            self.record.clone(),
            self.scope.clone(),
            self.lifecycle_state.clone(),
            self.authorization_policy,
            self.classification,
            self.descriptor_version,
            committed_source_position,
        )?;
        let value = ItemDescriptorChanged {
            descriptor,
            authorized_search_text: self.authorized_search_text.clone(),
        };
        value.validate()?;
        Ok(ItemDirectoryFact::DescriptorChanged(value))
    }
}

/// Owner-emitted, versioned facts are the only inputs accepted by the
/// federated Item Directory. Tombstone and revocation variants deliberately
/// carry no display label, record type, lifecycle state, or search text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "fact_kind", content = "fact", rename_all = "snake_case")]
pub enum ItemDirectoryFact {
    DescriptorChanged(ItemDescriptorChanged),
    EffectiveGrantChanged(EffectiveGrantChanged),
    EffectiveGrantRevoked(EffectiveGrantRevoked),
    RelationChanged(ReferenceRelationChanged),
    RelationRevoked(ReferenceRelationRevoked),
    RouteMoved(CanonicalRouteMoved),
    Tombstoned(ItemTombstoned),
}

impl ItemDirectoryFact {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        match self {
            Self::DescriptorChanged(value) => value.validate(),
            Self::EffectiveGrantChanged(value) => value.validate(),
            Self::EffectiveGrantRevoked(value) => value.validate(),
            Self::RelationChanged(value) => value.validate(),
            Self::RelationRevoked(value) => value.validate(),
            Self::RouteMoved(value) => value.validate(),
            Self::Tombstoned(value) => value.validate(),
        }
    }

    pub const fn source_event_position(&self) -> u64 {
        match self {
            Self::DescriptorChanged(value) => value.descriptor.source_event_position(),
            Self::EffectiveGrantChanged(value) => value.source_event_position,
            Self::EffectiveGrantRevoked(value) => value.source_event_position,
            Self::RelationChanged(value) => value.source_event_position,
            Self::RelationRevoked(value) => value.source_event_position,
            Self::RouteMoved(value) => value.source_event_position,
            Self::Tombstoned(value) => value.source_event_position,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldingItemDirectoryFact {
    pub holding_id: HoldingId,
    pub event_version: u32,
    pub fact: ItemDirectoryFact,
}

impl HoldingItemDirectoryFact {
    pub fn try_new(
        holding_id: HoldingId,
        fact: ItemDirectoryFact,
    ) -> Result<Self, ReferenceValidationError> {
        let value = Self {
            holding_id,
            event_version: ITEM_DIRECTORY_FACT_VERSION,
            fact,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        if self.event_version != ITEM_DIRECTORY_FACT_VERSION {
            return Err(ReferenceValidationError::UnsupportedFactVersion);
        }
        self.fact.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDescriptorChanged {
    pub descriptor: ItemDescriptor,
    /// Search text was explicitly authorized by the record owner; the
    /// directory never derives it from an arbitrary aggregate payload.
    pub authorized_search_text: String,
}

impl ItemDescriptorChanged {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        self.descriptor.validate()?;
        if self.authorized_search_text.is_empty()
            || self.authorized_search_text.len() > MAX_AUTHORIZED_SEARCH_TEXT_BYTES
            || self.authorized_search_text.chars().any(char::is_control)
        {
            return Err(ReferenceValidationError::InvalidAuthorizedSearchText);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveGrantChanged {
    pub record: RecordRef,
    pub principal_id: PrincipalId,
    pub authorization_policy: PolicyId,
    pub policy_version: u64,
    /// Exact validity window returned by the current authorization-policy
    /// evaluation. Organizational scope alone can never populate this fact.
    pub effective_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub source_event_position: u64,
}

impl EffectiveGrantChanged {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        self.record.validate()?;
        require_nonzero(self.policy_version, "policy_version")?;
        if let Some(definition) = record_policy(self.record.record_type().as_str())
            && (definition.policy_id() != self.authorization_policy
                || definition.version != self.policy_version)
        {
            return Err(ReferenceValidationError::RecordPolicyMismatch);
        }
        if self.expires_at <= self.effective_at {
            return Err(ReferenceValidationError::InvalidGrantWindow);
        }
        require_nonzero(self.source_event_position, "source_event_position")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveGrantRevoked {
    pub record_id: RecordId,
    pub principal_id: PrincipalId,
    pub authorization_policy: PolicyId,
    pub policy_version: u64,
    pub source_event_position: u64,
}

impl EffectiveGrantRevoked {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        require_nonzero(self.policy_version, "policy_version")?;
        require_nonzero(self.source_event_position, "source_event_position")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceRelationChanged {
    pub relation_id: uuid::Uuid,
    pub source: RecordRef,
    pub target: RecordRef,
    pub relation_type: String,
    pub relation_version: u64,
    pub source_event_position: u64,
}

impl ReferenceRelationChanged {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        self.source.validate()?;
        self.target.validate()?;
        if self.relation_id.is_nil()
            || self.source.id() == self.target.id()
            || !matches!(
                self.relation_type.as_str(),
                "relates-to"
                    | "blocks"
                    | "depends-on"
                    | "implements"
                    | "supersedes"
                    | "derived-from"
                    | "evidence-for"
            )
        {
            return Err(ReferenceValidationError::InvalidRelation);
        }
        require_nonzero(self.relation_version, "relation_version")?;
        require_nonzero(self.source_event_position, "source_event_position")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceRelationRevoked {
    pub relation_id: uuid::Uuid,
    pub source_event_position: u64,
}

impl ReferenceRelationRevoked {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        if self.relation_id.is_nil() {
            return Err(ReferenceValidationError::InvalidRelation);
        }
        require_nonzero(self.source_event_position, "source_event_position")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalRouteMoved {
    pub record: RecordRef,
    pub canonical_route: String,
    pub descriptor_version: u64,
    pub source_event_position: u64,
}

impl CanonicalRouteMoved {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        self.record.validate()?;
        if !canonical_route_matches_record(&self.canonical_route, self.record.id()) {
            return Err(ReferenceValidationError::CanonicalRouteMismatch);
        }
        require_nonzero(self.descriptor_version, "descriptor_version")?;
        require_nonzero(self.source_event_position, "source_event_position")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemTombstoned {
    pub record_id: RecordId,
    pub descriptor_version: u64,
    pub source_event_position: u64,
}

impl ItemTombstoned {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        require_nonzero(self.descriptor_version, "descriptor_version")?;
        require_nonzero(self.source_event_position, "source_event_position")
    }
}

fn require_nonzero(value: u64, field: &'static str) -> Result<(), ReferenceValidationError> {
    (value > 0)
        .then_some(())
        .ok_or(ReferenceValidationError::ZeroVersion(field))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceValidationError {
    UnknownRecordType,
    UnknownRecordPolicy,
    OwnerMismatch,
    InvalidLifecycleState,
    CanonicalRouteMismatch,
    ZeroVersion(&'static str),
    InvalidScope,
    InvalidText(ValidationError),
    InvalidAuthorizedSearchText,
    InvalidRelation,
    InvalidGrantWindow,
    UnsupportedFactVersion,
    RecordPolicyMismatch,
    ClassificationExceedsPolicy,
    MalformedAuthorityEvidence,
}

impl fmt::Display for ReferenceValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRecordType => formatter.write_str("record type is not cataloged"),
            Self::UnknownRecordPolicy => {
                formatter.write_str("record type has no approved record policy")
            }
            Self::OwnerMismatch => {
                formatter.write_str("record type is not owned by the declared context")
            }
            Self::InvalidLifecycleState => formatter.write_str("lifecycle state is invalid"),
            Self::CanonicalRouteMismatch => {
                formatter.write_str("canonical route does not match the record catalog")
            }
            Self::ZeroVersion(field) => write!(formatter, "{field} must be nonzero"),
            Self::InvalidScope => formatter.write_str("record scope is invalid"),
            Self::InvalidText(error) => error.fmt(formatter),
            Self::InvalidAuthorizedSearchText => {
                formatter.write_str("authorized search text is invalid")
            }
            Self::InvalidRelation => formatter.write_str("reference relation is invalid"),
            Self::InvalidGrantWindow => formatter.write_str("effective grant window is invalid"),
            Self::UnsupportedFactVersion => {
                formatter.write_str("Item Directory fact version is unsupported")
            }
            Self::RecordPolicyMismatch => {
                formatter.write_str("record descriptor policy does not match its owner registry")
            }
            Self::ClassificationExceedsPolicy => {
                formatter.write_str("record classification exceeds its policy ceiling")
            }
            Self::MalformedAuthorityEvidence => {
                formatter.write_str("authority evidence is malformed")
            }
        }
    }
}

impl Error for ReferenceValidationError {}

impl From<ScopeError> for ReferenceValidationError {
    fn from(_: ScopeError) -> Self {
        Self::InvalidScope
    }
}

impl From<ValidationError> for ReferenceValidationError {
    fn from(value: ValidationError) -> Self {
        Self::InvalidText(value)
    }
}

#[cfg(test)]
mod tests {
    use erpx_kernel::{PolicyId, RecordId};
    use uuid::Uuid;

    use super::*;

    fn record_id() -> RecordId {
        RecordId::try_from_uuid(Uuid::new_v4()).unwrap()
    }

    fn policy_id() -> PolicyId {
        PolicyId::try_from_uuid(Uuid::new_v4()).unwrap()
    }

    #[test]
    fn stable_identity_and_version_are_separate() {
        let record = RecordRef::try_new(
            record_id(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let first = RecordVersionRef::try_new(record.clone(), 1).unwrap();
        let later = RecordVersionRef::try_new(record.clone(), 99).unwrap();

        assert_eq!(first.record(), later.record());
        assert_ne!(first.version(), later.version());
        assert_eq!(
            record.canonical_detail_route(),
            format!("/finance/invoice/{}", record.id())
        );
        assert_eq!(
            record.copied_reference_route(),
            format!("/reference/item/{}", record.id())
        );
    }

    #[test]
    fn references_reject_wrong_owner_and_zero_version_but_accept_canonical_uuid_ids() {
        let kind = RecordTypeId::try_new("finance.invoice").unwrap();
        assert_eq!(
            RecordRef::try_new(record_id(), ContextId::Sales, kind.clone()),
            Err(ReferenceValidationError::OwnerMismatch)
        );
        let non_v4 = RecordId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        RecordRef::try_new(non_v4, ContextId::Finance, kind).unwrap();
        let record = RecordRef::try_new(
            record_id(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        assert_eq!(
            RecordVersionRef::try_new(record, 0),
            Err(ReferenceValidationError::ZeroVersion("record_version"))
        );
    }

    #[test]
    fn item_descriptor_derives_and_validates_the_canonical_route() {
        let record = RecordRef::try_new(
            record_id(),
            ContextId::Projects,
            RecordTypeId::try_new("project.item").unwrap(),
        )
        .unwrap();
        let descriptor = ItemDescriptor::try_new(
            record.clone(),
            AuthorityScope::Platform,
            "in-progress",
            record_policy_id(&record).unwrap(),
            DataClassification::Confidential,
            4,
            92,
        )
        .unwrap();

        assert_eq!(
            descriptor.canonical_route(),
            record.canonical_detail_route()
        );
        assert_eq!(descriptor.descriptor_version(), 4);
        assert_eq!(descriptor.source_event_position(), 92);
    }

    #[test]
    fn approved_route_move_keeps_stable_reference_identity() {
        let record = RecordRef::try_new(
            record_id(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let moved = format!("/accounting/invoice/{}", record.id());
        let descriptor = ItemDescriptor::try_new_with_route(
            record.clone(),
            AuthorityScope::Platform,
            &moved,
            "posted",
            record_policy_id(&record).unwrap(),
            DataClassification::Confidential,
            2,
            10,
        )
        .unwrap();
        assert_eq!(descriptor.canonical_route(), moved);
        assert_eq!(
            record.copied_reference_route(),
            format!("/reference/item/{}", record.id())
        );
    }

    #[test]
    fn revocation_and_tombstone_facts_are_opaque() {
        let revocation = ItemDirectoryFact::EffectiveGrantRevoked(EffectiveGrantRevoked {
            record_id: record_id(),
            principal_id: PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(),
            authorization_policy: policy_id(),
            policy_version: 2,
            source_event_position: 11,
        });
        let tombstone = ItemDirectoryFact::Tombstoned(ItemTombstoned {
            record_id: record_id(),
            descriptor_version: 3,
            source_event_position: 12,
        });
        for fact in [revocation, tombstone] {
            fact.validate().unwrap();
            let wire = serde_json::to_string(&fact).unwrap();
            for forbidden in ["record_type", "owner_context", "title", "lifecycle_state"] {
                assert!(!wire.contains(forbidden));
            }
        }
    }

    #[test]
    fn effective_grant_requires_an_explicit_finite_policy_window() {
        let effective_at = Utc::now();
        let record = RecordRef::try_new(
            record_id(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap();
        let definition = record_policy("finance.invoice").unwrap();
        let base = EffectiveGrantChanged {
            record,
            principal_id: PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(),
            authorization_policy: definition.policy_id(),
            policy_version: definition.version,
            effective_at,
            expires_at: effective_at + chrono::Duration::hours(1),
            source_event_position: 19,
        };
        base.validate().unwrap();

        let mut invalid = base;
        invalid.expires_at = invalid.effective_at;
        assert_eq!(
            invalid.validate(),
            Err(ReferenceValidationError::InvalidGrantWindow)
        );
    }

    #[test]
    fn current_record_policy_registry_is_exact_unique_and_wildcard_free() {
        use std::collections::BTreeSet;

        assert_eq!(CURRENT_RECORD_POLICIES.len(), 92);
        let policy_records = CURRENT_RECORD_POLICIES
            .iter()
            .map(|definition| definition.record_type)
            .collect::<BTreeSet<_>>();
        let current_catalog = crate::catalog::RECORD_TYPES
            .iter()
            .filter(|record| record.owner.lifecycle() == crate::catalog::ContextLifecycle::Current)
            .map(|record| record.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(policy_records, current_catalog);
        assert_eq!(
            CURRENT_RECORD_POLICIES
                .iter()
                .map(|definition| definition.policy_id())
                .collect::<BTreeSet<_>>()
                .len(),
            92
        );
        for definition in CURRENT_RECORD_POLICIES {
            assert_eq!(
                record_type(definition.record_type).unwrap().owner,
                definition.owner
            );
            assert!(definition.version > 0);
            assert!(!definition.permission.contains('*'));
            assert!(!definition.permission.contains("admin"));
            assert!(definition.is_active_at(1_787_001_600));
            assert_eq!(definition.authority_evidence, "record-policy-baseline-v1");
        }
    }

    #[test]
    fn combined_live_policy_registry_is_exact_unique_and_owner_approved() {
        use std::collections::BTreeSet;

        assert_eq!(FUTURE_RECORD_POLICIES.len(), 49);
        let future_catalog = crate::catalog::RECORD_TYPES
            .iter()
            .filter(|record| record.owner.lifecycle() != crate::catalog::ContextLifecycle::Current)
            .map(|record| record.id)
            .collect::<BTreeSet<_>>();
        let future_policies = FUTURE_RECORD_POLICIES
            .iter()
            .map(|definition| definition.record_type)
            .collect::<BTreeSet<_>>();
        assert_eq!(future_policies, future_catalog);

        let all = record_policies().collect::<Vec<_>>();
        assert_eq!(all.len(), 141);
        assert_eq!(
            all.iter()
                .map(|definition| definition.record_type)
                .collect::<BTreeSet<_>>()
                .len(),
            141
        );
        assert_eq!(
            all.iter()
                .map(|definition| definition.policy_id())
                .collect::<BTreeSet<_>>()
                .len(),
            141
        );
        for definition in all {
            assert_eq!(
                record_type(definition.record_type).unwrap().owner,
                definition.owner
            );
            assert!(definition.version > 0);
            assert!(definition.is_active_at(1_787_001_600));
            for permission in std::iter::once(definition.permission).chain(
                definition
                    .additional_access_rules
                    .iter()
                    .map(|rule| rule.permission),
            ) {
                assert!(!permission.contains('*'));
                assert!(!permission.contains("admin"));
            }
        }
    }

    #[test]
    fn field_service_customer_access_is_exactly_human_or_service_in_the_portal() {
        for record_type in [
            "field-service.agreement",
            "field-service.request",
            "field-service.visit",
        ] {
            let definition = record_policy(record_type).expect("field-service policy");
            assert!(definition.permits_access(
                "field_service.customer.read",
                RecordPolicyPrincipalKind::Human,
                RecordPolicyRealm::CustomerPortal,
            ));
            assert!(definition.permits_access(
                "field_service.customer.read",
                RecordPolicyPrincipalKind::Service,
                RecordPolicyRealm::CustomerPortal,
            ));
            for denied in [
                RecordPolicyPrincipalKind::Agent,
                RecordPolicyPrincipalKind::HumanGroup,
                RecordPolicyPrincipalKind::AgentPool,
            ] {
                assert!(!definition.permits_access(
                    "field_service.customer.read",
                    denied,
                    RecordPolicyRealm::CustomerPortal,
                ));
            }
        }
    }
}
