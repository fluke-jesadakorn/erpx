//! Source-only reconstruction of Commerce's persisted request linkage.
//!
//! This is not request visibility, authorization, or replay admission. The
//! caller must acquire all other required owner fences before any snapshot,
//! authorize the selected request, and retain the result through effects.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// Lock both owned relations before the first snapshot. The caller remains
/// responsible for the global order relative to all other owner sources.
pub async fn lock_sources(c: &mut PgConnection) -> Result<(), Error> {
    erpx_server_procurement::public::commerce_request_link::lock_commerce_request_links(&mut *c)
        .await?;
    erpx_server_records::public::commerce_request_links::lock_commerce_request_links(c).await?;
    Ok(())
}

pub struct CurrentCommerceRequestLinkage {
    records: erpx_server_records::public::commerce_request_links::CurrentCommerceRequestLinks,
    procurement:
        erpx_server_procurement::public::commerce_request_link::CurrentCommerceRequestLinks,
    company: String,
    selected: Option<String>,
}

impl CurrentCommerceRequestLinkage {
    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// Re-evaluate the actual joined decision after deferred effects in the
    /// same unit. An unordered SQL first is intentional, matching TypeScript.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.records.require_current(&mut *c).await?;
        self.procurement.require_current(&mut *c).await?;
        let current = select(&mut *c, &self.records, &self.procurement, &self.company).await?;
        self.records.require_current(&mut *c).await?;
        self.procurement.require_current(c).await?;
        if current != self.selected {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "COMMERCE_REQUEST_LINK_CHANGED",
                "The linked request changed.",
            )
            .into());
        }
        Ok(())
    }
}

/// Observe only after all owner locks have been taken. Company and order must
/// come from the authenticated workspace and parsed server command.
pub async fn inspect(
    c: &mut PgConnection,
    company: &str,
    order: &str,
) -> Result<CurrentCommerceRequestLinkage, Error> {
    let records =
        erpx_server_records::public::commerce_request_links::inspect_commerce_request_links(
            &mut *c,
        )
        .await?;
    let procurement =
        erpx_server_procurement::public::commerce_request_link::inspect_commerce_request_links(
            &mut *c, order,
        )
        .await?;
    let selected = select(&mut *c, &records, &procurement, company).await?;
    records.require_current(&mut *c).await?;
    procurement.require_current(c).await?;
    Ok(CurrentCommerceRequestLinkage {
        records,
        procurement,
        company: company.to_owned(),
        selected,
    })
}

async fn select(
    c: &mut PgConnection,
    records: &erpx_server_records::public::commerce_request_links::CurrentCommerceRequestLinks,
    procurement: &erpx_server_procurement::public::commerce_request_link::CurrentCommerceRequestLinks,
    company: &str,
) -> Result<Option<String>, Error> {
    // Both CTEs are fixed owner-provided SQL; all request values are bound.
    let query = format!(
        "WITH {}, {} SELECT r.id FROM commerce_request_links r WHERE r.company_id=$2 AND (r.purchase_order_id=$1 OR EXISTS (SELECT 1 FROM procurement_commerce_request_links l WHERE l.request_id=r.id AND l.order_id=$1))",
        records.cte_sql(),
        procurement.cte_sql()
    );
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str()))
        .bind(procurement.order_id())
        .bind(company)
        .fetch_optional(c)
        .await?)
}
