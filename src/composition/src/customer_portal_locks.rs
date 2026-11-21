//! Unmounted Portal owner fences. This helper grants no purchase, session or
//! business authority and must run before any Holding source snapshot.
use erpx_server_commercial::public::{purchase_command_boundary::lock_portal_purchase_writer_fence,purchases::PackageEnforcementMode};
use erpx_server_commerce::public::portal_commerce_locks::lock_portal_commerce_sources;
use erpx_server_procurement::public::portal_commerce_locks::lock_portal_procurement_sources;
use erpx_server_inventory::public::portal_commerce_locks::lock_portal_inventory_sources;
use erpx_server_quality::public::portal_commerce_locks::lock_portal_quality_sources;
use erpx_server_records::public::portal_commerce_locks::lock_portal_records_sources;
use erpx_server_finance::public::portal_commerce_locks::lock_portal_finance_sources;
use erpx_server_sales::public::portal_commerce_locks::lock_portal_sales_sources;
use erpx_server_customer_authority::public::portal_commerce_locks::lock_portal_customer_authority_sources;
use erpx_server_customer_360::public::portal_action_source::lock_portal_action_sources;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
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
