//! Bounded runtime loop for explicitly configured, Field-only Files scanners.
//! Each worker owns one reviewed Holding/Company scope; no request chooses it.
use crate::files_scan_native::{NativeFilesScanConfiguration,NativeFilesScanWorker,assemble_native_field_files_scan};
use crate::files_system_scan::DevelopmentFilesScanConfiguration;
use erpx_server_files::public::attachment_scan::AttachmentScanRunOutcome;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError,Clock,FailureKind};
use std::{collections::BTreeSet,future::Future,sync::Arc,time::Duration};

/// Explicit reviewed startup targets, never an HTTP or deserialized authority.
pub struct FieldFilesScanTarget {
    pub databases: DevelopmentFilesScanConfiguration,
    pub providers: NativeFilesScanConfiguration,
}

/// Assembles actual leased transactions and real providers; a partial failure
/// closes every previously opened worker before returning a redacted error.
pub async fn assemble_field_files_scan_workers<C: Clock + Send + Sync + 'static>(
    targets: Vec<FieldFilesScanTarget>, clock: Arc<C>,
) -> Result<Vec<NativeFilesScanWorker<C>>, PgTransactionError<BusinessError>> {
    let mut seen=BTreeSet::new();
    if targets.is_empty() || targets.len()>64 || targets.iter().any(|target| {
        let scope=target.databases.scope();
        !seen.insert((scope.holding_id.as_str().to_owned(),scope.company_id.as_str().to_owned()))
    }) {
        return Err(BusinessError::new(FailureKind::Unavailable,"FIELD_FILES_WORKER_TARGET_INVALID","The configured Field file scanner is unavailable.").into());
    }
    let mut workers=Vec::with_capacity(targets.len());
    for target in targets {
        match assemble_native_field_files_scan(target.databases,target.providers,clock.clone()).await {
            Ok(worker)=>workers.push(worker),
            Err(error)=>{for worker in workers { worker.close().await; } return Err(error);}
        }
    }
    Ok(workers)
}

/// Stop admission on shutdown, let the in-flight provider observe cancellation,
/// then drain provider processes and close all owned database pools.
pub async fn run_field_files_scan_workers<C: Clock + Send + Sync + 'static>(
    workers: Vec<NativeFilesScanWorker<C>>,
    shutdown: impl Future<Output = ()> + Send,
) {
    run_field_files_workers(workers,Vec::new(),shutdown).await;
}
/// The combined jobs entrypoint drains each admitted cleanup turn before
/// shutdown; irreversible filesystem work is never selected away or replayed.
pub async fn run_field_files_workers<C:Clock+Send+Sync+'static>(
    workers:Vec<NativeFilesScanWorker<C>>,
    mut cleanup:Vec<crate::field_files_cleanup_worker::FieldFilesCleanupWorker<C>>,
    shutdown:impl Future<Output=()>+Send,
){
    tokio::pin!(shutdown);
    'worker: loop {
        let mut worked = false;
        for worker in &workers {
            let candidate = tokio::select! {
                _ = &mut shutdown => break 'worker,
                result = worker.next_field_candidate() => match result {
                    Ok(Some(value)) => value,
                    Ok(None) => continue,
                    Err(_) => { eprintln!("field files candidate unavailable"); continue; },
                },
            };
            let next = worker.run_next(Some(&candidate));
            tokio::pin!(next);
            let result = tokio::select! {
                _ = &mut shutdown => {
                    for owned in &workers { owned.request_shutdown(); }
                    let _ = next.await;
                    break 'worker;
                },
                result = &mut next => result,
            };
            match result {
                Ok(AttachmentScanRunOutcome::NoWork) => {},
                Ok(AttachmentScanRunOutcome::Settled(_)) => worked = true,
                Ok(AttachmentScanRunOutcome::Interrupted) => {},
                Err(_) => eprintln!("field files scan iteration unavailable"),
            }
        }
        for cleaner in &mut cleanup {
            tokio::select! {
                biased;
                _=&mut shutdown=>break 'worker,
                _=std::future::ready(())=>{},
            }
            if cleaner.run_once().await.is_err(){eprintln!("field files cleanup turn unavailable");}
        }
        tokio::select! {
            _ = &mut shutdown => break,
            _ = tokio::time::sleep(Duration::from_millis(if worked { 10 } else { 1000 })) => {},
        }
    }
    for cleaner in cleanup {cleaner.writer.stop();cleaner.writer.drain().await;}
    for worker in &workers { worker.request_shutdown(); }
    for worker in workers { worker.close().await; }
}
