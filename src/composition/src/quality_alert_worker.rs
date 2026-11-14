//! Native scheduled Quality alert sweep. The Quality owner port owns the two
//! idempotent due-soon/overdue escalations; this loop is its composition caller.
use erpx_server_identity_access::public::SystemAccountClock;
use erpx_server_quality::public::quality_alerts::{
    QUALITY_ALERT_WARNING_WINDOW_MS, drain_quality_alerts,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PoolLimits};
use erpx_server_shared::public::{BusinessError, Clock};
use std::time::Duration;

/// One scheduled sweep of the caller's Holding database.
pub async fn run_once(
    write: &DatabasePool,
    now: i64,
) -> Result<(u64, u64), BusinessError> {
    drain_quality_alerts(
        write.clone(),
        now,
        QUALITY_ALERT_WARNING_WINDOW_MS,
    )
    .await
}

#[cfg(feature = "private-field-api")]
pub(crate) async fn run(
    databases: Vec<crate::runtime_database::RuntimeDatabase>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let limits = PoolLimits {
        max_connections: 2,
        acquire_timeout: Duration::from_secs(5),
    };
    let mut pools = Vec::new();
    for database in databases {
        match database.connect(limits).await {
            Ok(pool) => pools.push(pool),
            Err(_) => {
                eprintln!("QUALITY_ALERT_WORKER_UNAVAILABLE");
                return;
            }
        }
    }
    loop {
        if *stop.borrow() {
            return;
        }
        let now = SystemAccountClock.now().get();
        for pool in &pools {
            if run_once(pool, now).await.is_err() {
                eprintln!("QUALITY_ALERTS_UNAVAILABLE");
            }
        }
        tokio::select! {
            _ = stop.changed() => return,
            _ = tokio::time::sleep(Duration::from_secs(120)) => {}
        }
    }
}
