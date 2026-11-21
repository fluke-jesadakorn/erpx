//! Production Inbox destination resolver.
//!
//! [`retained_inbox_destination`] is the exact `source_type` -> destination
//! table retained from the TypeScript `visibleNotifications` projection in
//! `application/staff-notifications.ts`. The table never invents a link for an
//! unknown source type.
//!
//! [`PgInboxTargets`] proves the destinations owned by this package
//! (`notice`, `collaboration_action`) against the caller's connection. Every
//! other retained source belongs to another workspace owner, so composition
//! must supply a delegate; without one the read fails closed with
//! `INBOX_TARGET_SOURCE_UNSUPPORTED` instead of guessing a URL.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::json;
use sqlx::{PgConnection, Row};

/// How the retained TypeScript adapter established that an item may be shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InboxTargetVisibility {
    /// The destination belongs to the delivered notice itself.
    Delivery,
    /// The destination requires the owning workspace's current read authority.
    Owner,
}

/// One row of the retained TypeScript destination table. `href` is `None` when
/// the owning workspace must resolve the address from its own rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedInboxDestination {
    pub href: Option<String>,
    pub source_kind: String,
    pub label: String,
    pub visibility: InboxTargetVisibility,
}

/// The label table retained from `feature-records/lib/source-record`.
pub fn source_record_label(kind: &str) -> String {
    let label = match kind {
        "staff_task" => "Task",
        "staff_project" => "Project",
        "purchase_request" => "Purchase request",
        "service_request" => "Service request",
        "equipment_request" => "Equipment request",
        "expense" => "Expense",
        "leave" => "Leave",
        "quality_hold" => "Quality hold",
        "quality_alert" => "Quality follow-up",
        "reservation" => "Reservation",
        "stay" => "Stay",
        "guest_account" => "Guest Account",
        "refund" => "Refund",
        "correction" => "Correction",
        "shift" => "Shift",
        "night" => "Night audit",
        "room" => "Room",
        "task" => "Housekeeping task",
        "block" => "Room block",
        "transfer" => "Transfer",
        "pms_record" => "Hotel work",
        "order" => "Order",
        "invoice" => "Invoice",
        "journal" => "Journal",
        "calendar_appointment" => "Appointment",
        "maintenance_plan" => "Preventive maintenance",
        "maintenance_work" => "Maintenance work",
        _ => return capitalize(kind),
    };
    label.to_owned()
}

fn capitalize(kind: &str) -> String {
    let spaced = kind.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// JavaScript `encodeURIComponent` over the UTF-8 bytes, retained so Inbox
/// destinations match the TypeScript adapter byte for byte.
pub fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}

fn destination(
    href: Option<String>,
    source_kind: &str,
    visibility: InboxTargetVisibility,
) -> Option<RetainedInboxDestination> {
    Some(RetainedInboxDestination {
        href,
        source_kind: source_kind.to_owned(),
        label: source_record_label(source_kind),
        visibility,
    })
}

fn owner(href: Option<String>, source_kind: &str) -> Option<RetainedInboxDestination> {
    destination(href, source_kind, InboxTargetVisibility::Owner)
}

/// Resolve the retained destination for one Inbox source.
///
/// `None` means the source type was never part of the TypeScript projection.
/// Owner-resolved rows return `href: None` in the table; the delegated owner
/// must supply the exact address and version.
pub fn retained_inbox_destination(
    source_type: &str,
    source_id: &str,
) -> Option<RetainedInboxDestination> {
    let encoded = || encode_uri_component(source_id);
    match source_type {
        "notice" => destination(None, "notice", InboxTargetVisibility::Delivery),
        "calendar_appointment" => owner(
            Some(format!(
                "/calendar?event=appointment:{}",
                encode_uri_component(source_id)
            )),
            "calendar_appointment",
        ),
        "calendar_removed" => owner(None, "calendar_removed"),
        "handbook" | "handbook_article" => {
            owner(Some(format!("/handbook/{}", encoded())), "handbook_article")
        }
        "invoice" => owner(Some(format!("/finance/invoices/{}", encoded())), "invoice"),
        "journal" => owner(Some(format!("/finance/journals/{}", encoded())), "journal"),
        "organization_change" => owner(None, "organization_change"),
        "company_business_acceptance" => owner(None, "company_business_acceptance"),
        "customer_identity" => owner(
            Some(format!("/customer-identity/{}", encoded())),
            "customer_identity",
        ),
        "customer_duplicate" => owner(
            Some(format!("/customer-duplicates/{}", encoded())),
            "customer_duplicate",
        ),
        "customer_access" => owner(
            Some(format!("/customer-access/{}", encoded())),
            "customer_access",
        ),
        "support_sla_alert" => owner(Some("/support/sla".into()), "support_sla_alert"),
        // The retained adapter does not encode this path segment.
        "support_incident" => owner(
            Some(format!("/support/incidents/{source_id}")),
            "support_incident",
        ),
        "support_policy" => owner(Some("/support/sla".into()), "support_policy"),
        "support_ticket" => owner(Some(format!("/support/{}", encoded())), "support_ticket"),
        "operational_exception" => owner(
            Some(format!("/operations/{}", encoded())),
            "operational_exception",
        ),
        "support_session" => owner(
            Some(format!("/support/sessions/{}", encoded())),
            "support_session",
        ),
        "iam_access_review" => owner(
            Some(format!("/people/settings?section=reviews#{}", encoded())),
            "iam_access_review",
        ),
        "payroll_correction" => owner(None, "payroll_correction"),
        "payroll_advance" => owner(None, "payroll_advance"),
        "staff_task_alert" => owner(
            Some(format!("/staff-tasks/{}", encoded())),
            "staff_task_alert",
        ),
        "maintenance_work" => owner(
            Some(format!("/maintenance/{}", encoded())),
            "maintenance_work",
        ),
        "maintenance_plan" => owner(
            Some(format!("/maintenance/plans/{}", encoded())),
            "maintenance_plan",
        ),
        "project_handoff" => owner(None, "project_handoff"),
        "field_service_order" => owner(
            Some(format!("/field-service/{}", encoded())),
            "field_service_order",
        ),
        "commerce_order" => owner(
            Some(format!("/commerce/orders/{}", encoded())),
            "commerce_order",
        ),
        "collaboration_action" => owner(Some("/collaboration".into()), "collaboration_action"),
        "learning_assignment" => owner(
            Some(format!("/learning/{}", encoded())),
            "learning_assignment",
        ),
        "benefit_election" => owner(Some(format!("/benefits/{}", encoded())), "benefit_election"),
        "talent_application" => owner(Some(format!("/talent/{}", encoded())), "talent_application"),
        "marketing_candidate" => owner(
            Some(format!("/marketing/{}", encoded())),
            "marketing_candidate",
        ),
        "manufacturing_order" => owner(
            Some(format!("/manufacturing/{}", encoded())),
            "manufacturing_order",
        ),
        "crm_lead" => owner(Some(format!("/crm/{}", encoded())), "crm_lead"),
        "crm_activity" => owner(None, "crm_activity"),
        "landed_cost" => owner(None, "landed_cost"),
        "stock_count" => owner(Some(format!("/stock-counts/{}", encoded())), "stock_count"),
        "customer_followup" => owner(
            Some(format!("/customer-followups/{}", encoded())),
            "customer_followup",
        ),
        "supplier_bill" => owner(
            Some(format!("/supplier-bills/{}", encoded())),
            "supplier_bill",
        ),
        "procurement_rfq" => owner(
            Some(format!("/procurement-rfqs/{}", encoded())),
            "procurement_rfq",
        ),
        "supplier_promise" => owner(
            Some(format!("/commerce/orders/{}#supplier-promises", encoded())),
            "supplier_promise",
        ),
        "inventory_transit" => owner(
            Some(format!("/stock-transit/{}", encoded())),
            "inventory_transit",
        ),
        "quality_hold" => owner(Some(format!("/quality/{}", encoded())), "quality_hold"),
        "quality_alert" => owner(None, "quality_hold"),
        "supplier_batch" => owner(
            Some(format!("/finance/supplier-batches/{}", encoded())),
            "supplier_batch",
        ),
        "refund" => owner(Some(format!("/finance/refunds/{}", encoded())), "refund"),
        "legal" => owner(Some(format!("/legal?record={}", encoded())), "legal"),
        "retirement_clearance" => owner(None, "retirement_clearance"),
        "roster_week" => owner(None, "roster_week"),
        "roster_request" => owner(None, "roster_request"),
        "roster_coverage" => owner(None, "roster_coverage"),
        // The retained adapter does not encode these path segments.
        "overtime" => owner(Some(format!("/overtime/{source_id}")), "overtime"),
        "leave" | "leave_cancellation" => {
            owner(Some(format!("/tasks/people_payroll/{source_id}")), "leave")
        }
        "expense" => owner(Some(format!("/expenses/{source_id}")), "expense"),
        "request" => owner(Some(format!("/requests/{source_id}")), "request"),
        "task" => owner(Some(format!("/staff-tasks/{source_id}")), "staff_task"),
        "post" => owner(Some(format!("/posts/{source_id}")), "post"),
        _ => None,
    }
}

/// Sources whose visibility this package can prove against its own rows.
pub fn is_owned_inbox_target(source_type: &str) -> bool {
    matches!(source_type, "notice" | "collaboration_action")
}

fn unsupported() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_SOURCE_UNSUPPORTED",
        "This Inbox source is not part of the retained projection.",
    )
}

fn owner_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_PORT_UNAVAILABLE",
        "This Inbox destination requires its owning workspace.",
    )
}

fn storage() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_QUERY_FAILED",
        "The Inbox destination is unavailable.",
    )
}

/// Production destination resolver for the Inbox projection.
///
/// `branch_id`/`department_id` are the selected Work frame used by the retained
/// collaboration action-card read; the caller must pass the same frame it
/// supplied to [`crate::adapters::inbox_pg::InboxScope`].
pub struct PgInboxTargets {
    assignment_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    delegate: Option<Box<dyn InboxTargetPort>>,
}

impl PgInboxTargets {
    pub fn new(branch_id: Option<&str>, department_id: Option<&str>) -> Self {
        Self {
            assignment_id: None,
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            delegate: None,
        }
    }

    /// Bind the exact selected assignment. Without it the unit resolves the
    /// highest active assignment for the current user and Company.
    pub fn assignment(mut self, assignment_id: Option<&str>) -> Self {
        self.assignment_id = assignment_id
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        self
    }

    /// Compose the collaboration-owned destinations with another owner port.
    pub fn with_delegate(
        branch_id: Option<&str>,
        department_id: Option<&str>,
        delegate: impl InboxTargetPort + 'static,
    ) -> Self {
        Self {
            assignment_id: None,
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            delegate: Some(Box::new(delegate)),
        }
    }

    async fn current_reader(
        c: &mut PgConnection,
        company_id: &str,
        user_id: &str,
        assignment_id: Option<&str>,
    ) -> Result<Option<String>, BusinessError> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT a.role FROM assignments a \
             WHERE a.company_id=$1 AND a.user_id=$2 AND ($3::text IS NULL OR a.id=$3) \
             AND a.revoked_at IS NULL \
             AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
             AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
             AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND a.modules_json::jsonb @> '[\"collaboration\"]'::jsonb \
             AND iam_role_allows(a.role,a.company_id,'records.read') \
             ORDER BY a.version DESC LIMIT 1",
        )
        .bind(company_id)
        .bind(user_id)
        .bind(assignment_id)
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        Ok(role)
    }

    async fn collaboration_action<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company_id: &str,
        user_id: &str,
        source_id: &str,
    ) -> Result<Option<InboxTarget>, BusinessError> {
        if Self::current_reader(c, company_id, user_id, self.assignment_id.as_deref())
            .await?
            .is_none()
        {
            // The retained projection drops a denied card instead of failing
            // the whole Inbox page.
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT c.version FROM collaboration_action_cards c \
             WHERE c.id=$1 AND c.company_id=$2 AND c.source_type<>'field_service_order' \
             AND ($3::text IS NULL OR c.branch_id=$3) \
             AND ($4::text IS NULL OR c.department_id=$4)",
        )
        .bind(source_id)
        .bind(company_id)
        .bind(self.branch_id.as_deref())
        .bind(self.department_id.as_deref())
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?;
        let Some(row) = row else {
            return Ok(None);
        };
        let version: i64 = row.try_get(0).map_err(|_| storage())?;
        let href = "/collaboration";
        Ok(Some(InboxTarget {
            href: Some(href.into()),
            source: Some(json!({
                "kind": "collaboration_action",
                "id": source_id,
                "version": version,
                "href": href,
                "label": source_record_label("collaboration_action"),
            })),
        }))
    }
}

impl InboxTargetPort for PgInboxTargets {
    fn target<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        company_id: &'a str,
        user_id: &'a str,
        source_type: &'a str,
        source_id: &'a str,
        _source_version: i64,
    ) -> InboxTargetFuture<'a> {
        Box::pin(async move {
            match source_type {
                "notice" => {
                    let exists: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM inbox_notices WHERE id=$1 AND company_id=$2)",
                    )
                    .bind(source_id)
                    .bind(company_id)
                    .fetch_one(&mut *connection)
                    .await
                    .map_err(|_| storage())?;
                    Ok(exists.then(InboxTarget::default))
                }
                "collaboration_action" => {
                    self.collaboration_action(connection, company_id, user_id, source_id)
                        .await
                }
                _ => {
                    if let Some(delegate) = self.delegate.as_deref_mut() {
                        delegate
                            .target(
                                connection,
                                company_id,
                                user_id,
                                source_type,
                                source_id,
                                _source_version,
                            )
                            .await
                    } else if retained_inbox_destination(source_type, source_id).is_some() {
                        Err(owner_unavailable())
                    } else {
                        Err(unsupported())
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_matches_javascript_encode_uri_component() {
        assert_eq!(encode_uri_component("a b/c?d=e&f"), "a%20b%2Fc%3Fd%3De%26f");
        assert_eq!(encode_uri_component("A-z_0.9!~*'()"), "A-z_0.9!~*'()");
        assert_eq!(encode_uri_component("😀"), "%F0%9F%98%80");
    }

    #[test]
    fn labels_follow_the_retained_source_record_table() {
        assert_eq!(source_record_label("staff_task"), "Task");
        assert_eq!(source_record_label("quality_hold"), "Quality hold");
        assert_eq!(
            source_record_label("collaboration_action"),
            "Collaboration action"
        );
        assert_eq!(source_record_label("request"), "Request");
    }

    #[test]
    fn owned_sources_are_explicit() {
        assert!(is_owned_inbox_target("notice"));
        assert!(is_owned_inbox_target("collaboration_action"));
        assert!(!is_owned_inbox_target("support_ticket"));
        assert!(!is_owned_inbox_target("post"));
    }
}
