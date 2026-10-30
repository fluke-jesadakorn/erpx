//! Explicit Field Files assembly from registered leased development resources.
use super::*;
use crate::{
    field_attachment_intake_http::{
        FieldIntakeProviderReadiness,
        FieldIntakeProviderStatus, FieldIntakeTarget, field_intake_router,
    },
    field_files_runtime_configuration::parse_field_files_runtime_configuration,
    field_upload_http::{FieldUploadTarget, field_upload_router},
    files_scan_native::NativeFilesScanWorker,
    files_scan_worker::{
        FieldFilesScanTarget, assemble_field_files_scan_workers, run_field_files_workers,
    },
    files_system_scan::DevelopmentFilesScanConfiguration,
    service_company_ownership::ServiceCompanyOwnership,
};
use erpx_server_files::public::{
    attachment_scan::{AttachmentScanScope, LocalSanitizationStore},
    field_visit_upload::LocalFieldQuarantineWriter,
};

struct HoldingReadiness(Vec<NativeFilesScanWorker<SharedClock>>);
impl FieldIntakeProviderReadiness for HoldingReadiness {
    fn probe(
        &self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        FieldIntakeProviderStatus,
                        erpx_server_shared::public::BusinessError,
                    >,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let mut status = FieldIntakeProviderStatus {
                scanner_enabled: true,
                scanner_ready: true,
                preview_ready: true,
            };
            for worker in &self.0 {
                let s = worker.probe().await?;
                status.scanner_enabled &= s.scanner_enabled;
                status.scanner_ready &= s.scanner_ready;
                status.preview_ready &= s.preview_ready;
            }
            Ok(status)
        })
    }
}
pub(super) async fn assemble(
    config: &DevelopmentApiConfiguration,
    pools: &[DatabasePool],
    clock: Arc<dyn Clock + Send + Sync>,
) -> Result<
    (
        Router,
        Vec<NativeFilesScanWorker<SharedClock>>,
        BTreeSet<(String, String)>,
    ),
    &'static str,
> {
    let Some(raw) = &config.field_files_providers else {
        return Ok((Router::new(), Vec::new(), BTreeSet::new()));
    };
    if pools.len() != config.holdings.len() + 1 {
        return Err("FIELD_FILES_DATABASES_INVALID");
    }
    let bytes = serde_json::to_vec(raw).map_err(|_| "FIELD_FILES_CONFIGURATION_INVALID")?;
    let targets = parse_field_files_runtime_configuration(&bytes, &config.holdings_registry)
        .map_err(|_| "FIELD_FILES_CONFIGURATION_INVALID")?;
    let reviewed = config
        .holdings
        .iter()
        .map(|h| (h.registration.write_binding.clone(), h.database.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut grouped: BTreeMap<HoldingId, (BTreeSet<CompanyId>, std::path::PathBuf, Vec<usize>)> =
        BTreeMap::new();
    let mut scan_targets = Vec::new();
    for t in targets {
        let h = config
            .holdings
            .iter()
            .find(|h| h.registration.holding_id == t.holding_id)
            .ok_or("FIELD_FILES_CONFIGURATION_INVALID")?;
        let root = t
            .providers
            .files_roots
            .get(&h.registration.files_binding)
            .ok_or("FIELD_FILES_CONFIGURATION_INVALID")?
            .clone();
        let entry = grouped
            .entry(t.holding_id.clone())
            .or_insert_with(|| (BTreeSet::new(), root.clone(), Vec::new()));
        if entry.1 != root || !entry.0.insert(t.company_id.clone()) {
            return Err("FIELD_FILES_CONFIGURATION_INVALID");
        }
        entry.2.push(scan_targets.len());
        let databases = DevelopmentFilesScanConfiguration::new(
            config.control.clone(),
            reviewed.clone(),
            config.holdings_registry.clone(),
            AttachmentScanScope {
                holding_id: t.holding_id,
                company_id: t.company_id,
            },
        )
        .map_err(|_| "FIELD_FILES_CONFIGURATION_INVALID")?;
        scan_targets.push(FieldFilesScanTarget {
            databases,
            providers: t.providers,
        });
    }
    let clock = Arc::new(SharedClock(clock));
    let workers = assemble_field_files_scan_workers(scan_targets, clock.clone())
        .await
        .map_err(|_| "FIELD_FILES_WORKER_UNAVAILABLE")?;
    let result = (|| {
        let catalogue =
            Arc::new(current_package_catalogue().map_err(|_| "FIELD_FILES_CONFIGURATION_INVALID")?);
        let mut intake = Vec::new();
        let mut uploads = Vec::new();
        let mut mounted = BTreeSet::new();
        for (holding_id, (companies, root, indices)) in grouped {
            let (i, h) = config
                .holdings
                .iter()
                .enumerate()
                .find(|(_, h)| h.registration.holding_id == holding_id)
                .ok_or("FIELD_FILES_CONFIGURATION_INVALID")?;
            let authorizer = Arc::new(
                HoldingWorkAuthorizer::new(
                    pools[0].clone(),
                    pools[i + 1].clone(),
                    holding_id.clone(),
                    (*config.policy).clone(),
                    clock.0.clone(),
                    config.holdings_registry.clone(),
                )
                .map_err(|_| "FIELD_FILES_CONFIGURATION_INVALID")?,
            );
            let readiness = Arc::new(HoldingReadiness(
                indices.into_iter().map(|i| workers[i].clone()).collect(),
            ));
            let target = FieldIntakeTarget {
                host: h.host.clone(),
                holding_slug: h.registration.slug.clone(),
                holding: holding_id,
                company: ServiceCompanyOwnership::RegisteredHolding,
                companies: Some(companies),
                authorizer,
                catalogue: catalogue.clone(),
                clock: clock.clone(),
                policy: config.field_files_policy,
                readiness,
            };
            let writer = LocalFieldQuarantineWriter::open(&root, 2)
                .map_err(|_| "FIELD_FILES_STORE_UNAVAILABLE")?;
            let store = LocalSanitizationStore::open(&root, 2)
                .map_err(|_| "FIELD_FILES_STORE_UNAVAILABLE")?;
            mounted.insert((h.host.clone(), h.registration.slug.clone()));
            uploads.push(FieldUploadTarget {
                intake: Arc::new(target.clone()),
                pool: pools[i + 1].raw().clone(),
                writer,
                store,
            });
            intake.push(target);
        }
        let scopes = Arc::new(FieldServiceReadScopeHeaders);
        let a = field_intake_router(
            config.policy.clone(),
            config.holdings_registry.clone(),
            scopes.clone(),
            intake,
        )
        .map_err(|_| "FIELD_FILES_ROUTER_INVALID")?;
        let b = field_upload_router(
            config.policy.clone(),
            config.holdings_registry.clone(),
            scopes,
            uploads,
        )
        .map_err(|_| "FIELD_FILES_ROUTER_INVALID")?;
        Ok((a.merge(b), mounted))
    })();
    match result {
        Ok((router, mounted)) => Ok((router, workers, mounted)),
        Err(error) => {
            for w in workers {
                w.close().await;
            }
            Err(error)
        }
    }
}
pub(super) async fn run_workers(
    config: DevelopmentApiConfiguration,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> Result<(), &'static str> {
    if config.field_files_providers.is_none() {
        return Err("FIELD_FILES_WORKER_TARGET_REQUIRED");
    }
    let limits = PoolLimits {
        max_connections: 3,
        acquire_timeout: Duration::from_secs(5),
    };
    let mut pools = Vec::new();
    for database in
        std::iter::once(&config.control).chain(config.holdings.iter().map(|h| &h.database))
    {
        match database.connect(limits).await {
            Ok(p) => pools.push(p),
            Err(_) => {
                for p in pools {
                    p.close().await;
                }
                return Err("FIELD_FILES_DATABASE_UNAVAILABLE");
            }
        }
    }
    let result = assemble(&config, &pools, Arc::new(SystemAccountClock)).await;
    match result {
        Ok((_router, workers, _)) => {
            let cleanup_result=(|| {
                let raw=config.field_files_providers.as_ref().ok_or("FIELD_FILES_CONFIGURATION_INVALID")?;
                let targets=parse_field_files_runtime_configuration(&serde_json::to_vec(raw).map_err(|_|"FIELD_FILES_CONFIGURATION_INVALID")?,&config.holdings_registry).map_err(|_|"FIELD_FILES_CONFIGURATION_INVALID")?;
                if targets.len()!=workers.len(){return Err("FIELD_FILES_CONFIGURATION_INVALID");}
                targets.into_iter().zip(&workers).map(|(target,worker)|{
                    let scope=worker.cleanup_scope();
                    if scope.holding_id!=target.holding_id||scope.company_id!=target.company_id{return Err("FIELD_FILES_CONFIGURATION_INVALID");}
                    let h=config.holdings.iter().find(|h|h.registration.holding_id==target.holding_id).ok_or("FIELD_FILES_CONFIGURATION_INVALID")?;
                    let root=target.providers.files_roots.get(&h.registration.files_binding).ok_or("FIELD_FILES_CONFIGURATION_INVALID")?;
                    let writer=LocalFieldQuarantineWriter::open(root,2).map_err(|_|"FIELD_FILES_STORE_UNAVAILABLE")?;
                    worker.cleanup_worker(writer).map_err(|_|"FIELD_FILES_CONFIGURATION_INVALID")
                }).collect::<Result<Vec<_>,&'static str>>()
            })();
            let cleanup=match cleanup_result {Ok(value)=>value,Err(error)=>{
                for worker in workers {worker.close().await;}
                for pool in pools {pool.close().await;}
                return Err(error);
            }};
            run_field_files_workers(workers, cleanup, shutdown).await;
            for p in pools {
                p.close().await;
            }
            Ok(())
        }
        Err(e) => {
            for p in pools {
                p.close().await;
            }
            Err(e)
        }
    }
}
