//! Platform Executive read use cases.
//!
//! Authorization is limited to the current Platform role and permission
//! supplied by the composition backend. The reads are bounded and never return
//! Holding identity, provisioning failure detail or health detail.
use crate::domain::platform_executive::*;
use crate::domain::platform_operations::sha256_hex;
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type PlatformExecutiveFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner read boundary over the Control and Control-read sources.
pub trait PlatformExecutiveStore: Send + Sync {
    fn failed_provisioning<'a>(
        &'a self,
        from_at: i64,
        to_at: i64,
    ) -> PlatformExecutiveFuture<'a, Vec<PlatformExecutiveGroup>>;
    fn abnormal_services<'a>(&'a self) -> PlatformExecutiveFuture<'a, Vec<PlatformExecutiveGroup>>;
    fn failed_provisioning_sources<'a>(
        &'a self,
        from_at: i64,
        to_at: i64,
        label: &'a str,
        offset: u32,
    ) -> PlatformExecutiveFuture<'a, Vec<ProvisioningFailureSourceRow>>;
    fn abnormal_service_sources<'a>(
        &'a self,
        label: &'a str,
        offset: u32,
    ) -> PlatformExecutiveFuture<'a, Vec<AbnormalHealthSourceRow>>;
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        code,
        message,
    )
}

fn revision(
    failed: &[PlatformExecutiveGroup],
    abnormal: &[PlatformExecutiveGroup],
) -> Result<String, BusinessError> {
    let payload = serde_json::json!({
        "provisioningFailures": failed,
        "abnormalServices": abnormal,
    });
    serde_json::to_string(&payload)
        .map(|value| sha256_hex(value.as_bytes()))
        .map_err(|_| {
            unavailable(
                "PLATFORM_EXECUTIVE_UNAVAILABLE",
                "Platform Executive insights are unavailable.",
            )
        })
}

pub async fn platform_executive_snapshot<S: PlatformExecutiveStore + ?Sized>(
    actor: &PlatformExecutiveActor,
    store: &S,
    raw_filters: Option<&ExecutiveDateRangeInput>,
    now: i64,
) -> Result<PlatformExecutiveSnapshot, BusinessError> {
    require_platform_executive(actor)?;
    let (filters, from_at, to_at) = resolve_filters(raw_filters, now)?;
    let failed = store.failed_provisioning(from_at, to_at).await?;
    let abnormal = store.abnormal_services().await?;
    let source_revision = revision(&failed, &abnormal)?;
    let referenced_at = now;
    let metrics = vec![
        PlatformExecutiveMetric {
            id: PlatformExecutiveMetricId::FailedProvisioning,
            label: "Failed provisioning jobs".into(),
            formula: "Count immutable platform_holding_provisioning_events with action failed in the selected UTC interval.".into(),
            date_basis: "Failure event time (UTC)".into(),
            total: metric_total(&failed)?,
            status: PlatformExecutiveMetricStatus::for_rows(&failed),
            rows: failed,
            source_references: vec![PlatformExecutiveSourceReference {
                dataset: "platform_holding_provisioning_events".into(),
                revision: source_revision.clone(),
                referenced_at,
            }],
        },
        PlatformExecutiveMetric {
            id: PlatformExecutiveMetricId::AbnormalPlatformServices,
            label: "Abnormal platform services".into(),
            formula: "Count monitored platform_health components whose current status is outside healthy, ready, and ok.".into(),
            date_basis: "Latest health observation".into(),
            total: metric_total(&abnormal)?,
            status: PlatformExecutiveMetricStatus::for_rows(&abnormal),
            rows: abnormal,
            source_references: vec![PlatformExecutiveSourceReference {
                dataset: "platform_health".into(),
                revision: source_revision.clone(),
                referenced_at,
            }],
        },
    ];
    Ok(PlatformExecutiveSnapshot {
        scope: EXECUTIVE_SCOPE.into(),
        reference_at: referenced_at,
        filters,
        source_revision,
        metrics,
    })
}

pub async fn platform_executive_sources<S: PlatformExecutiveStore + ?Sized>(
    actor: &PlatformExecutiveActor,
    store: &S,
    request: &PlatformExecutiveSourceRequest,
) -> Result<PlatformExecutiveSources, BusinessError> {
    require_platform_executive(actor)?;
    let offset = (request.page - 1) * EXECUTIVE_SOURCE_PAGE_SIZE as u32;
    let rows = match request.metric {
        PlatformExecutiveMetricId::FailedProvisioning => store
            .failed_provisioning_sources(request.from_at, request.to_at, &request.label, offset)
            .await?
            .into_iter()
            .map(PlatformExecutiveSourceRow::ProvisioningFailure)
            .collect::<Vec<_>>(),
        PlatformExecutiveMetricId::AbnormalPlatformServices => store
            .abnormal_service_sources(&request.label, offset)
            .await?
            .into_iter()
            .map(PlatformExecutiveSourceRow::AbnormalHealth)
            .collect::<Vec<_>>(),
    };
    let has_next = rows.len() > EXECUTIVE_SOURCE_PAGE_SIZE;
    Ok(PlatformExecutiveSources {
        id: request.label.clone(),
        name: request.label.clone(),
        metric: request.metric,
        filters: request.filters.clone(),
        rows: rows.into_iter().take(EXECUTIVE_SOURCE_PAGE_SIZE).collect(),
        page: request.page,
        has_next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Store {
        failed: Vec<PlatformExecutiveGroup>,
        abnormal: Vec<PlatformExecutiveGroup>,
        failed_sources: Vec<ProvisioningFailureSourceRow>,
        abnormal_sources: Vec<AbnormalHealthSourceRow>,
        calls: Mutex<Vec<String>>,
    }

    impl PlatformExecutiveStore for Store {
        fn failed_provisioning<'a>(
            &'a self,
            _: i64,
            _: i64,
        ) -> PlatformExecutiveFuture<'a, Vec<PlatformExecutiveGroup>> {
            Box::pin(async move {
                self.calls.lock().unwrap().push("failed".into());
                Ok(self.failed.clone())
            })
        }
        fn abnormal_services<'a>(
            &'a self,
        ) -> PlatformExecutiveFuture<'a, Vec<PlatformExecutiveGroup>> {
            Box::pin(async move {
                self.calls.lock().unwrap().push("abnormal".into());
                Ok(self.abnormal.clone())
            })
        }
        fn failed_provisioning_sources<'a>(
            &'a self,
            _: i64,
            _: i64,
            _: &'a str,
            _: u32,
        ) -> PlatformExecutiveFuture<'a, Vec<ProvisioningFailureSourceRow>> {
            Box::pin(async move {
                self.calls.lock().unwrap().push("failed-sources".into());
                Ok(self.failed_sources.clone())
            })
        }
        fn abnormal_service_sources<'a>(
            &'a self,
            _: &'a str,
            _: u32,
        ) -> PlatformExecutiveFuture<'a, Vec<AbnormalHealthSourceRow>> {
            Box::pin(async move {
                self.calls.lock().unwrap().push("abnormal-sources".into());
                Ok(self.abnormal_sources.clone())
            })
        }
    }

    fn actor(permission: bool) -> PlatformExecutiveActor {
        PlatformExecutiveActor {
            user_id: "operator".into(),
            assignment_id: "platform-assignment".into(),
            role: "platform_admin".into(),
            permissions: if permission {
                vec![EXECUTIVE_PERMISSION.into()]
            } else {
                Vec::new()
            },
        }
    }

    fn group(label: &str, value: &str) -> PlatformExecutiveGroup {
        PlatformExecutiveGroup {
            label: label.into(),
            value: value.into(),
        }
    }

    #[tokio::test]
    async fn snapshot_aggregates_exact_totals_and_a_stable_bounded_revision() {
        let store = Store {
            failed: vec![group("2026-06", "2")],
            abnormal: vec![group("jobs · degraded", "1")],
            failed_sources: Vec::new(),
            abnormal_sources: Vec::new(),
            calls: Mutex::new(Vec::new()),
        };
        let filters = ExecutiveDateRangeInput {
            from: Some("2026-06-01".into()),
            to: Some("2026-06-30".into()),
        };
        let snapshot =
            platform_executive_snapshot(&actor(true), &store, Some(&filters), 1_800_000_000_000)
                .await
                .unwrap();
        assert_eq!(snapshot.scope, "platform");
        assert_eq!(snapshot.reference_at, 1_800_000_000_000);
        assert_eq!(snapshot.filters.from, "2026-06-01");
        assert_eq!(snapshot.metrics.len(), 2);
        assert_eq!(snapshot.metrics[0].total, "2");
        assert_eq!(
            snapshot.metrics[0].status,
            PlatformExecutiveMetricStatus::Ready
        );
        assert_eq!(snapshot.metrics[1].total, "1");
        assert_eq!(snapshot.source_revision.len(), 64);
        assert_eq!(
            snapshot.metrics[0].source_references[0].revision,
            snapshot.source_revision
        );
        assert_eq!(
            snapshot.metrics[1].source_references[0].dataset,
            "platform_health"
        );
        let again =
            platform_executive_snapshot(&actor(true), &store, Some(&filters), 1_800_000_000_000)
                .await
                .unwrap();
        assert_eq!(again.source_revision, snapshot.source_revision);
        let calls_before_denial = store.calls.lock().unwrap().len();
        let empty_store = Store {
            failed: Vec::new(),
            abnormal: Vec::new(),
            failed_sources: Vec::new(),
            abnormal_sources: Vec::new(),
            calls: Mutex::new(Vec::new()),
        };
        let empty = platform_executive_snapshot(
            &actor(true),
            &empty_store,
            Some(&filters),
            1_800_000_000_000,
        )
        .await
        .unwrap();
        assert_eq!(
            empty.metrics[0].status,
            PlatformExecutiveMetricStatus::Empty
        );
        assert_eq!(empty.metrics[0].total, "0");
        assert_eq!(
            platform_executive_snapshot(&actor(false), &store, Some(&filters), 1)
                .await
                .unwrap_err()
                .code,
            "PLATFORM_EXECUTIVE_FORBIDDEN"
        );
        assert_eq!(store.calls.lock().unwrap().len(), calls_before_denial);
    }

    #[tokio::test]
    async fn sources_are_bounded_to_thirty_rows_and_report_the_next_page() {
        let mut failed_sources = Vec::new();
        let mut abnormal_sources = Vec::new();
        for index in 0..31 {
            failed_sources.push(ProvisioningFailureSourceRow {
                created_at: index,
                action: "failed".into(),
            });
            abnormal_sources.push(AbnormalHealthSourceRow {
                component: format!("component-{index}"),
                status: "degraded".into(),
                checked_at: index,
            });
        }
        let store = Store {
            failed: Vec::new(),
            abnormal: Vec::new(),
            failed_sources,
            abnormal_sources,
            calls: Mutex::new(Vec::new()),
        };
        let request = validate_source_request(
            "failed-provisioning",
            "2026-06",
            "2026-06-01",
            "2026-06-30",
            Some("2"),
        )
        .unwrap();
        let sources = platform_executive_sources(&actor(true), &store, &request)
            .await
            .unwrap();
        assert_eq!(sources.rows.len(), EXECUTIVE_SOURCE_PAGE_SIZE);
        assert!(sources.has_next);
        assert_eq!(sources.page, 2);
        assert_eq!(sources.id, "2026-06");
        assert_eq!(sources.name, "2026-06");
        let request = validate_source_request(
            "abnormal-platform-services",
            "jobs · degraded",
            "2026-06-01",
            "2026-06-30",
            None,
        )
        .unwrap();
        let sources = platform_executive_sources(&actor(true), &store, &request)
            .await
            .unwrap();
        assert_eq!(sources.rows.len(), EXECUTIVE_SOURCE_PAGE_SIZE);
        assert!(sources.has_next);
        assert_eq!(store.calls.lock().unwrap().len(), 2);
        assert_eq!(
            platform_executive_sources(&actor(false), &store, &request)
                .await
                .unwrap_err()
                .code,
            "PLATFORM_EXECUTIVE_FORBIDDEN"
        );
    }
}
