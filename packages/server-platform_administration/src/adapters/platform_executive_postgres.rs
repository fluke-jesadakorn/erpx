//! PostgreSQL adapter for the bounded Platform Executive metrics and sources.
//!
//! Provisioning failures are immutable Control receipts; abnormal services are
//! the current Control-read `platform_health` observations. Both sources are
//! read directly and bounded, and no Holding identity, failure reason or health
//! detail is selected.
use crate::application::platform_executive::{PlatformExecutiveFuture, PlatformExecutiveStore};
use crate::domain::platform_executive::*;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

fn database_error() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLATFORM_EXECUTIVE_UNAVAILABLE",
        "Platform Executive insights are unavailable.",
    )
}

#[derive(Clone)]
pub struct PgPlatformExecutive {
    control: DatabasePool,
    control_read: DatabasePool,
}

impl PgPlatformExecutive {
    /// The Control pool owns immutable provisioning receipts; the Control-read
    /// pool owns current platform health. They are explicit so a deployment
    /// cannot silently substitute one source for the other.
    pub fn new(control: DatabasePool, control_read: DatabasePool) -> Self {
        Self {
            control,
            control_read,
        }
    }

    pub fn control(&self) -> &DatabasePool {
        &self.control
    }

    pub fn control_read(&self) -> &DatabasePool {
        &self.control_read
    }
}

impl PlatformExecutiveStore for PgPlatformExecutive {
    fn failed_provisioning<'a>(
        &'a self,
        from_at: i64,
        to_at: i64,
    ) -> PlatformExecutiveFuture<'a, Vec<PlatformExecutiveGroup>> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,\
                 count(*)::text value FROM platform_holding_provisioning_events \
                 WHERE action='failed' AND created_at>=$1 AND created_at<$2 \
                 GROUP BY 1 ORDER BY 1",
            )
            .bind(from_at)
            .bind(to_at)
            .fetch_all(self.control.raw())
            .await
            .map_err(|_| database_error())?;
            Ok(rows
                .into_iter()
                .map(|row| PlatformExecutiveGroup {
                    label: row.get("label"),
                    value: row.get("value"),
                })
                .collect())
        })
    }

    fn abnormal_services<'a>(&'a self) -> PlatformExecutiveFuture<'a, Vec<PlatformExecutiveGroup>> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT component||' · '||status label,count(*)::text value FROM platform_health \
                 WHERE lower(status) NOT IN ('healthy','ready','ok') \
                 GROUP BY component,status ORDER BY component,status",
            )
            .fetch_all(self.control_read.raw())
            .await
            .map_err(|_| database_error())?;
            Ok(rows
                .into_iter()
                .map(|row| PlatformExecutiveGroup {
                    label: row.get("label"),
                    value: row.get("value"),
                })
                .collect())
        })
    }

    fn failed_provisioning_sources<'a>(
        &'a self,
        from_at: i64,
        to_at: i64,
        label: &'a str,
        offset: u32,
    ) -> PlatformExecutiveFuture<'a, Vec<ProvisioningFailureSourceRow>> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT created_at,action FROM platform_holding_provisioning_events \
                 WHERE action='failed' AND created_at>=$1 AND created_at<$2 \
                 AND to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')=$3 \
                 ORDER BY created_at,id LIMIT 31 OFFSET $4",
            )
            .bind(from_at)
            .bind(to_at)
            .bind(label)
            .bind(i64::from(offset))
            .fetch_all(self.control.raw())
            .await
            .map_err(|_| database_error())?;
            Ok(rows
                .into_iter()
                .map(|row| ProvisioningFailureSourceRow {
                    created_at: row.get("created_at"),
                    action: row.get("action"),
                })
                .collect())
        })
    }

    fn abnormal_service_sources<'a>(
        &'a self,
        label: &'a str,
        offset: u32,
    ) -> PlatformExecutiveFuture<'a, Vec<AbnormalHealthSourceRow>> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT component,status,checked_at FROM platform_health \
                 WHERE lower(status) NOT IN ('healthy','ready','ok') \
                 AND component||' · '||status=$1 ORDER BY component LIMIT 31 OFFSET $2",
            )
            .bind(label)
            .bind(i64::from(offset))
            .fetch_all(self.control_read.raw())
            .await
            .map_err(|_| database_error())?;
            Ok(rows
                .into_iter()
                .map(|row| AbnormalHealthSourceRow {
                    component: row.get("component"),
                    status: row.get("status"),
                    checked_at: row.get("checked_at"),
                })
                .collect())
        })
    }
}
