//! PostgreSQL adapter for the native Brand attachment-source image read.
//!
//! The caller owns the Holding transaction. This adapter reads only the
//! brand-profile and storefront rows the retained
//! `brandAttachmentReadSourceImage` family returns: same tables, same
//! predicates, same bind order. It performs no branch/department row
//! filtering beyond the retained scope pair; the caller keeps its own
//! projection window exactly like the TypeScript family.
use crate::domain::brand_attachment_read::BrandAttachmentKind;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn transaction_required() -> Error {
    PersistenceError::new("BRAND_ATTACHMENT_READ_TRANSACTION_REQUIRED").into()
}

fn unsupported() -> Error {
    BusinessError::new(
        FailureKind::InvalidInput,
        "BRAND_ATTACHMENT_TARGET_UNKNOWN",
        "Unknown Brand attachment read target.",
    )
    .into()
}

/// Read the brand image for one attachment-source target on the caller's
/// transaction. The statement is byte-equivalent to the retained TypeScript
/// read; only the crate boundary moves into Rust.
pub async fn read_brand_attachment_image(
    connection: &mut PgConnection,
    company_id: &str,
    kind: &str,
    scope_id: Option<&str>,
) -> Result<String, Error> {
    if !connection.is_in_transaction() {
        return Err(transaction_required());
    }
    let kind = BrandAttachmentKind::parse(kind).ok_or_else(unsupported)?;
    if company_id.is_empty() {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "BRAND_ATTACHMENT_COMPANY_REQUIRED",
            "A Company scope is required.",
        )
        .into());
    }
    let row: (String,) = sqlx::query_as(
        "SELECT jsonb_build_object('profile',(SELECT to_jsonb(profile) FROM organization_brand_profiles profile WHERE profile.scope_kind=$1 AND profile.scope_id=$2),\
         'storefront',(SELECT to_jsonb(storefront) FROM storefront_app_brands storefront WHERE storefront.company_id=$3 AND $4::text='company_storefront_brand'))::text",
    )
    .bind(kind.scope_kind())
    .bind(scope_id)
    .bind(company_id)
    .bind(kind.as_target())
    .fetch_one(&mut *connection)
    .await?;
    Ok(row.0)
}
