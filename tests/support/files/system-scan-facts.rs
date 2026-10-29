#[allow(
    dead_code,
    reason = "shared Files facts include legacy/media fixtures used in sibling suites"
)]
#[path = "scan-facts.rs"]
pub mod facts;
#[allow(
    dead_code,
    reason = "shared real leased Account fixture also serves browser suites"
)]
#[path = "../runtime/private-session-api.rs"]
pub mod fixture;
use erpx_server_files::public::attachment_scan::{AttachmentScanFailure, AttachmentScanScope};
use erpx_server_runtime::public::postgres::LeasedDevelopmentDatabase;
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId};
use erpx_service::files_system_scan::DevelopmentFilesScanConfiguration;
use std::{collections::BTreeMap, sync::Arc};
pub async fn lease(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
) -> LeasedDevelopmentDatabase {
    let d = fixture::database(pool).await;
    LeasedDevelopmentDatabase::parse(
        d["url"].as_str().unwrap(),
        d["leaseToken"].as_str().unwrap(),
    )
    .unwrap()
}
pub async fn config(
    s: &fixture::account::System,
    company: &str,
) -> DevelopmentFilesScanConfiguration {
    DevelopmentFilesScanConfiguration::new(
        lease(&s.control).await,
        BTreeMap::from([
            ("HOLDING_H001_DB".into(), lease(&s.holdings[0]).await),
            ("HOLDING_H002_DB".into(), lease(&s.holdings[1]).await),
        ]),
        fixture::account::fixture::account::holding_registry(),
        AttachmentScanScope {
            holding_id: HoldingId::new("h001").unwrap(),
            company_id: CompanyId::new(company).unwrap(),
        },
    )
    .unwrap()
}
pub fn clock() -> Arc<facts::TestClock> {
    Arc::new(facts::TestClock::new(1000))
}
pub fn missing() -> AttachmentScanFailure {
    AttachmentScanFailure::from_business(&BusinessError::invalid(
        "ATTACHMENT_OBJECT_MISSING",
        "Owned missing original evidence.",
    ))
    .unwrap()
}
pub async fn state(s: &fixture::account::System, id: &str) -> (String, String, i64, i64) {
    sqlx::query_as("SELECT a.source_state,j.state,(SELECT count(*)FROM attachment_scan_attempts WHERE attachment_id=a.id),(SELECT count(*)FROM attachment_events WHERE attachment_id=a.id AND action LIKE 'scan.%')FROM attachments a JOIN attachment_scan_attempts j ON j.attachment_id=a.id WHERE a.id=$1 ORDER BY j.attempt LIMIT 1").bind(id).fetch_one(s.holdings[0].raw()).await.unwrap()
}
