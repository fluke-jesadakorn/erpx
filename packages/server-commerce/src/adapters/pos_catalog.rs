//! Commerce-owned POS catalogue read for native cross-owner commands.
//!
//! Commerce owns `commerce_products` and the POS catalogue projections; the POS
//! command still re-checks its own register, sale, version and money guards.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosCatalogProduct {
    pub id: String,
    pub name: String,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub tax_inclusive: bool,
    pub kind: String,
    pub station: String,
    pub recipe_json: String,
    pub modifiers_json: String,
    pub catalog_version: i64,
}

fn transaction_required() -> PersistenceError {
    PersistenceError::new("COMMERCE_POS_CATALOG_TRANSACTION_REQUIRED")
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "POS_CATALOG_CHANGED",
        "The POS catalogue changed. Review the bill.",
    )
}

/// Utility lock: call with the other owner source locks before the first
/// serializable snapshot.
pub async fn lock_pos_catalog(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(transaction_required().into());
    }
    sqlx::query("LOCK TABLE commerce_products,pos_catalog,pos_register_catalog IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

/// Read the active catalogue row sold by this register. The caller holds the
/// catalogue lock and repeats the exact price/version guard before writing.
pub async fn read_pos_catalog_product(
    c: &mut PgConnection,
    company: &CompanyId,
    register_id: &str,
    product_id: &str,
) -> Result<Option<PosCatalogProduct>, Error> {
    if !c.is_in_transaction() {
        return Err(transaction_required().into());
    }
    let row = sqlx::query(
        "SELECT p.id,p.name,p.price_minor,p.tax_bps,COALESCE(c.kind,'stock') kind,\
         COALESCE(c.station,'Kitchen') station,COALESCE(c.recipe_json,'[]') recipe_json,\
         COALESCE(c.modifiers_json,'[]') modifiers_json,COALESCE(c.tax_inclusive,false) tax_inclusive,\
         COALESCE(c.version,0) catalog_version \
         FROM commerce_products p \
         JOIN pos_catalog c ON c.product_id=p.id AND c.company_id=p.company_id AND c.enabled=true \
         JOIN pos_register_catalog rc ON rc.product_id=c.product_id AND rc.register_id=$3 AND rc.enabled=true \
         WHERE p.id=$1 AND p.company_id=$2 AND p.status='active' \
         FOR SHARE OF p,c,rc",
    )
    .bind(product_id)
    .bind(company.as_str())
    .bind(register_id)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let product = PosCatalogProduct {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        price_minor: row.try_get("price_minor")?,
        tax_bps: row.try_get("tax_bps")?,
        tax_inclusive: row.try_get("tax_inclusive")?,
        kind: row.try_get("kind")?,
        station: row.try_get("station")?,
        recipe_json: row.try_get("recipe_json")?,
        modifiers_json: row.try_get("modifiers_json")?,
        catalog_version: row.try_get("catalog_version")?,
    };
    if product.catalog_version < 0 || product.price_minor < 0 || !(0..=10_000).contains(&product.tax_bps)
    {
        return Err(changed().into());
    }
    Ok(Some(product))
}
