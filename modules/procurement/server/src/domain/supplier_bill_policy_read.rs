//! Native Procurement supplier-invoice policy read facts.
//!
//! These are owner read projections only. The Account/Holding session, current
//! Company scope, commercial purchased-package decision and Company Finance
//! authority stay at the composition boundary: composition supplies the
//! `purchase` overlay and the Company authority boolean, and the owner never
//! grants either. The owner enforces `finance.read` and the work-context
//! expiry for this bounded read, and projects the retained
//! `supplierInvoicePolicy` payload without inventing fields.
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde::Serialize;

/// Current Company policy row, already joined to the active HR person name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyHeadRow {
    pub matching_mode: String,
    pub version: i64,
    pub updated_by: String,
    pub updated_at: i64,
    pub updated_by_name: Option<String>,
}

/// Immutable matching-policy event, already joined to the actor display name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyEventRow {
    pub id: String,
    pub company_id: String,
    pub version: i64,
    pub previous_mode: String,
    pub matching_mode: String,
    pub actor_id: String,
    pub reason: String,
    pub created_at: i64,
    pub actor_name: Option<String>,
}

/// Account-derived facts for the selected work context. Composition builds this
/// from the current assignment; no field is accepted from the request.
#[derive(Clone, Debug)]
pub struct PolicyReadActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

impl PolicyReadActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn can_approve(&self) -> bool {
        self.has("finance.approve")
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("finance.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant Finance read access.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

/// Commercial purchased-package overlay, read by composition in the same
/// Holding unit and omitted entirely when package enforcement is disabled.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyPurchase {
    pub new_work: bool,
    pub settle_existing: bool,
    pub history: bool,
    pub revision: String,
}

/// Retained policy event shape: the TS `orderedRead('e.*, ... actor_name')`
/// projection keeps snake-case row keys. Do not camel-case these fields.
#[derive(Clone, Debug, Serialize)]
pub struct PolicyEvent {
    pub id: String,
    pub company_id: String,
    pub version: i64,
    pub previous_mode: String,
    pub matching_mode: String,
    pub actor_id: String,
    pub reason: String,
    pub created_at: i64,
    pub actor_name: Option<String>,
}

/// Exact `supplierInvoicePolicy` response `data`: the application DTO keys plus
/// composition's `companyId` and optional `purchase` overlay.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoicePolicyView {
    pub matching_mode: String,
    pub version: i64,
    pub updated_by_name: Option<String>,
    pub updated_at: Option<i64>,
    pub can_manage: bool,
    pub events: Vec<PolicyEvent>,
    pub company_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase: Option<PolicyPurchase>,
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPLIER_BILL_POLICY_INVALID",
        "The current supplier invoice policy is invalid.",
    )
}

fn matching_mode(value: &str) -> Result<String, BusinessError> {
    if value != "optional" && value != "required" {
        return Err(invalid());
    }
    Ok(value.to_owned())
}

fn event(row: &PolicyEventRow, company: &str) -> Result<PolicyEvent, BusinessError> {
    if row.company_id != company || row.version <= 0 {
        return Err(invalid());
    }
    Ok(PolicyEvent {
        id: row.id.clone(),
        company_id: row.company_id.clone(),
        version: row.version,
        previous_mode: matching_mode(&row.previous_mode)?,
        matching_mode: matching_mode(&row.matching_mode)?,
        actor_id: row.actor_id.clone(),
        reason: row.reason.clone(),
        created_at: row.created_at,
        actor_name: row.actor_name.clone(),
    })
}

/// Project the owner rows into the retained policy response. A missing policy
/// is the retained default (`optional`, version `0`, no updater): the TS reader
/// spread a nullable row and the application supplied these defaults.
pub fn project_policy(
    head: Option<PolicyHeadRow>,
    rows: Vec<PolicyEventRow>,
    can_manage: bool,
    company: &CompanyId,
    purchase: Option<PolicyPurchase>,
) -> Result<InvoicePolicyView, BusinessError> {
    let (matching_mode, version, updated_by_name, updated_at) = match head {
        Some(row) => {
            let name = row.updated_by_name.unwrap_or(row.updated_by);
            (
                matching_mode(&row.matching_mode)?,
                row.version,
                Some(name),
                Some(row.updated_at),
            )
        }
        None => ("optional".to_owned(), 0, None, None),
    };
    if version < 0 {
        return Err(invalid());
    }
    let events = if can_manage {
        let mut projected = rows
            .iter()
            .map(|row| event(row, company.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        projected.sort_by(|left, right| right.version.cmp(&left.version));
        projected
    } else {
        Vec::new()
    };
    Ok(InvoicePolicyView {
        matching_mode,
        version,
        updated_by_name,
        updated_at,
        can_manage,
        events,
        company_id: company.as_str().to_owned(),
        purchase,
    })
}
