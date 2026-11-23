//! Collaboration-owned Support notification projections.
//!
//! Collaboration owns `staff_notifications`. The eleven effects below are the
//! native, closed projection of the retired TypeScript Support notification
//! effect shims (`support-notification-effects.ts` and the five
//! `support-*-notification-sources.ts` modules deleted during retirement).
//! They carry the exact record type, message, id template, recipient shape and
//! conflict semantics of those sources so the native owner can emit every
//! Support notice without a TypeScript writer or oracle.
//!
//! The projection SELECTs are kept verbatim (with named fragments for the
//! cross-owner Identity/HR predicates that the caller supplies from its own
//! closed witnesses). A caller executes the returned statement in its existing
//! serializable unit; this module never opens a transaction or grants Support
//! authority.

use erpx_server_shared::public::{BusinessError, FailureKind};

const INSERT_PREFIX: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at)";
const ON_CONFLICT: &str = " ON CONFLICT DO NOTHING";

/// Closed cross-owner fragments. The caller supplies them from its own
/// authoritative projections; request input is never interpolated here.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SupportSourceFragments {
    /// Identity `assignments` projection SQL.
    pub assignments: String,
    /// Support recipient eligibility predicate (references alias `t`).
    pub eligibility: String,
    /// Scheduled-feature predicate (references the company alias).
    pub scheduled_feature: String,
    /// Optional extra purchased predicate for the SLA breach effect.
    pub purchased: String,
}

impl SupportSourceFragments {
    fn resolve(&self, source: &'static str) -> String {
        source
            .replace("{assignments}", &self.assignments)
            .replace("{eligibility}", &self.eligibility)
            .replace("{scheduled}", &self.scheduled_feature)
            .replace("{purchased}", &self.purchased)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SupportNotificationEffect {
    IncidentUpdated,
    TicketReopened,
    TicketCreated,
    TicketUpdated,
    TicketReviewers,
    SlaBreached,
    SessionReviewers,
    SessionUpdated,
    PolicyReviewers,
    PolicyUpdated,
    TicketClockAttached,
}

impl SupportNotificationEffect {
    pub const ALL: [Self; 11] = [
        Self::IncidentUpdated,
        Self::TicketReopened,
        Self::TicketCreated,
        Self::TicketUpdated,
        Self::TicketReviewers,
        Self::SlaBreached,
        Self::SessionReviewers,
        Self::SessionUpdated,
        Self::PolicyReviewers,
        Self::PolicyUpdated,
        Self::TicketClockAttached,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::IncidentUpdated => "incident_updated",
            Self::TicketReopened => "ticket_reopened",
            Self::TicketCreated => "ticket_created",
            Self::TicketUpdated => "ticket_updated",
            Self::TicketReviewers => "ticket_reviewers",
            Self::SlaBreached => "sla_breached",
            Self::SessionReviewers => "session_reviewers",
            Self::SessionUpdated => "session_updated",
            Self::PolicyReviewers => "policy_reviewers",
            Self::PolicyUpdated => "policy_updated",
            Self::TicketClockAttached => "ticket_clock_attached",
        }
    }

    pub fn from_str(value: &str) -> Result<Self, BusinessError> {
        Ok(match value {
            "incident_updated" => Self::IncidentUpdated,
            "ticket_reopened" => Self::TicketReopened,
            "ticket_created" => Self::TicketCreated,
            "ticket_updated" => Self::TicketUpdated,
            "ticket_reviewers" => Self::TicketReviewers,
            "sla_breached" => Self::SlaBreached,
            "session_reviewers" => Self::SessionReviewers,
            "session_updated" => Self::SessionUpdated,
            "policy_reviewers" => Self::PolicyReviewers,
            "policy_updated" => Self::PolicyUpdated,
            "ticket_clock_attached" => Self::TicketClockAttached,
            _ => return Err(invalid("SUPPORT_NOTIFICATION_EFFECT_UNKNOWN")),
        })
    }

    pub fn record_type(self) -> &'static str {
        match self {
            Self::IncidentUpdated => "support_incident",
            Self::TicketReopened
            | Self::TicketCreated
            | Self::TicketUpdated
            | Self::TicketReviewers
            | Self::TicketClockAttached => "support_ticket",
            Self::SlaBreached => "support_sla_alert",
            Self::SessionReviewers | Self::SessionUpdated => "support_session",
            Self::PolicyReviewers | Self::PolicyUpdated => "support_policy",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::IncidentUpdated => {
                "Incident updated: review impact and follow up each linked customer case"
            }
            Self::TicketReopened => "New issue needs Support triage",
            Self::TicketCreated => "New customer support case: accept and review the issue",
            Self::TicketUpdated => "Support case updated: review current status and next action",
            Self::TicketReviewers => "Review proposed issue resolution",
            Self::SlaBreached => "Resolution target breached: take responsibility for follow-up",
            Self::SessionReviewers => "Review a temporary customer support session",
            Self::SessionUpdated => "Temporary support access changed: review current grants",
            Self::PolicyReviewers => "Review a proposed working-time resolution policy",
            Self::PolicyUpdated => "SLA policy reviewed: check its current availability",
            Self::TicketClockAttached => {
                "Approved working-time target attached: review the deadline"
            }
        }
    }

    /// Retained shims used `ON CONFLICT DO NOTHING` only for the multi-recipient
    /// (non-unique id) effects. Exact parity requires preserving that split.
    pub fn conflict_safe(self) -> bool {
        matches!(
            self,
            Self::IncidentUpdated
                | Self::TicketReopened
                | Self::TicketReviewers
                | Self::SlaBreached
                | Self::SessionReviewers
                | Self::PolicyReviewers
        )
    }

    /// `true` when the source derives one row per recipient and prefixes the
    /// id with the command key; `false` when it derives exactly one row whose
    /// whole id is the bound key.
    pub fn multi_recipient(self) -> bool {
        matches!(
            self,
            Self::IncidentUpdated
                | Self::TicketReopened
                | Self::TicketReviewers
                | Self::SlaBreached
                | Self::SessionReviewers
                | Self::PolicyReviewers
        )
    }

    /// The closed source projection SELECT with named cross-owner fragments.
    /// `?` placeholders are, in order: command key, record id and (for the
    /// breach effect) the detected-at timestamp.
    pub fn source(self) -> &'static str {
        match self {
            Self::TicketReopened => " SELECT DISTINCT ?||m.user_id,t.company_id,m.user_id,'support_ticket',t.id,t.version,'New issue needs Support triage',t.updated_at FROM support_tickets t JOIN ({assignments}) m ON m.company_id=t.company_id WHERE t.id=? AND m.user_id<>t.created_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND {eligibility}",
            Self::TicketCreated => " SELECT ?,company_id,owner_id,'support_ticket',id,version,'New customer support case: accept and review the issue',updated_at FROM support_tickets WHERE id=?",
            Self::TicketUpdated => " SELECT ?,company_id,owner_id,'support_ticket',id,version,'Support case updated: review current status and next action',updated_at FROM support_tickets WHERE id=? AND owner_id IS NOT NULL",
            Self::TicketReviewers => " SELECT DISTINCT ?||m.user_id,t.company_id,m.user_id,'support_ticket',t.id,t.version,'Review proposed issue resolution',t.updated_at FROM support_tickets t JOIN ({assignments}) m ON m.company_id=t.company_id WHERE t.id=? AND m.user_id<>t.owner_id AND m.user_id<>t.resolved_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND {eligibility}",
            Self::IncidentUpdated => " SELECT DISTINCT ?||u.user_id,t.company_id,u.user_id,'support_incident',t.id,t.version,'Incident updated: review impact and follow up each linked customer case',t.updated_at FROM support_incidents t JOIN LATERAL(SELECT t.owner_id user_id UNION SELECT s.owner_id FROM support_incident_tickets l JOIN support_tickets s ON s.id=l.ticket_id WHERE l.incident_id=t.id AND l.active UNION SELECT m.user_id FROM ({assignments}) m WHERE t.status='mitigated' AND m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND {eligibility}) u ON true WHERE t.id=?",
            Self::SlaBreached => " SELECT DISTINCT s.id||':'||m.user_id,t.company_id,m.user_id,'support_sla_alert',s.id,s.version,'Resolution target breached: take responsibility for follow-up',?::bigint FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id JOIN ({assignments}) m ON m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND {eligibility} WHERE s.acknowledged_by IS NULL AND {scheduled} AND {purchased}",
            Self::SessionReviewers => " SELECT DISTINCT ?||m.user_id,s.company_id,m.user_id,'support_session',s.id,s.version,'Review a temporary customer support session',s.updated_at FROM support_sessions s JOIN support_tickets t ON t.id=s.ticket_id JOIN ({assignments}) m ON m.company_id=s.company_id AND m.user_id<>s.created_by AND m.user_id<>s.requested_for AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) WHERE s.id=?",
            Self::SessionUpdated => " SELECT ?,company_id,requested_for,'support_session',id,version,'Temporary support access changed: review current grants',updated_at FROM support_sessions WHERE id=?",
            Self::PolicyReviewers => " SELECT DISTINCT ?||m.user_id,p.company_id,m.user_id,'support_policy',p.id,p.version,'Review a proposed working-time resolution policy',p.updated_at FROM support_sla_policies p JOIN ({assignments}) m ON m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND {eligibility} WHERE p.id=?",
            Self::PolicyUpdated => " SELECT ?,company_id,created_by,'support_policy',id,version,'SLA policy reviewed: check its current availability',updated_at FROM support_sla_policies WHERE id=?",
            Self::TicketClockAttached => " SELECT ?,company_id,owner_id,'support_ticket',id,version,'Approved working-time target attached: review the deadline',updated_at FROM support_tickets WHERE id=?",
        }
    }

    /// Number of bind placeholders in the closed source projection.
    pub fn bind_count(self) -> usize {
        match self {
            Self::SlaBreached => 1,
            _ => 2,
        }
    }

    /// Final INSERT statement: the closed projection over the caller's
    /// cross-owner fragments, with the retained conflict semantics.
    pub fn statement(
        self,
        fragments: &SupportSourceFragments,
    ) -> Result<NotificationStatement, BusinessError> {
        if self == Self::SlaBreached
            && (fragments.assignments.is_empty()
                || fragments.eligibility.is_empty()
                || fragments.scheduled_feature.is_empty()
                || fragments.purchased.is_empty())
        {
            return Err(invalid("SUPPORT_NOTIFICATION_FRAGMENTS_REQUIRED"));
        }
        if self.multi_recipient()
            && self != Self::SlaBreached
            && (fragments.assignments.is_empty() || fragments.eligibility.is_empty())
        {
            return Err(invalid("SUPPORT_NOTIFICATION_FRAGMENTS_REQUIRED"));
        }
        let source = fragments.resolve(self.source());
        let sql = format!(
            "{}{}{}",
            INSERT_PREFIX,
            source,
            if self.conflict_safe() { ON_CONFLICT } else { "" }
        );
        Ok(NotificationStatement {
            sql,
            values: Vec::new(),
        })
    }

    /// A single expected notification row for an already-resolved recipient.
    /// This is the native-seeded oracle: given authoritative facts, it derives
    /// the exact row the closed INSERT must produce (or no-op on conflict).
    pub fn derive(
        self,
        facts: &SupportNotificationFacts,
        recipient: &str,
    ) -> Result<SupportNotificationRow, BusinessError> {
        if facts.company_id.is_empty()
            || facts.record_id.is_empty()
            || facts.command_key.is_empty()
            || recipient.is_empty()
            || facts.version < 1
            || facts.updated_at <= 0
        {
            return Err(invalid("SUPPORT_NOTIFICATION_FACTS_INVALID"));
        }
        let id = match self {
            Self::SlaBreached => format!("{}:{}", facts.record_id, recipient),
            _ if self.multi_recipient() => format!("{}{}", facts.command_key, recipient),
            _ => facts.command_key.clone(),
        };
        Ok(SupportNotificationRow {
            id,
            company_id: facts.company_id.clone(),
            user_id: recipient.to_owned(),
            record_type: self.record_type().to_owned(),
            record_id: facts.record_id.clone(),
            record_version: facts.version,
            message: self.message().to_owned(),
            created_at: facts.updated_at,
            read_at: None,
            conflict_safe: self.conflict_safe(),
        })
    }
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, "Review the Support notification request.")
}

/// Facts for a closed Support notification effect. Every field comes from the
/// owner's current source after-image and command key; no browser proof is
/// accepted by this module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportNotificationFacts {
    pub command_key: String,
    pub company_id: String,
    pub record_id: String,
    pub version: i64,
    pub updated_at: i64,
}

/// Exact notification row produced by a closed effect for one recipient.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportNotificationRow {
    pub id: String,
    pub company_id: String,
    pub user_id: String,
    pub record_type: String,
    pub record_id: String,
    pub record_version: i64,
    pub message: String,
    pub created_at: i64,
    pub read_at: Option<i64>,
    conflict_safe: bool,
}

impl SupportNotificationRow {
    pub fn conflict_safe(&self) -> bool {
        self.conflict_safe
    }
    pub fn event_id(&self) -> String {
        format!("legacy:{}", self.id)
    }
    /// JSON image asserted by the native-seeded oracle.
    pub fn image(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "company_id": self.company_id,
            "user_id": self.user_id,
            "record_type": self.record_type,
            "record_id": self.record_id,
            "record_version": self.record_version,
            "message": self.message,
            "created_at": self.created_at,
            "read_at": self.read_at,
        })
    }
}

/// Owner-owned statement descriptor, matching the existing notification
/// statement transport used by composition and retained TS callers.
#[derive(Clone, Debug, Serialize)]
pub struct NotificationStatement {
    pub sql: String,
    pub values: Vec<serde_json::Value>,
}

use serde::Serialize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_notification_all_eleven_effects_have_closed_contract() {
        assert_eq!(SupportNotificationEffect::ALL.len(), 11);
        for effect in SupportNotificationEffect::ALL {
            assert!(!effect.as_str().is_empty());
            assert!(!effect.record_type().is_empty());
            assert!(!effect.message().is_empty());
            assert_eq!(SupportNotificationEffect::from_str(effect.as_str()), Ok(effect));
            assert_eq!(effect.source().contains(effect.message()), true);
            assert_eq!(effect.source().contains(&format!("'{}'", effect.record_type())), true);
        }
        let unique: std::collections::BTreeSet<_> =
            SupportNotificationEffect::ALL.iter().map(|e| e.as_str()).collect();
        assert_eq!(unique.len(), 11);
    }

    #[test]
    fn support_notification_conflict_and_bind_parity() {
        let safe: Vec<_> = SupportNotificationEffect::ALL
            .iter()
            .filter(|e| e.conflict_safe())
            .copied()
            .collect();
        assert_eq!(
            safe,
            vec![
                SupportNotificationEffect::IncidentUpdated,
                SupportNotificationEffect::TicketReopened,
                SupportNotificationEffect::TicketReviewers,
                SupportNotificationEffect::SlaBreached,
                SupportNotificationEffect::SessionReviewers,
                SupportNotificationEffect::PolicyReviewers,
            ]
        );
        assert_eq!(SupportNotificationEffect::SlaBreached.bind_count(), 1);
        for e in SupportNotificationEffect::ALL {
            if e != SupportNotificationEffect::SlaBreached {
                assert_eq!(e.bind_count(), 2);
            }
        }
    }

    #[test]
    fn support_notification_single_and_multi_recipient_ids() {
        let facts = SupportNotificationFacts {
            command_key: "ntf-123".into(),
            company_id: "c1".into(),
            record_id: "ticket-9".into(),
            version: 4,
            updated_at: 1_800_000_000_007,
        };
        let single = SupportNotificationEffect::TicketCreated
            .derive(&facts, "owner")
            .unwrap();
        assert_eq!(single.id, "ntf-123");
        assert_eq!(single.user_id, "owner");
        assert_eq!(single.record_type, "support_ticket");
        assert_eq!(single.message, "New customer support case: accept and review the issue");
        assert!(!single.conflict_safe());
        assert_eq!(single.event_id(), "legacy:ntf-123");

        let multi = SupportNotificationEffect::TicketReviewers
            .derive(&facts, "manager")
            .unwrap();
        assert_eq!(multi.id, "ntf-123manager");
        assert!(multi.conflict_safe());

        let breach = SupportNotificationEffect::SlaBreached
            .derive(&facts, "manager")
            .unwrap();
        assert_eq!(breach.id, "ticket-9:manager");
        assert_eq!(breach.record_type, "support_sla_alert");
    }

    #[test]
    fn support_notification_facts_fail_closed() {
        let facts = SupportNotificationFacts {
            command_key: "".into(),
            company_id: "c1".into(),
            record_id: "r".into(),
            version: 1,
            updated_at: 1,
        };
        assert!(SupportNotificationEffect::TicketCreated.derive(&facts, "u").is_err());
        let facts = SupportNotificationFacts { command_key: "k".into(), ..facts };
        assert!(SupportNotificationEffect::TicketCreated.derive(&facts, "").is_err());
        assert!(SupportNotificationEffect::from_str("unknown").is_err());
    }

    #[test]
    fn support_notification_statement_preserves_source_and_conflict() {
        let fragments = SupportSourceFragments {
            assignments: "(assignments)".into(),
            eligibility: "EXISTS(eligible)".into(),
            scheduled_feature: "scheduled".into(),
            purchased: "TRUE".into(),
        };
        let created = SupportNotificationEffect::TicketCreated
            .statement(&fragments)
            .unwrap();
        assert!(created.sql.starts_with(INSERT_PREFIX));
        assert!(created.sql.contains("New customer support case"));
        assert!(!created.sql.contains(ON_CONFLICT));

        let reopened = SupportNotificationEffect::TicketReopened
            .statement(&fragments)
            .unwrap();
        assert!(reopened.sql.contains(ON_CONFLICT));
        assert!(reopened.sql.contains("(assignments)"));
        assert!(reopened.sql.contains("EXISTS(eligible)"));
    }

    #[test]
    fn support_notification_breach_requires_closed_fragments() {
        let empty = SupportSourceFragments::default();
        assert!(SupportNotificationEffect::SlaBreached.statement(&empty).is_err());
        let full = SupportSourceFragments {
            assignments: "a".into(),
            eligibility: "e".into(),
            scheduled_feature: "scheduled".into(),
            purchased: "TRUE".into(),
        };
        let plan = SupportNotificationEffect::SlaBreached.statement(&full).unwrap();
        assert!(plan.sql.contains("?::bigint"));
        assert!(plan.sql.contains("scheduled"));
        assert!(plan.sql.contains("TRUE"));
        assert!(plan.sql.contains(ON_CONFLICT));
    }

    include!("../../../../tests/unit/collaboration/rust-support-notifications-oracle.rs");
}
