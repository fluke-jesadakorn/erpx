//! One iteration of the shared Service Count projection worker.
//! Composition supplies the static Inventory-owned claim statement.
use erpx_server_workflow::public::service_count_outbox::{claim, eligible, complete, fail, project, renew};
use sqlx::PgPool;
use std::time::Instant;

/// Commit the Holding read projection before acknowledging the write-side lease.
/// A crash between the two commits is recovered through the idempotent receipt.
pub async fn run_once(write: &PgPool, read: &PgPool, token: &str, now: i64) -> Result<bool, sqlx::Error> {
    let mut connection=write.acquire().await?;
    let Some(event)=claim(&mut connection, erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_QUARANTINE, token, now).await? else { return Ok(false) };
    match eligible(&mut connection, erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_ELIGIBLE, &event).await {
        Ok(true) => {},
        Ok(false) => {
            let ambiguous=match eligible(&mut connection, erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_AMBIGUOUS, &event).await {
                Ok(value)=>value,
                Err(error)=> { fail(&mut connection, &event, now, "SOURCE_CHECK_FAILED").await?; return Err(error); },
            };
            fail(&mut connection, &event, now, if ambiguous { "SOURCE_AMBIGUOUS" } else { "SOURCE_INVALID" }).await?;
            return Ok(true);
        },
        Err(error) => { fail(&mut connection, &event, now, "SOURCE_CHECK_FAILED").await?; return Err(error); },
    }
    drop(connection);
    let start=Instant::now();
    let projection=async {
        let mut transaction=read.begin().await?;
        project(&mut transaction, &event, now).await?;
        transaction.commit().await
    };
    tokio::pin!(projection);
    let mut timer=tokio::time::interval(std::time::Duration::from_secs(15));
    timer.tick().await;
    let result=loop {
        tokio::select! {
            result=&mut projection => break result,
            _=timer.tick() => {
                let mut c=write.acquire().await?;
                let clock=now.saturating_add(i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX));
                if !renew(&mut c,&event,clock).await? { return Err(sqlx::Error::RowNotFound); }
            }
        }
    };
    let mut connection=write.acquire().await?;
    let clock=now.saturating_add(i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX));
    match result {
        Ok(()) => { if !complete(&mut connection, &event, clock).await? { return Err(sqlx::Error::RowNotFound); } },
        Err(error) => { fail(&mut connection, &event, clock, "PROJECTION_FAILED").await?; return Err(error); }
    }
    Ok(true)
}

/// Drain all registered Holding pairs on one worker task. The caller validates
/// both leased databases and their migration tracks before passing the pools.
pub async fn run(pairs: Vec<(PgPool, PgPool)>, shutdown: impl std::future::Future<Output=()> + Send) {
    tokio::pin!(shutdown);
    loop {
        let mut worked=false;
        for (write,read) in &pairs {
            let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX)).unwrap_or(0);
            let token=uuid::Uuid::new_v4().to_string();
            tokio::select! {
                _=&mut shutdown => return,
                result=run_once(write,read,&token,now) => match result {
                    Ok(value)=>worked|=value,
                    Err(_) => eprintln!("service count projection iteration failed"),
                }
            }
        }
        tokio::select! {
            _=&mut shutdown => break,
            _=tokio::time::sleep(std::time::Duration::from_millis(if worked { 10 } else { 1000 })) => {},
        }
    }
}
