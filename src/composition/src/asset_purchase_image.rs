//! Assets and Finance source image for purchase checks. This is not authority.
//! Capture it after intended writes and recheck after deferred effects; callers
//! must acquire all required owner locks before the first transaction snapshot.
use erpx_server_assets::public::asset_purchase_source::CurrentAssetPurchaseSource;
use erpx_server_finance::public::asset_purchase_source::CurrentAssetPurchaseFinanceJoin;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Postgres, QueryBuilder, Row};
type Error = PgTransactionError<BusinessError>;

pub struct AssetPurchaseSources<'a> {
    pub asset: &'a CurrentAssetPurchaseSource,
    pub finance: &'a CurrentAssetPurchaseFinanceJoin,
}
impl AssetPurchaseSources<'_> {
    async fn evaluate(&self, c: &mut PgConnection) -> Result<Option<String>, Error> {
        self.asset.require_current(&mut *c).await?;
        self.finance.require_current(&mut *c).await?;
        let mut q = QueryBuilder::<Postgres>::new("WITH ");
        self.asset.append_purchase_ctes(&mut q);
        q.push(", ").push(self.finance.cte_sql());
        q.push(" SELECT jsonb_build_object('asset', CASE WHEN ").push_bind(self.asset.file_mode())
            .push("::bool THEN to_jsonb(r)-'version'-'updated_at' ELSE to_jsonb(r) END, 'finance',to_jsonb(f), 'links',(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM asset_purchase_links link WHERE link.asset_id=r.id))::text AS image FROM asset_purchase_selected r LEFT JOIN finance_asset_purchase_rows f ON f.id=r.finance_asset_id AND f.company_id=r.company_id");
        let row = q.build().fetch_optional(&mut *c).await?;
        let image = row.map(|r| r.try_get("image")).transpose()?;
        self.asset.require_current(&mut *c).await?;
        self.finance.require_current(c).await?;
        Ok(image)
    }
    pub async fn inspect<'a>(
        &'a self,
        c: &mut PgConnection,
    ) -> Result<CurrentAssetPurchaseImage<'a, 'a>, Error> {
        Ok(CurrentAssetPurchaseImage {
            image: self.evaluate(c).await?,
            sources: self,
        })
    }
}
pub struct CurrentAssetPurchaseImage<'a, 's> {
    sources: &'a AssetPurchaseSources<'s>,
    image: Option<String>,
}
impl CurrentAssetPurchaseImage<'_, '_> {
    pub fn image(&self) -> Option<&str> {
        self.image.as_deref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if self.sources.evaluate(c).await? != self.image {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "ASSET_PURCHASE_IMAGE_CHANGED",
                "The Asset purchase source changed.",
            )
            .into());
        }
        Ok(())
    }
}
