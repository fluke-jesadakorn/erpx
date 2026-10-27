//! Explicit disposable-only source seed/rotation policy for component tests.
//! It proves the native adapter transaction, not real admin/review/MFA admission.
use erpx_server_identity_access::public::directory::*;
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row};
use std::sync::Arc;
pub struct SyntheticSourcePolicy {
    pub kind: DirectorySourceChangeKind,
}
struct Unit {
    database: String,
    backend: i32,
    xid: String,
}
async fn unit(c: &mut PgConnection) -> Result<Unit, sqlx::Error> {
    let row=sqlx::query("SELECT current_database() database,pg_backend_pid() backend,pg_current_xact_id()::text xid").fetch_one(c).await?;
    Ok(Unit {
        database: row.try_get("database")?,
        backend: row.try_get("backend")?,
        xid: row.try_get("xid")?,
    })
}
fn guarded(unit: &Unit) {
    assert!(
        unit.database.starts_with("erpx_dev_rust_"),
        "synthetic source policy requires disposable DB"
    );
}
async fn same(c: &mut PgConnection, original: &Unit) -> Result<(), sqlx::Error> {
    let current = unit(c).await?;
    guarded(&current);
    assert!(
        current.database == original.database
            && current.backend == original.backend
            && current.xid == original.xid,
        "source proof must stay in its actual transaction"
    );
    Ok(())
}
struct Control {
    unit: Unit,
    deadline: i64,
    kind: DirectorySourceChangeKind,
}
struct Holding {
    unit: Unit,
}
impl PgDirectorySourceChangePolicy for SyntheticSourcePolicy {
    fn prepare_control<'c>(
        &'c self,
        _c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
    fn retain_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
        change: &'c PreparedDirectorySourceChange,
        now: i64,
    ) -> PgWorkFuture<'c, Arc<dyn PgDirectorySourceControlPolicy>, BusinessError> {
        Box::pin(async move {
            assert!(change.kind() == self.kind);
            let unit = unit(c).await?;
            guarded(&unit);
            Ok(Arc::new(Control {
                unit,
                deadline: now + 120000,
                kind: self.kind,
            }) as Arc<dyn PgDirectorySourceControlPolicy>)
        })
    }
}
impl PgDirectorySourceControlPolicy for Control {
    fn prepare_holding<'c>(
        &'c self,
        _c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
    fn authorize_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
        change: &'c PreparedDirectorySourceChange,
        now: i64,
    ) -> PgWorkFuture<'c, Box<dyn PgDirectorySourceHoldingPolicy>, BusinessError> {
        Box::pin(async move {
            assert!(change.kind() == self.kind);
            assert!(now < self.deadline);
            assert!(change.holding_id().is_some());
            let unit = unit(c).await?;
            guarded(&unit);
            Ok(Box::new(Holding { unit }) as Box<dyn PgDirectorySourceHoldingPolicy>)
        })
    }
    fn deadline(&self) -> i64 {
        self.deadline
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            assert!(now < self.deadline);
            same(c, &self.unit).await?;
            Ok(())
        })
    }
}
impl PgDirectorySourceHoldingPolicy for Holding {
    fn audit_actor(&self) -> &str {
        "synthetic-explicit-source-operator"
    }
    fn audit_assignment(&self) -> &str {
        "synthetic-source-component-policy"
    }
    fn audit_reason(&self) -> &str {
        "Explicit disposable fixture source selection; not administrator acceptance"
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        _now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            same(c, &self.unit).await?;
            Ok(())
        })
    }
}
pub async fn select(
    directory: &PostgresDirectory,
    id: &str,
    connection_version: i64,
    previous: Option<i64>,
    key: &str,
) -> Result<i64, BusinessError> {
    let request = DirectorySourceChangeRequest::new(
        erpx_server_shared::public::RecordId::new(id)?,
        key.into(),
        connection_version,
        previous
            .map(provenance::SourceRevision::checked)
            .transpose()
            .map_err(provenance::Denial::business)?,
    )?;
    let kind = request.kind();
    let prepared = directory.prepare_source_change(request).await?;
    Ok(directory
        .change_source(prepared, Arc::new(SyntheticSourcePolicy { kind }))
        .await?
        .get())
}
