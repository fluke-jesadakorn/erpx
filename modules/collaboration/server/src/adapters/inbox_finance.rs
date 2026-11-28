//! Native Finance visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained per-row TypeScript read gates for the
//! notification-visible Finance sources:
//! - `invoice`: `getInvoice` in
//!   `modules/finance/server/src/adapters/finance-invoice-read.ts`
//!   (`requireFinanceDocumentHistory` for `invoice` plus the company-scoped
//!   invoice identity read, 404 `INVOICE_NOT_FOUND`).
//! - `journal`: `journalDetail` in
//!   `modules/finance/server/src/adapters/ledger.ts` (the ordinary,
//!   non-initial-stock path: `requireFinanceDocumentHistory` for `journal`
//!   plus the company-scoped journal identity read, 404 `JOURNAL_NOT_FOUND`).
//! - `refund`: `getRefund` in
//!   `modules/finance/server/src/adapters/refunds.ts`
//!   (`withRefundPurchasedRead` for an obligation — `requireFinanceDocumentHistory`
//!   for `refund` — plus the frame-scoped obligation read, 404
//!   `REFUND_NOT_FOUND`).
//!
//! Denials surface with the exact TypeScript codes so composition can drop
//! those rows exactly like the retained per-row 403/404 catch. A refund
//! carries no source override, so the composition final map applies the
//! retained default (`{kind:"refund", id, href, label:"Refund"}`).
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
//!
//! Faithful-mirror notes (documented, not silent):
//! - The command-side guards (`workContextGuards`, `iamMutationGuards`,
//!   assertion clearing) authorize commands, not reads, and are not part of
//!   this read-only port; the JWT-static `requirePermission` half is enforced
//!   database-current via `iam_role_allows` on the reader's assignment.
//! - Only the base-capability purchase proof is mirrored
//!   (`finance.accounting` for invoice/journal, `finance.banking` for
//!   refund). The included-foundation fallback with cross-owner commerce /
//!   inventory / POS / PMS / payroll proofs, the additional-capabilities
//!   check, and the standalone service-invoice receipt proof belong to other
//!   owners; where the oracle would grant through them, this delegate fails
//!   closed instead of duplicating divergent logic.
//! - Initial physical-count journals (service / POS / manufacturing / sales
//!   markers) take entirely different retained branches with their own
//!   capabilities and 404s; this delegate proves every journal on the
//!   ordinary path. A marked journal may resolve here while the oracle
//!   hides it (or the reverse when only a foundation is purchased).
//! - Invoice and journal identity reads are company-scoped only, exactly
//!   like the retained reads; the selected Work frame constrains the refund
//!   obligation and the assignment branch check, never the invoice/journal.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{
    encode_uri_component, retained_inbox_destination, source_record_label,
};
use erpx_commercial_server::public::capability_decision::PgCapabilityDecisionReader;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::json;
use sqlx::{PgConnection, Row};

/// Source types this delegate can prove against Finance rows.
pub fn is_finance_inbox_target(source_type: &str) -> bool {
    matches!(source_type, "invoice" | "journal" | "refund")
}

/// Retained destination address for one invoice, byte-identical to the
/// TypeScript `` `/finance/invoices/${encodeURIComponent(id)}` ``.
pub fn finance_invoice_href(source_id: &str) -> String {
    format!("/finance/invoices/{}", encode_uri_component(source_id))
}

/// Retained destination address for one journal, byte-identical to the
/// TypeScript `` `/finance/journals/${encodeURIComponent(id)}` ``.
pub fn finance_journal_href(source_id: &str) -> String {
    format!("/finance/journals/{}", encode_uri_component(source_id))
}

/// Retained destination address for one refund, byte-identical to the
/// TypeScript `` `/finance/refunds/${encodeURIComponent(id)}` ``.
pub fn finance_refund_href(source_id: &str) -> String {
    format!("/finance/refunds/{}", encode_uri_component(source_id))
}

/// Retained base capability per document kind: `finance.accounting` backs
/// invoices and journals, `finance.banking` backs refunds.
pub fn finance_document_capability(source_type: &str) -> Option<&'static str> {
    match source_type {
        "invoice" | "journal" => Some("finance.accounting"),
        "refund" => Some("finance.banking"),
        _ => None,
    }
}

fn invoice_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "INVOICE_NOT_FOUND",
        "Invoice not found in this company.",
    )
}

fn journal_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "JOURNAL_NOT_FOUND",
        "Journal not found.",
    )
}

fn refund_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "REFUND_NOT_FOUND",
        "Refund obligation not found.",
    )
}

fn finance_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_ACCESS_CHANGED",
        "Choose a current Finance assignment.",
    )
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

fn pivot(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => storage(),
    }
}

/// Native Finance visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Finance-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
///
/// `branch_id`/`department_id` carry the selected Work frame used by the
/// retained refund scope and the assignment branch check; invoice and journal
/// identity reads stay company-scoped exactly like the retained reads.
pub struct FinanceInboxTargets {
    branch_id: Option<String>,
    department_id: Option<String>,
    assignment_id: Option<String>,
    enforcement: Option<PackageEnforcementMode>,
}

impl FinanceInboxTargets {
    pub fn new(branch_id: Option<&str>, department_id: Option<&str>) -> Self {
        Self {
            branch_id: branch_id.map(str::to_owned),
            department_id: department_id.map(str::to_owned),
            assignment_id: None,
            enforcement: None,
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

    /// Pin the package-enforcement mode. Without it the delegate mirrors the
    /// retained finance environment-driven mode exactly. This seam exists so
    /// tests and composition can select a mode deterministically; it never
    /// changes what the mode means.
    pub fn enforcement_mode(mut self, mode: PackageEnforcementMode) -> Self {
        self.enforcement = Some(mode);
        self
    }
}

impl Default for FinanceInboxTargets {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// Retained `financePackageEnforcementMode`, mirrored from the TypeScript
/// oracle: disabled opens the read, a non-local environment fails with the
/// environment code, and an unknown value fails closed with the
/// configuration code.
fn enforcement_mode() -> Result<PackageEnforcementMode, BusinessError> {
    match std::env::var("ERPX_FINANCE_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned())
        .as_str()
    {
        "disabled" => Ok(PackageEnforcementMode::Disabled),
        "shadow" => Ok(PackageEnforcementMode::Shadow),
        "enforced" => {
            if std::env::var("ERPX_ENVIRONMENT").ok().as_deref() != Some("local") {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "PACKAGE_ENFORCEMENT_ENVIRONMENT",
                    "Package enforcement is unavailable in this environment.",
                ));
            }
            Ok(PackageEnforcementMode::Enforced)
        }
        _ => Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_CONFIGURATION",
            "Package enforcement is unavailable.",
        )),
    }
}

/// Retained `requireFinanceCurrentAuthority` database half: the reader's
/// assignment must be current over the selected frame, with the JWT-static
/// `finance.read` permission enforced database-current via `iam_role_allows`.
async fn current_authority(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    assignment_id: Option<&str>,
    branch_id: Option<&str>,
    now: i64,
) -> Result<(), BusinessError> {
    let authorized: Option<String> = sqlx::query_scalar(
        "SELECT g.id FROM assignments g \
          WHERE g.company_id=$1 AND g.user_id=$2 AND ($3::text IS NULL OR g.id=$3) \
          AND g.valid_from<=$4 AND g.valid_until>$4 AND g.revoked_at IS NULL \
          AND ($5::text IS NULL OR g.branch_id IS NULL OR g.branch_id=$5) \
          AND iam_assignment_active(g.id,$4) \
          AND iam_principal_active(g.user_id,g.company_id,$4) \
          AND iam_role_allows(g.role,g.company_id,'finance.read') \
          ORDER BY g.version DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(assignment_id)
    .bind(now)
    .bind(branch_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    if authorized.is_none() {
        return Err(finance_access_changed());
    }
    Ok(())
}

/// Retained purchase-history gate for the document base capability through
/// the native Commercial capability port. The refusal carries the exact
/// decision code and message from the TypeScript throw.
async fn purchase_history(
    c: &mut PgConnection,
    company_id: &CompanyId,
    capability: &str,
    mode: PackageEnforcementMode,
    now: i64,
) -> Result<(), BusinessError> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(());
    }
    let catalogue = current_package_catalogue()?;
    let mut reader = PgCapabilityDecisionReader::new(company_id.clone(), c, &catalogue);
    reader
        .require(
            capability,
            CapabilityPurpose::History,
            mode,
            TimestampMillis::new(now),
        )
        .await
        .map_err(pivot)?;
    Ok(())
}

/// Retained invoice identity read: company-scoped with the party join, plus
/// the numeric version the projection surfaces.
async fn invoice_identity(
    c: &mut PgConnection,
    company_id: &str,
    source_id: &str,
) -> Result<Option<(String, i64)>, BusinessError> {
    sqlx::query(
        "SELECT i.id,i.version FROM finance_invoices i \
          JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id \
          WHERE i.id=$1 AND i.company_id=$2",
    )
    .bind(source_id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?
    .map(|row| {
        let id: String = row.try_get("id").map_err(|_| storage())?;
        let version: i64 = row.try_get("version").map_err(|_| storage())?;
        Ok((id, version))
    })
    .transpose()
}

/// Retained journal identity read: company-scoped, plus the numeric version
/// the projection surfaces.
async fn journal_identity(
    c: &mut PgConnection,
    company_id: &str,
    source_id: &str,
) -> Result<Option<(String, i64)>, BusinessError> {
    sqlx::query("SELECT id,version FROM finance_journals WHERE id=$1 AND company_id=$2")
        .bind(source_id)
        .bind(company_id)
        .fetch_optional(&mut *c)
        .await
        .map_err(|_| storage())?
        .map(|row| {
            let id: String = row.try_get("id").map_err(|_| storage())?;
            let version: i64 = row.try_get("version").map_err(|_| storage())?;
            Ok((id, version))
        })
        .transpose()
}

/// Retained refund obligation identity read: company- plus Work-frame-scoped
/// with the party join, exactly like the retained obligation scope.
async fn refund_identity(
    c: &mut PgConnection,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    source_id: &str,
) -> Result<Option<String>, BusinessError> {
    sqlx::query_scalar(
        "SELECT o.id FROM finance_refund_obligations o \
          JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id \
          WHERE o.id=$1 AND o.company_id=$2 \
          AND ($3::text IS NULL OR o.branch_id=$3) \
          AND ($4::text IS NULL OR o.department_id=$4)",
    )
    .bind(source_id)
    .bind(company_id)
    .bind(branch_id)
    .bind(department_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())
}

impl InboxTargetPort for FinanceInboxTargets {
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
            let company = CompanyId::new(company_id).map_err(|_| storage())?;
            let mode = match self.enforcement {
                Some(mode) => mode,
                None => enforcement_mode()?,
            };
            let now: i64 = sqlx::query_scalar(
                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            )
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| storage())?;
            match source_type {
                "invoice" | "journal" | "refund" => {
                    // The retained document-history gate runs authority before
                    // purchase, then the identity read proves the row.
                    current_authority(
                        connection,
                        company.as_str(),
                        user_id,
                        self.assignment_id.as_deref(),
                        self.branch_id.as_deref(),
                        now,
                    )
                    .await?;
                    match source_type {
                        "invoice" => {
                            purchase_history(
                                connection,
                                &company,
                                "finance.accounting",
                                mode,
                                now,
                            )
                            .await?;
                            let Some((id, version)) =
                                invoice_identity(connection, company.as_str(), source_id).await?
                            else {
                                return Err(invoice_not_found());
                            };
                            let href = finance_invoice_href(source_id);
                            Ok(Some(InboxTarget {
                                href: Some(href.clone()),
                                source: Some(json!({
                                    "kind": "invoice",
                                    "id": id,
                                    "version": version,
                                    "href": href,
                                    "label": source_record_label("invoice"),
                                })),
                            }))
                        }
                        "journal" => {
                            purchase_history(
                                connection,
                                &company,
                                "finance.accounting",
                                mode,
                                now,
                            )
                            .await?;
                            let Some((id, version)) =
                                journal_identity(connection, company.as_str(), source_id).await?
                            else {
                                return Err(journal_not_found());
                            };
                            let href = finance_journal_href(source_id);
                            Ok(Some(InboxTarget {
                                href: Some(href.clone()),
                                source: Some(json!({
                                    "kind": "journal",
                                    "id": id,
                                    "version": version,
                                    "href": href,
                                    "label": source_record_label("journal"),
                                })),
                            }))
                        }
                        _ => {
                            purchase_history(
                                connection,
                                &company,
                                "finance.banking",
                                mode,
                                now,
                            )
                            .await?;
                            refund_identity(
                                connection,
                                company.as_str(),
                                self.branch_id.as_deref(),
                                self.department_id.as_deref(),
                                source_id,
                            )
                            .await?
                            .ok_or_else(refund_not_found)?;
                            // No source override: the composition final map
                            // applies the retained refund default.
                            let href = finance_refund_href(source_id);
                            Ok(Some(InboxTarget {
                                href: Some(href),
                                source: None,
                            }))
                        }
                    }
                }
                _ if retained_inbox_destination(source_type, source_id).is_some() => {
                    Err(owner_unavailable())
                }
                _ => Err(unsupported()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_covers_only_the_finance_slice() {
        assert!(is_finance_inbox_target("invoice"));
        assert!(is_finance_inbox_target("journal"));
        assert!(is_finance_inbox_target("refund"));
        for other in [
            "notice",
            "collaboration_action",
            "calendar_appointment",
            "crm_lead",
            "crm_activity",
            "commerce_order",
            "supplier_batch",
            "support_ticket",
            "finance",
            "future_source",
            "",
        ] {
            assert!(!is_finance_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn hrefs_encode_exactly_like_the_typescript_projection() {
        assert_eq!(finance_invoice_href("inv-1"), "/finance/invoices/inv-1");
        assert_eq!(
            finance_invoice_href("seg/one two"),
            "/finance/invoices/seg%2Fone%20two"
        );
        assert_eq!(finance_journal_href("jou-1"), "/finance/journals/jou-1");
        assert_eq!(
            finance_journal_href("seg/one two"),
            "/finance/journals/seg%2Fone%20two"
        );
        assert_eq!(finance_refund_href("ref-1"), "/finance/refunds/ref-1");
        assert_eq!(
            finance_refund_href("seg/one two"),
            "/finance/refunds/seg%2Fone%20two"
        );
        assert_eq!(finance_journal_href("😀"), "/finance/journals/%F0%9F%98%80");
    }

    #[test]
    fn document_capability_table_matches_the_retained_bases() {
        assert_eq!(
            finance_document_capability("invoice"),
            Some("finance.accounting")
        );
        assert_eq!(
            finance_document_capability("journal"),
            Some("finance.accounting")
        );
        assert_eq!(
            finance_document_capability("refund"),
            Some("finance.banking")
        );
        assert_eq!(finance_document_capability("supplier_batch"), None);
        assert_eq!(finance_document_capability("payment"), None);
        assert_eq!(finance_document_capability(""), None);
    }

    #[test]
    fn finance_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("invoice"), "Invoice");
        assert_eq!(source_record_label("journal"), "Journal");
        assert_eq!(source_record_label("refund"), "Refund");
    }

    #[test]
    fn oracle_denials_keep_their_exact_codes_kinds_and_messages() {
        let missing = invoice_not_found();
        assert_eq!(missing.kind, FailureKind::NotFound);
        assert_eq!(missing.code, "INVOICE_NOT_FOUND");
        assert_eq!(missing.message, "Invoice not found in this company.");
        let journal = journal_not_found();
        assert_eq!(journal.kind, FailureKind::NotFound);
        assert_eq!(journal.code, "JOURNAL_NOT_FOUND");
        assert_eq!(journal.message, "Journal not found.");
        let refund = refund_not_found();
        assert_eq!(refund.kind, FailureKind::NotFound);
        assert_eq!(refund.code, "REFUND_NOT_FOUND");
        assert_eq!(refund.message, "Refund obligation not found.");
        let changed = finance_access_changed();
        assert_eq!(changed.kind, FailureKind::Forbidden);
        assert_eq!(changed.code, "FINANCE_ACCESS_CHANGED");
        assert_eq!(changed.message, "Choose a current Finance assignment.");
    }
}
