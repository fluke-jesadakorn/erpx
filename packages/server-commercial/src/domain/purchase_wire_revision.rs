//! Legacy TypeScript wire revision. This is presentation data, not authority.
use crate::domain::{collation::package_id_collator, packages::{CompanyPackageGrant, CompanyPackageHistoryRight}};
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use sha2::{Digest, Sha256};

fn encoding_error() -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, "COMMERCIAL_WIRE_REVISION_INVALID", "The purchase revision is unavailable.")
}

fn text(value: &str) -> Result<String, BusinessError> {
    serde_json::to_string(value).map_err(|_| encoding_error())
}

// TS reads the purchase snapshot via JSONB then JSON.parse, so BIGINT values
// become JavaScript Numbers before phase checks and JSON.stringify.
fn number(value: i64) -> String {
    let value = value as f64;
    if value == 0.0 { return "0".into(); }
    ryu_js::Buffer::new().format_finite(value).to_owned()
}

/// Compute the wire-format revision from ordered, already-retained facts.
/// Presentation callers must preserve `readCompanyPurchaseFacts` database
/// ordering (package_id, then package_version for history) before calling.
/// The separate checked-observation SQL uses unordered aggregates; its input
/// order cannot be reconstructed from this function's result.
/// This never grants authority, replaces PurchaseWitness, or opens a unit.
/// Callers must source/revalidate facts in their own authorized transaction.
pub fn purchase_wire_revision(
    company_id: &str,
    grants: &[CompanyPackageGrant],
    history: &[CompanyPackageHistoryRight],
    now: TimestampMillis,
) -> Result<String, BusinessError> {
    let now = now.get() as f64;
    let collator = package_id_collator()?;
    let mut grant_rows = Vec::new();
    for grant in grants.iter().filter(|g| g.company_id == company_id) {
        let starts = grant.starts_at as f64;
        let phase = if starts > now { "future" } else if grant.ends_at.is_some_and(|end| (end as f64) <= now) { "expired" } else { "current" };
        let ends = grant.ends_at.map(number).unwrap_or_else(|| "null".into());
        grant_rows.push(format!("[{},{},{},{},{},{},{}]", text(&grant.package_id)?, number(grant.package_version), number(grant.version), text(&grant.state)?, number(grant.starts_at), ends, text(phase)?));
    }
    // sort_by is stable, matching JS sort for localeCompare-equal tuples.
    grant_rows.sort_by(|a, b| collator.compare(a, b));
    let mut history_rows = Vec::new();
    for right in history.iter().filter(|h| h.company_id == company_id) {
        let phase = if (right.retained_at as f64) > now { "future" } else { "retained" };
        history_rows.push(format!("[{},{},{},{},{},{}]", text(&right.package_id)?, number(right.package_version), number(right.source_grant_version), text(&right.source_event_id)?, number(right.retained_at), text(phase)?));
    }
    history_rows.sort_by(|a, b| collator.compare(a, b));
    let source = format!("{{\"companyId\":{},\"grants\":[{}],\"history\":[{}]}}", text(company_id)?, grant_rows.join(","), history_rows.join(","));
    Ok(hex::encode(Sha256::digest(source.as_bytes())))
}
