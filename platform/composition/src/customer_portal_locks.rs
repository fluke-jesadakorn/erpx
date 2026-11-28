//! Unmounted Portal owner fences. This helper grants no purchase, session or
//! business authority and must run before any Holding source snapshot.
use erpx_commercial_server::public::{purchase_command_boundary::lock_portal_purchase_writer_fence,purchases::PackageEnforcementMode};
use erpx_commerce_server::public::portal_commerce_locks::lock_portal_commerce_sources;
use erpx_procurement_server::public::portal_commerce_locks::lock_portal_procurement_sources;
use erpx_inventory_server::public::portal_commerce_locks::lock_portal_inventory_sources;
use erpx_quality_server::public::portal_commerce_locks::lock_portal_quality_sources;
use erpx_records_server::public::portal_commerce_locks::lock_portal_records_sources;
use erpx_finance_server::public::portal_commerce_locks::lock_portal_finance_sources;
use erpx_sales_server::public::portal_commerce_locks::lock_portal_sales_sources;
use erpx_customer_authority_server::public::portal_commerce_locks::lock_portal_customer_authority_sources;
use erpx_customer_360_server::public::portal_action_source::lock_portal_action_sources;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::BusinessError;
use sqlx::PgConnection;

pub(crate) async fn lock_portal_enabled_sources(c:&mut PgConnection,mode:PackageEnforcementMode)->Result<(),PgTransactionError<BusinessError>>{
 if mode==PackageEnforcementMode::Disabled{return Ok(());}
 lock_portal_purchase_writer_fence(c).await?;
 lock_portal_commerce_sources(c).await?;
 lock_portal_procurement_sources(c).await?;
 lock_portal_inventory_sources(c).await?;
 lock_portal_quality_sources(c).await?;
 lock_portal_records_sources(c).await?;
 lock_portal_finance_sources(c).await?;
 lock_portal_sales_sources(c).await?;
 lock_portal_customer_authority_sources(c).await?;
 lock_portal_action_sources(c).await?;
 Ok(())
}
